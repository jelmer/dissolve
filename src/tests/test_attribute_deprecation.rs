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

use crate::{ConstructType, RuffDeprecatedFunctionCollector};

#[test]
fn test_replace_me_call_pattern() {
    let source = r#"
from dissolve import replace_me

OLD_CONSTANT = replace_me(42)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.OLD_CONSTANT"));
    let info = &result.replacements["test_module.OLD_CONSTANT"];
    assert_eq!(info.old_name, "test_module.OLD_CONSTANT");
    assert_eq!(info.replacement_expr, "42");
    assert_eq!(info.construct_type, ConstructType::ModuleAttribute);
}

#[test]
fn test_replace_me_call_with_string() {
    let source = r#"
OLD_URL = replace_me("https://new.example.com")
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.OLD_URL"));
    let info = &result.replacements["test_module.OLD_URL"];
    assert_eq!(info.replacement_expr, r#""https://new.example.com""#);
}

#[test]
fn test_replace_me_call_in_class() {
    let source = r#"
class Settings:
    OLD_TIMEOUT = replace_me(30)
    OLD_DEBUG = replace_me(True)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result
        .replacements
        .contains_key("test_module.Settings.OLD_TIMEOUT"));
    assert_eq!(
        result.replacements["test_module.Settings.OLD_TIMEOUT"].replacement_expr,
        "30"
    );
    assert_eq!(
        result.replacements["test_module.Settings.OLD_TIMEOUT"].construct_type,
        ConstructType::ClassAttribute
    );

    assert!(result
        .replacements
        .contains_key("test_module.Settings.OLD_DEBUG"));
    assert_eq!(
        result.replacements["test_module.Settings.OLD_DEBUG"].replacement_expr,
        "True"
    );
}

#[test]
fn test_replace_me_with_complex_value() {
    let source = r#"
from dissolve import replace_me

OLD_CONFIG = replace_me({"timeout": 30, "retries": 3})
OLD_CALC = replace_me(2 * 3 + 1)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.OLD_CONFIG"));
    assert_eq!(
        result.replacements["test_module.OLD_CONFIG"].replacement_expr,
        r#"{"timeout": 30, "retries": 3}"#
    );

    assert!(result.replacements.contains_key("test_module.OLD_CALC"));
    assert_eq!(
        result.replacements["test_module.OLD_CALC"].replacement_expr,
        "2 * 3 + 1"
    );
}

#[test]
fn test_annotated_replace_me_call() {
    let source = r#"
DEFAULT_TIMEOUT: int = replace_me(30)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result
        .replacements
        .contains_key("test_module.DEFAULT_TIMEOUT"));
    assert_eq!(
        result.replacements["test_module.DEFAULT_TIMEOUT"].replacement_expr,
        "30"
    );
}

#[test]
fn test_no_args_to_replace_me() {
    let source = r#"
# This should not be collected as an attribute
SOMETHING = replace_me()
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(!result.replacements.contains_key("test_module.SOMETHING"));
}

#[test]
fn test_multiple_args_to_replace_me() {
    let source = r#"
# Only the first argument should be used
OLD_VAL = replace_me(42, since="1.0")
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    assert!(result.replacements.contains_key("test_module.OLD_VAL"));
    assert_eq!(
        result.replacements["test_module.OLD_VAL"].replacement_expr,
        "42"
    );
}
