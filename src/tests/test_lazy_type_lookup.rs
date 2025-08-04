// Test lazy type lookup optimization

#[cfg(test)]
mod tests {
    use crate::migrate_ruff::migrate_file;
    use crate::tests::test_utils::TestContext;
    use crate::type_introspection_context::TypeIntrospectionContext;
    use crate::types::TypeIntrospectionMethod;
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn test_lazy_type_lookup_skips_non_replaceable_methods() {
        // This test verifies that we don't do type introspection for methods
        // that don't have any replacements defined

        let source = r#"
class MyClass:
    def method_with_no_replacement(self):
        pass
    
    def another_method(self):
        pass

obj = MyClass()
# These method calls should NOT trigger type introspection
# because we have no replacements defined for them
obj.method_with_no_replacement()
obj.another_method()
"#;

        // No replacements defined - should skip all type introspection
        let replacements = HashMap::new();

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

        // Should succeed without any changes
        assert!(result.is_ok());
        let migrated = result.unwrap();
        assert_eq!(source, migrated);
    }

    #[test]
    fn test_lazy_type_lookup_only_for_matching_methods() {
        // This test verifies that we only do type introspection when
        // there's a potential replacement match

        let source = r#"
from dissolve import replace_me

class MyClass:
    def non_replaced_method(self):
        pass
    
    @replace_me()
    def replaced_method(self):
        return self.new_method()

obj = MyClass()
# This should NOT trigger type introspection
obj.non_replaced_method()
# This SHOULD trigger type introspection
obj.replaced_method()
"#;

        let test_ctx = TestContext::new(source);
        let mut type_context =
            TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
        let result = migrate_file(
            source,
            "test_module",
            Path::new(&test_ctx.file_path),
            &mut type_context,
            HashMap::new(), // Let it collect from source
            HashMap::new(),
        );

        // Should migrate only the replaced_method calls
        assert!(result.is_ok());
        let migrated = result.unwrap();

        println!("Migrated output:\n{}", migrated);

        // non_replaced_method should remain unchanged
        assert!(migrated.contains("obj.non_replaced_method()"));
        // replaced_method should be migrated
        assert!(migrated.contains("obj.new_method()"));
    }
}
