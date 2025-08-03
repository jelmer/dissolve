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
fn test_basic_classmethod_replacement() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @classmethod
    @replace_me()
    def old_class_method(cls, x):
        return cls.new_class_method(x + 1)

result = MyClass.old_class_method(10)
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

    assert!(migrated.contains("result = MyClass.new_class_method(10 + 1)"));
}

#[test]
fn test_classmethod_with_inheritance() {
    let source = r#"
from dissolve import replace_me

class BaseClass:
    @classmethod
    @replace_me()
    def old_method(cls, value):
        return cls.new_method(value * 2)

class DerivedClass(BaseClass):
    pass

result = DerivedClass.old_method(5)
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

    assert!(migrated.contains("result = DerivedClass.new_method(5 * 2)"));
}

#[test]
fn test_classmethod_decorator_order() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    @classmethod
    def old_method1(cls, x):
        return cls.new_method1(x)
    
    @classmethod
    @replace_me()
    def old_method2(cls, x):
        return cls.new_method2(x)

result1 = MyClass.old_method1(5)
result2 = MyClass.old_method2(10)
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

    assert!(migrated.contains("result1 = MyClass.new_method1(5)"));
    assert!(migrated.contains("result2 = MyClass.new_method2(10)"));
}

#[test]
fn test_classmethod_with_kwargs() {
    let source = r#"
from dissolve import replace_me

class Builder:
    @classmethod
    @replace_me()
    def old_build(cls, name, **kwargs):
        return cls.new_build(name.title(), **kwargs)

result = Builder.old_build("test", debug=True, verbose=False)
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

    // The implementation correctly expands **kwargs to the actual keyword arguments
    assert!(
        migrated
            .contains(r#"result = Builder.new_build("test".title(), debug=True, verbose=False)"#)
            || migrated
                .contains("result = Builder.new_build('test'.title(), debug=True, verbose=False)")
    );
}

#[test]
fn test_classmethod_vs_staticmethod_distinction() {
    let source = r#"
from dissolve import replace_me

class Utils:
    @classmethod
    @replace_me()
    def old_class_util(cls, x):
        return cls.new_class_util(x)
    
    @staticmethod
    @replace_me()
    def old_static_util(x):
        return new_static_util(x)

result1 = Utils.old_class_util(5)
result2 = Utils.old_static_util(10)
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

    assert!(migrated.contains("result1 = Utils.new_class_util(5)"));
    assert!(migrated.contains("result2 = Utils.new_static_util(10)"));
}

#[test]
fn test_classmethod_with_async() {
    let source = r#"
from dissolve import replace_me

class AsyncClass:
    @classmethod
    @replace_me()
    async def old_async_class_method(cls, x):
        return await cls.new_async_class_method(x + 1)
    
    @classmethod
    async def new_async_class_method(cls, x):
        return x * 2

# Call the old method
result = await AsyncClass.old_async_class_method(10)
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

    // The call site should be replaced
    assert!(migrated.contains("result = await AsyncClass.new_async_class_method(10 + 1)"));
}

#[test]
fn test_classmethod_called_on_instance() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @classmethod
    @replace_me()
    def old_class_method(cls, value):
        return cls.new_class_method(value + 100)

obj = MyClass()
result = obj.old_class_method(5)  # Called on instance
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

    assert!(migrated.contains("result = obj.new_class_method(5 + 100)"));
}

#[test]
fn test_classmethod_in_comprehensions() {
    let source = r#"
from dissolve import replace_me

class Converter:
    @classmethod
    @replace_me()
    def old_convert(cls, value):
        return cls.new_convert(value * 10)

results = [Converter.old_convert(x) for x in range(3)]
gen = (Converter.old_convert(x) for x in [1, 2, 3])
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

    assert!(migrated.contains("results = [Converter.new_convert(x * 10) for x in range(3)]"));
    assert!(migrated.contains("gen = (Converter.new_convert(x * 10) for x in [1, 2, 3])"));
}

#[test]
fn test_multiple_classmethods_same_class() {
    let source = r#"
from dissolve import replace_me

class MultiClass:
    @classmethod
    @replace_me()
    def old_method_a(cls, x):
        return cls.new_method_a(x + 1)
    
    @classmethod
    @replace_me()
    def old_method_b(cls, y):
        return cls.new_method_b(y * 2)
    
    def regular_method(self):
        return "normal"

result_a = MultiClass.old_method_a(5)
result_b = MultiClass.old_method_b(10)
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

    assert!(migrated.contains("result_a = MultiClass.new_method_a(5 + 1)"));
    assert!(migrated.contains("result_b = MultiClass.new_method_b(10 * 2)"));
    // Ensure regular method is not affected
    assert!(migrated.contains("def regular_method(self):"));
}
