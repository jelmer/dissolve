// Test cases for specific migration issues found in dulwich
// These tests reproduce bugs found when migrating the dulwich codebase

#[cfg(test)]
mod tests {
    use crate::core::{ConstructType, ParameterInfo, ReplaceInfo};
    use crate::migrate_ruff::migrate_file;
    use crate::tests::test_utils::TestContext;
    use crate::type_introspection_context::TypeIntrospectionContext;
    use crate::types::TypeIntrospectionMethod;
    use std::collections::HashMap;

    // Helper function to migrate source code with replacements
    fn migrate_source_with_replacements(
        source: &str,
        replacements: HashMap<String, ReplaceInfo>,
    ) -> String {
        let test_ctx = TestContext::new(source);
        let mut type_context =
            TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
        let result = migrate_file(
            source,
            "test_module",
            test_ctx.file_path.clone(),
            &mut type_context,
            replacements,
            HashMap::new(),
        )
        .unwrap();
        // Keep test_ctx alive until after migration completes
        drop(test_ctx);
        result
    }

    // Helper function to create a test module with base classes
    fn create_test_with_base_classes(test_code: &str) -> String {
        format!(
            r#"
# Test base classes
class BaseRepo:
    def do_commit(self, message, **kwargs):
        pass
    
    def stage(self, fs_paths):
        pass
    
    def get_worktree(self):
        return WorkTree()
    
    def reset_index(self, tree=None):
        pass
    
    def do_something(self, **kwargs):
        pass

class WorkTree:
    def stage(self, fs_paths):
        pass
    
    def unstage(self, fs_paths):
        pass
    
    def commit(self, message=None, **kwargs):
        pass
    
    def reset_index(self, tree=None):
        pass

class Repo(BaseRepo):
    def stage(self, fs_paths):
        pass
    
    @staticmethod
    def init(path) -> 'Repo':
        return Repo()

class Index:
    def __init__(self, path):
        pass
    
    def get_entry(self, path):
        return IndexEntry()

class IndexEntry:
    def stage(self):
        return 0

{}
"#,
            test_code
        )
    }

    // Helper function to create a simple replacement info
    fn create_replacement_info(
        old_name: &str,
        replacement_expr: &str,
        parameters: Vec<&str>,
    ) -> ReplaceInfo {
        // For test purposes, manually create AST for the common case
        // In real code, this comes from the actual function definition
        let replacement_ast = if replacement_expr == "{self}.get_worktree().reset_index({tree})" {
            // Manually create the AST for self.get_worktree().reset_index(tree)
            // This represents the structure before placeholder substitution
            use ruff_python_ast::*;

            let self_expr = Expr::Name(ExprName {
                id: "self".into(),
                ctx: ExprContext::Load,
                range: ruff_text_size::TextRange::default(),
            });

            let get_worktree_call = Expr::Call(ExprCall {
                func: Box::new(Expr::Attribute(ExprAttribute {
                    value: Box::new(self_expr),
                    attr: Identifier::new(
                        "get_worktree".to_string(),
                        ruff_text_size::TextRange::default(),
                    ),
                    ctx: ExprContext::Load,
                    range: ruff_text_size::TextRange::default(),
                })),
                arguments: Arguments {
                    args: Box::new([]),
                    keywords: Box::new([]),
                    range: ruff_text_size::TextRange::default(),
                },
                range: ruff_text_size::TextRange::default(),
            });

            let tree_param = Expr::Name(ExprName {
                id: "tree".into(),
                ctx: ExprContext::Load,
                range: ruff_text_size::TextRange::default(),
            });

            let reset_index_call = Expr::Call(ExprCall {
                func: Box::new(Expr::Attribute(ExprAttribute {
                    value: Box::new(get_worktree_call),
                    attr: Identifier::new(
                        "reset_index".to_string(),
                        ruff_text_size::TextRange::default(),
                    ),
                    ctx: ExprContext::Load,
                    range: ruff_text_size::TextRange::default(),
                })),
                arguments: Arguments {
                    args: Box::new([tree_param]),
                    keywords: Box::new([]),
                    range: ruff_text_size::TextRange::default(),
                },
                range: ruff_text_size::TextRange::default(),
            });

            Some(Box::new(reset_index_call))
        } else {
            None
        };

        ReplaceInfo {
            old_name: old_name.to_string(),
            replacement_expr: replacement_expr.to_string(),
            replacement_ast,
            construct_type: ConstructType::Function,
            parameters: parameters
                .iter()
                .map(|&name| {
                    if let Some(stripped) = name.strip_prefix("**") {
                        ParameterInfo {
                            name: stripped.to_string(), // Remove ** prefix
                            has_default: false,
                            default_value: None,
                            is_vararg: false,
                            is_kwarg: true,
                            is_kwonly: false,
                        }
                    } else if let Some(stripped) = name.strip_prefix("*") {
                        ParameterInfo {
                            name: stripped.to_string(), // Remove * prefix
                            has_default: false,
                            default_value: None,
                            is_vararg: true,
                            is_kwarg: false,
                            is_kwonly: false,
                        }
                    } else {
                        ParameterInfo {
                            name: name.to_string(),
                            has_default: false,
                            default_value: None,
                            is_vararg: false,
                            is_kwarg: false,
                            is_kwonly: false,
                        }
                    }
                })
                .collect(),
            return_type: None,
            since: None,
            remove_in: None,
            message: None,
        }
    }

