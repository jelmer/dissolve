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

use crate::core::types::{ConstructType, ParameterInfo, ReplaceInfo};
use crate::migrate_ruff::migrate_file_interactive;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::types::TypeIntrospectionMethod;
use std::collections::HashMap;

#[test]
fn test_interactive_migration_basic() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_function(x, y):
    return new_function(x, y)

def test_func():
    result = old_function(5, 10)
    return result
"#;

    // Since interactive mode isn't implemented yet, it should work like non-interactive
    let test_ctx = crate::tests::test_utils::TestContext::new(source);
    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let result = migrate_file_interactive(
        source,
        "test_module",
        test_ctx.file_path,
        &mut type_context,
        HashMap::new(), // Empty replacements since collector will find them
        HashMap::new(),
    )
    .unwrap();

    // Check that the replacement happened
    assert!(result.contains("new_function(5, 10)"));
    assert!(!result.contains("old_function(5, 10)"));
}

#[test]
fn test_interactive_with_preloaded_replacements() {
    let source = r#"
def test_func():
    result = old_function(5, 10)
    return result
"#;

    let mut replacements = HashMap::new();
    replacements.insert(
        "old_function".to_string(),
        ReplaceInfo {
            old_name: "old_function".to_string(),
            replacement_expr: "new_function({x}, {y})".to_string(),
            replacement_ast: None,
            construct_type: ConstructType::Function,
            parameters: vec![
                ParameterInfo {
                    name: "x".to_string(),
                    has_default: false,
                    default_value: None,
                    is_vararg: false,
                    is_kwarg: false,
                    is_kwonly: false,
                },
                ParameterInfo {
                    name: "y".to_string(),
                    has_default: false,
                    default_value: None,
                    is_vararg: false,
                    is_kwarg: false,
                    is_kwonly: false,
                },
            ],
            return_type: None,
            since: None,
            remove_in: None,
            message: None,
        },
    );

    let test_ctx = crate::tests::test_utils::TestContext::new(source);
    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let result = migrate_file_interactive(
        source,
        "test_module",
        test_ctx.file_path,
        &mut type_context,
        replacements,
        HashMap::new(),
    )
    .unwrap();

    assert!(result.contains("new_function(5, 10)"));
}

// TODO: Add more comprehensive interactive tests when interactive mode is fully implemented
