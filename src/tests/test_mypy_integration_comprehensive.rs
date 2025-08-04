use dissolve::mypy_integration::query_type_with_python;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a test file with the given content in a temporary directory
fn create_test_file(dir: &TempDir, filename: &str, content: &str) -> PathBuf {
    let file_path = dir.path().join(filename);
    fs::write(&file_path, content).unwrap();
    file_path
}

#[test]
#[ignore] // Requires Python environment
fn test_name_expr_nodes() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "name_expr_test.py",
        r#"
x: int = 42
y: str = "hello"
z = x + 10
print(y)
"#,
    );

    // Test 'x' in assignment
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "name_expr_test", 2, 1).unwrap();
    assert_eq!(result.variable, "x");
    assert_eq!(result.type_, "builtins.int");

    // Test 'y' in assignment
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "name_expr_test", 3, 1).unwrap();
    assert_eq!(result.variable, "y");
    assert_eq!(result.type_, "builtins.str");

    // Test 'z' in assignment
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "name_expr_test", 4, 1).unwrap();
    assert_eq!(result.variable, "z");
    assert_eq!(result.type_, "builtins.int");

    // Test 'y' in function call
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "name_expr_test", 5, 7).unwrap();
    assert_eq!(result.variable, "y");
    assert_eq!(result.type_, "builtins.str");
}

#[test]
#[ignore] // Requires Python environment
fn test_member_expr_nodes() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "member_expr_test.py",
        r#"
class MyClass:
    attr: int = 10
    
    def method(self) -> str:
        return "result"

obj = MyClass()
val1 = obj.attr
val2 = obj.method()
"#,
    );

    // Test 'obj' in obj.attr
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "member_expr_test", 9, 10).unwrap();
    assert_eq!(result.variable, "obj");
    assert_eq!(result.type_, "member_expr_test.MyClass");

    // Test 'obj' in obj.method()
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "member_expr_test", 10, 10).unwrap();
    assert_eq!(result.variable, "obj");
    assert_eq!(result.type_, "member_expr_test.MyClass");
}

#[test]
#[ignore] // Requires Python environment
fn test_call_expr_nodes() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "call_expr_test.py",
        r#"
def func() -> int:
    return 42

class MyClass:
    @classmethod
    def create(cls) -> "MyClass":
        return cls()
        
    def method(self, x: int) -> str:
        return str(x)

# Function call
result1 = func()

# Class instantiation
obj = MyClass()

# Class method call
obj2 = MyClass.create()

# Instance method call
result2 = obj.method(5)
"#,
    );

    // Test function call return type
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "call_expr_test", 14, 7).unwrap();
    assert_eq!(result.variable, "result1");
    assert_eq!(result.type_, "builtins.int");

    // Test class instantiation
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "call_expr_test", 17, 3).unwrap();
    assert_eq!(result.variable, "obj");
    assert_eq!(result.type_, "call_expr_test.MyClass");

    // Test class method call
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "call_expr_test", 20, 4).unwrap();
    assert_eq!(result.variable, "obj2");
    assert_eq!(result.type_, "call_expr_test.MyClass");

    // Test instance method call
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "call_expr_test", 23, 7).unwrap();
    assert_eq!(result.variable, "result2");
    assert_eq!(result.type_, "builtins.str");
}

#[test]
#[ignore] // Requires Python environment
fn test_list_and_tuple_expr() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "collection_test.py",
        r#"
from typing import List, Tuple

# List expressions
list1: List[int] = [1, 2, 3]
list2 = [x * 2 for x in list1]

# Tuple expressions
tuple1: Tuple[int, str] = (42, "hello")
tuple2 = (1, "a", True)

# Nested collections
nested: List[Tuple[str, int]] = [("a", 1), ("b", 2)]
"#,
    );

    // Test list variable
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "collection_test", 5, 5).unwrap();
    assert_eq!(result.variable, "list1");
    assert_eq!(result.type_, "builtins.list[builtins.int]");

    // Test tuple variable
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "collection_test", 9, 6).unwrap();
    assert_eq!(result.variable, "tuple1");
    assert_eq!(result.type_, "tuple[builtins.int, builtins.str]");

    // Test nested collection
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "collection_test", 13, 6).unwrap();
    assert_eq!(result.variable, "nested");
    assert_eq!(
        result.type_,
        "builtins.list[tuple[builtins.str, builtins.int]]"
    );
}

#[test]
#[ignore] // Requires Python environment
fn test_control_flow_statements() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "control_flow_test.py",
        r#"
from typing import List

items: List[str] = ["a", "b", "c"]

# If statement
if len(items) > 0:
    first = items[0]
    print(first)

# For loop
for item in items:
    print(item)
    
# While loop
i = 0
while i < len(items):
    current = items[i]
    i += 1
"#,
    );

    // Test variable in if block (first in print statement)
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "control_flow_test", 9, 15).unwrap();
    assert_eq!(result.variable, "first");
    assert_eq!(result.type_, "builtins.str");

    // Test first assignment
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "control_flow_test", 8, 9).unwrap();
    assert_eq!(result.variable, "first");
    assert_eq!(result.type_, "builtins.str");

    // Test variable in while loop
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "control_flow_test", 18, 11).unwrap();
    assert_eq!(result.variable, "current");
    assert_eq!(result.type_, "builtins.str");
}

#[test]
#[ignore] // Requires Python environment
fn test_deeply_nested_indentation() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "indentation_test.py",
        r#"
