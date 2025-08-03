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

// ========== Position Edge Cases ==========

#[test]
#[ignore] // Requires Python environment
fn test_column_at_line_boundaries() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "boundary_test.py",
        r#"x = 1  # Column 0 is 'x', column 1 is ' ', column 2 is '='
long_variable_name = "test"
"#,
    );

    // Test at column 1 (end of 'x')
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "boundary_test", 1, 1).unwrap();
    assert_eq!(result.variable, "x");
    assert_eq!(result.type_, "builtins.int");

    // Test at end of variable name
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "boundary_test", 2, 18).unwrap();
    assert_eq!(result.variable, "long_variable_name");
    assert_eq!(result.type_, "builtins.str");
}

#[test]
#[ignore] // Requires Python environment
fn test_multiline_expressions() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "multiline_test.py",
        r#"
# Parentheses continuation
result = (
    1 +
    2 +
    3
)

# String continuation
text = '''This is a
multiline
string'''

# List with complex formatting
data = [
    {"key": "value1"},
    {"key": "value2"},
    {
        "nested": {
            "deep": "value"
        }
    }
]
"#,
    );

    // Test variable split across lines
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "multiline_test", 3, 6).unwrap();
    assert_eq!(result.variable, "result");
    assert_eq!(result.type_, "builtins.int");

    // Test multiline string
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "multiline_test", 10, 4).unwrap();
    assert_eq!(result.variable, "text");
    assert_eq!(result.type_, "builtins.str");

    // Test complex nested structure
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "multiline_test", 15, 4).unwrap();
    assert_eq!(result.variable, "data");
    assert!(result.type_.contains("list"));
}

#[test]
#[ignore] // Requires Python environment
fn test_position_in_comments_and_strings() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "comment_string_test.py",
        r#"
# This is x in a comment
x = 42
text = "x is in this string"
multiline = '''x appears
in this multiline
string too'''
"#,
    );

    // Position in actual code should work
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "comment_string_test", 3, 1).unwrap();
    assert_eq!(result.variable, "x");
    assert_eq!(result.type_, "builtins.int");

    // Position in comment line should fail
    let result = query_type_with_python(test_file.to_str().unwrap(), "comment_string_test", 2, 10);
    assert!(result.is_err());
}

// ========== Module and Import Edge Cases ==========

#[test]
#[ignore] // Requires Python environment
fn test_circular_imports() {
    let temp_dir = TempDir::new().unwrap();

    // Create module A
    let _mod_a = create_test_file(
        &temp_dir,
        "mod_a.py",
        r#"
from mod_b import ClassB

class ClassA:
    def use_b(self, b: ClassB) -> None:
        pass
        
a_instance = ClassA()
"#,
    );

    // Create module B with circular import
    let _mod_b = create_test_file(
        &temp_dir,
        "mod_b.py",
        r#"
from mod_a import ClassA

class ClassB:
    def use_a(self, a: ClassA) -> None:
        pass
        
b_instance = ClassB()
"#,
    );

    // This might fail or return Any due to circular import issues
    let result = query_type_with_python(_mod_a.to_str().unwrap(), "mod_a", 8, 10);

    // Either succeeds with proper type or fails due to circular import
    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "a_instance");
            // Type might be Any or mod_a.ClassA
        }
        Err(_) => {
            // Expected for circular imports
        }
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_missing_imports() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "missing_import_test.py",
        r#"
from nonexistent_module import Something  # This module doesn't exist

def use_something(s: Something) -> None:
    pass
    
# This will likely be Any
var = Something()
"#,
    );

    // Should handle gracefully even with import errors
    let result = query_type_with_python(test_file.to_str().unwrap(), "missing_import_test", 8, 3);

    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "var");
            // Type will likely be Any due to import failure
            assert!(
                query_result.type_ == "Any" || query_result.type_ == "nonexistent_module.Something"
            );
        }
        Err(_) => {
            // Also acceptable - import errors might prevent analysis
        }
    }
}

// ========== Type Inference Edge Cases ==========

#[test]
#[ignore] // Requires Python environment
fn test_recursive_types() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "recursive_test.py",
        r#"
from typing import Optional

class Node:
    def __init__(self, value: int, next: Optional["Node"] = None):
        self.value = value
        self.next = next
        
# Create a linked list
node1 = Node(1)
node2 = Node(2, node1)
node3 = Node(3, node2)

# Access nested
current = node3
while current:
    current = current.next
"#,
    );

    // Test recursive type reference
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "recursive_test", 10, 5).unwrap();
    assert_eq!(result.variable, "node1");
    assert_eq!(result.type_, "recursive_test.Node");

    // Test accessing recursive field
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "recursive_test", 15, 7).unwrap();
    assert_eq!(result.variable, "current");
    assert!(result.type_.contains("Node"));
}

#[test]
#[ignore] // Requires Python environment
fn test_type_aliases_and_newtype() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "type_alias_test.py",
        r#"
from typing import NewType, List, Dict, Union

