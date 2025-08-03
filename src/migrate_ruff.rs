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

//! Migration functionality using Ruff parser.

use anyhow::Result;
use std::collections::HashMap;

use crate::core::{ReplaceInfo, RuffDeprecatedFunctionCollector};
use crate::ruff_parser::PythonModule;
use crate::ruff_parser_improved::ImprovedFunctionCallReplacer;
use crate::type_introspection_context::TypeIntrospectionContext;
use ruff_python_ast::{visitor::Visitor, Mod};

/// Migrate a single file using Ruff parser
pub fn migrate_file(
    source: &str,
    module_name: &str,
    file_path: String,
    type_introspection_context: &mut TypeIntrospectionContext,
    mut replacements: HashMap<String, ReplaceInfo>,
    dependency_inheritance_map: HashMap<String, Vec<String>>,
) -> Result<String> {
    // Always collect from source to get inheritance information
    let collector =
        RuffDeprecatedFunctionCollector::new(module_name.to_string(), Some(file_path.clone()));
    let collector_result = collector.collect_from_source(source.to_string())?;

    // Merge provided replacements with ones collected from the source file
    // Source file replacements take priority over dependency replacements
    for (key, value) in collector_result.replacements {
        replacements.insert(key, value);
    }

    // Parse source with Ruff
    let parsed_module = PythonModule::parse(source)?;

    // Merge inheritance maps
    let mut merged_inheritance_map = collector_result.inheritance_map;
    merged_inheritance_map.extend(dependency_inheritance_map);

    // Find and replace calls
    let mut replacer = ImprovedFunctionCallReplacer::new_with_context(
        replacements,
        &parsed_module,
        type_introspection_context,
        file_path.clone(),
        module_name.to_string(),
        std::collections::HashSet::new(), // Not used anymore
        source.to_string(),
        merged_inheritance_map,
    )?;

    // Visit the AST to find replacements
    match parsed_module.ast() {
        Mod::Module(module) => {
            for stmt in &module.body {
                replacer.visit_stmt(stmt);
            }
        }
        Mod::Expression(_) => {
            // Not handling expression mode
        }
    }

    let replacements = replacer.get_replacements();

    // Apply replacements
    tracing::debug!("Applying {} replacements", replacements.len());
    for (range, replacement) in &replacements {
        let original = &source[range.start().to_usize()..range.end().to_usize()];
        tracing::debug!("Replacing '{}' with '{}'", original, replacement);
    }
    let migrated_source = crate::ruff_parser::apply_replacements(source, replacements.clone());

    // Try to parse the migrated source to verify it's valid
    if let Err(e) = PythonModule::parse(&migrated_source) {
        tracing::error!("Generated invalid Python: {}", e);
        tracing::error!("Migrated source:\n{}", migrated_source);
    }

    // Update the file in type introspection context if changes were made
    if !replacements.is_empty() {
        type_introspection_context.update_file(&file_path, &migrated_source)?;
    }

    Ok(migrated_source)
}

/// Interactive migration using Ruff parser
pub fn migrate_file_interactive(
    source: &str,
    module_name: &str,
    file_path: String,
    type_introspection_context: &mut TypeIntrospectionContext,
    replacements: HashMap<String, ReplaceInfo>,
    dependency_inheritance_map: HashMap<String, Vec<String>>,
) -> Result<String> {
    // For now, just use non-interactive version
    // TODO: Implement interactive replacer for Ruff
    migrate_file(
        source,
        module_name,
        file_path,
        type_introspection_context,
        replacements,
        dependency_inheritance_map,
    )
}

/// Check if a file has deprecated functions
pub fn check_file(
    source: &str,
    module_name: &str,
    file_path: String,
) -> Result<crate::checker::CheckResult> {
    let collector = RuffDeprecatedFunctionCollector::new(module_name.to_string(), Some(file_path));
    let result = collector.collect_from_source(source.to_string())?;

    let mut check_result = crate::checker::CheckResult::new();

    // Add all found functions to checked_functions
    for func_name in result.replacements.keys() {
        check_result.checked_functions.push(func_name.clone());
    }

    // Also add unreplaceable functions to checked_functions
    for func_name in result.unreplaceable.keys() {
        check_result.checked_functions.push(func_name.clone());
    }

    // Add errors for unreplaceable functions
    for (func_name, unreplaceable) in result.unreplaceable {
        check_result.add_error(format!(
            "Function '{}' cannot be replaced: {:?}",
            func_name, unreplaceable.reason
        ));
    }

    Ok(check_result)
}

/// Remove @replace_me decorators
pub fn remove_decorators(
    source: &str,
    _before_version: Option<&str>,
    _module_name: &str,
) -> Result<String> {
    // For now, return unchanged
    // TODO: Implement decorator removal using Ruff
    Ok(source.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function for tests that still use the old API
    #[allow(dead_code)]
    pub fn migrate_file_with_method(
        source: &str,
        module_name: &str,
        file_path: String,
        method: crate::types::TypeIntrospectionMethod,
        replacements: HashMap<String, ReplaceInfo>,
    ) -> Result<String> {
        let mut context = TypeIntrospectionContext::new(method)?;
        migrate_file(
            source,
            module_name,
            file_path,
            &mut context,
            replacements,
            HashMap::new(),
        )
    }

    #[test]
    fn test_migrate_simple_function() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x, y):
    return new_func(x * 2, y + 1)

result = old_func(5, 10)
"#;

        let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
        let result = collector.collect_from_source(source.to_string()).unwrap();

        // Debug: print what we collected
        println!(
            "Collected replacements: {:?}",
            result.replacements.keys().collect::<Vec<_>>()
        );
        for (name, info) in &result.replacements {
            println!("  {} -> {}", name, info.replacement_expr);
        }

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let mut type_context =
            TypeIntrospectionContext::new(crate::types::TypeIntrospectionMethod::PyrightLsp)
                .unwrap();
        let migrated = migrate_file(
            source,
            "test_module",
            test_ctx.file_path,
            &mut type_context,
            result.replacements,
            HashMap::new(),
        )
        .unwrap();

        println!("Original:\n{}", source);
        println!("\nMigrated:\n{}", migrated);

        assert!(migrated.contains("new_func(5 * 2, 10 + 1)"));
        assert!(!migrated.contains("result = old_func(5, 10)"));
    }
}
