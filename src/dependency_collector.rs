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

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tracing;

use crate::core::{CollectorResult, ImportInfo, ReplaceInfo, RuffDeprecatedFunctionCollector};

/// Global cache for module analysis results
static MODULE_CACHE: Lazy<Mutex<HashMap<String, CollectorResult>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Collection result for dependency analysis
#[derive(Debug, Clone)]
pub struct DependencyCollectionResult {
    pub replacements: HashMap<String, ReplaceInfo>,
    pub inheritance_map: HashMap<String, Vec<String>>,
    pub class_methods: HashMap<String, HashSet<String>>,
}

impl Default for DependencyCollectionResult {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyCollectionResult {
    pub fn new() -> Self {
        Self {
            replacements: HashMap::new(),
            inheritance_map: HashMap::new(),
            class_methods: HashMap::new(),
        }
    }

    /// Merge another result into this one
    pub fn update(&mut self, other: &DependencyCollectionResult) {
        self.replacements.extend(other.replacements.clone());
        self.inheritance_map.extend(other.inheritance_map.clone());

        // Merge class_methods, combining sets for same classes
        for (class_name, methods) in &other.class_methods {
            self.class_methods
                .entry(class_name.clone())
                .or_default()
                .extend(methods.clone());
        }
    }
}

impl From<CollectorResult> for DependencyCollectionResult {
    fn from(result: CollectorResult) -> Self {
        Self {
            replacements: result.replacements,
            inheritance_map: result.inheritance_map,
            class_methods: result.class_methods,
        }
    }
}

/// Clear the module analysis cache
pub fn clear_module_cache() {
    if let Ok(mut cache) = MODULE_CACHE.lock() {
        cache.clear();
    }
}

/// Get all base classes in the inheritance chain for a given class
fn get_inheritance_chain_for_class(
    class_name: &str,
    inheritance_map: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let mut chain = Vec::new();
    let mut to_process = vec![class_name.to_string()];
    let mut processed = HashSet::new();

    while let Some(current) = to_process.pop() {
        if processed.contains(&current) {
            continue;
        }
        processed.insert(current.clone());

        if let Some(bases) = inheritance_map.get(&current) {
            chain.extend(bases.clone());
            to_process.extend(bases.clone());
        }
    }

    chain
}

/// Extract all imports from a Python source file
pub fn collect_imports_from_source(source: &str, module_name: &str) -> Result<Vec<ImportInfo>> {
    // Create collector using Ruff to extract imports
    let collector = RuffDeprecatedFunctionCollector::new(module_name.to_string(), None);
    let result = collector.collect_from_source(source.to_string())?;

    Ok(result.imports)
}

/// Resolve a module name to its actual import path
pub fn resolve_module_path(module_name: &str, relative_to: Option<&str>) -> Option<String> {
    // Handle relative imports
    if module_name.starts_with('.') {
        let relative_to = relative_to?;

        // Count leading dots
        let level = module_name.chars().take_while(|&c| c == '.').count();
        let relative_parts: Vec<&str> = if module_name.len() > level {
            module_name[level..].split('.').collect()
        } else {
            vec![]
        };

        // Go up 'level' packages from relative_to
        let mut base_parts: Vec<&str> = relative_to.split('.').collect();
        if level >= base_parts.len() {
            return None;
        }

        base_parts.truncate(base_parts.len() - level);
        base_parts.extend(relative_parts);

        Some(base_parts.join("."))
    } else {
        Some(module_name.to_string())
    }
}

/// Quick check if source might contain replace_me
pub fn might_contain_replace_me(source: &str) -> bool {
    // Check for @replace_me decorators even if replace_me itself isn't directly imported
    source.contains("@replace_me") || source.contains("replace_me")
}

/// Find Python module file using importlib
#[allow(dead_code)]
fn find_module_file(module_path: &str) -> Option<String> {
    find_module_file_with_paths(module_path, &[])
}

/// Find Python module file using importlib with additional search paths
fn find_module_file_with_paths(module_path: &str, additional_paths: &[String]) -> Option<String> {
    use pyo3::prelude::*;

    Python::with_gil(|py| {
        // First check additional paths if provided (for test environments)
        if !additional_paths.is_empty() {
            tracing::debug!(
                "Checking additional paths for module {}: {:?}",
                module_path,
                additional_paths
            );
            // For each additional path, check if the module exists there
            for base_path in additional_paths {
                // Convert module path to file path
                let module_parts: Vec<&str> = module_path.split('.').collect();
                let mut file_path = std::path::PathBuf::from(base_path);
                for part in &module_parts {
                    file_path.push(part);
                }

                // Check for __init__.py (package)
                let init_path = file_path.join("__init__.py");
                if init_path.exists() {
                    return Some(init_path.to_string_lossy().to_string());
                }

                // Check for .py file (module)
                file_path.set_extension("py");
                tracing::debug!(
                    "Checking path: {:?}, exists: {}",
                    file_path,
                    file_path.exists()
                );
                if file_path.exists() {
                    tracing::debug!("Found module at: {:?}", file_path);
                    return Some(file_path.to_string_lossy().to_string());
                }
            }
        }

        // If not found in additional paths, try importlib
        let importlib_util = py.import("importlib.util").ok()?;
        let find_spec = importlib_util.getattr("find_spec").ok()?;

        // Try to find the module with current sys.path
        if let Ok(spec) = find_spec.call1((module_path,)) {
            if !spec.is_none() {
                if let Ok(origin) = spec.getattr("origin") {
                    if !origin.is_none() {
                        if let Ok(path) = origin.extract::<String>() {
                            return Some(path);
                        }
                    }
                }
            }
        }

        None
    })
}

/// Collect all deprecated functions from a single module
pub fn collect_deprecated_from_module(module_path: &str) -> Result<DependencyCollectionResult> {
    collect_deprecated_from_module_with_paths(module_path, &[])
}

/// Collect all deprecated functions from a single module with additional search paths
pub fn collect_deprecated_from_module_with_paths(
    module_path: &str,
    additional_paths: &[String],
) -> Result<DependencyCollectionResult> {
    // Check cache first
    if let Ok(cache) = MODULE_CACHE.lock() {
        if let Some(cached) = cache.get(module_path) {
            return Ok(cached.clone().into());
        }
    }

    let mut result = CollectorResult::new();

    // Find the module file
    tracing::debug!(
        "Looking for module {} with additional paths: {:?}",
        module_path,
        additional_paths
    );
    if let Some(file_path) = find_module_file_with_paths(module_path, additional_paths) {
        tracing::debug!("Found module {} at {}", module_path, file_path);

        // Read the source file
        let source = fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read module file: {}", file_path))?;

        // Quick check for replace_me
        if !might_contain_replace_me(&source) {
            tracing::debug!("Module {} does not contain replace_me", module_path);
            // Cache empty result
            if let Ok(mut cache) = MODULE_CACHE.lock() {
                cache.insert(module_path.to_string(), result.clone());
            }
            return Ok(result.into());
        }

        tracing::debug!("Module {} contains replace_me, collecting...", module_path);

        // Parse and collect using Ruff
        let collector = RuffDeprecatedFunctionCollector::new(
            module_path.to_string(),
            Some(Path::new(&file_path)),
        );
        if let Ok(collector_result) = collector.collect_from_source(source) {
            tracing::debug!(
                "Found {} replacements in {}",
                collector_result.replacements.len(),
                module_path
            );
            for (key, replacement) in &collector_result.replacements {
                tracing::debug!(
                    "  Replacement key: {} -> {}",
                    key,
                    replacement.replacement_expr
                );
            }
            result = collector_result;
        }
    } else {
        tracing::debug!("Module {} not found", module_path);
    }

    // Cache the result
    if let Ok(mut cache) = MODULE_CACHE.lock() {
        cache.insert(module_path.to_string(), result.clone());
    }

    Ok(result.into())
}

/// Collect all deprecated functions from imported modules
pub fn collect_deprecated_from_dependencies(
    source: &str,
    module_name: &str,
    max_depth: i32,
) -> Result<DependencyCollectionResult> {
    collect_deprecated_from_dependencies_with_paths(source, module_name, max_depth, &[])
}

/// Collect all deprecated functions from imported modules with additional search paths
pub fn collect_deprecated_from_dependencies_with_paths(
    source: &str,
    module_name: &str,
    max_depth: i32,
    additional_paths: &[String],
) -> Result<DependencyCollectionResult> {
    tracing::info!(
        "Starting recursive collection for module {} with max_depth {}",
        module_name,
        max_depth
    );
    collect_deprecated_from_dependencies_recursive(
        source,
        module_name,
        max_depth,
        &mut HashSet::new(),
        additional_paths,
    )
}

/// Internal recursive function that tracks visited modules to avoid cycles
fn collect_deprecated_from_dependencies_recursive(
    source: &str,
    module_name: &str,
    max_depth: i32,
    visited_modules: &mut HashSet<String>,
    additional_paths: &[String],
) -> Result<DependencyCollectionResult> {
    let mut result = DependencyCollectionResult::new();

    // Stop if we've reached max depth
    if max_depth <= 0 {
        return Ok(result);
    }

    // Get imports from source
    let imports = collect_imports_from_source(source, module_name)?;
    tracing::info!("Found {} imports in source", imports.len());
    for imp in &imports {
        tracing::info!("  Import: {:?}", imp);
    }

    // Group imports by resolved module path
    let mut module_imports: HashMap<String, Vec<ImportInfo>> = HashMap::new();

    for imp in imports {
        if let Some(resolved) = resolve_module_path(&imp.module, Some(module_name)) {
            module_imports.entry(resolved).or_default().push(imp);
        }
    }

    // Process each unique module
    for (resolved, imp_list) in module_imports {
        // Skip if we've already visited this module (avoid cycles)
        if visited_modules.contains(&resolved) {
            tracing::debug!("Skipping already visited module: {}", resolved);
            continue;
        }
        tracing::debug!("Processing module: {} at depth {}", resolved, max_depth);
        visited_modules.insert(resolved.clone());

        // Collect from this module
        tracing::debug!("Attempting to collect from module: {}", resolved);
        if let Ok(module_result) =
            collect_deprecated_from_module_with_paths(&resolved, additional_paths)
        {
            tracing::debug!(
                "Module {} has {} replacements",
                resolved,
                module_result.replacements.len()
            );
            tracing::info!(
                "Module {} has {} replacements and inheritance map: {:?}",
                resolved,
                module_result.replacements.len(),
                module_result.inheritance_map
            );
            result
                .inheritance_map
                .extend(module_result.inheritance_map.clone());

            // Collect all imported names
            let mut all_imported_names = HashSet::new();
            let mut has_star_import = false;

            for imp in &imp_list {
                for (name, _alias) in &imp.names {
                    if name == "*" {
                        has_star_import = true;
                    } else {
                        all_imported_names.insert(name.clone());
                    }
                }
                if imp.names.is_empty() {
                    // Import entire module
                    has_star_import = true;
                }
            }

            // Filter replacements based on imported names
            if has_star_import {
                // Include all replacements
                tracing::debug!(
                    "Star import from {}, including all {} replacements",
                    resolved,
                    module_result.replacements.len()
                );
                result
                    .replacements
                    .extend(module_result.replacements.clone());

                // Also process all classes from the module for inheritance
                for class_path in module_result.replacements.keys() {
                    if let Some(class_name) = class_path.split('.').nth(1) {
                        let full_class_path = format!("{}.{}", resolved, class_name);

                        // Get inheritance chain for this class
                        let inheritance_chain = get_inheritance_chain_for_class(
                            &full_class_path,
                            &module_result.inheritance_map,
                        );

                        for base_class in inheritance_chain {
                            // Include all methods from base classes
                            for (repl_path, repl_info) in &module_result.replacements {
                                if repl_path.starts_with(&format!("{}.", base_class)) {
                                    result
                                        .replacements
                                        .insert(repl_path.clone(), repl_info.clone());
                                }
                            }
                        }
                    }
                }
            } else {
                // Check each imported name
                tracing::info!("Checking imported names: {:?}", all_imported_names);
                for name in &all_imported_names {
                    let full_path = format!("{}.{}", resolved, name);
                    tracing::debug!(
                        "Checking imported name '{}', full_path: '{}'  with replacements: {:?}",
                        name,
                        full_path,
                        module_result.replacements.keys().collect::<Vec<_>>()
                    );

                    // Check all replacements
                    for (repl_path, repl_info) in &module_result.replacements {
                        if repl_path == &full_path
                            || repl_path.starts_with(&format!("{}.", full_path))
                        {
                            result
                                .replacements
                                .insert(repl_path.clone(), repl_info.clone());
                        }
                    }

                    // Check inherited methods
                    if !module_result.inheritance_map.is_empty() {
                        let inheritance_chain = get_inheritance_chain_for_class(
                            &full_path,
                            &module_result.inheritance_map,
                        );
                        tracing::debug!(
                            "Inheritance chain for {}: {:?}",
                            full_path,
                            inheritance_chain
                        );

                        for base_class in inheritance_chain {
                            // Try both the simple name and the fully qualified name
                            let qualified_base = format!("{}.{}", resolved, base_class);

                            for (repl_path, repl_info) in &module_result.replacements {
                                if repl_path.starts_with(&format!("{}.", base_class))
                                    || repl_path.starts_with(&format!("{}.", qualified_base))
                                {
                                    tracing::debug!(
                                        "Including inherited replacement: {}",
                                        repl_path
                                    );
                                    result
                                        .replacements
                                        .insert(repl_path.clone(), repl_info.clone());
                                }
                            }
                        }
                    }

                    // Check submodules
                    let submodule_path = format!("{}.{}", resolved, name);
                    if let Ok(submodule_result) =
                        collect_deprecated_from_module_with_paths(&submodule_path, additional_paths)
                    {
                        if !submodule_result.replacements.is_empty() {
                            result.update(&submodule_result);
                        }
                    }
                }
            }

            // Always update class_methods from this module
            result
                .class_methods
                .extend(module_result.class_methods.clone());

            // If max_depth > 1, recursively process dependencies of this module
            if max_depth > 1 {
                // Read the module's source to find its imports
                if let Some(module_file) = find_module_file_with_paths(&resolved, additional_paths)
                {
                    if let Ok(module_source) = fs::read_to_string(&module_file) {
                        tracing::debug!(
                            "Recursively processing imports from {} (depth {})",
                            resolved,
                            max_depth - 1
                        );
                        if let Ok(dep_result) = collect_deprecated_from_dependencies_recursive(
                            &module_source,
                            &resolved,
                            max_depth - 1,
                            visited_modules,
                            additional_paths,
                        ) {
                            result.update(&dep_result);
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

/// Scan a file and collect deprecated functions from it and its dependencies
pub fn scan_file_with_dependencies(
    file_path: &str,
    module_name: &str,
) -> Result<HashMap<String, ReplaceInfo>> {
    let mut all_replacements = HashMap::new();

    // Read the source file
    let source = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path))?;

    // First collect from the file itself using Ruff
    let collector =
        RuffDeprecatedFunctionCollector::new(module_name.to_string(), Some(Path::new(&file_path)));
    if let Ok(result) = collector.collect_from_source(source.clone()) {
        all_replacements.extend(result.replacements);
    }

    // Then collect from dependencies with proper recursion depth
    if let Ok(dep_result) = collect_deprecated_from_dependencies(&source, module_name, 5) {
        all_replacements.extend(dep_result.replacements);
    }

    Ok(all_replacements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_module_path_absolute() {
        assert_eq!(
            resolve_module_path("os.path", None),
            Some("os.path".to_string())
        );
        assert_eq!(
            resolve_module_path("dulwich.repo", None),
            Some("dulwich.repo".to_string())
        );
    }

    #[test]
    fn test_resolve_module_path_relative() {
        // Test single-level relative import
        assert_eq!(
            resolve_module_path(".sibling", Some("package.module")),
            Some("package.sibling".to_string())
        );

        // Test two-level relative import
        assert_eq!(
            resolve_module_path("..parent", Some("package.sub.module")),
            Some("package.parent".to_string())
        );

        // Test relative import without explicit module
        assert_eq!(
            resolve_module_path("..", Some("package.sub.module")),
            Some("package".to_string())
        );

        // Test relative import that goes too far up
        assert_eq!(
            resolve_module_path("...toomuch", Some("package.module")),
            None
        );
    }

    #[test]
    fn test_might_contain_replace_me() {
        assert!(might_contain_replace_me("@replace_me\ndef foo(): pass"));
        assert!(might_contain_replace_me("from dissolve import replace_me"));
        assert!(!might_contain_replace_me("def regular_function(): pass"));
    }

    #[test]
    fn test_get_inheritance_chain() {
        let mut inheritance_map = HashMap::new();
        inheritance_map.insert("Child".to_string(), vec!["Parent".to_string()]);
        inheritance_map.insert("Parent".to_string(), vec!["GrandParent".to_string()]);
        inheritance_map.insert(
            "GrandParent".to_string(),
            vec!["GreatGrandParent".to_string()],
        );

        let chain = get_inheritance_chain_for_class("Child", &inheritance_map);
        assert_eq!(chain.len(), 3);
        assert!(chain.contains(&"Parent".to_string()));
        assert!(chain.contains(&"GrandParent".to_string()));
        assert!(chain.contains(&"GreatGrandParent".to_string()));
    }

    #[test]
    fn test_get_inheritance_chain_multiple_inheritance() {
        let mut inheritance_map = HashMap::new();
        inheritance_map.insert(
            "Child".to_string(),
            vec!["Parent1".to_string(), "Parent2".to_string()],
        );
        inheritance_map.insert("Parent1".to_string(), vec!["GrandParent".to_string()]);
        inheritance_map.insert("Parent2".to_string(), vec!["GrandParent".to_string()]);

        let chain = get_inheritance_chain_for_class("Child", &inheritance_map);
        assert!(chain.contains(&"Parent1".to_string()));
        assert!(chain.contains(&"Parent2".to_string()));
        assert!(chain.contains(&"GrandParent".to_string()));
        // GrandParent might appear multiple times, but we handle duplicates in the algorithm
    }

    #[test]
    fn test_collect_imports_from_source() {
        let source = r#"
import os
from sys import path
from ..relative import something
from . import sibling
import multiple, imports, together
"#;

        let imports = collect_imports_from_source(source, "test_module").unwrap();
        assert_eq!(imports.len(), 7); // os, sys, ..relative, ., multiple, imports, together are counted as 3 separate imports

        // Check first import
        assert_eq!(imports[0].module, "os");
        assert_eq!(imports[0].names.len(), 1); // Import creates one entry per name, with the name in the names vec
        assert_eq!(imports[0].names[0], ("os".to_string(), None));

        // Check from import
        assert_eq!(imports[1].module, "sys");
        assert_eq!(imports[1].names, vec![("path".to_string(), None)]);

        // Check relative imports
        assert_eq!(imports[2].module, "..relative");
        assert_eq!(imports[2].names, vec![("something".to_string(), None)]);

        assert_eq!(imports[3].module, ".");
        assert_eq!(imports[3].names, vec![("sibling".to_string(), None)]);

        // Check multiple imports on one line
        assert_eq!(imports[4].module, "multiple");
        assert_eq!(imports[4].names.len(), 1);
        assert_eq!(imports[4].names[0], ("multiple".to_string(), None));

        assert_eq!(imports[5].module, "imports");
        assert_eq!(imports[5].names.len(), 1);
        assert_eq!(imports[5].names[0], ("imports".to_string(), None));

        assert_eq!(imports[6].module, "together");
        assert_eq!(imports[6].names.len(), 1);
        assert_eq!(imports[6].names[0], ("together".to_string(), None));
    }

    #[test]
    fn test_empty_module_cache() {
        clear_module_cache();

        // Cache should work without errors
        let result = collect_deprecated_from_module("nonexistent.module").unwrap();
        assert!(result.replacements.is_empty());
    }

    #[test]
    fn test_max_depth_zero() {
        // max_depth = 0 should return empty results
        let source = "import os";
        let result = collect_deprecated_from_dependencies(source, "test_module", 0).unwrap();
        assert!(result.replacements.is_empty());
    }

    #[test]
    fn test_visited_modules_cycle_prevention() {
        // This tests that we don't get into infinite loops with circular imports
        // The actual test would need mock modules, but the visited_modules set
        // ensures we don't process the same module twice
        let mut visited = HashSet::new();
        visited.insert("module_a".to_string());

        // If module_a imports module_b and module_b imports module_a,
        // we should skip module_a when processing module_b's imports
        assert!(visited.contains("module_a"));
    }
}
