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
    assert_eq!(
        replacement.message,
        Some("Use the new API instead".to_string())
    );
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

    assert!(result
        .replacements
        .contains_key("test_module.OuterClass.InnerClass.old_method"));
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
    assert_eq!(
        replacement.replacement_expr,
        "other.module.submodule.helper()"
    );
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

    assert!(result
        .replacements
        .contains_key("test_module.old_calculation"));
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
    assert_eq!(
        replacement.replacement_expr,
        "test_module.new_function(*{items})"
    );
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

    assert!(result
        .replacements
        .contains_key("test_module.old_async_function"));
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

    assert!(result
        .replacements
        .contains_key("test_module.MyClass.CONSTANT"));
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
    assert!(replacement
        .replacement_expr
        .contains("test_module.new_function"));
}

// Additional error path tests for better mutation coverage

#[test]
fn test_function_with_multiple_statements_error() {
    // Test that functions with multiple statements are rejected
    let source = r#"
from dissolve import replace_me

@replace_me
def bad_function():
    x = 1
    return x + 1
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    // Should be in unreplaceable due to multiple statements
    assert!(result
        .unreplaceable
        .contains_key("test_module.bad_function"));
    let unreplaceable = &result.unreplaceable["test_module.bad_function"];
    assert_eq!(
        unreplaceable.reason,
        crate::core::ReplacementFailureReason::MultipleStatements
    );
}

#[test]
fn test_function_with_no_return_statement_error() {
    // Test that functions without return statements are rejected
    let source = r#"
from dissolve import replace_me

@replace_me
def bad_function():
    print("hello")
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    // Should be in unreplaceable due to no return statement
    assert!(result
        .unreplaceable
        .contains_key("test_module.bad_function"));
    let unreplaceable = &result.unreplaceable["test_module.bad_function"];
    assert_eq!(
        unreplaceable.reason,
        crate::core::ReplacementFailureReason::NoReturnStatement
    );
}

#[test]
fn test_function_with_empty_return_error() {
    // Test that functions with empty return statements are rejected
    let source = r#"
from dissolve import replace_me

@replace_me
def bad_function():
    return
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    // Should be in unreplaceable due to empty return
    assert!(result
        .unreplaceable
        .contains_key("test_module.bad_function"));
    let unreplaceable = &result.unreplaceable["test_module.bad_function"];
    assert_eq!(
        unreplaceable.reason,
        crate::core::ReplacementFailureReason::NoReturnStatement
    );
}

#[test]
fn test_class_with_no_init_method_error() {
    // Test that classes without __init__ methods are rejected
    let source = r#"
from dissolve import replace_me

@replace_me
class BadClass:
    def some_method(self):
        pass
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    // Should be in unreplaceable due to no __init__ method
    assert!(result.unreplaceable.contains_key("test_module.BadClass"));
    let unreplaceable = &result.unreplaceable["test_module.BadClass"];
    assert_eq!(
        unreplaceable.reason,
        crate::core::ReplacementFailureReason::NoInitMethod
    );
}

#[test]
fn test_function_with_only_pass_statements() {
    // Test that functions with only pass statements result in empty replacement
    let source = r#"
from dissolve import replace_me

@replace_me
def remove_this():
    pass
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    // Should have empty replacement expression
    assert!(result.replacements.contains_key("test_module.remove_this"));
    let replacement = &result.replacements["test_module.remove_this"];
    assert_eq!(replacement.replacement_expr, "");
}

#[test]
fn test_function_with_docstring_and_pass() {
    // Test that functions with docstring and pass statements result in empty replacement
    let source = r#"
from dissolve import replace_me

@replace_me
def remove_this():
    """This function will be removed."""
    pass
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    // Should have empty replacement expression
    assert!(result.replacements.contains_key("test_module.remove_this"));
    let replacement = &result.replacements["test_module.remove_this"];
    assert_eq!(replacement.replacement_expr, "");
}

#[test]
fn test_complex_parameter_patterns() {
    // Test functions with complex parameter patterns
    let source = r#"
from dissolve import replace_me

@replace_me
def complex_func(a, b=None, *args, c, d=42, **kwargs):
    return new_func(a, b, *args, c=c, d=d, **kwargs)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.complex_func"));
    let replacement = &result.replacements["test_module.complex_func"];

    // Check that all parameter types are properly detected
    assert_eq!(replacement.parameters.len(), 6);

    // Check parameter flags
    let param_a = replacement
        .parameters
        .iter()
        .find(|p| p.name == "a")
        .unwrap();
    assert!(!param_a.has_default && !param_a.is_vararg && !param_a.is_kwarg && !param_a.is_kwonly);

    let param_b = replacement
        .parameters
        .iter()
        .find(|p| p.name == "b")
        .unwrap();
    assert!(param_b.has_default && !param_b.is_vararg && !param_b.is_kwarg && !param_b.is_kwonly);

    let param_args = replacement
        .parameters
        .iter()
        .find(|p| p.name == "args")
        .unwrap();
    assert!(
        !param_args.has_default
            && param_args.is_vararg
            && !param_args.is_kwarg
            && !param_args.is_kwonly
    );

    let param_c = replacement
        .parameters
        .iter()
        .find(|p| p.name == "c")
        .unwrap();
    assert!(!param_c.has_default && !param_c.is_vararg && !param_c.is_kwarg && param_c.is_kwonly);

    let param_kwargs = replacement
        .parameters
        .iter()
        .find(|p| p.name == "kwargs")
        .unwrap();
    assert!(
        !param_kwargs.has_default
            && !param_kwargs.is_vararg
            && param_kwargs.is_kwarg
            && !param_kwargs.is_kwonly
    );
}

#[test]
fn test_nested_class_collection() {
    // Test more complex nested class scenarios
    let source = r#"
from dissolve import replace_me

class Outer:
    class Middle:
        class Inner:
            @replace_me
            def deep_method(self):
                return self.new_deep_method()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result
        .replacements
        .contains_key("test_module.Outer.Middle.Inner.deep_method"));
    let replacement = &result.replacements["test_module.Outer.Middle.Inner.deep_method"];
    assert_eq!(replacement.replacement_expr, "{self}.new_deep_method()");
}

#[test]
fn test_dependency_collector_edge_cases() {
    use crate::dependency_collector::{might_contain_replace_me, resolve_module_path};

    // Test edge cases in module path resolution
    assert_eq!(
        resolve_module_path(".", Some("package.module")),
        Some("package".to_string())
    );
    assert_eq!(resolve_module_path("..", Some("a")), None); // Goes too far up

    // Test edge cases in replace_me detection
    assert!(might_contain_replace_me("# @replace_me in comment"));
    assert!(might_contain_replace_me("'@replace_me' in string"));
    assert!(!might_contain_replace_me("# just a comment"));
}
