use dissolve::mypy_integration::{
    clear_mypy_querier_cache, get_mypy_querier, get_type_for_variable, query_type_with_python,
};
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
fn test_basic_type_query() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "test_file.py",
        r#"import os

class MyClass:
    def my_method(self, arg1: int, var1: str) -> str:
        return var1

my_instance = MyClass()
result = my_instance.my_method(1, "test_string")
"#,
    );

    // Test querying type of my_instance on line 8
    let result = query_type_with_python(test_file.to_str().unwrap(), "test_file", 8, 20);

    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "my_instance");
            assert_eq!(query_result.type_, "test_file.MyClass");
        }
        Err(e) => panic!("Query failed: {}", e),
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_with_statement_type_inference() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "with_test.py",
        r#"class Resource:
    def __enter__(self):
        return self
    def __exit__(self, *args):
        pass
    def close(self):
        pass

def get_resource() -> Resource:
    return Resource()

with get_resource() as r:
    r.close()
"#,
    );

    // Try to get type of 'r' on line 13 (r.close())
    let result = query_type_with_python(test_file.to_str().unwrap(), "with_test", 13, 5);

    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "r");
            // Note: mypy infers 'Any' for with statement targets without explicit type annotation
            assert_eq!(query_result.type_, "Any");
        }
        Err(e) => panic!("Query failed: {}", e),
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_function_parameter_type() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "param_test.py",
        r#"class MyClass:
    def method(self):
        pass

def process(obj: MyClass):
    obj.method()
"#,
    );

    // Get type of 'obj' on line 6 where it's used
    let result = query_type_with_python(test_file.to_str().unwrap(), "param_test", 6, 7);

    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "obj");
            assert_eq!(query_result.type_, "param_test.MyClass");
        }
        Err(e) => panic!("Query failed: {}", e),
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_chained_method_return_type() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "chain_test.py",
        r#"class Builder:
    def with_name(self, name: str) -> "Builder":
        return self
    def build(self) -> "Product":
        return Product()

class Product:
    def use(self):
        pass

builder = Builder()
product = builder.with_name("test").build()
product.use()
"#,
    );

    // Get type of 'product' on line 13 where it's used
    let result = query_type_with_python(test_file.to_str().unwrap(), "chain_test", 13, 7);

    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "product");
            assert_eq!(query_result.type_, "chain_test.Product");
        }
        Err(e) => panic!("Query failed: {}", e),
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_for_loop_variable() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "loop_test.py",
        r#"from typing import List

class Item:
    def process(self):
        pass

items: List[Item] = []

for item in items:
    item.process()
"#,
    );

    // Get type of 'item' on line 10 where it's used
    let result = query_type_with_python(test_file.to_str().unwrap(), "loop_test", 10, 8);

    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "item");
            assert_eq!(query_result.type_, "loop_test.Item");
        }
        Err(e) => panic!("Query failed: {}", e),
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_function_call_return_type() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "func_test.py",
        r#"class Resource:
    def close(self):
        pass

def get_resource() -> Resource:
    return Resource()

# Test function call return type
r = get_resource()
r.close()
"#,
    );

    // Get type of 'r' which is assigned from get_resource()
    let result = query_type_with_python(test_file.to_str().unwrap(), "func_test", 9, 1);

    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "r");
            assert_eq!(query_result.type_, "func_test.Resource");
        }
        Err(e) => panic!("Query failed: {}", e),
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_method_call_return_type() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "method_test.py",
        r#"class Builder:
    def build(self) -> "Product":
        return Product()

class Product:
    def use(self):
        pass

builder = Builder()
# Test method call return type
p = builder.build()
p.use()
"#,
    );

    // Get type of 'p' which is assigned from builder.build()
    let result = query_type_with_python(test_file.to_str().unwrap(), "method_test", 11, 1);

    match result {
        Ok(query_result) => {
            assert_eq!(query_result.variable, "p");
            assert_eq!(query_result.type_, "method_test.Product");
        }
        Err(e) => panic!("Query failed: {}", e),
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_mypy_querier_caching() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "cache_test.py",
        r#"x: int = 42
y: str = "hello"
"#,
    );

    // Clear cache first
    clear_mypy_querier_cache();

    // Get querier twice - second should use cache
    let querier1 = get_mypy_querier(
        test_file.to_str().unwrap().to_string(),
        "cache_test".to_string(),
    );
    assert!(querier1.is_ok());

    let querier2 = get_mypy_querier(
        test_file.to_str().unwrap().to_string(),
        "cache_test".to_string(),
    );
    assert!(querier2.is_ok());

    // Clear cache again
    clear_mypy_querier_cache();
}

#[test]
#[ignore] // Requires Python environment
fn test_get_type_for_variable() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "simple_test.py",
        r#"class MyClass:
    pass

instance = MyClass()
"#,
    );

    // Get type using simple interface
    let result = get_type_for_variable(test_file.to_str().unwrap(), "simple_test", 4, 10);

    match result {
        Ok(type_str) => {
            assert_eq!(type_str, "simple_test.MyClass");
        }
        Err(e) => panic!("Query failed: {}", e),
    }
}

#[test]
#[ignore] // Requires Python environment
fn test_query_type_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = create_test_file(
        &temp_dir,
        "empty_test.py",
        r#"# Empty file
"#,
    );

    // Try to query at a position with no variable
    let result = query_type_with_python(test_file.to_str().unwrap(), "empty_test", 1, 1);

    // Should return an error
    assert!(result.is_err());
}
