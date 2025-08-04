// Test that type introspection failures are handled gracefully

#[cfg(test)]
mod tests {
    use crate::core::{ConstructType, ParameterInfo, ReplaceInfo};
    use crate::migrate_ruff::migrate_file;
    use crate::tests::test_utils::TestContext;
    use crate::type_introspection_context::TypeIntrospectionContext;
    use crate::types::TypeIntrospectionMethod;
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn test_type_introspection_failure_logs_error() {
        // This test verifies that when type introspection fails,
        // we log an error instead of panicking

        let source = r#"
# Variable with unknown type
mystery_var = get_unknown_object()
# This should log an error but not panic
mystery_var.reset_index()
"#;

        // Create a replacement that would match if we knew the type
        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.SomeClass.reset_index".to_string(),
            ReplaceInfo {
                old_name: "reset_index".to_string(),
                replacement_expr: "{self}.new_reset_index()".to_string(),
                replacement_ast: None,
                construct_type: ConstructType::Function,
                parameters: vec![ParameterInfo {
                    name: "self".to_string(),
                    has_default: false,
                    default_value: None,
                    is_vararg: false,
                    is_kwarg: false,
                    is_kwonly: false,
                }],
                return_type: None,
                since: None,
                remove_in: None,
                message: None,
            },
        );

        let test_ctx = TestContext::new(source);
        let mut type_context =
            TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
        let result = migrate_file(
            source,
            "test_module",
            Path::new(&test_ctx.file_path),
            &mut type_context,
            replacements,
            HashMap::new(),
        );

        // Should succeed without changes (since we can't determine the type)
        assert!(result.is_ok());
        let migrated = result.unwrap();
        // The call should remain unchanged
        assert!(migrated.contains("mystery_var.reset_index()"));
        // It should NOT be migrated
        assert!(!migrated.contains("new_reset_index"));
    }

    #[test]
    fn test_successful_migration_with_type_info() {
        // This test verifies that when type introspection succeeds,
        // we do perform the migration

        let source = r#"
class SomeClass:
    def reset_index(self):
        pass

# Variable with known type
obj = SomeClass()
# This should be migrated successfully
obj.reset_index()
"#;

        // Create a replacement
        let mut replacements = HashMap::new();
        replacements.insert(
            "test_module.SomeClass.reset_index".to_string(),
            ReplaceInfo {
                old_name: "reset_index".to_string(),
                replacement_expr: "{self}.new_reset_index()".to_string(),
                replacement_ast: None,
                construct_type: ConstructType::Function,
                parameters: vec![ParameterInfo {
                    name: "self".to_string(),
                    has_default: false,
                    default_value: None,
                    is_vararg: false,
                    is_kwarg: false,
                    is_kwonly: false,
                }],
                return_type: None,
                since: None,
                remove_in: None,
                message: None,
            },
        );

        let test_ctx = TestContext::new(source);
        let mut type_context =
            TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
        let result = migrate_file(
            source,
            "test_module",
            Path::new(&test_ctx.file_path),
            &mut type_context,
            replacements,
            HashMap::new(),
        );

        // Should succeed with changes
        assert!(result.is_ok());
        let migrated = result.unwrap();
        // The call should be migrated
        assert!(!migrated.contains("obj.reset_index()"));
        assert!(migrated.contains("obj.new_reset_index()"));
    }
}
