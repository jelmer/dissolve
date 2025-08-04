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

//! Tests for Ruff parser replacement functionality

#[cfg(test)]
mod tests {
    use crate::ruff_parser_improved::migrate_file_with_improved_ruff;
    use crate::types::TypeIntrospectionMethod;

    #[test]
    fn test_simple_function_replacement() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x, y):
    return new_func(x * 2, y + 1)

result = old_func(5, 10)
"#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = migrate_file_with_improved_ruff(
            source,
            "test_module",
            test_ctx.file_path,
            TypeIntrospectionMethod::PyrightLsp,
        )
        .unwrap();

        // Should replace old_func(5, 10) with new_func(5 * 2, 10 + 1)
        assert!(result.contains("new_func(5 * 2, 10 + 1)"));
        assert!(!result.contains("result = old_func(5, 10)"));
    }

    #[test]
    fn test_method_replacement() {
        let source = r#"
from dissolve import replace_me

class OldClass:
    @replace_me()
    def old_method(self, x):
        return self.new_method(x * 2)

obj = OldClass()
result = obj.old_method(5)
"#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = migrate_file_with_improved_ruff(
            source,
            "test_module",
            test_ctx.file_path,
            TypeIntrospectionMethod::PyrightLsp,
        )
        .unwrap();

        // Should replace obj.old_method(5) with obj.new_method(5 * 2)
        assert!(result.contains("obj.new_method(5 * 2)"));
        assert!(!result.contains("result = obj.old_method(5)"));
    }

    #[test]
    fn test_args_kwargs_replacement() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(a, b, *args, **kwargs):
    return new_func(a + 1, b * 2, *args, **kwargs)

# Various call patterns
old_func(1, 2)
old_func(1, 2, 3, 4)
old_func(1, 2, x=3)
old_func(1, 2, 3, x=4, y=5)
"#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = migrate_file_with_improved_ruff(
            source,
            "test_module",
            test_ctx.file_path,
            TypeIntrospectionMethod::PyrightLsp,
        )
        .unwrap();

        // Check replacements
        assert!(result.contains("new_func(1 + 1, 2 * 2)"));
        assert!(result.contains("new_func(1 + 1, 2 * 2, 3, 4)"));
        assert!(result.contains("new_func(1 + 1, 2 * 2, x=3)"));
        assert!(result.contains("new_func(1 + 1, 2 * 2, 3, x=4, y=5)"));
    }

    #[test]
    fn test_preserves_formatting() {
        let source = r#"from dissolve import replace_me

@replace_me()
def old_func(x):
    """Old function docstring"""
    return new_func(x * 2)

# This is an important comment
result = old_func(5)  # Inline comment

# Another comment
print(result)
"#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = migrate_file_with_improved_ruff(
            source,
            "test_module",
            test_ctx.file_path,
            TypeIntrospectionMethod::PyrightLsp,
        )
        .unwrap();

        // Should preserve all comments and formatting
        assert!(result.contains("# This is an important comment"));
        assert!(result.contains("# Inline comment"));
        assert!(result.contains("# Another comment"));
        assert!(result.contains("\"\"\"Old function docstring\"\"\""));
    }

    #[test]
    fn test_nested_calls() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_outer(x):
    return new_outer(x)

@replace_me()
def old_inner(y):
    return new_inner(y)

# Nested call
result = old_outer(old_inner(5))
"#;

        // Apply iterative replacement for nested calls
        let mut result = source.to_string();
        loop {
            let test_ctx = crate::tests::test_utils::TestContext::new(&result);
            let migrated = migrate_file_with_improved_ruff(
                &result,
                "test_module",
                test_ctx.file_path,
                TypeIntrospectionMethod::PyrightLsp,
            )
            .unwrap();

            if migrated == result {
                break;
            }
            result = migrated;
        }

        // Should replace both nested calls
        assert!(result.contains("new_outer(new_inner(5))"));
    }

    #[test]
    fn test_class_replacement() {
        let source = r#"
from dissolve import replace_me

@replace_me()
class OldClass:
    def __init__(self, x):
        self.value = NewClass(x * 2)

obj = OldClass(5)
"#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = migrate_file_with_improved_ruff(
            source,
            "test_module",
            test_ctx.file_path,
            TypeIntrospectionMethod::PyrightLsp,
        )
        .unwrap();

        // Should replace OldClass(5) with NewClass(5 * 2)
        assert!(result.contains("NewClass(5 * 2)"));
        assert!(!result.contains("obj = OldClass(5)"));
    }
}
