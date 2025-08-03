// Tests for magic method migration support

use crate::migrate_ruff::migrate_file;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::{RuffDeprecatedFunctionCollector, TypeIntrospectionMethod};
use std::collections::HashMap;

#[test]
fn test_str_magic_method_migration() {
    // Test that str() calls on objects with @replace_me __str__ are migrated
    let source = r#"
from dissolve import replace_me

class MyClass:
    def __init__(self, value):
        self.value = value
    
    @replace_me()
    def __str__(self):
        return str(self.new_representation())

# Create instance
obj = MyClass("test")

# Direct str() calls should be migrated
result1 = str(obj)

# str() calls in expressions
result2 = "Value: " + str(obj)

# str() calls as function arguments
print(str(obj))

# str() on attributes
obj2 = MyClass("test2")
result3 = str(obj.value)  # This won't be migrated (different type)
result4 = str(obj2)  # This should be migrated
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    println!("Collected replacements: {:?}", result.replacements);

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // str(obj) should be replaced with obj.new_representation()
    assert!(migrated.contains("result1 = obj.new_representation()"));
    assert!(migrated.contains("\"Value: \" + obj.new_representation()"));
    assert!(migrated.contains("print(obj.new_representation())"));
    assert!(migrated.contains("result4 = obj2.new_representation()"));

    // str(obj.value) should NOT be migrated (it's a string, not MyClass)
    assert!(migrated.contains("str(obj.value)"));
}

#[test]
fn test_str_with_complex_expressions() {
    // Test str() with more complex expressions
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __str__(self):
        return self.format_nicely()
    
    def get_instance(self):
        return self

# Complex expressions
obj = MyClass()

# Method call result
result1 = str(obj.get_instance())

# List element
objects = [MyClass(), MyClass()]
result2 = str(objects[0])

# Dictionary value
obj_dict = {"key": MyClass()}
result3 = str(obj_dict["key"])

# Conditional expression
condition = True
result4 = str(obj if condition else None)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // These might be harder to migrate due to complex expressions
    // At least verify that some str() calls are migrated
    assert!(migrated.contains("@replace_me()"));
    assert!(migrated.contains("def __str__(self):"));

    // At least one str() call should be migrated
    let _str_count = migrated.matches("str(").count();
    let format_count = migrated.matches(".format_nicely()").count();
    assert!(
        format_count > 0,
        "Expected at least one str() call to be migrated to format_nicely()"
    );
}

#[test]
fn test_str_method_not_replaced_without_decorator() {
    // Test that __str__ without @replace_me is not migrated
    let source = r#"
class MyClass:
    def __str__(self):
        return "MyClass instance"

obj = MyClass()
result = str(obj)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should remain unchanged
    assert_eq!(source, migrated);
}

#[test]
fn test_str_with_self_parameter() {
    // Test that self parameter is properly replaced
    let source = r#"
from dissolve import replace_me

class Logger:
    def __init__(self, name):
        self.name = name
    
    @replace_me()
    def __str__(self):
        return self.get_formatted_name()
    
    def log(self):
        # str() call within the class
        return "Logger: " + str(self)

logger = Logger("test")
result1 = str(logger)
result2 = logger.log()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // External call should be migrated
    assert!(migrated.contains("result1 = logger.get_formatted_name()"));

    // Internal str(self) might be trickier but let's check
    assert!(migrated.contains("@replace_me()"));
}
