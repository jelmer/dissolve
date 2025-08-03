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
mod test_check_replacements {
    use crate::migrate_ruff::check_file;
    use std::path::Path;

    #[test]
    fn test_valid_replacement_function() {
        let source = r#"
@replace_me()
def old_func(x, y):
    return new_func(x, y, mode="legacy")
        "#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = check_file(source, "test_module", Path::new(&test_ctx.file_path)).unwrap();
        assert!(result.success);
        assert_eq!(result.checked_functions, vec!["test_module.old_func"]);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_empty_body_function() {
        let source = r#"
@replace_me()
def old_func(x, y):
    pass
        "#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = check_file(source, "test_module", Path::new(&test_ctx.file_path)).unwrap();
        assert!(result.success);
        assert_eq!(result.checked_functions, vec!["test_module.old_func"]);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_multiple_statements() {
        let source = r#"
@replace_me()
def old_func(x, y):
    print("hello")
    return new_func(x, y)
        "#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = check_file(source, "test_module", Path::new(&test_ctx.file_path)).unwrap();
        assert!(!result.success);
        assert_eq!(result.checked_functions, vec!["test_module.old_func"]);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_invalid_replacement_no_return() {
        let source = r#"
@replace_me()
def old_func(x, y):
    new_func(x, y)
        "#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = check_file(source, "test_module", Path::new(&test_ctx.file_path)).unwrap();
        assert!(!result.success);
        assert_eq!(result.checked_functions, vec!["test_module.old_func"]);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_class_method() {
        let source = r#"
class MyClass:
    @classmethod
    @replace_me()
    def old_method(cls, x):
        return cls.new_method(x)
        "#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = check_file(source, "test_module", Path::new(&test_ctx.file_path)).unwrap();
        assert!(result.success);
        assert_eq!(
            result.checked_functions,
            vec!["test_module.MyClass.old_method"]
        );
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_multiple_functions() {
        let source = r#"
@replace_me()
def old_func1(x):
    return new_func1(x)

@replace_me()
def old_func2(y):
    return new_func2(y)

def regular_func(z):
    return z * 2
        "#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = check_file(source, "test_module", Path::new(&test_ctx.file_path)).unwrap();
        assert!(result.success);
        assert_eq!(result.checked_functions.len(), 2);
        assert!(result
            .checked_functions
            .contains(&"test_module.old_func1".to_string()));
        assert!(result
            .checked_functions
            .contains(&"test_module.old_func2".to_string()));
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_syntax_error() {
        let source = r#"
@replace_me()
def old_func(x, y
    return new_func(x, y)
        "#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = check_file(source, "test_module", Path::new(&test_ctx.file_path));
        assert!(result.is_err());
    }

    #[test]
    fn test_no_replace_me_decorators() {
        let source = r#"
def regular_func(x):
    return x * 2

def another_func(y):
    return y + 1
        "#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = check_file(source, "test_module", Path::new(&test_ctx.file_path)).unwrap();
        assert!(result.success);
        assert!(result.checked_functions.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_nested_class_method() {
        let source = r#"
class OuterClass:
    class InnerClass:
        @replace_me()
        def old_method(self):
            return self.new_method()
        "#;

        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = check_file(source, "test_module", Path::new(&test_ctx.file_path)).unwrap();
        assert!(result.success);
        assert_eq!(
            result.checked_functions,
            vec!["test_module.OuterClass.InnerClass.old_method"]
        );
        assert!(result.errors.is_empty());
    }
}
