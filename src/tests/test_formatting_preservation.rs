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
fn test_preserves_comments() {
    let source = r#"
from dissolve import replace_me

# Module level comment
@replace_me()
def old_func(x):
    # Function comment
    return new_func(x + 1)  # Inline comment

# Before call
result = old_func(10)  # After call
# After line
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

    // All comments should be preserved
    assert!(migrated.contains("# Module level comment"));
    assert!(migrated.contains("# Function comment"));
    assert!(migrated.contains("# Inline comment"));
    assert!(migrated.contains("# Before call"));
    assert!(migrated.contains("# After call"));
    assert!(migrated.contains("# After line"));
}

#[test]
fn test_preserves_docstrings() {
    let source = r#"
"""Module docstring."""
from dissolve import replace_me

@replace_me()
def old_func(x):
    """Function docstring.
    
    Multi-line docstring
    with details.
    """
    return new_func(x + 1)

class MyClass:
    """Class docstring."""
    
    @replace_me()
    def old_method(self, x):
        """Method docstring."""
        return self.new_method(x * 2)

result = old_func(10)
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

    // All docstrings should be preserved
    assert!(migrated.contains(r#""""Module docstring.""""#));
    assert!(migrated.contains(r#""""Function docstring."#));
    assert!(migrated.contains("Multi-line docstring"));
    assert!(migrated.contains(r#""""Class docstring.""""#));
    assert!(migrated.contains(r#""""Method docstring.""""#));
}

#[test]
fn test_preserves_string_literals() {
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

    // Function call should be replaced
    assert!(migrated.contains("result = new_func(10)"));

    // String content should not be replaced
    assert!(migrated.contains("Please call old_func with a value"));
    assert!(migrated.contains("old_func internally"));
}

#[test]
fn test_preserves_type_annotations() {
    let source = r#"
from dissolve import replace_me
from typing import List, Optional, Any

@replace_me()
def old_func(x: int) -> int:
    return new_func(x + 1)

@replace_me()
def old_func_complex(
    data: List[str],
    flag: Optional[bool] = None
) -> dict[str, Any]:
    return new_func_complex(data, flag)

# With type comments (older style)
result = old_func(10)  # type: int
result2 = old_func_complex(["a", "b"])  # type: dict[str, Any]
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

    // Type annotations in function definitions should be preserved
    assert!(migrated.contains("def old_func(x: int) -> int:"));
    assert!(migrated.contains("data: List[str]"));
    assert!(migrated.contains("flag: Optional[bool] = None"));
    assert!(migrated.contains(") -> dict[str, Any]"));

    // Type comments should be preserved
    assert!(migrated.contains("# type: int"));
    assert!(migrated.contains("# type: dict[str, Any]"));
}

#[test]
fn test_preserves_decorators() {
    let source = r#"
from dissolve import replace_me
import functools

@functools.lru_cache(maxsize=128)
@replace_me()
def old_func(x):
    return new_func(x + 1)

class MyClass:
    @property
    @replace_me()
    def old_prop(self):
        return self.new_prop
    
    @staticmethod
    @replace_me()
    def old_static(x):
        return new_static(x)

result = old_func(10)
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

    // All decorators should be preserved
    assert!(migrated.contains("@functools.lru_cache(maxsize=128)"));
    assert!(migrated.contains("@property"));
    assert!(migrated.contains("@staticmethod"));
}

#[test]
fn test_preserves_special_comments() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x):
    # TODO: This is important
    # NOTE: Another note
    # FIXME: Something to fix
    return new_func(x + 1)  # type: ignore

# Call the function
result = old_func(10)  # noqa: E501
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

    // Special comments should be preserved
    assert!(migrated.contains("# TODO: This is important"));
    assert!(migrated.contains("# NOTE: Another note"));
    assert!(migrated.contains("# FIXME: Something to fix"));
    assert!(migrated.contains("# type: ignore"));
    assert!(migrated.contains("# noqa: E501"));
}

#[test]
fn test_preserves_shebang_and_encoding() {
    let source = r#"#!/usr/bin/env python3
# -*- coding: utf-8 -*-
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

if __name__ == "__main__":
    result = old_func(10)
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

    // Shebang and encoding should be preserved
    assert!(migrated.starts_with("#!/usr/bin/env python3"));
    assert!(migrated.contains("# -*- coding: utf-8 -*-"));
    assert!(migrated.contains(r#"if __name__ == "__main__":"#));
}

#[test]
fn test_preserves_nested_structures() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Nested function calls in various structures
data = {
    "key1": old_func(1),  # In dict
    "key2": [old_func(2), old_func(3)],  # In list
}

# In comprehensions
list_comp = [old_func(i) for i in range(3)]
dict_comp = {i: old_func(i) for i in range(2)}

# In lambda
f = lambda x: old_func(x)
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

    // Verify replacements in various contexts
    assert!(migrated.contains("new_func(1 + 1)")); // In dict
    assert!(migrated.contains("new_func(2 + 1)")); // In list
    assert!(migrated.contains("new_func(3 + 1)")); // In list
    assert!(migrated.contains("new_func(i + 1) for i in range(3)")); // In list comp
    assert!(migrated.contains("new_func(i + 1) for i in range(2)")); // In dict comp
    assert!(migrated.contains("lambda x: new_func(x + 1)")); // In lambda

    // Comments should be preserved
    assert!(migrated.contains("# In dict"));
    assert!(migrated.contains("# In list"));
    assert!(migrated.contains("# In comprehensions"));
    assert!(migrated.contains("# In lambda"));
}

#[test]
fn test_preserves_import_organization() {
    let source = r#"
# Standard library imports
import os
import sys

# Third-party imports
from dissolve import replace_me

# Local imports
from .utils import helper  # noqa

@replace_me()
def old_func(x):
    return new_func(x + 1)

result = old_func(10)
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

    // Import comments should be preserved
    assert!(migrated.contains("# Standard library imports"));
    assert!(migrated.contains("# Third-party imports"));
    assert!(migrated.contains("# Local imports"));
    assert!(migrated.contains("# noqa"));
}