# Type alias
UserId = NewType('UserId', int)
DataDict = Dict[str, Union[int, str, List[int]]]

# Using type aliases
user_id: UserId = UserId(42)
data: DataDict = {"numbers": [1, 2, 3], "name": "test"}

# Function using type alias
def process_user(uid: UserId) -> DataDict:
    return {"user_id": uid}
    
result = process_user(user_id)
"#,
    );

    // Test NewType variable
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "type_alias_test", 9, 7).unwrap();
    assert_eq!(result.variable, "user_id");
    assert_eq!(result.type_, "type_alias_test.UserId");

    // Test complex type alias
    let result =
        query_type_with_python(test_file.to_str().unwrap(), "type_alias_test", 10, 4).unwrap();
    assert_eq!(result.variable, "data");
    assert_eq!(
        result.type_,
        "builtins.dict[builtins.str, Union[builtins.int, builtins.str, builtins.list[builtins.int]]]"
    );
}

// ========== Error Handling ==========

#[test]
#[ignore] // Requires Python environment
fn test_malformed_python_syntax() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "syntax_error.py",
        r#"
def broken_function(
    x = 1  # Missing closing parenthesis
    y = 2
"#,
    );

    // Should raise an error for syntax issues
    let result = query_type_with_python(test_file.to_str().unwrap(), "syntax_error", 3, 5);
    assert!(result.is_err());
}

#[test]
#[ignore] // Requires Python environment
fn test_incomplete_code() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "incomplete.py",
        r#"
class MyClass:
    def method(self):
        # Incomplete - missing return
        x = 42
        # Function ends abruptly
"#,
    );

    // Should still work for complete parts
    let result = query_type_with_python(test_file.to_str().unwrap(), "incomplete", 5, 9).unwrap();
    assert_eq!(result.variable, "x");
    assert_eq!(result.type_, "builtins.int");
}

#[test]
#[ignore] // Requires Python environment
fn test_extreme_nesting() {
    let temp_dir = TempDir::new().unwrap();

    // Generate deeply nested code
    let nesting_levels = 10;
    let indent = "    ";
    let mut code_lines = vec!["def outer():".to_string()];

    for i in 0..nesting_levels {
        code_lines.push(format!("{}def level{}():", indent.repeat(i + 1), i));
    }

    code_lines.push(format!("{}x = 42", indent.repeat(nesting_levels + 1)));
    code_lines.push(format!("{}return x", indent.repeat(nesting_levels + 1)));

    // Close all functions
    for i in (0..=nesting_levels).rev() {
        if i > 0 {
            code_lines.push(format!("{}return level{}", indent.repeat(i + 1), i));
        } else {
            code_lines.push(format!("{}return level0", indent));
        }
    }

    let test_file = create_test_file(&temp_dir, "deep_nesting.py", &code_lines.join("\n"));

    // Test finding deeply nested variable
    let line_num = nesting_levels + 2; // Account for function defs
    let col = (indent.len() * (nesting_levels + 1) + 1) as i32;

    let result = query_type_with_python(
        test_file.to_str().unwrap(),
        "deep_nesting",
        line_num as i32,
        col,
    );

    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "x");
            assert_eq!(query_result.type_, "builtins.int");
        }
        Err(_) => {
            // Extreme nesting might cause issues
        }
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_nonexistent_file() {
    let result = query_type_with_python("/path/that/does/not/exist.py", "module", 1, 0);
    assert!(result.is_err());
}

#[test]
#[ignore] // Requires Python environment
fn test_binary_file() {
    let temp_dir = TempDir::new().unwrap();
    let binary_file = temp_dir.path().join("binary.pyc");
    fs::write(&binary_file, b"\x00\x01\x02\x03\x04\x05").unwrap();

    // Should fail gracefully
    let result = query_type_with_python(binary_file.to_str().unwrap(), "binary", 1, 0);
    assert!(result.is_err());
}

#[test]
#[ignore] // Requires Python environment
fn test_very_long_lines() {
    let temp_dir = TempDir::new().unwrap();

    // Create a line longer than typical buffer sizes
    let long_var_name = "x".repeat(1000);
    let content = format!(
        r#"
{} = 42
short = {} + 1
"#,
        long_var_name, long_var_name
    );

    let test_file = create_test_file(&temp_dir, "long_lines.py", &content);

    // Should handle long variable names
    let result = query_type_with_python(test_file.to_str().unwrap(), "long_lines", 3, 5).unwrap();
    assert_eq!(result.variable, "short");
    assert_eq!(result.type_, "builtins.int");
}

#[test]
#[ignore] // Requires Python environment
fn test_unicode_variables() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "unicode_test.py",
        r#"
# Unicode variable names
π: float = 3.14159
δ: float = 0.001

# String with unicode
greeting: str = "Hello, 世界! 🌍"
"#,
    );

    // Test unicode variable
    let result = query_type_with_python(test_file.to_str().unwrap(), "unicode_test", 3, 1).unwrap();
    assert_eq!(result.variable, "π");
    assert_eq!(result.type_, "builtins.float");
}
