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
use ruff_python_ast::{self as ast, AnyNodeRef, Expr, PySourceType, Stmt};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use ty_module_resolver::file_to_module;
use ty_project::{ProjectDatabase, ProjectMetadata};
use ty_python_core::definition::Definition;
use ty_python_semantic::types::{Type, TypeDefinition};
use ty_python_semantic::{
    definitions_for_name, HasType, ImportAliasResolution, ResolvedDefinition, SemanticModel,
};

/// How long a chain of un-annotated bindings to follow before giving up.
///
/// Recovering an un-annotated type means walking from a name to its binding to the
/// function it called, and so on. Mutually recursive factories never bottom out.
const MAX_INFERENCE_DEPTH: u32 = 8;

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
        Ok(self.class_of_expr(&model, expr, 0))
    }

    /// The qualified name of the class `expr` evaluates to, if it is a class instance.
    ///
    /// Asks ty first. ty leaves an un-annotated return type as `Unknown`
    /// (astral-sh/ty#128), so where it gives up this recovers the type the way that
    /// issue proposes: from the types of the function's own return expressions.
    ///
    /// `depth` bounds the chain of un-annotated bindings we are willing to follow.
    fn class_of_expr(&self, model: &SemanticModel<'_>, expr: &Expr, depth: u32) -> Option<String> {
        if depth > MAX_INFERENCE_DEPTH {
            return None;
        }

        if let Some(name) = expr
            .inferred_type(model)
            .and_then(|ty| self.qualified_name(ty))
        {
            return Some(name);
        }

        match expr {
            Expr::Call(call) => self.class_of_call(model, call, depth),
            // ty propagated `Unknown` into the binding, so look at what it was bound from.
            Expr::Name(name) => self.class_of_name(model, name, depth),
            _ => None,
        }
    }

    /// Infer the result of calling a function whose return type is un-annotated.
    fn class_of_call(
        &self,
        model: &SemanticModel<'_>,
        call: &ast::ExprCall,
        depth: u32,
    ) -> Option<String> {
        let TypeDefinition::Function(definition) =
            call.func.inferred_type(model)?.definition(&self.db)?
        else {
            return None;
        };

        // The callee may live in another module, so infer within its own file.
        let function_file = definition.file(&self.db);
        let function_module = parsed_module(&self.db, function_file).load(&self.db);
        let range = definition.full_range(&self.db, &function_module).range();
        let function = function_at_range(function_module.syntax(), range)?;
        let function_model = SemanticModel::new(&self.db, function_file);

        self.single_return_class(&function_model, &function.body, depth)
    }

    /// Infer a name whose binding ty could only type as `Unknown`.
    fn class_of_name(
        &self,
        model: &SemanticModel<'_>,
        name: &ast::ExprName,
        depth: u32,
    ) -> Option<String> {
        let definitions = definitions_for_name(
            model,
            name.id.as_str(),
            AnyNodeRef::from(name),
            ImportAliasResolution::ResolveAliases,
        );

        // Only a single binding tells us unambiguously what the name holds.
        let [ResolvedDefinition::Definition(definition)] = definitions.as_slice() else {
            return None;
        };

        let binding_file = definition.file(&self.db);
        let binding_module = parsed_module(&self.db, binding_file).load(&self.db);
        let range = definition.full_range(&self.db, &binding_module).range();
        let binding = bound_value_at_range(binding_module.syntax(), range)?;

        let binding_model = SemanticModel::new(&self.db, binding_file);
        match binding {
            Binding::Assigned(value) => self.class_of_expr(&binding_model, value, depth + 1),
            // `with cm as name` binds what the context manager's `__enter__` returns,
            // which need not be the context manager itself.
            Binding::ContextManager(value) => self.class_of_enter(&binding_model, value, depth + 1),
        }
    }

    /// The class bound by `with <context_manager> as ...`.
    ///
    /// That is the return type of the context manager's `__enter__`, so this resolves
    /// the manager's class and infers `__enter__` from its body.
    fn class_of_enter(
        &self,
        model: &SemanticModel<'_>,
        context_manager: &Expr,
        depth: u32,
    ) -> Option<String> {
        let class = self.class_definition(model, context_manager, depth)?;

        let class_file = class.file(&self.db);
        let class_module = parsed_module(&self.db, class_file).load(&self.db);
        let range = class.full_range(&self.db, &class_module).range();
        let class_body = class_at_range(class_module.syntax(), range)?;

        let enter = class_body.body.iter().find_map(|stmt| match stmt {
            Stmt::FunctionDef(f) if f.name.as_str() == "__enter__" => Some(f),
            _ => None,
        })?;

        // `return self` is the common case, and ty types `self` as a TypeVar rather
        // than an instance, so answer it with the context manager's own class.
        let returns = return_values(&enter.body);
        let self_param = enter.parameters.args.first().map(|p| p.name().as_str());
        if !returns.is_empty()
            && returns.iter().all(|value| match value {
                Expr::Name(name) => Some(name.id.as_str()) == self_param,
                _ => false,
            })
        {
            return self.class_of_expr(model, context_manager, depth + 1);
        }

        let class_model = SemanticModel::new(&self.db, class_file);
        self.single_return_class(&class_model, &enter.body, depth + 1)
    }

    /// The class definition of whatever `expr` evaluates to.
    fn class_definition<'db>(
        &'db self,
        model: &SemanticModel<'db>,
        expr: &Expr,
        depth: u32,
    ) -> Option<Definition<'db>> {
        // Mutually recursive factories would otherwise never bottom out.
        if depth > MAX_INFERENCE_DEPTH {
            return None;
        }

        // ty knows the class outright unless the type came from an un-annotated return.
        if let Some(ty) = expr.inferred_type(model) {
            if let Some(TypeDefinition::StaticClass(definition)) = ty.definition(&self.db) {
                return Some(definition);
            }
        }

        // Otherwise recover it from the call's return expressions.
        let Expr::Call(call) = expr else {
            return None;
        };
        let TypeDefinition::Function(function) =
            call.func.inferred_type(model)?.definition(&self.db)?
        else {
            return None;
        };

        let file = function.file(&self.db);
        let module = parsed_module(&self.db, file).load(&self.db);
        let range = function.full_range(&self.db, &module).range();
        let function = function_at_range(module.syntax(), range)?;
        let function_model = SemanticModel::new(&self.db, file);

        let mut found: Option<Definition<'db>> = None;
        for value in return_values(&function.body) {
            let definition = self.class_definition(&function_model, value, depth + 1)?;
            match found {
                Some(existing) if existing != definition => return None,
                Some(_) => {}
                None => found = Some(definition),
            }
        }
        found
    }

    /// The single class every `return` in `body` evaluates to, if they agree.
    fn single_return_class(
        &self,
        model: &SemanticModel<'_>,
        body: &[Stmt],
        depth: u32,
    ) -> Option<String> {
        let mut returned: Option<String> = None;
        for value in return_values(body) {
            let name = self.class_of_expr(model, value, depth + 1)?;
            match &returned {
                // A migration needs one class to look the deprecated method up on.
                Some(existing) if *existing != name => return None,
                Some(_) => {}
                None => returned = Some(name),
            }
        }
        returned
    }

    /// Render a type as the qualified name the replacement map is keyed on.
    ///
    /// Only a class carries a deprecated method, so anything else (callables,
    /// literals, unions, `Unknown`) has no name to return.
    fn qualified_name(&self, ty: Type<'_>) -> Option<String> {
        match ty {
            Type::NominalInstance(instance) => {
                let name = instance.class_name(&self.db);
                match instance.class_module_name(&self.db) {
                    Some(module) => Some(format!("{module}.{name}")),
                    // A class defined in a script has no resolvable module.
                    None => Some(name.to_string()),
                }
            }
            // The receiver of a classmethod or staticmethod call is the class itself.
            Type::ClassLiteral(_) => {
                let TypeDefinition::StaticClass(definition) = ty.definition(&self.db)? else {
                    return None;
                };
                self.qualified_class_name(definition)
            }
            _ => None,
        }
    }

    /// The qualified name of a class from its definition.
    fn qualified_class_name(&self, definition: Definition<'_>) -> Option<String> {
        let file = definition.file(&self.db);
        let module = parsed_module(&self.db, file).load(&self.db);
        let range = definition.full_range(&self.db, &module).range();
        let class = class_at_range(module.syntax(), range)?;

        let name = class.name.as_str();
        match file_to_module(&self.db, file) {
            Some(module) => Some(format!("{}.{}", module.name(&self.db), name)),
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

/// How a name was bound, for the binding forms whose type ty can leave as `Unknown`.
enum Binding<'a> {
    /// `name = <value>`
    Assigned(&'a Expr),
    /// `with <context manager> as name`
    ContextManager(&'a Expr),
}

/// Find the class definition at `range` in ty's parse of the file.
fn class_at_range(
    module: &ruff_python_ast::ModModule,
    range: TextRange,
) -> Option<&ast::StmtClassDef> {
    fn walk(body: &[Stmt], range: TextRange) -> Option<&ast::StmtClassDef> {
        for stmt in body {
            if !stmt.range().contains_range(range) {
                continue;
            }
            if let Stmt::ClassDef(class) = stmt {
                if class.range() == range {
                    return Some(class);
                }
            }
            let nested = match stmt {
                Stmt::FunctionDef(f) => &f.body,
                Stmt::ClassDef(c) => &c.body,
                _ => continue,
            };
            if let Some(found) = walk(nested, range) {
                return Some(found);
            }
        }
        None
    }
    walk(&module.body, range)
}

/// The binding a name at `range` came from.
///
/// `range` covers the whole binding statement, not just the target name.
fn bound_value_at_range(
    module: &ruff_python_ast::ModModule,
    range: TextRange,
) -> Option<Binding<'_>> {
    fn walk<'a>(body: &'a [Stmt], range: TextRange) -> Option<Binding<'a>> {
        for stmt in body {
            if !stmt.range().contains_range(range) {
                continue;
            }
            match stmt {
                Stmt::Assign(assign) if assign.range() == range => {
                    return Some(Binding::Assigned(&assign.value));
                }
                Stmt::With(with) => {
                    // The binding ty reports may be the item or just its target name.
                    for item in &with.items {
                        if item.range().contains_range(range) {
                            return Some(Binding::ContextManager(&item.context_expr));
                        }
                    }
                    if let Some(found) = walk(&with.body, range) {
                        return Some(found);
                    }
                }
                Stmt::FunctionDef(f) => {
                    if let Some(found) = walk(&f.body, range) {
                        return Some(found);
                    }
                }
                Stmt::ClassDef(c) => {
                    if let Some(found) = walk(&c.body, range) {
                        return Some(found);
                    }
                }
                Stmt::If(node) => {
                    if let Some(found) = walk(&node.body, range) {
                        return Some(found);
                    }
                    for clause in &node.elif_else_clauses {
                        if let Some(found) = walk(&clause.body, range) {
                            return Some(found);
                        }
                    }
                }
                Stmt::For(node) => {
                    if let Some(found) = walk(&node.body, range) {
                        return Some(found);
                    }
                }
                Stmt::While(node) => {
                    if let Some(found) = walk(&node.body, range) {
                        return Some(found);
                    }
                }
                Stmt::Try(node) => {
                    if let Some(found) = walk(&node.body, range) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(&module.body, range)
}

/// Find the function definition at `range` in ty's parse of the file.
fn function_at_range(
    module: &ruff_python_ast::ModModule,
    range: TextRange,
) -> Option<&ast::StmtFunctionDef> {
    fn walk(body: &[Stmt], range: TextRange) -> Option<&ast::StmtFunctionDef> {
        for stmt in body {
            if !stmt.range().contains_range(range) {
                continue;
            }
            if let Stmt::FunctionDef(function) = stmt {
                if function.range() == range {
                    return Some(function);
                }
            }
            // The definition may be a method, so descend into classes and functions.
            let nested = match stmt {
                Stmt::FunctionDef(f) => &f.body,
                Stmt::ClassDef(c) => &c.body,
                _ => continue,
            };
            if let Some(found) = walk(nested, range) {
                return Some(found);
            }
        }
        None
    }
    walk(&module.body, range)
}

/// The value of every `return` in `body`, skipping nested scopes.
///
/// Returns inside a nested function or lambda belong to that function, not this one.
fn return_values(body: &[Stmt]) -> Vec<&Expr> {
    fn walk<'a>(body: &'a [Stmt], out: &mut Vec<&'a Expr>) {
        for stmt in body {
            match stmt {
                Stmt::Return(ret) => out.extend(ret.value.as_deref()),
                // A nested definition has its own returns.
                Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
                Stmt::If(node) => {
                    walk(&node.body, out);
                    for clause in &node.elif_else_clauses {
                        walk(&clause.body, out);
                    }
                }
                Stmt::While(node) => {
                    walk(&node.body, out);
                    walk(&node.orelse, out);
                }
                Stmt::For(node) => {
                    walk(&node.body, out);
                    walk(&node.orelse, out);
                }
                Stmt::With(node) => walk(&node.body, out),
                Stmt::Try(node) => {
                    walk(&node.body, out);
                    for handler in &node.handlers {
                        let ast::ExceptHandler::ExceptHandler(handler) = handler;
                        walk(&handler.body, out);
                    }
                    walk(&node.orelse, out);
                    walk(&node.finalbody, out);
                }
                Stmt::Match(node) => {
                    for case in &node.cases {
                        walk(&case.body, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(body, &mut out);
    out
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