class OuterClass:
    class InnerClass:
        def deep_method(self) -> int:  # Add return type annotation
            local_var: int = 42
            if True:
                nested_var: str = "nested"
                if True:
                    deeply_nested = local_var + 10
                    return deeply_nested
            return local_var

outer = OuterClass()
inner = OuterClass.InnerClass()
result = inner.deep_method()
"#,
    );

    // Test deeply nested variable
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "indentation_test", 9, 33).unwrap();
    assert_eq!(result.variable, "deeply_nested");
    assert_eq!(result.type_, "builtins.int");

    // Test method return type
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "indentation_test", 15, 6).unwrap();
    assert_eq!(result.variable, "result");
    assert_eq!(result.type_, "builtins.int");
}

#[test]
#[ignore] // Requires Python environment
fn test_line_continuations() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "continuation_test.py",
        r#"
from typing import Dict, List

# Backslash continuation
very_long_variable_name: int = \
    42 + \
    100

# Parentheses continuation
result = (
    very_long_variable_name +
    200 +
    300
)

# Method chaining with continuation
class Builder:
    def with_x(self, x: int) -> "Builder":
        return self
    def with_y(self, y: int) -> "Builder":
        return self
    def build(self) -> Dict[str, int]:
        return {"x": 0, "y": 0}

builder_result = (
    Builder()
    .with_x(10)
    .with_y(20)
    .build()
)

# List with continuation
long_list: List[str] = [
    "first",
    "second",
    "third",
    "fourth"
]
"#,
    );

    // Test expression with parentheses continuation
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "continuation_test", 10, 6).unwrap();
    assert_eq!(result.variable, "result");
    assert_eq!(result.type_, "builtins.int");

    // Test method chaining result
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "continuation_test", 25, 14).unwrap();
    assert_eq!(result.variable, "builder_result");
    assert_eq!(result.type_, "builtins.dict[builtins.str, builtins.int]");

    // Test list with continuation
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "continuation_test", 33, 9).unwrap();
    assert_eq!(result.variable, "long_list");
    assert_eq!(result.type_, "builtins.list[builtins.str]");
}

#[test]
#[ignore] // Requires Python environment
fn test_unicode_and_special_chars() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "unicode_test.py",
        r#"
# Unicode variable names (Python 3 allows this)
π: float = 3.14159
δ: float = 0.001

# String with unicode
greeting: str = "Hello, 世界! 🌍"

# Variables with underscores
_private_var: int = 42
__very_private: str = "secret"
snake_case_var: bool = True
"#,
    );

    // Test unicode variable
    let result = query_type_with_python(test_file.to_str().unwrap(), "unicode_test", 3, 1).unwrap();
    assert_eq!(result.variable, "π");
    assert_eq!(result.type_, "builtins.float");

    // Test string with unicode content
    let result = query_type_with_python(test_file.to_str().unwrap(), "unicode_test", 7, 8).unwrap();
    assert_eq!(result.variable, "greeting");
    assert_eq!(result.type_, "builtins.str");

    // Test underscore variables
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "unicode_test", 10, 12).unwrap();
    assert_eq!(result.variable, "_private_var");
    assert_eq!(result.type_, "builtins.int");
}

#[test]
#[ignore] // Requires Python environment
fn test_empty_file() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(&temp_dir, "empty.py", "");

    let result = query_type_with_python(test_file.to_str().unwrap(), "empty", 1, 0);
    assert!(result.is_err());
}

#[test]
#[ignore] // Requires Python environment
fn test_comment_only_file() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "comments_only.py",
        r#"
# This is a comment
# Another comment
"#,
    );

    let result = query_type_with_python(test_file.to_str().unwrap(), "comments_only", 2, 5);
    assert!(result.is_err());
}

#[test]
#[ignore] // Requires Python environment
fn test_out_of_bounds_position() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "bounds_test.py",
        r#"
x = 1
y = 2
"#,
    );

    // Line beyond file
    let result = query_type_with_python(test_file.to_str().unwrap(), "bounds_test", 10, 0);
    assert!(result.is_err());

    // Column beyond line length
    let result = query_type_with_python(test_file.to_str().unwrap(), "bounds_test", 2, 50);
    assert!(result.is_err());
}

#[test]
#[ignore] // Requires Python environment
fn test_complex_type_annotations() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "complex_types.py",
        r#"
from typing import Union, Optional, Callable, TypeVar, Generic

# Union types
var1: Union[int, str] = 42
var2: Optional[str] = None

# Callable types
func_var: Callable[[int, str], bool] = lambda x, y: True

# TypeVar and Generic
T = TypeVar('T')

class Container(Generic[T]):
    def __init__(self, value: T):
        self.value = value

# Generic usage
int_container = Container(42)
str_container = Container("hello")
"#,
    );

    // Test union type
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "complex_types", 5, 4).unwrap();
    assert_eq!(result.variable, "var1");
    assert!(
        result.type_.contains("Union")
            || result.type_.contains("int")
            || result.type_.contains("str")
    );

    // Test optional type
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "complex_types", 6, 4).unwrap();
    assert_eq!(result.variable, "var2");
    assert!(
        result.type_.contains("Optional")
            || result.type_.contains("Union")
            || result.type_.contains("None")
    );

    // Test generic container
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "complex_types", 19, 13).unwrap();
    assert_eq!(result.variable, "int_container");
    assert!(result.type_.contains("Container"));
}
