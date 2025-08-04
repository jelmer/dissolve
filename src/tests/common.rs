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

//! Common test utilities for dissolve integration tests.

use anyhow::Error;
use dissolve::core::{CollectorResult, RuffDeprecatedFunctionCollector};

/// Helper function to collect replacements from source code with default module name
pub fn collect_replacements(source: &str) -> CollectorResult {
    collect_replacements_with_module(source, "test_module")
}

/// Helper function to collect replacements from source code with custom module name
pub fn collect_replacements_with_module(source: &str, module_name: &str) -> CollectorResult {
    let collector = RuffDeprecatedFunctionCollector::new(module_name.to_string(), None);
    collector.collect_from_source(source.to_string()).unwrap()
}

/// Helper function that returns Result for tests that need to check error conditions
pub fn try_collect_replacements_with_module(
    source: &str,
    module_name: &str,
) -> Result<CollectorResult, Error> {
    let collector = RuffDeprecatedFunctionCollector::new(module_name.to_string(), None);
    collector.collect_from_source(source.to_string())
}

/// Assert that a replacement exists and has expected properties
pub fn assert_replacement_exists(
    result: &CollectorResult,
    key: &str,
    expected_expr: &str,
    expected_construct: dissolve::core::ConstructType,
) {
    assert!(
        result.replacements.contains_key(key),
        "Expected replacement '{}' not found",
        key
    );
    let replacement = &result.replacements[key];
    assert_eq!(replacement.replacement_expr, expected_expr);
    assert_eq!(replacement.construct_type, expected_construct);
}

/// Assert that a replacement has the expected number of parameters
pub fn assert_parameter_count(result: &CollectorResult, key: &str, expected_count: usize) {
    let replacement = &result.replacements[key];
    assert_eq!(replacement.parameters.len(), expected_count);
}

/// Create a simple test source with @replace_me decorator
pub fn simple_function_source(old_name: &str, new_name: &str, params: &str) -> String {
    format!(
        r#"from dissolve import replace_me

@replace_me
def {}({}):
    return {}({})
"#,
        old_name, params, new_name, params
    )
}

/// Create a simple class replacement source
pub fn simple_class_source(old_name: &str, new_name: &str, init_params: &str) -> String {
    format!(
        r#"from dissolve import replace_me

@replace_me
class {}:
    def __init__(self, {}):
        self._wrapped = {}({})
"#,
        old_name, init_params, new_name, init_params
    )
}
