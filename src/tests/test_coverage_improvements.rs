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

//! Tests specifically to improve mutation test coverage.

use crate::core::{ConstructType, RuffDeprecatedFunctionCollector};

#[test]
fn test_extract_since_version_with_tuple() {
    let source = r#"
from dissolve import replace_me

@replace_me(since=(1, 2, 3))
def old_function():
    return new_function()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.old_function"));
    let replacement = &result.replacements["test_module.old_function"];
    assert_eq!(replacement.since, Some("1.2.3".to_string()));
}

#[test]
fn test_extract_since_version_with_mixed_tuple() {
    let source = r#"
from dissolve import replace_me

@replace_me(since=(1, 2, "beta"))
def old_function():
    return new_function()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.old_function"));
    let replacement = &result.replacements["test_module.old_function"];
    assert_eq!(replacement.since, Some("1.2.beta".to_string()));
}

#[test]
fn test_extract_message_from_decorator() {
    let source = r#"
from dissolve import replace_me

@replace_me(message="Use the new API instead")
def old_function():
    return new_function()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.old_function"));
    let replacement = &result.replacements["test_module.old_function"];
    assert_eq!(replacement.message, Some("Use the new API instead".to_string()));
}

#[test]
fn test_extract_remove_in_version() {
    let source = r#"
from dissolve import replace_me

@replace_me(remove_in="2.0.0")
def old_function():
    return new_function()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.old_function"));
    let replacement = &result.replacements["test_module.old_function"];
    assert_eq!(replacement.remove_in, Some("2.0.0".to_string()));
}

#[test]
fn test_nested_class_path_building() {
    let source = r#"
from dissolve import replace_me

class OuterClass:
    class InnerClass:
        @replace_me
        def old_method(self):
            return self.new_method()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.OuterClass.InnerClass.old_method"));
    let replacement = &result.replacements["test_module.OuterClass.InnerClass.old_method"];
    assert_eq!(replacement.construct_type, ConstructType::Function);
    assert_eq!(replacement.replacement_expr, "{self}.new_method()");
}

#[test]
fn test_complex_attribute_expression() {
    let source = r#"
from dissolve import replace_me
import other.module

@replace_me
def old_function():
    return other.module.submodule.helper()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.old_function"));
    let replacement = &result.replacements["test_module.old_function"];
    assert_eq!(replacement.replacement_expr, "other.module.submodule.helper()");
}

#[test]
fn test_binary_operation_in_replacement() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_calculation(x, y):
    return x * 2 + y
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.old_calculation"));
    let replacement = &result.replacements["test_module.old_calculation"];
    assert_eq!(replacement.replacement_expr, "{x} * 2 + {y}");
}

#[test]
fn test_starred_expression_handling() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_function(items):
    return new_function(*items)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.old_function"));
    let replacement = &result.replacements["test_module.old_function"];
    assert_eq!(replacement.replacement_expr, "test_module.new_function(*{items})");
}

#[test]
fn test_await_expression_handling() {
    let source = r#"
from dissolve import replace_me

@replace_me
async def old_async_function():
    return await async_helper()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.old_async_function"));
    let replacement = &result.replacements["test_module.old_async_function"];
    // await should be unwrapped from the replacement
    assert_eq!(replacement.replacement_expr, "test_module.async_helper()");
}

#[test]
fn test_class_with_complex_base_classes() {
    let source = r#"
from dissolve import replace_me
from other.module import BaseClass

class NewClass(BaseClass, other.module.MixinClass):
    pass

@replace_me
class OldClass(BaseClass):
    def __init__(self, value):
        self._obj = NewClass(value)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    // Check inheritance tracking
    assert!(result.inheritance_map.contains_key("test_module.NewClass"));
    let bases = &result.inheritance_map["test_module.NewClass"];
    // The collector might qualify imports differently, so check for the class names
    assert!(bases.iter().any(|b| b.contains("BaseClass")));
    assert!(bases.iter().any(|b| b.contains("MixinClass")));
}

#[test]
fn test_module_level_annotated_assignment() {
    let source = r#"
from dissolve import replace_me

TIMEOUT: int = replace_me(30)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.TIMEOUT"));
    let replacement = &result.replacements["test_module.TIMEOUT"];
    assert_eq!(replacement.construct_type, ConstructType::ModuleAttribute);
    assert_eq!(replacement.replacement_expr, "30");
}

#[test]
fn test_class_level_assignment() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    CONSTANT = replace_me(42)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.MyClass.CONSTANT"));
    let replacement = &result.replacements["test_module.MyClass.CONSTANT"];
    assert_eq!(replacement.construct_type, ConstructType::ClassAttribute);
    assert_eq!(replacement.replacement_expr, "42");
}

#[test]
fn test_multiline_function_call_formatting() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_function(arg1, arg2, arg3):
    return new_function(
        arg1,
        arg2,
        mode=arg3
    )
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.old_function"));
    let replacement = &result.replacements["test_module.old_function"];
    // Should preserve multiline formatting
    assert!(replacement.replacement_expr.contains('\n'));
    assert!(replacement.replacement_expr.contains("test_module.new_function"));
}