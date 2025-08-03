// Edge case tests for magic method migrations to improve mutation test coverage

use crate::migrate_ruff::migrate_file;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::{RuffDeprecatedFunctionCollector, TypeIntrospectionMethod};
use std::collections::HashMap;
use std::path::Path;

#[test]
fn test_magic_method_with_no_arguments() {
    // Test that magic method builtins with no arguments are not migrated
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __str__(self):
        return self.display()

# These should not be migrated - wrong number of arguments
result1 = str()  # No arguments
result2 = str(1, 2)  # Too many arguments
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should not migrate str() with wrong number of arguments
    assert!(migrated.contains("result1 = str()"));
    assert!(migrated.contains("result2 = str(1, 2)"));
}

#[test]
fn test_builtin_name_not_magic_method() {
    // Test builtins that are not in our magic method list
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __len__(self):
        return self.size()

obj = MyClass()
# len() is not in our supported list yet
result = len(obj)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // len() should not be migrated as it's not in our supported list
    assert!(migrated.contains("result = len(obj)"));
}

#[test]
fn test_magic_method_type_introspection_failure() {
    // Test when type introspection returns None
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __str__(self):
        return self.display()

# Variable with unknown type
unknown_obj = get_unknown_object()
result = str(unknown_obj)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should not migrate when type can't be determined
    assert!(migrated.contains("str(unknown_obj)"));
}

#[test]
fn test_magic_method_without_self_prefix_in_replacement() {
    // Test magic method replacement that doesn't start with "self"
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __repr__(self):
        return format_repr(self)

obj = MyClass()
result = repr(obj)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // Should replace repr(obj) with format_repr(obj)
    assert!(migrated.contains("result = format_repr(obj)"));
}

#[test]
fn test_builtin_wrapper_not_at_start() {
    // Test when builtin wrapper is not at the start of replacement
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __int__(self):
        return max(0, int(self.value))

obj = MyClass()
result = int(obj)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // Should replace int(obj) with max(0, int(obj.value))
    assert!(migrated.contains("result = max(0, int(obj.value))"));
}

#[test]
fn test_magic_method_with_empty_replacement() {
    // Test edge case where replacement expression might be empty or malformed
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()  
    def __bool__(self):
        return True

obj = MyClass()
result = bool(obj)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should replace bool(obj) with True
    assert!(migrated.contains("result = True"));
}

#[test]
fn test_module_prefix_already_present() {
    // Test type names that already contain module prefix
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __hash__(self):
        return self.get_id()

# Assuming type introspection returns full module path
obj = MyClass()
result = hash(obj)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should work whether type has module prefix or not
    assert!(migrated.contains("result = obj.get_id()"));
}

#[test]
fn test_len_with_complex_expressions() {
    let source = r#"
from dissolve import replace_me

class Container:
    @replace_me()
    def __len__(self):
        return self.count()

container1 = Container()
container2 = Container()

# Test len() in various contexts
size1 = len(container1)
size2 = len(container2)
total = len(container1) + len(container2)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        Path::new("test.py"),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // All len() calls should be replaced with .count()
    assert!(migrated.contains("size1 = container1.count()"));
    assert!(migrated.contains("size2 = container2.count()"));
    assert!(migrated.contains("total = container1.count() + container2.count()"));
}
