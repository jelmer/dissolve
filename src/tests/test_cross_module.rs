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

use crate::dependency_collector::{
    clear_module_cache, collect_deprecated_from_dependencies_with_paths,
};
use crate::migrate_ruff::migrate_file;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::TypeIntrospectionMethod;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create a Python module file
fn create_module(dir: &std::path::Path, rel_path: &str, content: &str) -> PathBuf {
    let full_path = dir.join(rel_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full_path, content).unwrap();
    full_path
}

#[test]
fn test_simple_function_cross_module() {
    // Clear module cache to ensure test isolation
    clear_module_cache();

    let temp_dir = TempDir::new().unwrap();

    // Create deprecated module
    let deprecated_module = r#"
from dissolve import replace_me

@replace_me()
def old_function(x, y):
    return new_function(x, y)

def new_function(x, y):
    return x + y
"#;

    // Create module that uses the deprecated function
    let user_module = r#"
from testpkg.deprecated import old_function

def test():
    result = old_function(1, 2)
    return result
"#;

    // Create the files
    create_module(temp_dir.path(), "testpkg/__init__.py", "");
    create_module(temp_dir.path(), "testpkg/deprecated.py", deprecated_module);
    let user_path = create_module(temp_dir.path(), "testpkg/user.py", user_module);

    // Create pyrightconfig.json to help Pyright find modules
    let pyright_config = r#"{
        "include": ["testpkg"],
        "pythonVersion": "3.8",
        "pythonPlatform": "All",
        "typeCheckingMode": "basic",
        "useLibraryCodeForTypes": true
    }"#;
    fs::write(temp_dir.path().join("pyrightconfig.json"), pyright_config).unwrap();

    // Collect deprecations from the user module with temp directory in search path
    let additional_paths = vec![temp_dir.path().to_string_lossy().to_string()];
    let dep_result = collect_deprecated_from_dependencies_with_paths(
        user_module,
        "testpkg.user",
        5,
        &additional_paths,
    )
    .unwrap();

    // Should find the deprecated function
    assert!(dep_result
        .replacements
        .contains_key("testpkg.deprecated.old_function"));

    // Create type introspection context with temp directory as workspace
    let mut type_context = TypeIntrospectionContext::new_with_workspace(
        TypeIntrospectionMethod::PyrightLsp,
        Some(temp_dir.path().to_str().unwrap()),
    )
    .unwrap();

    // Migrate the user module
    let result = migrate_file(
        user_module,
        "testpkg.user",
        &user_path,
        &mut type_context,
        dep_result.replacements,
        dep_result.inheritance_map,
    )
    .unwrap();

    type_context.shutdown().unwrap();

    // Check the result
    assert!(result.contains("new_function(1, 2)"));
    assert!(!result.contains("old_function(1, 2)"));
}

#[test]
fn test_class_method_cross_module() {
    // Clear module cache to ensure test isolation
    clear_module_cache();

    let temp_dir = TempDir::new().unwrap();

    // Create module with deprecated class
    let deprecated_module = r#"
from dissolve import replace_me

class OldAPI:
    @replace_me()
    def old_method(self, data):
        return self.new_method(data)
        
    def new_method(self, data):
        return data
"#;

    // Create module that uses the deprecated method
    let user_module = r#"
from testpkg.api import OldAPI

def process():
    api = OldAPI()
    api.old_method("test")
    
def process_with_variable():
    api = OldAPI()
    obj = api
    obj.old_method("data")
"#;

    // Create the files
    create_module(temp_dir.path(), "testpkg/__init__.py", "");
    create_module(temp_dir.path(), "testpkg/api.py", deprecated_module);
    let client_path = create_module(temp_dir.path(), "testpkg/client.py", user_module);

    // Collect deprecations from dependencies with temp directory in search path
    let additional_paths = vec![temp_dir.path().to_string_lossy().to_string()];
    let dep_result = collect_deprecated_from_dependencies_with_paths(
        user_module,
        "testpkg.client",
        5,
        &additional_paths,
    )
    .unwrap();

    // Should find the deprecated method
    assert!(dep_result
        .replacements
        .contains_key("testpkg.api.OldAPI.old_method"));

    // Create type introspection context with temp directory as workspace
    let mut type_context = TypeIntrospectionContext::new_with_workspace(
        TypeIntrospectionMethod::PyrightLsp,
        Some(temp_dir.path().to_str().unwrap()),
    )
    .unwrap();

    // Migrate the client module
    let result = migrate_file(
        user_module,
        "testpkg.client",
        &client_path,
        &mut type_context,
        dep_result.replacements,
        dep_result.inheritance_map,
    )
    .unwrap();

    type_context.shutdown().unwrap();

    // Both calls should be replaced
    assert!(result.contains("api.new_method(\"test\")"));
    assert!(result.contains("obj.new_method(\"data\")"));
    assert!(!result.contains("old_method"));
}

