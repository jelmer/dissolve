// Test modules
#[cfg(test)]
mod test_ast_edge_cases;
#[cfg(test)]
mod test_ast_edge_cases_advanced;
#[cfg(test)]
mod test_ast_edge_cases_extended;
#[cfg(test)]
mod test_attribute_deprecation;
#[cfg(test)]
mod test_bug_fixes;
#[cfg(test)]
mod test_check;
#[cfg(test)]
mod test_class_methods;
#[cfg(test)]
mod test_class_wrapper_deprecation;
#[cfg(test)]
mod test_cross_module;
#[cfg(test)]
mod test_dependency_inheritance;
#[cfg(test)]
mod test_dulwich_scenario;
#[cfg(test)]
mod test_edge_cases;
#[cfg(test)]
mod test_file_refresh;
#[cfg(test)]
mod test_formatting_preservation;
#[cfg(test)]
mod test_interactive;
#[cfg(test)]
mod test_lazy_type_lookup;
#[cfg(test)]
mod test_magic_method_edge_cases;
#[cfg(test)]
mod test_magic_method_migration;
#[cfg(test)]
mod test_magic_methods_all;
#[cfg(test)]
mod test_migrate;
#[cfg(test)]
mod test_migration_issues;
#[cfg(test)]
mod test_remove;
#[cfg(test)]
mod test_replace_me_corner_cases;
#[cfg(test)]
mod test_ruff_parser;
#[cfg(test)]
mod test_ruff_replacements;
#[cfg(test)]
mod test_type_introspection_failure;
#[cfg(test)]
mod test_coverage_improvements;

#[cfg(test)]
pub mod test_utils {
    use std::fs;
    use tempfile::TempDir;

    /// Test context that manages temporary files
    pub struct TestContext {
        _temp_dir: TempDir,
        pub file_path: String,
    }

    impl TestContext {
        /// Create a new test context with a temporary Python file
        pub fn new(content: &str) -> Self {
            Self::new_with_module_name(content, "test_module")
        }

        /// Create a new test context with a specific module name
        pub fn new_with_module_name(content: &str, module_name: &str) -> Self {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let file_path = temp_dir.path().join(format!("{}.py", module_name));

            fs::write(&file_path, content).expect("Failed to write test file");

            TestContext {
                _temp_dir: temp_dir,
                file_path: file_path.to_string_lossy().to_string(),
            }
        }
    }
}
