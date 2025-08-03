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

//! Tests for real-world scenarios including inheritance patterns and repository-specific patterns.

use dissolve::core::ConstructType;

mod common;
use common::*;

// === Inheritance Scenarios ===

#[test]
fn test_method_inherited_from_base() {
    let source = r#"
from dissolve import replace_me

class Base:
    @replace_me(remove_in="2.0.0")
    def old_method(self, x):
        return self.new_method(x * 2)
    
    def new_method(self, x):
        return x + 1

class Derived(Base):
    pass
"#;

    let result = collect_replacements(source);
    assert!(result
        .replacements
        .contains_key("test_module.Base.old_method"));

    let replacement = &result.replacements["test_module.Base.old_method"];
    assert_eq!(replacement.replacement_expr, "{self}.new_method({x} * 2)");
    assert_eq!(replacement.construct_type, ConstructType::Function);
}

#[test]
fn test_method_overridden_in_derived() {
    let source = r#"
from dissolve import replace_me

class Base:
    @replace_me(remove_in="2.0.0")
    def old_method(self, x):
        return self.new_method(x * 2)
    
    def new_method(self, x):
        return x + 1

class Derived(Base):
    # Override without @replace_me
    def old_method(self, x):
        return x * 10
"#;

    let result = collect_replacements(source);

    // Should find the deprecated method in base class
    assert!(result
        .replacements
        .contains_key("test_module.Base.old_method"));

    // Should NOT find a replacement for the derived class method (no @replace_me)
    assert!(!result
        .replacements
        .contains_key("test_module.Derived.old_method"));
}

#[test]
fn test_multi_level_inheritance() {
    let source = r#"
from dissolve import replace_me

class GrandParent:
    @replace_me(remove_in="2.0.0")
    def deprecated_method(self):
        return self.modern_method()
    
    def modern_method(self):
        return "modern"

class Parent(GrandParent):
    pass

class Child(Parent):
    pass
"#;

    let result = collect_replacements(source);

    // Should find the deprecated method in the grandparent class
    assert!(result
        .replacements
        .contains_key("test_module.GrandParent.deprecated_method"));

    let replacement = &result.replacements["test_module.GrandParent.deprecated_method"];
    assert_eq!(replacement.replacement_expr, "{self}.modern_method()");
}

// === Repository/Worktree Delegation Patterns ===

#[test]
fn test_repo_stage_replacement_pattern() {
    let source = r#"
from dissolve import replace_me

class BaseRepo:
    pass

class Repo(BaseRepo):
    @replace_me
    def stage(self, paths):
        return self.get_worktree().stage(paths)
    
    def get_worktree(self):
        return WorkTree()

class WorkTree:
    def stage(self, paths):
        pass
"#;

    let result = collect_replacements_with_module(source, "dulwich.repo");

    assert_eq!(result.replacements.len(), 1);
    assert!(result.replacements.contains_key("dulwich.repo.Repo.stage"));

    let replacement = &result.replacements["dulwich.repo.Repo.stage"];
    assert_eq!(
        replacement.replacement_expr,
        "{self}.get_worktree().stage({paths})"
    );
    assert_eq!(replacement.construct_type, ConstructType::Function);

    // Check parameters
    assert_eq!(replacement.parameters.len(), 2);
    assert_eq!(replacement.parameters[0].name, "self");
    assert_eq!(replacement.parameters[1].name, "paths");
}

#[test]
fn test_multiple_repo_methods_with_worktree() {
    let source = r#"
from dissolve import replace_me

class Repo:
    @replace_me
    def stage(self, paths):
        return self.get_worktree().stage(paths)
    
    @replace_me
    def unstage(self, paths):
        return self.get_worktree().unstage(paths)
    
    @replace_me
    def add(self, paths, force=False):
        return self.get_worktree().add(paths, force=force)
    
    def get_worktree(self):
        return WorkTree()
"#;

    let result = collect_replacements_with_module(source, "dulwich.repo");

    assert_eq!(result.replacements.len(), 3);

    // Check stage method
    assert!(result.replacements.contains_key("dulwich.repo.Repo.stage"));
    let stage_replacement = &result.replacements["dulwich.repo.Repo.stage"];
    assert_eq!(
        stage_replacement.replacement_expr,
        "{self}.get_worktree().stage({paths})"
    );

    // Check unstage method
    assert!(result
        .replacements
        .contains_key("dulwich.repo.Repo.unstage"));
    let unstage_replacement = &result.replacements["dulwich.repo.Repo.unstage"];
    assert_eq!(
        unstage_replacement.replacement_expr,
        "{self}.get_worktree().unstage({paths})"
    );

    // Check add method with keyword argument
    assert!(result.replacements.contains_key("dulwich.repo.Repo.add"));
    let add_replacement = &result.replacements["dulwich.repo.Repo.add"];
    assert_eq!(
        add_replacement.replacement_expr,
        "{self}.get_worktree().add({paths}, force={force})"
    );
}

