use crate::migrate_ruff::migrate_file;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::types::TypeIntrospectionMethod;
use std::collections::HashMap;

#[test]
fn test_file_refresh_after_migration() {
    let source1 = r#"
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x * 2)

# First usage
result1 = old_func(5)
"#;

    let source2 = r#"
from dissolve import replace_me

# This file imports from the first file
from test_module1 import old_func

# Second usage
result2 = old_func(10)
"#;

    // Create a type introspection context
    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightWithMypyFallback).unwrap();

    // Simulate opening both files initially
    type_context.open_file("test_module1.py", source1).unwrap();
    type_context.open_file("test_module2.py", source2).unwrap();

    // Collect replacements for the first file
    let collector = crate::core::RuffDeprecatedFunctionCollector::new(
        "test_module1".to_string(),
        Some("test_module1.py".to_string()),
    );
    let result1 = collector.collect_from_source(source1.to_string()).unwrap();

    // Migrate the first file
    let migrated1 = migrate_file(
        source1,
        "test_module1",
        "test_module1.py".to_string(),
        &mut type_context,
        result1.replacements,
        HashMap::new(),
    )
    .unwrap();

    // The first file should be updated
    assert!(migrated1.contains("new_func(5 * 2)"));
    assert!(!migrated1.contains("old_func(5)"));

    // Now migrate the second file that depends on the first
    // It should see the updated type information
    let migrated2 = migrate_file(
        source2,
        "test_module2",
        "test_module2.py".to_string(),
        &mut type_context,
        HashMap::new(), // No local replacements in file 2
        HashMap::new(),
    )
    .unwrap();

    // For now, just verify migration completes successfully
    // The actual cross-file migration would require more setup
    assert_eq!(migrated2, source2); // No changes expected without proper setup
}

#[test]
fn test_file_version_tracking() {
    let source = r#"
def example():
    return 42
"#;

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();

    // Open a file
    type_context.open_file("test.py", source).unwrap();

    // Update it multiple times
    let updated1 = "def example():\n    return 43\n";
    type_context.update_file("test.py", updated1).unwrap();

    let updated2 = "def example():\n    return 44\n";
    type_context.update_file("test.py", updated2).unwrap();

    // File versions should be tracked internally
    // (We can't directly test the version numbers, but we can verify no errors occur)
}
