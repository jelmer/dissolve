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

use crate::migrate_ruff::migrate_file;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::{RuffDeprecatedFunctionCollector, TypeIntrospectionMethod};
use std::collections::HashMap;
use std::path::Path;

#[test]
fn test_async_double_await_fix() {
    let source = r#"
from dissolve import replace_me

@replace_me()
async def old_async_func(x):
    return await new_async_func(x + 1)

result = await old_async_func(10)
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

    // The replacement should handle await properly
    assert!(migrated.contains("result = await new_async_func(10 + 1)"));
}

#[test]
fn test_async_method_double_await_fix() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    async def old_async_method(self, x):
        return await self.new_async_method(x * 2)

obj = MyClass()
result = await obj.old_async_method(10)
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

    println!("Async method migrated output:\n{}", migrated);

    // The replacement should handle await properly for methods too
    assert!(migrated.contains("result = await obj.new_async_method(10 * 2)"));
}

#[test]
fn test_args_kwargs_fixed_handling() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x, *args, **kwargs):
    return new_func(x, *args, **kwargs)

result = old_func(1, 2, 3, y=4, z=5)
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

    // Should expand arguments correctly
    assert!(migrated.contains("result = new_func(1, 2, 3, y=4, z=5)"));
}

#[test]
fn test_method_reference_vs_call_distinction() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def old_method(self, x):
        return self.new_method(x * 2)

obj = MyClass()
# This call should be replaced
result1 = obj.old_method(10)
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

    assert!(migrated.contains("result1 = obj.new_method(10 * 2)"));
}

#[test]
fn test_complex_expression_evaluation_order() {
    let source = r#"
from dissolve import replace_me

def expensive_call1():
    return 5

def expensive_call2():
    return 10

@replace_me()
def old_func(a, b):
    return new_func(a + b)

# Order of evaluation should be preserved
result = old_func(expensive_call1(), expensive_call2())
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

    // Should preserve the function call order
    assert!(migrated.contains("result = new_func(expensive_call1() + expensive_call2())"));
}

#[test]
fn test_property_setter_replacement() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @property
    @replace_me()
    def old_prop(self):
        return self._value
    
    @old_prop.setter
    def old_prop(self, value):
        self._value = value

obj = MyClass()
# Getter should be replaced
value = obj.old_prop
# Setter assignment currently gets replaced too (unexpected but documented behavior)
obj.old_prop = 42
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    println!("Collected replacements: {:?}", result.replacements);

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

    println!("Property setter migrated output:\n{}", migrated);

    // Property replacement is not currently supported - properties are accessed like attributes, not called
    // This is a known limitation
    assert!(
        migrated.contains("value = obj.old_prop"),
        "Property access is not migrated"
    );
    assert!(
        migrated.contains("obj.old_prop = 42"),
        "Property setter is not migrated"
    );
}

#[test]
fn test_nested_class_method_replacement() {
    let source = r#"
from dissolve import replace_me

class Outer:
    @replace_me()
    def old_method(self):
        return self.new_method()
    
    class Inner:
        @replace_me()
        def old_method(self):
            return self.inner_new_method()

outer = Outer()
inner = Outer.Inner()
result1 = outer.old_method()
result2 = inner.old_method()
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

    // Both methods should be replaced appropriately
    assert!(migrated.contains("inner_new_method()"));
}

#[test]
fn test_generator_expression_replacement() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Generator expressions should work
gen = (old_func(x) for x in range(3))
result = list(gen)
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

    assert!(migrated.contains("(new_func(x + 1) for x in range(3))"));
}

#[test]
fn test_exception_handling_context() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x)

try:
    result = old_func(10)
except Exception as e:
    error_result = old_func(0)
finally:
    cleanup_result = old_func(-1)
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

    assert!(migrated.contains("result = new_func(10)"));
    assert!(migrated.contains("error_result = new_func(0)"));
    assert!(migrated.contains("cleanup_result = new_func(-1)"));
}

#[test]
fn test_string_literal_no_replacement() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x)

# Function calls should be replaced
result = old_func(10)

# But string content should not
message = "Please call old_func with a value"
docstring = '''This function uses old_func internally'''
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

    assert!(migrated.contains("result = new_func(10)"));
    assert!(migrated.contains("Please call old_func with a value"));
    assert!(migrated.contains("old_func internally")); // String content unchanged
}

#[test]
fn test_parameter_name_edge_cases() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(c, cc, format, formatter):
    return new_func(c + cc, format + formatter)

# Test parameter names that are substrings
result = old_func("a", "bb", "x", "y")
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

    // Should correctly substitute without substring conflicts
    assert!(
        migrated.contains(r#"result = new_func("a" + "bb", "x" + "y")"#)
            || migrated.contains("result = new_func('a' + 'bb', 'x' + 'y')")
    );
}

#[test]
fn test_walrus_operator_edge_case() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x)

# Walrus operator in different contexts
data = [old_func(x) for x in range(3) if (result := old_func(x)) > 0]
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

    assert!(migrated.contains("new_func(x)"));
    assert!(migrated.contains("(result := new_func(x))"));
}
