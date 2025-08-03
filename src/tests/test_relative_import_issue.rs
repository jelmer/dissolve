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

//! Test case for relative import issue when processing external packages.

mod common;
use common::*;

#[test]
fn test_relative_import_beyond_package_root() {
    // This reproduces the issue found when processing dulwich codebase
    let source = r#"
from ..object_store import BucketBasedObjectStore
from ..pack import PACK_SPOOL_FILE_MAX_SIZE, Pack, PackData, load_pack_index_file

class GcsObjectStore(BucketBasedObjectStore):
    def __init__(self, bucket, subpath="") -> None:
        super().__init__()
        self.bucket = bucket
        self.subpath = subpath

    def some_method(self):
        return "test"
"#;

    // This should not fail when parsing relative imports
    let result = try_collect_replacements_with_module(source, "dulwich.cloud.gcs");

    // Should parse successfully even with relative imports beyond package root
    assert!(result.is_ok());
    let result = result.unwrap();

    // Should collect the relative imports
    assert_eq!(result.imports.len(), 2);
    assert_eq!(result.imports[0].module, "..object_store");
    assert_eq!(result.imports[1].module, "..pack");

    // No replacements since no @replace_me decorators
    assert_eq!(result.replacements.len(), 0);
}

#[test]
fn test_relative_import_within_package() {
    let source = r#"
from .objects import SomeClass
from .utils import helper_function

class TestClass:
    def __init__(self):
        self.obj = SomeClass()
        helper_function()
"#;

    // This should work fine
    let result = try_collect_replacements_with_module(source, "mypackage.submodule");

    assert!(result.is_ok());
    let result = result.unwrap();

    // Should collect the relative imports
    assert_eq!(result.imports.len(), 2);
    assert_eq!(result.imports[0].module, ".objects");
    assert_eq!(result.imports[1].module, ".utils");

    // No replacements since no @replace_me decorators
    assert_eq!(result.replacements.len(), 0);
}

#[test]
fn test_relative_import_with_replacement() {
    let source = r#"
from .objects import SomeClass
from dissolve import replace_me

class TestClass:
    @replace_me
    def old_method(self):
        return self.new_method()
    
    def new_method(self):
        return "new"
"#;

    // This should work and apply the replacement
    let result = try_collect_replacements_with_module(source, "mypackage.submodule");

    assert!(result.is_ok());
    let result = result.unwrap();

    // Should collect the imports including relative import
    assert_eq!(result.imports.len(), 2);
    assert_eq!(result.imports[0].module, ".objects");
    assert_eq!(result.imports[1].module, "dissolve");

    // Should have found the replacement
    assert_eq!(result.replacements.len(), 1);
    assert!(result
        .replacements
        .contains_key("mypackage.submodule.TestClass.old_method"));

    let replacement = &result.replacements["mypackage.submodule.TestClass.old_method"];
    assert_eq!(replacement.replacement_expr, "{self}.new_method()");
}

#[test]
fn test_mixed_absolute_and_relative_imports() {
    let source = r#"
import sys
from typing import Optional
from .internal import helper
from ..sibling import utility

class MyClass:
    def process(self):
        return helper() + utility()
"#;

    let result = try_collect_replacements_with_module(source, "package.subpackage.module");

    assert!(result.is_ok());
    let result = result.unwrap();

    // Should collect all imports
    assert_eq!(result.imports.len(), 4);

    // Check import types
    let import_modules: Vec<&str> = result.imports.iter().map(|i| i.module.as_str()).collect();
    assert!(import_modules.contains(&"sys"));
    assert!(import_modules.contains(&"typing"));
    assert!(import_modules.contains(&".internal"));
    assert!(import_modules.contains(&"..sibling"));
}

#[test]
fn test_relative_import_with_aliased_names() {
    let source = r#"
from .config import DEFAULT_VALUE as default
from ..utils import helper_func as helper

def process():
    return helper(default)
"#;

    let result = try_collect_replacements_with_module(source, "mypackage.submodule");

    assert!(result.is_ok());
    let result = result.unwrap();

    // Should collect the relative imports with aliases
    assert_eq!(result.imports.len(), 2);

    assert_eq!(result.imports[0].module, ".config");
    assert_eq!(
        result.imports[0].names,
        vec![("DEFAULT_VALUE".to_string(), Some("default".to_string()))]
    );

    assert_eq!(result.imports[1].module, "..utils");
    assert_eq!(
        result.imports[1].names,
        vec![("helper_func".to_string(), Some("helper".to_string()))]
    );
}