    #[test]
    fn test_worktree_double_access_issue() {
        // This tests the specific issue where self.worktree is already a WorkTree object,
        // so we should NOT migrate self.worktree.stage() to
        // self.worktree.get_worktree().stage()
        let test_code = r#"
def test_worktree_operations():
    # Create a WorkTree instance
    worktree: WorkTree = WorkTree()
    
    # This should NOT be migrated - worktree is already a WorkTree object
    worktree.stage(["file.txt"])
    worktree.unstage(["file.txt"])
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.Repo.stage".to_string(),
            create_replacement_info(
                "stage",
                "{self}.get_worktree().stage({fs_paths})",
                vec!["self", "fs_paths"],
            ),
        );

        // Try with pyright which should handle self.worktree properly
        let test_ctx = TestContext::new(&source);
        let mut type_context =
            TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
        let result = migrate_file(
            &source,
            "test_module",
            test_ctx.file_path.clone(),
            &mut type_context,
            replacements,
            HashMap::new(),
        )
        .unwrap();
        drop(test_ctx);

        // The migration should NOT change worktree.stage calls
        assert!(result.contains("worktree.stage"));
        assert!(result.contains("worktree.unstage"));
        assert!(!result.contains("worktree.get_worktree().stage"));
        assert!(!result.contains("worktree.get_worktree().unstage"));
    }

    #[test]
    fn test_parameter_expansion_with_kwargs() {
        // Test that parameters are correctly expanded when some are passed as kwargs
        let test_code = r#"
repo = BaseRepo()
repo.do_commit(
    b"Initial commit",
    committer=b"Test Committer <test@nodomain.com>",
    author=b"Test Author <test@nodomain.com>",
    commit_timestamp=12345,
    commit_timezone=0,
    author_timestamp=12345,
    author_timezone=0,
)
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        let params = vec![
            ParameterInfo {
                name: "self".to_string(),
                has_default: false,
                default_value: None,
                is_vararg: false,
                is_kwarg: false,
                is_kwonly: false,
            },
            ParameterInfo {
                name: "message".to_string(),
                has_default: true,
                default_value: Some("None".to_string()),
                is_vararg: false,
                is_kwarg: false,
                is_kwonly: false,
            },
            ParameterInfo {
                name: "kwargs".to_string(),
                has_default: false,
                default_value: None,
                is_vararg: false,
                is_kwarg: true,
                is_kwonly: false,
            },
        ];

        replacements.insert(
            "test_module.BaseRepo.do_commit".to_string(),
            ReplaceInfo {
                old_name: "do_commit".to_string(),
                replacement_expr: "{self}.get_worktree().commit(message={message}, {**kwargs})"
                    .to_string(),
                replacement_ast: None,
                construct_type: ConstructType::Function,
                parameters: params,
                return_type: None,
                since: None,
                remove_in: None,
                message: None,
            },
        );

        let result = migrate_source_with_replacements(&source, replacements);

        // Should expand properly with all kwargs
        assert!(result.contains("repo.get_worktree().commit("));
        assert!(result.contains("message=b\"Initial commit\""));
        assert!(result.contains("committer=b\"Test Committer <test@nodomain.com>\""));

        // Check that the migrated call doesn't have tree= parameter
        // Extract just the migrated line
        let lines: Vec<&str> = result.lines().collect();
        let commit_line = lines
            .iter()
            .find(|line| line.contains("repo.get_worktree().commit("))
            .expect("Should find the migrated commit line");
        assert!(
            !commit_line.contains("tree="),
            "The migrated commit call should not have tree= parameter"
        );
    }

    #[test]
    fn test_default_parameter_pollution() {
        // Test that we don't add unnecessary default parameters
        let test_code = r#"
repo = BaseRepo()
repo.do_commit(b"Simple commit")
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        let params = vec![
            ParameterInfo {
                name: "self".to_string(),
                has_default: false,
                default_value: None,
                is_vararg: false,
                is_kwarg: false,
                is_kwonly: false,
            },
            ParameterInfo {
                name: "message".to_string(),
                has_default: true,
                default_value: Some("None".to_string()),
                is_vararg: false,
                is_kwarg: false,
                is_kwonly: false,
            },
            // Many more optional parameters...
            ParameterInfo {
                name: "tree".to_string(),
                has_default: true,
                default_value: Some("None".to_string()),
                is_vararg: false,
                is_kwarg: false,
                is_kwonly: false,
            },
            ParameterInfo {
                name: "encoding".to_string(),
                has_default: true,
                default_value: Some("None".to_string()),
                is_vararg: false,
                is_kwarg: false,
                is_kwonly: false,
            },
        ];

        replacements.insert(
            "test_module.BaseRepo.do_commit".to_string(),
            ReplaceInfo {
                old_name: "do_commit".to_string(),
                // The replacement expression should only include placeholders for params that will be provided
                replacement_expr: "{self}.get_worktree().commit(message={message})".to_string(),
                replacement_ast: None,
                construct_type: ConstructType::Function,
                parameters: params,
                return_type: None,
                since: None,
                remove_in: None,
                message: None,
            },
        );

        let result = migrate_source_with_replacements(&source, replacements);

        // Should only include the message parameter, not defaults
        assert!(result.contains("repo.get_worktree().commit(message=b\"Simple commit\")"));
        // Check that the migrated call doesn't have tree= or encoding= parameters
        let commit_call = "repo.get_worktree().commit(message=b\"Simple commit\")";
        assert!(result.contains(commit_call));
        assert!(!result.contains("commit(message=b\"Simple commit\", tree="));
        assert!(!result.contains("commit(message=b\"Simple commit\", encoding="));
    }

    #[test]
    fn test_incomplete_migration_stage_and_commit() {
        // Test that both stage and do_commit in the same block are migrated
        let test_code = r#"
# Inline the operations so pyright can track the type
r = Repo()
r.stage(["file.txt"])
r.do_commit("test commit")
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.Repo.stage".to_string(),
            create_replacement_info(
                "stage",
                "{self}.get_worktree().stage({fs_paths})",
                vec!["self", "fs_paths"],
            ),
        );
        replacements.insert(
            "test_module.BaseRepo.do_commit".to_string(),
            create_replacement_info(
                "do_commit",
                "{self}.get_worktree().commit(message={message})",
                vec!["self", "message"],
            ),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        // Both should be migrated
        assert!(result.contains("r.get_worktree().stage([\"file.txt\"])"));
        assert!(result.contains("r.get_worktree().commit(message=\"test commit\")"));
    }

    #[test]
    fn test_worktree_stage_calls() {
        // Test that worktree.stage() calls are NOT migrated
        let test_code = r#"
wt = WorkTree()
wt.stage(["file1.txt", "file2.txt"])
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.Repo.stage".to_string(),
            create_replacement_info(
                "stage",
                "{self}.get_worktree().stage({fs_paths})",
                vec!["self", "fs_paths"],
            ),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        // Should NOT be migrated - it's already a WorkTree
        assert!(result.contains("wt.stage([\"file1.txt\", \"file2.txt\"])"));
        assert!(!result.contains("wt.get_worktree()"));
    }

    #[test]
    fn test_unprovided_parameter_placeholders() {
        // Regression test: placeholders like {tree} should be removed when parameters aren't provided
        let test_code = r#"
repo = BaseRepo()
target = repo
target.reset_index()
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.BaseRepo.reset_index".to_string(),
            create_replacement_info(
                "reset_index",
                "{self}.get_worktree().reset_index({tree})",
                vec!["self", "tree"],
            ),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        println!("Test source:\n{}", source);
        println!("Migration result:\n{}", result);

        // Should remove the unprovided parameter placeholder
        assert!(result.contains("target.get_worktree().reset_index()"));
        assert!(!result.contains("{tree}"));
    }

    #[test]
    fn test_kwarg_pattern_detection() {
        // Test that keyword={param} patterns are correctly detected and replaced
        let test_code = r#"
def process(data, mode="fast"):
    process_v2(data, mode)
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.process_v2".to_string(),
            create_replacement_info(
                "process_v2",
                "process_v2({data}, processing_mode={mode})",
                vec!["data", "mode"],
            ),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        // Should detect and replace the keyword pattern
        assert!(result.contains("process_v2(data, processing_mode=mode)"));
    }

    #[test]
    fn test_kwargs_passthrough() {
        // Test that **kwargs are passed through correctly
        let test_code = r#"
repo = BaseRepo()
repo.do_something(a=1, b=2, c=3)
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.BaseRepo.do_something".to_string(),
            create_replacement_info(
                "do_something",
                "{self}.new_method({**kwargs})",
                vec!["self", "**kwargs"],
            ),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        assert!(result.contains("repo.new_method(a=1, b=2, c=3)"));
    }

    #[test]
    fn test_kwargs_with_dict_expansion() {
        // Test that dict expansions like **commit_kwargs are preserved
        let test_code = r#"
repo = BaseRepo()
commit_kwargs = {"author": "Test"}
repo.do_something(**commit_kwargs)
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.BaseRepo.do_something".to_string(),
            create_replacement_info(
                "do_something",
                "{self}.new_method({**kwargs})",
                vec!["self", "**kwargs"],
            ),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        // Should preserve dict expansion
        assert!(result.contains("repo.new_method(**commit_kwargs)"));
    }

    #[test]
    fn test_dict_unpacking_without_kwarg_param() {
        // Test that **dict is preserved even when function doesn't have **kwargs
        let test_code = r#"
def process_data(a, b):
    return a + b

extra_args = {"b": 2}
result = process_data(1, **extra_args)
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.process_data".to_string(),
            create_replacement_info("process_data", "new_process({a}, {b})", vec!["a", "b"]),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        // The dict expansion should be preserved
        assert!(result.contains("result = new_process(1, **extra_args)"));
    }

    #[test]
    fn test_dict_unpacking_no_extra_comma() {
        // Test that we don't add an unnecessary comma before **kwargs when it's the only argument
        let test_code = r#"
def func(**kwargs):
    pass

d = {"key": "value"}
func(**d)
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.func".to_string(),
            create_replacement_info("func", "new_func({**kwargs})", vec!["**kwargs"]),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        assert!(result.contains("new_func(**d)"));
        assert!(!result.contains("new_func(, **d)")); // No extra comma
    }

    #[test]
    fn test_method_call_on_variable_repo() {
        // Test method calls on variables holding repo objects
        let test_code = r#"
r = BaseRepo()
r.do_commit(b"Test commit", author=b"Test Author <test@example.com>")
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.BaseRepo.do_commit".to_string(),
            create_replacement_info(
                "do_commit",
                "{self}.get_worktree().commit(message={message}, {**kwargs})",
                vec!["self", "message", "**kwargs"],
            ),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        // Different variable names should still work
        assert!(result.contains("r.get_worktree().commit("));
        assert!(result.contains("message=b\"Test commit\""));
        assert!(result.contains("author=b\"Test Author <test@example.com>\""));
    }

    #[test]
    fn test_import_replacement_function() {
        // Test that function imports are updated when the function is replaced
        let test_code = r#"
# Import at module level
from test_module import checkout_branch

def test_module_import():
    # Module-qualified call should be replaced with FQN
    test_module.checkout_branch(repo, "main")
    
def test_direct_call():
    # Direct call without module prefix
    checkout_branch(repo, "feature")
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.checkout_branch".to_string(),
            create_replacement_info(
                "checkout_branch",
                "test_module.checkout({repo}, {target}, force={force})",
                vec!["repo", "target", "force"],
            ),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        // The import should remain as-is since we're using FQN for the replacement
        assert!(result.contains("from test_module import checkout_branch"));

        // Module-qualified call should be replaced with FQN
        assert!(result.contains("test_module.checkout(repo, \"main\")"));

        // Direct call should also be replaced with FQN
        assert!(result.contains("test_module.checkout(repo, \"feature\")"));
    }

    #[test]
    fn test_no_migration_without_type_info() {
        // Test that without type information, we don't migrate
        // This tests the case where we can't determine the type of 'entry'
        let source = r#"
def test_unknown_type():
    # entry type is unknown - we don't know if it's IndexEntry or something else
    stage_num = entry.stage()
"#;

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.Repo.stage".to_string(),
            create_replacement_info(
                "stage",
                "{self}.get_worktree().stage({fs_paths})",
                vec!["self", "fs_paths"],
            ),
        );

        // This should not migrate because we can't determine the type of 'entry'
        let result = migrate_source_with_replacements(source, replacements);
        assert!(result.contains("entry.stage()"));
        assert!(!result.contains("get_worktree()"));
    }

    #[test]
    fn test_method_on_known_type() {
        // Test that we DO migrate when we have type information
        let test_code = r#"
def test_repo_stage():
    repo = Repo.init(".")
    repo.stage(["file.txt"])
"#;
        let source = create_test_with_base_classes(test_code);

        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.Repo.stage".to_string(),
            create_replacement_info(
                "stage",
                "{self}.get_worktree().stage({fs_paths})",
                vec!["self", "fs_paths"],
            ),
        );

        let result = migrate_source_with_replacements(&source, replacements);

        println!("Test source:\n{}", source);
        println!("\nMigration result:\n{}", result);

        // Should be migrated because we know repo is a Repo instance
        assert!(result.contains("repo.get_worktree().stage([\"file.txt\"])"));
    }
}