#[test]
fn test_classmethod_cross_module() {
    // Clear module cache to ensure test isolation
    clear_module_cache();

    let temp_dir = TempDir::new().unwrap();

    let deprecated_module = r#"
from dissolve import replace_me

class Factory:
    @classmethod
    @replace_me()
    def old_create(cls, name):
        return cls.new_create(name)
        
    @classmethod
    def new_create(cls, name):
        return cls(name)
"#;

    let user_module = r#"
from testpkg.factory import Factory

def create_instance():
    return Factory.old_create("test")
"#;

    // Create the files
    create_module(temp_dir.path(), "testpkg/__init__.py", "");
    create_module(temp_dir.path(), "testpkg/factory.py", deprecated_module);
    let user_path = create_module(temp_dir.path(), "testpkg/user.py", user_module);

    // Create pyrightconfig.json to help Pyright find modules
    let pyright_config = r#"{
        "include": ["testpkg"],
        "pythonVersion": "3.8",
        "pythonPlatform": "All",
        "typeCheckingMode": "basic",
        "useLibraryCodeForTypes": true
    }"#;
    fs::write(temp_dir.path().join("pyrightconfig.json"), pyright_config).unwrap();

    // Collect deprecations from dependencies with temp directory in search path
    let additional_paths = vec![temp_dir.path().to_string_lossy().to_string()];
    let dep_result = collect_deprecated_from_dependencies_with_paths(
        user_module,
        "testpkg.user",
        5,
        &additional_paths,
    )
    .unwrap();

    // Should find the deprecated classmethod
    assert!(dep_result
        .replacements
        .contains_key("testpkg.factory.Factory.old_create"));

    // Create type introspection context with temp directory as workspace
    let mut type_context = TypeIntrospectionContext::new_with_workspace(
        TypeIntrospectionMethod::PyrightLsp,
        Some(temp_dir.path().to_str().unwrap()),
    )
    .unwrap();

    // Open the package files in Pyright so it knows about the module structure
    type_context
        .open_file(&temp_dir.path().join("testpkg/__init__.py"), "")
        .unwrap();
    type_context
        .open_file(
            &temp_dir.path().join("testpkg/factory.py"),
            deprecated_module,
        )
        .unwrap();

    // Migrate
    let result = migrate_file(
        user_module,
        "testpkg.user",
        &user_path,
        &mut type_context,
        dep_result.replacements,
        dep_result.inheritance_map,
    )
    .unwrap();

    type_context.shutdown().unwrap();

    assert!(result.contains("Factory.new_create(\"test\")"));
    assert!(!result.contains("old_create"));
}

#[test]
fn test_staticmethod_cross_module() {
    // Clear module cache to ensure test isolation
    clear_module_cache();

    let temp_dir = TempDir::new().unwrap();

    let deprecated_module = r#"
from dissolve import replace_me

class Utils:
    @staticmethod
    @replace_me()
    def old_helper(x):
        return new_helper(x)

def new_helper(x):
    return x * 2
"#;

    let user_module = r#"
from testpkg.utils import Utils

def calculate():
    return Utils.old_helper(5)
"#;

    // Create the files
    create_module(temp_dir.path(), "testpkg/__init__.py", "");
    create_module(temp_dir.path(), "testpkg/utils.py", deprecated_module);
    let user_path = create_module(temp_dir.path(), "testpkg/user.py", user_module);

    // Collect deprecations from dependencies with temp directory in search path
    let additional_paths = vec![temp_dir.path().to_string_lossy().to_string()];
    let dep_result = collect_deprecated_from_dependencies_with_paths(
        user_module,
        "testpkg.user",
        5,
        &additional_paths,
    )
    .unwrap();

    // Should find the deprecated staticmethod
    assert!(dep_result
        .replacements
        .contains_key("testpkg.utils.Utils.old_helper"));

    // Create type introspection context with temp directory as workspace
    let mut type_context = TypeIntrospectionContext::new_with_workspace(
        TypeIntrospectionMethod::PyrightLsp,
        Some(temp_dir.path().to_str().unwrap()),
    )
    .unwrap();

    // Open the dependency file in Pyright so it knows about the Utils class
    type_context
        .open_file(&temp_dir.path().join("testpkg/utils.py"), deprecated_module)
        .unwrap();

    // Migrate
    let result = migrate_file(
        user_module,
        "testpkg.user",
        &user_path,
        &mut type_context,
        dep_result.replacements,
        dep_result.inheritance_map,
    )
    .unwrap();

    type_context.shutdown().unwrap();

    assert!(result.contains("new_helper(5)"));
    assert!(!result.contains("old_helper"));
}

