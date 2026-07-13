// Copyright (C) 2024 Jelmer Vernooij <jelmer@samba.org>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Type inference using ty, embedded in-process.
//!
//! Unlike the pyright and mypy backends, which shell out to a language server
//! and parse a type name out of the hover text, this queries ty's inference
//! engine directly and asks the resulting type for its class and module name.
//!
//! Migration works on source that has been rewritten in memory and may never
//! have existed on disk, so files under migration are held in an overlay (see
//! `OverlaySystem`) that ty reads in preference to the real filesystem.

use anyhow::{anyhow, Result};
use ruff_db::file_revision::FileRevision;
use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::walk_directory::WalkDirectoryBuilder;
use ruff_db::system::{
    DirectoryEntry, FileType, Metadata, OsSystem, Result as SystemResult, System, SystemPath,
    SystemPathBuf, SystemVirtualPath, WhichResult, WritableSystem,
};
use ruff_db::Db as _;
use ruff_notebook::{Notebook, NotebookError};
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::{AnyNodeRef, Expr, PySourceType};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use ty_project::{ProjectDatabase, ProjectMetadata};
use ty_python_semantic::types::Type;
use ty_python_semantic::{HasType, SemanticModel};

/// A source file held in memory, and how many times it has changed.
#[derive(Debug, Clone)]
struct Overlay {
    contents: String,
    revision: u128,
}

/// The files under migration, which ty must see instead of whatever is on disk.
#[derive(Debug, Default)]
struct Overlays(RwLock<HashMap<SystemPathBuf, Overlay>>);

impl Overlays {
    fn get(&self, path: &SystemPath) -> Option<Overlay> {
        self.0.read().unwrap().get(path).cloned()
    }

    /// Store `contents` for `path`, bumping its revision so ty re-reads it.
    fn set(&self, path: SystemPathBuf, contents: String) {
        let mut overlays = self.0.write().unwrap();
        let revision = overlays.get(&path).map_or(1, |o| o.revision + 1);
        overlays.insert(path, Overlay { contents, revision });
    }
}

/// A [`System`] that serves the files under migration from memory and defers
/// everything else -- typeshed, site-packages, imported modules -- to the real
/// filesystem, so imports still resolve.
#[derive(Debug, Clone)]
struct OverlaySystem {
    overlays: Arc<Overlays>,
    native: OsSystem,
}

impl System for OverlaySystem {
    fn path_metadata(&self, path: &SystemPath) -> SystemResult<Metadata> {
        match self.overlays.get(path) {
            // The bumped revision is what tells ty the contents changed.
            Some(overlay) => Ok(Metadata::new(
                FileRevision::new(overlay.revision),
                None,
                FileType::File,
            )),
            None => self.native.path_metadata(path),
        }
    }

    fn read_to_string(&self, path: &SystemPath) -> SystemResult<String> {
        match self.overlays.get(path) {
            Some(overlay) => Ok(overlay.contents),
            None => self.native.read_to_string(path),
        }
    }

    fn canonicalize_path(&self, path: &SystemPath) -> SystemResult<SystemPathBuf> {
        // An overlaid file need not exist on disk, so it cannot be canonicalized.
        if self.overlays.get(path).is_some() {
            return Ok(path.to_path_buf());
        }
        self.native.canonicalize_path(path)
    }

    fn path_exists(&self, path: &SystemPath) -> bool {
        self.overlays.get(path).is_some() || self.native.path_exists(path)
    }

    fn is_file(&self, path: &SystemPath) -> bool {
        self.overlays.get(path).is_some() || self.native.is_file(path)
    }

    fn read_to_notebook(&self, path: &SystemPath) -> std::result::Result<Notebook, NotebookError> {
        self.native.read_to_notebook(path)
    }

    fn read_virtual_path_to_string(&self, path: &SystemVirtualPath) -> SystemResult<String> {
        self.native.read_virtual_path_to_string(path)
    }

    fn read_virtual_path_to_notebook(
        &self,
        path: &SystemVirtualPath,
    ) -> std::result::Result<Notebook, NotebookError> {
        self.native.read_virtual_path_to_notebook(path)
    }

    fn source_type(&self, path: &SystemPath) -> Option<PySourceType> {
        self.native.source_type(path)
    }

    fn is_same_file(&self, first: &SystemPath, second: &SystemPath) -> SystemResult<bool> {
        self.native.is_same_file(first, second)
    }

    fn which(&self, binary_name: &str) -> WhichResult {
        self.native.which(binary_name)
    }

