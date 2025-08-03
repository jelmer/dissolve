//! Tests for import tracking and name resolution

use crate::ruff_parser_improved::migrate_file_with_improved_ruff;
use crate::types::TypeIntrospectionMethod;

#[test]
fn test_direct_import_migration() {
    let source = r#"
from dulwich.porcelain import checkout_branch
from dulwich.repo import Repo

def test_checkout():
    repo = Repo("/tmp/test-repo")
    checkout_branch(repo, "main", force=True)
    checkout_branch(repo, "develop")
"#;

    // This test would need the actual replacement info to work
    // For now, we just test that it doesn't crash
    let test_ctx = crate::tests::test_utils::TestContext::new(source);
    let result = migrate_file_with_improved_ruff(
        source,
        "test_module",
        test_ctx.file_path,
        TypeIntrospectionMethod::PyrightLsp,
    );

    // Should not crash
    assert!(result.is_ok());
}

#[test]
fn test_import_map_creation() {
    let source = r#"
from dulwich.porcelain import checkout_branch, other_func
from dulwich.repo import Repo
import os.path as ospath
"#;

    // Test that import collection works without crashing
    let test_ctx = crate::tests::test_utils::TestContext::new(source);
    let result = migrate_file_with_improved_ruff(
        source,
        "test_module",
        test_ctx.file_path,
        TypeIntrospectionMethod::PyrightLsp,
    );

    assert!(result.is_ok());
}

#[test]
fn test_default_parameter_handling() {
    let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(param, optional=None):
    return new_func(param, optional)

# Test calls
old_func("test")  # Should use default None
old_func("test", "explicit")  # Should use explicit value
"#;

    let test_ctx = crate::tests::test_utils::TestContext::new(source);
    let result = migrate_file_with_improved_ruff(
        source,
        "test_module",
        test_ctx.file_path,
        TypeIntrospectionMethod::PyrightLsp,
    );

    assert!(result.is_ok());
    // In a full test, we'd check that default values are correctly applied
}