#[test]
fn test_import_alias() {
    // Clear module cache to ensure test isolation
    clear_module_cache();

    let temp_dir = TempDir::new().unwrap();

    let deprecated_module = r#"
from dissolve import replace_me

@replace_me()
def old_function(x):
    return new_function(x)

def new_function(x):
    return x * 2
"#;

    let user_module = r#"
from testpkg.deprecated import old_function as legacy_func

def test():
    return legacy_func(42)
"#;

    // Create the files
    create_module(temp_dir.path(), "testpkg/__init__.py", "");
    create_module(temp_dir.path(), "testpkg/deprecated.py", deprecated_module);
    let user_path = create_module(temp_dir.path(), "testpkg/user.py", user_module);

    // Collect deprecations from dependencies with temp directory in search path
    let additional_paths = vec![temp_dir.path().to_string_lossy().to_string()];
    let dep_result = collect_deprecated_from_dependencies_with_paths(
        user_module,
        "testpkg.user",
        5,
        &additional_paths,
    )
    .unwrap();

    // Should find the deprecated function even with alias
    assert!(dep_result
        .replacements
        .contains_key("testpkg.deprecated.old_function"));

    // Create type introspection context with temp directory as workspace
    let mut type_context = TypeIntrospectionContext::new_with_workspace(
        TypeIntrospectionMethod::PyrightLsp,
        Some(temp_dir.path().to_str().unwrap()),
    )
    .unwrap();

    // Migrate
    let result = migrate_file(
        user_module,
        "testpkg.user",
        &user_path,
        &mut type_context,
        dep_result.replacements,
        dep_result.inheritance_map,
    )
    .unwrap();

    type_context.shutdown().unwrap();

    assert!(result.contains("new_function(42)"));
    assert!(!result.contains("legacy_func(42)"));
}

#[test]
fn test_with_statement_context_manager() {
    // Clear module cache to ensure test isolation
    clear_module_cache();

    let temp_dir = TempDir::new().unwrap();

    let deprecated_module = r#"
from dissolve import replace_me

class Resource:
    @replace_me()
    def old_close(self):
        return self.new_close()
        
    def new_close(self):
        pass
        
    def __enter__(self):
        return self
        
    def __exit__(self, *args):
        pass
"#;

    let user_module = r#"
from testpkg.resource import Resource

def use_resource():
    with Resource() as res:
        # do something
        res.old_close()
"#;

    // Create the files
    create_module(temp_dir.path(), "testpkg/__init__.py", "");
    create_module(temp_dir.path(), "testpkg/resource.py", deprecated_module);
    let user_path = create_module(temp_dir.path(), "testpkg/user.py", user_module);

    // Collect deprecations from dependencies with temp directory in search path
    let additional_paths = vec![temp_dir.path().to_string_lossy().to_string()];
    let dep_result = collect_deprecated_from_dependencies_with_paths(
        user_module,
        "testpkg.user",
        5,
        &additional_paths,
    )
    .unwrap();

    // Should find the deprecated method
    assert!(dep_result
        .replacements
        .contains_key("testpkg.resource.Resource.old_close"));

    // Create type introspection context with temp directory as workspace
    let mut type_context = TypeIntrospectionContext::new_with_workspace(
        TypeIntrospectionMethod::PyrightLsp,
        Some(temp_dir.path().to_str().unwrap()),
    )
    .unwrap();

    // Migrate
    let result = migrate_file(
        user_module,
        "testpkg.user",
        &user_path,
        &mut type_context,
        dep_result.replacements,
        dep_result.inheritance_map,
    )
    .unwrap();

    type_context.shutdown().unwrap();

    assert!(result.contains("res.new_close()"));
    assert!(!result.contains("old_close"));
}

#[test]
fn test_scan_dependencies_disabled() {
    // Clear module cache to ensure test isolation
    clear_module_cache();

    // Test with dependency scanning disabled - the function shouldn't be replaced
    let source = r#"
from testpkg.api import old_function

def test():
    old_function()
"#;

    // Create type introspection context
    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();

    // Migrate with empty replacements (simulating no dependency scanning)
    let result = migrate_file(
        source,
        "testmodule",
        Path::new("test.py"),
        &mut type_context,
        HashMap::new(),
        HashMap::new(),
    )
    .unwrap();

    type_context.shutdown().unwrap();

    // Should not change since we didn't provide any replacements
    assert!(result.contains("old_function()"));
}
