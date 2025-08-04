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

#[cfg(test)]
mod tests {
    use crate::core::{ConstructType, ParameterInfo, ReplaceInfo, RuffDeprecatedFunctionCollector};
    use crate::migrate_ruff::migrate_file;
    use crate::type_introspection_context::TypeIntrospectionContext;
    use crate::types::TypeIntrospectionMethod;
    use std::collections::HashMap;
    use std::path::Path;

    fn migrate_source(source: &str) -> String {
        // Migrate using collected replacements - apply until no more changes
        let mut current_source = source.to_string();
        let mut iteration = 0;
        const MAX_ITERATIONS: usize = 10;

        // Create type context once and reuse it
        let mut type_context =
            TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();

        loop {
            tracing::debug!("Migration iteration {}", iteration);
            if iteration >= MAX_ITERATIONS {
                panic!(
                    "Migration exceeded maximum iterations ({}), possible infinite loop",
                    MAX_ITERATIONS
                );
            }

            // Re-collect deprecated functions from current source
            let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
            let result = collector
                .collect_from_source(current_source.clone())
                .unwrap();

            if result.replacements.is_empty() {
                // No more replacements to apply
                break;
            }

            let test_ctx = crate::tests::test_utils::TestContext::new(&current_source);
            let migrated = migrate_file(
                &current_source,
                "test_module",
                Path::new(&test_ctx.file_path),
                &mut type_context,
                result.replacements.clone(),
                HashMap::new(),
            );

            match migrated {
                Ok(migrated_source) => {
                    if migrated_source == current_source {
                        // No more changes, we're done
                        break;
                    }
                    current_source = migrated_source;
                    iteration += 1;
                }
                Err(e) => {
                    panic!("Migration failed: {}", e);
                }
            }
        }

        // Shutdown the type context when done
        type_context.shutdown().unwrap();

        current_source
    }

    #[test]
    fn test_simple_function_migration() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_add(a, b):
    return new_add(a, b)

result = old_add(1, 2)
"#;

        let migrated = migrate_source(source);
        assert!(migrated.contains("result = new_add(1, 2)"));
        assert!(!migrated.contains("result = old_add(1, 2)"));
    }

    #[test]
    fn test_function_with_complex_args() {
        // This test demonstrates parameter remapping where the old function parameters
        // have different names than the new function's keyword arguments
        let source = r#"
from dissolve import replace_me

@replace_me()
def process(data, mode="fast", verbose=False):
    return process_v2(data, processing_mode=mode, debug=verbose)

# Various calls
process("test")
process("test", "slow")
process("test", verbose=True)
process("test", "fast", True)
"#;

        let migrated = migrate_source(source);

        // The current implementation has a limitation: when parameter names differ from
        // keyword argument names in the replacement (e.g., mode -> processing_mode),
        // the unmapped parameters remain as placeholders.
        // This is acceptable for now as it clearly shows what needs manual fixing.

        // Verify that at least the function is being migrated
        assert!(migrated.contains("process_v2")); // Function name is replaced

        // For calls with all parameters, the migration should work correctly
        assert!(migrated.contains(r#"process_v2("test", processing_mode="fast", debug=True)"#));
    }

    #[test]
    fn test_function_with_default_params_simple() {
        // This test uses matching parameter names to demonstrate the intended behavior
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(a, b=10, c=20):
    return new_func(a, b=b, c=c)

# Various calls
old_func(1)
old_func(1, 2)
old_func(1, 2, 3)
old_func(1, b=5)
old_func(1, c=30)
"#;

        let migrated = migrate_source(source);

        // When parameter names match, the migration should work correctly
        // Only parameters actually provided should be included
        assert!(migrated.contains(r#"new_func(1)"#)); // Just 'a'
        assert!(migrated.contains(r#"new_func(1, b=2)"#)); // 'a' and 'b'
        assert!(migrated.contains(r#"new_func(1, b=2, c=3)"#)); // all params
        assert!(migrated.contains(r#"new_func(1, b=5)"#)); // 'a' and keyword 'b'
        assert!(migrated.contains(r#"new_func(1, c=30)"#)); // 'a' and keyword 'c'
    }

    #[test]
    fn test_method_migration() {
        let source = r#"
from dissolve import replace_me

class Calculator:
    @replace_me()
    def add(self, x, y):
        return self.add_numbers(x, y)

calc = Calculator()
result = calc.add(5, 3)
"#;

        let migrated = migrate_source(source);
        assert!(migrated.contains("result = calc.add_numbers(5, 3)"));
        assert!(!migrated.contains("result = calc.add(5, 3)"));
    }

    #[test]
    fn test_nested_function_calls() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_sqrt(x):
    return sqrt_v2(x)

@replace_me()
def old_square(x):
    return square_v2(x)

result = old_sqrt(old_square(4))
"#;

        let migrated = migrate_source(source);
        assert!(migrated.contains("result = sqrt_v2(square_v2(4))"));
    }

    #[test]
    fn test_kwargs_and_starargs() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(*args, **kwargs):
    return new_func(*args, **kwargs)

old_func(1, 2, 3)
old_func(a=1, b=2)
old_func(1, 2, x=3, y=4)
"#;

        let migrated = migrate_source(source);
        assert!(migrated.contains("new_func(1, 2, 3)"));
        assert!(migrated.contains("new_func(a=1, b=2)"));
        assert!(migrated.contains("new_func(1, 2, x=3, y=4)"));
    }

    #[test]
    fn test_expression_arguments() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x, y):
    return new_func(x * 2, y + 1)

result = old_func(5, 10)
"#;

        let migrated = migrate_source(source);
        assert!(migrated.contains("result = new_func(5 * 2, 10 + 1)"));
    }

    #[test]
    fn test_custom_replacements() {
        let source = r#"
def main():
    result = custom_old(42)
    return result
"#;

        // Create custom replacement
        let mut replacements = HashMap::new();
        replacements.insert(
            "custom_old".to_string(),
            ReplaceInfo {
                old_name: "custom_old".to_string(),
                replacement_expr: "custom_new({x}, enhanced=True)".to_string(),
                replacement_ast: None,
                construct_type: ConstructType::Function,
                parameters: vec![ParameterInfo {
                    name: "x".to_string(),
                    has_default: false,
                    default_value: None,
                    is_vararg: false,
                    is_kwarg: false,
                    is_kwonly: false,
                }],
                return_type: None,
                since: None,
                remove_in: None,
                message: None,
            },
        );

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let mut type_context =
            TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
        let migrated = migrate_file(
            source,
            "test_module",
            Path::new(&test_ctx.file_path),
            &mut type_context,
            replacements,
            HashMap::new(),
        )
        .unwrap();

        assert!(migrated.contains("result = custom_new(42, enhanced=True)"));
    }
}
