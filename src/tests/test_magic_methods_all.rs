// Tests for all magic method migrations

use crate::migrate_ruff::migrate_file;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::{RuffDeprecatedFunctionCollector, TypeIntrospectionMethod};
use std::collections::HashMap;

#[test]
fn test_repr_magic_method_migration() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __repr__(self):
        return self.debug_representation()

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
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // repr(obj) should be replaced with obj.debug_representation()
    assert!(migrated.contains("result = obj.debug_representation()"));
}

#[test]
fn test_bool_magic_method_migration() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __bool__(self):
        return self.is_valid()

obj = MyClass()
result = bool(obj)
if obj:  # This won't be migrated in this implementation
    pass
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

    // bool(obj) should be replaced with obj.is_valid()
    assert!(migrated.contains("result = obj.is_valid()"));
    // if obj: is not migrated in this implementation
    assert!(migrated.contains("if obj:"));
}

#[test]
fn test_int_magic_method_migration() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __int__(self):
        return self.to_integer()

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
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // int(obj) should be replaced with obj.to_integer()
    assert!(migrated.contains("result = obj.to_integer()"));
}

#[test]
fn test_float_magic_method_migration() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __float__(self):
        return float(self.get_value())

obj = MyClass()
result = float(obj)
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

    // float(obj) should be replaced with self.get_value() (unwrapped from float())
    assert!(migrated.contains("result = obj.get_value()"));
}

#[test]
fn test_bytes_magic_method_migration() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __bytes__(self):
        return self.to_bytes()

obj = MyClass()
result = bytes(obj)
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

    // bytes(obj) should be replaced with obj.to_bytes()
    assert!(migrated.contains("result = obj.to_bytes()"));
}

#[test]
fn test_hash_magic_method_migration() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __hash__(self):
        return hash(self.get_key())

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
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    println!("Migrated output:\n{}", migrated);

    // hash(obj) should be replaced with self.get_key() (unwrapped from hash())
    assert!(migrated.contains("result = obj.get_key()"));
}

#[test]
fn test_len_magic_method_migration() {
    let source = r#"
from dissolve import replace_me

class MyContainer:
    @replace_me()
    def __len__(self):
        return self.size()

container = MyContainer()
length = len(container)
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

    // len(container) should be replaced with container.size()
    assert!(migrated.contains("length = container.size()"));
}

#[test]
fn test_mixed_magic_methods() {
    // Test multiple magic methods in the same class
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def __str__(self):
        return self.display()
    
    @replace_me()
    def __repr__(self):
        return repr(self.debug_info())
    
    @replace_me()
    def __int__(self):
        return int(self.value)

obj = MyClass()
s = str(obj)
r = repr(obj)
i = int(obj)
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

    // Check all migrations
    assert!(migrated.contains("s = obj.display()"));
    assert!(migrated.contains("r = obj.debug_info()")); // unwrapped from repr()
    assert!(migrated.contains("i = obj.value")); // unwrapped from int()
}

#[test]
fn test_magic_method_without_decorator_not_migrated() {
    // Test that magic methods without @replace_me are not migrated
    let source = r#"
class MyClass:
    def __str__(self):
        return "string"
    
    def __repr__(self):
        return "repr"
    
    def __bool__(self):
        return True

obj = MyClass()
s = str(obj)
r = repr(obj)
b = bool(obj)
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
fn test_magic_method_with_complex_expressions() {
    let source = r#"
from dissolve import replace_me

class Container:
    def __init__(self):
        self.item = Item()

class Item:
    @replace_me()
    def __str__(self):
        return self.name()
    
    @replace_me()
    def __int__(self):
        return self.count()

container = Container()

# Attribute access
s = str(container.item)
i = int(container.item)

# In expressions
result = "Item: " + str(container.item)
total = 10 + int(container.item)
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

    // Check migrations
    assert!(migrated.contains("s = container.item.name()"));
    assert!(migrated.contains("i = container.item.count()"));
    assert!(migrated.contains("\"Item: \" + container.item.name()"));
    assert!(migrated.contains("10 + container.item.count()"));
}
