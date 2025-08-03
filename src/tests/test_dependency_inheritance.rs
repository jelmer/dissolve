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

use crate::dependency_collector::collect_deprecated_from_dependencies_with_paths;
use std::fs;
use std::path::PathBuf;
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
fn test_dependency_collection_includes_base_class_methods() {
    // Test that importing a derived class includes deprecated methods from base classes
    let temp_dir = TempDir::new().unwrap();

    // Create dulwich package structure
    create_module(temp_dir.path(), "dulwich/__init__.py", "");
    create_module(
        temp_dir.path(),
        "dulwich/repo.py",
        r#"
from dissolve import replace_me

class BaseRepo:
    """Base repository class."""
    
    @replace_me(remove_in="0.26.0")
    def do_commit(self, message=None):
        """Deprecated method on base class."""
        return self.get_worktree().commit(message=message)
    
    def get_worktree(self):
        """Get worktree."""
        return WorkTree()

class Repo(BaseRepo):
    """Derived repository class."""
    
    @replace_me(remove_in="0.26.0")
    def stage(self, paths):
        """Deprecated method on derived class."""
        return self.get_worktree().stage(paths)

class WorkTree:
    """Work tree class."""
    
    def commit(self, message=None):
        """New commit method."""
        return "commit_result"
    
    def stage(self, paths):
        """New stage method."""
        return "stage_result"
"#,
    );

    // Create a test source file that imports the derived class
    let test_source = r#"
from dulwich.repo import Repo
r = Repo()
r.do_commit(message="test")  # This should be found even though do_commit is on BaseRepo
r.stage(["file.txt"])        # This should also be found
"#;

    let additional_paths = vec![temp_dir.path().to_string_lossy().to_string()];
    let result = collect_deprecated_from_dependencies_with_paths(
        test_source,
        "test_module",
        5,
        &additional_paths,
    )
    .unwrap();

    // Check that both methods are found
    let replacement_keys: Vec<&String> = result.replacements.keys().collect();

    // Should find both the base class method and derived class method
    assert_eq!(replacement_keys.len(), 2);
    assert!(replacement_keys
        .iter()
        .any(|k| k.contains("BaseRepo.do_commit")));
    assert!(replacement_keys.iter().any(|k| k.contains("Repo.stage")));

    // Check the inheritance map
    assert!(!result.inheritance_map.is_empty());

    // The Repo class should inherit from BaseRepo
    if let Some(base_classes) = result.inheritance_map.get("dulwich.repo.Repo") {
        assert!(
            base_classes.contains(&"BaseRepo".to_string())
                || base_classes.contains(&"dulwich.repo.BaseRepo".to_string())
        );
    } else {
        panic!("No inheritance info found for dulwich.repo.Repo");
    }
}

#[test]
fn test_dependency_collection_inheritance_chain() {
    // Test that deep inheritance chains are properly handled
    let temp_dir = TempDir::new().unwrap();

    // Create test package
    create_module(temp_dir.path(), "testpkg/__init__.py", "");
    create_module(
        temp_dir.path(),
        "testpkg/module.py",
        r#"
from dissolve import replace_me

class GrandParent:
    """Grandparent class."""
    
    @replace_me(remove_in="1.0.0")
    def old_method(self):
        """Method on grandparent."""
        return self.new_method()
    
    def new_method(self):
        return "new_result"

class Parent(GrandParent):
    """Parent class."""
    pass

class Child(Parent):
    """Child class."""
    pass
"#,
    );

    // Test source that imports the child class
    let test_source = r#"
from testpkg.module import Child
c = Child()
c.old_method()  # Should find this from GrandParent
"#;

    let additional_paths = vec![temp_dir.path().to_string_lossy().to_string()];
    let result = collect_deprecated_from_dependencies_with_paths(
        test_source,
        "test_module",
        5,
        &additional_paths,
    )
    .unwrap();

    println!(
        "Found replacements: {:?}",
        result.replacements.keys().collect::<Vec<_>>()
    );
    println!("Inheritance map: {:?}", result.inheritance_map);

    // Should find the method from the grandparent class
    assert_eq!(result.replacements.len(), 1);
    let key = result.replacements.keys().next().unwrap();
    assert!(key.contains("GrandParent.old_method"));

    let replacement = &result.replacements[key];
    assert!(replacement.replacement_expr.contains("new_method"));

    // Check inheritance chain
    assert!(!result.inheritance_map.is_empty());

    // Child should inherit from Parent
    if let Some(parent_classes) = result.inheritance_map.get("testpkg.module.Child") {
        assert!(parent_classes.contains(&"testpkg.module.Parent".to_string()));
    }

    // Parent should inherit from GrandParent
    if let Some(grandparent_classes) = result.inheritance_map.get("testpkg.module.Parent") {
        assert!(grandparent_classes.contains(&"testpkg.module.GrandParent".to_string()));
    }
}

#[test]
fn test_multiple_inheritance() {
    // Test that multiple inheritance is handled correctly
    let temp_dir = TempDir::new().unwrap();

    create_module(temp_dir.path(), "testpkg/__init__.py", "");
    create_module(
        temp_dir.path(),
        "testpkg/mixins.py",
        r#"
from dissolve import replace_me

class MixinA:
    @replace_me()
    def method_a(self):
        return self.new_method_a()

class MixinB:
    @replace_me()
    def method_b(self):
        return self.new_method_b()

class Combined(MixinA, MixinB):
    def new_method_a(self):
        return "a"
    
    def new_method_b(self):
        return "b"
"#,
    );

    let test_source = r#"
from testpkg.mixins import Combined
obj = Combined()
obj.method_a()
obj.method_b()
"#;

    let additional_paths = vec![temp_dir.path().to_string_lossy().to_string()];
    let result = collect_deprecated_from_dependencies_with_paths(
        test_source,
        "test_module",
        5,
        &additional_paths,
    )
    .unwrap();

    // Should find both methods from both mixins
    assert_eq!(result.replacements.len(), 2);
    assert!(result
        .replacements
        .keys()
        .any(|k| k.contains("MixinA.method_a")));
    assert!(result
        .replacements
        .keys()
        .any(|k| k.contains("MixinB.method_b")));

    // Combined should inherit from both mixins
    if let Some(base_classes) = result.inheritance_map.get("testpkg.mixins.Combined") {
        assert!(base_classes.contains(&"testpkg.mixins.MixinA".to_string()));
        assert!(base_classes.contains(&"testpkg.mixins.MixinB".to_string()));
    }
}
