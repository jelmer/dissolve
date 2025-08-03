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

//! Comprehensive tests for collection functionality including edge cases and regressions.

use dissolve::core::ConstructType;

mod common;
use common::*;

// === Fully Qualified Replacement Tests ===

#[test]
fn test_function_call_replacement_is_fully_qualified() {
    let source = r#"
from dissolve import replace_me

def checkout(repo, target, force=False):
    '''New checkout function.'''
    pass

@replace_me(since="0.22.9", remove_in="0.24.0")
def checkout_branch(repo, target, force=False):
    '''Deprecated checkout function.'''
    return checkout(repo, target, force=force)
"#;

    let result = collect_replacements_with_module(source, "mymodule.porcelain");

    assert!(result
        .replacements
        .contains_key("mymodule.porcelain.checkout_branch"));
    let replacement = &result.replacements["mymodule.porcelain.checkout_branch"];
    let expected = "mymodule.porcelain.checkout({repo}, {target}, force={force})";
    assert_eq!(replacement.replacement_expr, expected);
    assert_eq!(replacement.construct_type, ConstructType::Function);
}

#[test]
fn test_self_method_calls_not_qualified() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me
    def old_method(self, x):
        return self.new_method(x)
    
    def new_method(self, x):
        return x * 2
"#;

    let result = collect_replacements(source);
    let replacement = &result.replacements["test_module.MyClass.old_method"];
    assert_eq!(replacement.replacement_expr, "{self}.new_method({x})");
}

#[test]
fn test_builtin_function_calls_not_qualified() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_len_wrapper(obj):
    return len(obj)

@replace_me
def old_str_wrapper(obj):
    return str(obj)
"#;

    let result = collect_replacements_with_module(source, "mymodule");

    let len_replacement = &result.replacements["mymodule.old_len_wrapper"];
    assert_eq!(len_replacement.replacement_expr, "len({obj})");

    let str_replacement = &result.replacements["mymodule.old_str_wrapper"];
    assert_eq!(str_replacement.replacement_expr, "str({obj})");
}

// === Parameter Substitution Tests ===

#[test]
fn test_parameter_substitution_in_replacement_expressions() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_method(repo, message):
    return new_method(repo=repo, msg=message)
"#;

    let result = collect_replacements(source);
    let replacement = &result.replacements["test_module.old_method"];
    assert_eq!(
        replacement.replacement_expr,
        "test_module.new_method(repo={repo}, msg={message})"
    );
    assert_eq!(replacement.parameters.len(), 2);
    assert_eq!(replacement.parameters[0].name, "repo");
    assert_eq!(replacement.parameters[1].name, "message");
}

#[test]
fn test_complex_parameter_patterns() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_api(x, y=10, mode="default"):
    return new_api(x, y, mode=mode, extra=True)
"#;

    let result = collect_replacements_with_module(source, "api_module");
    let replacement = &result.replacements["api_module.old_api"];
    assert_eq!(
        replacement.replacement_expr,
        "api_module.new_api({x}, {y}, mode={mode}, extra=True)"
    );
    assert_eq!(replacement.parameters.len(), 3);

    assert_eq!(replacement.parameters[0].name, "x");
    assert!(!replacement.parameters[0].has_default);

    assert_eq!(replacement.parameters[1].name, "y");
    assert!(replacement.parameters[1].has_default);

    assert_eq!(replacement.parameters[2].name, "mode");
    assert!(replacement.parameters[2].has_default);
}

#[test]
fn test_class_with_complex_init() {
    let source = r#"
from dissolve import replace_me

@replace_me
class OldClass:
    def __init__(self, value, config=None, *args, **kwargs):
        self._wrapped = NewClass(value, config, *args, **kwargs)
"#;

    let result = collect_replacements(source);
    let replacement = &result.replacements["test_module.OldClass"];
    assert_eq!(replacement.construct_type, ConstructType::Class);
    assert_eq!(
        replacement.replacement_expr,
        "NewClass({value}, {config}, *{args}, **{kwargs})"
    );

    assert_eq!(replacement.parameters.len(), 4);
    assert_eq!(replacement.parameters[0].name, "value");
    assert!(!replacement.parameters[0].has_default);

    assert_eq!(replacement.parameters[1].name, "config");
    assert!(replacement.parameters[1].has_default);

    assert_eq!(replacement.parameters[2].name, "args");
    assert!(replacement.parameters[2].is_vararg);

    assert_eq!(replacement.parameters[3].name, "kwargs");
    assert!(replacement.parameters[3].is_kwarg);
}

// === Module Path Tests ===

#[test]
fn test_nested_package_structure_detection() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_util(x):
    return new_util(x)
"#;

    let result = collect_replacements_with_module(source, "mypkg.subpkg.module");
    assert!(result
        .replacements
        .contains_key("mypkg.subpkg.module.old_util"));

    let replacement = &result.replacements["mypkg.subpkg.module.old_util"];
    assert_eq!(replacement.old_name, "mypkg.subpkg.module.old_util");
    assert_eq!(
        replacement.replacement_expr,
        "mypkg.subpkg.module.new_util({x})"
    );
}

#[test]
fn test_already_qualified_calls_preserved() {
    let source = r#"
from dissolve import replace_me
import other.module

@replace_me
def old_function(x):
    return other.module.helper(x)
"#;

    let result = collect_replacements_with_module(source, "mymodule");
    let replacement = &result.replacements["mymodule.old_function"];
    assert_eq!(replacement.replacement_expr, "other.module.helper({x})");
}

// === Edge Cases ===

#[test]
fn test_edge_case_parameter_names() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_function(param_name):
    return new_function(param_name, other_param_name)
"#;

    let result = collect_replacements(source);
    let replacement = &result.replacements["test_module.old_function"];
    // Should only substitute exact parameter names, not substrings
    assert_eq!(
        replacement.replacement_expr,
        "test_module.new_function({param_name}, other_param_name)"
    );
}

#[test]
fn test_multiple_inheritance_handling() {
    let source = r#"
from dissolve import replace_me

class A:
    pass

class B:
    pass

class C(A, B):
    @replace_me
    def old_method(self):
        return self.new_method()
    
    def new_method(self):
        return "result"
"#;

    let result = collect_replacements(source);
    assert!(result.replacements.contains_key("test_module.C.old_method"));
    let replacement = &result.replacements["test_module.C.old_method"];
    assert_eq!(replacement.replacement_expr, "{self}.new_method()");
}