#[test]
fn test_worktree_method_not_replaced() {
    let source = r#"
from dissolve import replace_me

class WorkTree:
    def stage(self, paths):
        """This method should NOT be replaced."""
        pass
    
    def add(self, paths):
        """This method should NOT be replaced."""
        pass

class Repo:
    @replace_me
    def stage(self, paths):
        return self.get_worktree().stage(paths)
"#;

    let result = collect_replacements_with_module(source, "dulwich.worktree");

    // Should only find the Repo.stage method, not WorkTree methods
    assert_eq!(result.replacements.len(), 1);
    assert!(result
        .replacements
        .contains_key("dulwich.worktree.Repo.stage"));
    assert!(!result
        .replacements
        .contains_key("dulwich.worktree.WorkTree.stage"));
    assert!(!result
        .replacements
        .contains_key("dulwich.worktree.WorkTree.add"));
}

#[test]
fn test_complex_worktree_delegation() {
    let source = r#"
from dissolve import replace_me

class Repo:
    @replace_me
    def commit(self, message, author=None):
        worktree = self.get_worktree()
        return worktree.commit(message, author=author)
    
    @replace_me
    def diff(self, target=None):
        return self.get_worktree().diff(target)
    
    def get_worktree(self):
        return WorkTree()
"#;

    let result = collect_replacements_with_module(source, "test_repo");

    assert_eq!(result.replacements.len(), 2);

    // Check commit method
    assert!(result.replacements.contains_key("test_repo.Repo.commit"));
    let commit_replacement = &result.replacements["test_repo.Repo.commit"];
    // This is more complex - multiple statements
    assert!(commit_replacement
        .replacement_expr
        .contains("worktree = {self}.get_worktree()"));
    assert!(commit_replacement
        .replacement_expr
        .contains("worktree.commit({message}, author={author})"));

    // Check diff method
    assert!(result.replacements.contains_key("test_repo.Repo.diff"));
    let diff_replacement = &result.replacements["test_repo.Repo.diff"];
    assert_eq!(
        diff_replacement.replacement_expr,
        "{self}.get_worktree().diff({target})"
    );
}

#[test]
fn test_inherited_repo_methods() {
    let source = r#"
from dissolve import replace_me

class BaseRepo:
    @replace_me
    def stage(self, paths):
        return self.get_worktree().stage(paths)
    
    def get_worktree(self):
        return WorkTree()

class GitRepo(BaseRepo):
    pass

class SvnRepo(BaseRepo):
    # Override the method
    def stage(self, paths):
        """Custom implementation without @replace_me"""
        return super().stage(paths)
"#;

    let result = collect_replacements_with_module(source, "vcs_module");

    // Should find the base class method
    assert_eq!(result.replacements.len(), 1);
    assert!(result
        .replacements
        .contains_key("vcs_module.BaseRepo.stage"));

    // Should NOT find the overridden method in SvnRepo (no @replace_me)
    assert!(!result.replacements.contains_key("vcs_module.SvnRepo.stage"));

    let replacement = &result.replacements["vcs_module.BaseRepo.stage"];
    assert_eq!(
        replacement.replacement_expr,
        "{self}.get_worktree().stage({paths})"
    );
}

// === Mixed Construct Scenarios ===

#[test]
fn test_mixed_constructs_with_inheritance() {
    let source = r#"
from dissolve import replace_me

@replace_me
def old_function(x):
    return new_function(x)

@replace_me
class OldClass:
    def __init__(self, value):
        self._wrapped = NewClass(value)

class Base:
    @replace_me
    def old_method(self):
        return self.new_method()
    
    def new_method(self):
        return "new"

class Derived(Base):
    pass

OLD_CONSTANT = replace_me("new_value")
"#;

    let result = collect_replacements(source);

    // Should find all deprecated items
    assert!(result.replacements.contains_key("test_module.old_function"));
    assert!(result.replacements.contains_key("test_module.OldClass"));
    assert!(result
        .replacements
        .contains_key("test_module.Base.old_method"));
    assert!(result.replacements.contains_key("test_module.OLD_CONSTANT"));

    // Verify construct types
    assert_eq!(
        result.replacements["test_module.old_function"].construct_type,
        ConstructType::Function
    );
    assert_eq!(
        result.replacements["test_module.OldClass"].construct_type,
        ConstructType::Class
    );
    assert_eq!(
        result.replacements["test_module.Base.old_method"].construct_type,
        ConstructType::Function
    );
    assert_eq!(
        result.replacements["test_module.OLD_CONSTANT"].construct_type,
        ConstructType::ModuleAttribute
    );
}
