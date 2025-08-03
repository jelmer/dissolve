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
    use crate::remover::remove_decorators;

    #[test]
    fn test_remove_all_decorators() {
        let source = r#"
from dissolve import replace_me

@replace_me(since="1.0.0")
def old_func(x):
    return x + 1

@replace_me(since="2.0.0")
def another_func(y):
    return y * 2

def regular_func(z):
    return z - 1
"#;

        let result = remove_decorators(source, None, true, None).unwrap();

        // Check that entire decorated functions are removed
        assert!(!result.contains("@replace_me"));
        assert!(!result.contains("def old_func(x):"));
        assert!(!result.contains("def another_func(y):"));
        assert!(result.contains("def regular_func(z):")); // This one should remain
        assert!(!result.contains("return x + 1"));
        assert!(!result.contains("return y * 2"));
        assert!(result.contains("return z - 1")); // This one should remain
    }

    #[test]
    fn test_remove_property_decorators() {
        let source = r#"
from dissolve import replace_me

class MyClass:
    @property
    @replace_me(since="1.0.0")
    def old_property(self):
        return self.new_property
        
    @property
    def new_property(self):
        return self._value
"#;

        let result = remove_decorators(source, None, true, None).unwrap();

        // Check that entire decorated function is removed
        assert!(!result.contains("@replace_me"));
        assert!(!result.contains("def old_property(self):"));
        assert!(result.contains("def new_property(self):")); // This one should remain
        assert!(result.contains("@property")); // The remaining property should still have @property
    }

    #[test]
    fn test_remove_before_version() {
        let source = r#"
from dissolve import replace_me

@replace_me(since="0.5.0")
def very_old_func(x):
    return x + 1

@replace_me(since="1.0.0")
def old_func(y):
    return y * 2

@replace_me(since="2.0.0")
def newer_func(z):
    return z - 1

def regular_func(w):
    return w / 2
"#;

        let result = remove_decorators(source, Some("1.5.0"), false, None).unwrap();

        // Check that only functions with decorators before 1.5.0 are removed
        assert!(!result.contains(r#"@replace_me(since="0.5.0")"#));
        assert!(!result.contains(r#"@replace_me(since="1.0.0")"#));
        assert!(!result.contains("def very_old_func(x):"));
        assert!(!result.contains("def old_func(y):"));

        // These should remain
        assert!(result.contains(r#"@replace_me(since="2.0.0")"#));
        assert!(result.contains("def newer_func(z):"));
        assert!(result.contains("def regular_func(w):"));
    }

    #[test]
    fn test_no_remove_criteria() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x):
    return x + 1
"#;

        // Without any removal criteria, nothing should be removed
        let result = remove_decorators(source, None, false, None).unwrap();
        assert_eq!(result, source);
    }

    #[test]
    fn test_remove_in_version() {
        let source = r#"
from dissolve import replace_me

@replace_me(since="1.0.0", remove_in="2.0.0")
def func_to_remove(x):
    return x + 1

@replace_me(since="1.0.0", remove_in="3.0.0")
def func_to_keep(y):
    return y + 1
"#;

        // With current version 2.0.0, func_to_remove should be removed
        let result = remove_decorators(source, None, false, Some("2.0.0")).unwrap();

        assert!(!result.contains("def func_to_remove(x):"));
        assert!(result.contains("def func_to_keep(y):"));
    }

    #[test]
    fn test_class_methods() {
        let source = r#"
from dissolve import replace_me

class Calculator:
    @classmethod
    @replace_me(since="1.0.0")
    def old_add(cls, x, y):
        return cls.new_add(x, y)
    
    @staticmethod
    @replace_me(since="1.0.0")
    def old_multiply(x, y):
        return x * y
    
    def regular_method(self, x):
        return x + 1
"#;

        let result = remove_decorators(source, None, true, None).unwrap();

        assert!(!result.contains("def old_add(cls, x, y):"));
        assert!(!result.contains("def old_multiply(x, y):"));
        assert!(result.contains("def regular_method(self, x):"));
    }

    #[test]
    fn test_nested_classes() {
        let source = r#"
class Outer:
    class Inner:
        @replace_me()
        def old_method(self):
            return self.new_method()
        
        def new_method(self):
            return 42
"#;

        let result = remove_decorators(source, None, true, None).unwrap();

        assert!(!result.contains("def old_method(self):"));
        assert!(result.contains("def new_method(self):"));
        assert!(result.contains("class Outer:"));
        assert!(result.contains("class Inner:"));
    }

    #[test]
    fn test_multiple_decorators() {
        let source = r#"
from dissolve import replace_me

@deprecated
@replace_me(since="1.0.0")
@another_decorator
def old_func(x):
    return x + 1

@deprecated
def other_func(y):
    return y - 1
"#;

        let result = remove_decorators(source, None, true, None).unwrap();

        // old_func should be completely removed (including all its decorators)
        assert!(!result.contains("def old_func(x):"));
        assert!(!result.contains("@another_decorator"));

        // other_func should remain with its decorator
        assert!(result.contains("def other_func(y):"));
        assert!(result.contains("@deprecated")); // The @deprecated on other_func should remain
    }
}