    fn current_directory(&self) -> &SystemPath {
        self.native.current_directory()
    }

    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        self.native.user_config_directory()
    }

    fn cache_dir(&self) -> Option<SystemPathBuf> {
        self.native.cache_dir()
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> SystemResult<Box<dyn Iterator<Item = SystemResult<DirectoryEntry>> + 'a>> {
        self.native.read_directory(path)
    }

    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        self.native.walk_directory(path)
    }

    fn as_writable(&self) -> Option<&dyn WritableSystem> {
        None
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn dyn_clone(&self) -> Box<dyn System> {
        Box::new(self.clone())
    }
}

pub struct TyTypeIntrospector {
    db: ProjectDatabase,
    overlays: Arc<Overlays>,
}

impl TyTypeIntrospector {
    /// Create an introspector rooted at `workspace_root` (or the current directory).
    pub fn new(workspace_root: Option<&str>) -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let cwd = SystemPath::from_std_path(&cwd)
            .ok_or_else(|| anyhow!("current directory is not valid UTF-8: {}", cwd.display()))?
            .to_path_buf();

        let root = match workspace_root {
            Some(root) => SystemPath::from_std_path(Path::new(root))
                .ok_or_else(|| anyhow!("workspace root is not valid UTF-8: {root}"))?
                .to_path_buf(),
            None => cwd.clone(),
        };

        let overlays = Arc::new(Overlays::default());
        let system = OverlaySystem {
            overlays: Arc::clone(&overlays),
            native: OsSystem::new(&cwd),
        };

        let metadata = ProjectMetadata::discover(&root, &system)
            .map_err(|e| anyhow!("failed to discover project at {root}: {e}"))?;
        let db = ProjectDatabase::fallible(metadata, system)?;

        Ok(Self { db, overlays })
    }

    /// Make `content` the source ty sees for `file_path`, replacing any earlier version.
    pub fn set_file_content(&mut self, file_path: &Path, content: &str) -> Result<()> {
        let path = self.system_path(file_path)?;
        self.overlays.set(path.clone(), content.to_string());

        // Salsa caches file contents, so it only re-reads once told the metadata changed.
        if let Ok(file) = system_path_to_file(&self.db, &path) {
            file.sync(&mut self.db);
        }
        Ok(())
    }

    /// Look up the type of the expression at `range`, returning its qualified name.
    pub fn query_type(&self, file_path: &Path, range: TextRange) -> Result<Option<String>> {
        let path = self.system_path(file_path)?;
        let file = system_path_to_file(&self.db, &path)
            .map_err(|e| anyhow!("could not load {}: {e:?}", file_path.display()))?;

        let module = parsed_module(&self.db, file).load(&self.db);
        let Some(expr) = expr_at_range(module.syntax(), range) else {
            return Ok(None);
        };

        let model = SemanticModel::new(&self.db, file);
        let Some(ty) = expr.inferred_type(&model) else {
            return Ok(None);
        };

        Ok(self.qualified_name(ty))
    }

    /// Render a type as the qualified name the replacement map is keyed on.
    ///
    /// Only an instance of a class can carry a deprecated method, so anything
    /// else (callables, literals, unions, `Unknown`) has no name to return.
    fn qualified_name(&self, ty: Type<'_>) -> Option<String> {
        let Type::NominalInstance(instance) = ty else {
            return None;
        };

        let name = instance.class_name(&self.db);
        match instance.class_module_name(&self.db) {
            Some(module) => Some(format!("{module}.{name}")),
            // A class defined in a script has no resolvable module.
            None => Some(name.to_string()),
        }
    }

    /// Resolve `file_path` against the project root, without requiring it to exist.
    fn system_path(&self, file_path: &Path) -> Result<SystemPathBuf> {
        let path = SystemPath::from_std_path(file_path)
            .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", file_path.display()))?;
        if file_path.is_absolute() {
            Ok(path.to_path_buf())
        } else {
            Ok(self.db.system().current_directory().join(path))
        }
    }
}

/// Find the expression exactly covering `range` in ty's parse of the file.
fn expr_at_range(module: &ruff_python_ast::ModModule, range: TextRange) -> Option<&Expr> {
    let mut finder = ExprFinder { range, found: None };
    source_order::walk_body(&mut finder, &module.body);
    finder.found
}

struct ExprFinder<'a> {
    range: TextRange,
    found: Option<&'a Expr>,
}

impl<'a> SourceOrderVisitor<'a> for ExprFinder<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        // Skip whole subtrees that cannot contain the target range.
        if self.found.is_some() || !node.range().contains_range(self.range) {
            TraversalSignal::Skip
        } else {
            TraversalSignal::Traverse
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.range() == self.range {
            self.found = Some(expr);
            return;
        }
        if expr.range().contains_range(self.range) {
            source_order::walk_expr(self, expr);
        }
    }
}
