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

//! Tests for the basic collection functionality.

use dissolve::core::ConstructType;

mod common;
use common::*;

#[test]
fn test_simple_function_collection() {
    let source = simple_function_source("old_function", "new_function", "x, y");
    let result = collect_replacements(&source);

    assert_eq!(result.replacements.len(), 1);
    assert_replacement_exists(
        &result,
        "test_module.old_function",
        "new_function({x}, {y})",
        ConstructType::Function,
    );
    assert_parameter_count(&result, "test_module.old_function", 2);
}

#[test]
fn test_class_collection() {
    let source = simple_class_source("OldClass", "NewClass", "value");
    let result = collect_replacements(&source);

    assert_eq!(result.replacements.len(), 1);
    assert_replacement_exists(
        &result,
        "test_module.OldClass",
        "NewClass({value})",
        ConstructType::Class,
    );
}

#[test]
fn test_function_with_default_parameters() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_function(x, y=10):
    return new_function(x, y)
"#;

    let result = collect_replacements(source);

    assert_eq!(result.replacements.len(), 1);
    let info = &result.replacements["test_module.old_function"];
    assert_eq!(info.parameters.len(), 2);
    assert!(!info.parameters[0].has_default);
    assert!(info.parameters[1].has_default);
    assert_eq!(info.parameters[1].name, "y");
}

#[test]
fn test_function_with_varargs() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_function(x, *args, **kwargs):
    return new_function(x, *args, **kwargs)
"#;

    let result = collect_replacements(source);

    assert_eq!(result.replacements.len(), 1);
    let info = &result.replacements["test_module.old_function"];
    assert_eq!(info.parameters.len(), 3);
    assert_eq!(info.parameters[0].name, "x");
    assert_eq!(info.parameters[1].name, "args");
    assert!(info.parameters[1].is_vararg);
    assert_eq!(info.parameters[2].name, "kwargs");
    assert!(info.parameters[2].is_kwarg);
}

#[test]
fn test_method_collection() {
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me
    def old_method(self, x):
        return self.new_method(x)
"#;

    let result = collect_replacements(source);

    assert_eq!(result.replacements.len(), 1);
    assert_replacement_exists(
        &result,
        "test_module.MyClass.old_method",
        "{self}.new_method({x})",
        ConstructType::Function,
    );
}

#[test]
fn test_module_attribute_collection() {
    let source = r#"
from dissolve import replace_me

OLD_CONSTANT = replace_me("new_value")
"#;

    let result = collect_replacements(source);

    assert_eq!(result.replacements.len(), 1);
    assert_replacement_exists(
        &result,
        "test_module.OLD_CONSTANT",
        "\"new_value\"",
        ConstructType::ModuleAttribute,
    );
}

#[test]
fn test_multiple_replacements() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_function(x):
    return new_function(x)

@replace_me
class OldClass:
    def __init__(self, value):
        self._wrapped = NewClass(value)

OLD_CONSTANT = replace_me("new_value")
"#;

    let result = collect_replacements(source);

    assert_eq!(result.replacements.len(), 3);
    assert!(result.replacements.contains_key("test_module.old_function"));
    assert!(result.replacements.contains_key("test_module.OldClass"));
    assert!(result.replacements.contains_key("test_module.OLD_CONSTANT"));
}

#[test]
fn test_nested_class_collection() {
    let source = r#"
from dissolve import replace_me

class Outer:
    @replace_me
    class Inner:
        def __init__(self, value):
            self._wrapped = NewInner(value)
"#;

    let result = collect_replacements(source);

    assert_eq!(result.replacements.len(), 1);
    assert_replacement_exists(
        &result,
        "test_module.Outer.Inner",
        "NewInner({value})",
        ConstructType::Class,
    );
}

#[test]
fn test_imports_collection() {
    let source = r#"
import sys
from typing import Optional
from other_module import helper
from dissolve import replace_me

@replace_me
def old_function():
    return new_function()
"#;

    let result = collect_replacements(source);

    assert_eq!(result.imports.len(), 4);
    let import_modules: Vec<&str> = result.imports.iter().map(|i| i.module.as_str()).collect();
    assert!(import_modules.contains(&"sys"));
    assert!(import_modules.contains(&"typing"));
    assert!(import_modules.contains(&"other_module"));
    assert!(import_modules.contains(&"dissolve"));
}
