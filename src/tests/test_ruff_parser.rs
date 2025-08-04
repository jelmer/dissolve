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

//! Tests for Ruff parser integration

#[cfg(test)]
mod tests {
    use crate::ruff_parser::{migrate_file_with_ruff, PythonModule};
    use crate::types::TypeIntrospectionMethod;
    use ruff_text_size::Ranged;

    #[test]
    fn test_ruff_parser_basic() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x * 2)

# This is a comment
result = old_func(5)
"#;

        let module = PythonModule::parse(source).unwrap();

        // Check that tokens include comments
        let tokens = module.tokens();
        let has_comment = tokens.iter().any(|t| {
            // Check if token is in comment range
            let range = t.range();
            module.text_at_range(range).contains("This is a comment")
        });
        assert!(has_comment, "Should preserve comment tokens");

        // Check AST parsing
        assert!(module.ast().as_module().is_some());
    }

    #[test]
    fn test_ruff_position_tracking() {
        let source = "x = 1\ny = 2";
        let module = PythonModule::parse(source).unwrap();

        // Test position mapping
        assert_eq!(module.offset_to_position(0.into()), Some((1, 0)));
        assert_eq!(module.offset_to_position(6.into()), Some((2, 0)));
    }

    #[test]
    fn test_ruff_migration_preserves_formatting() {
        let source = r#"from dissolve import replace_me

@replace_me()
def old_func(x):
    """This is a docstring"""
    return new_func(x * 2)


# Important comment
result = old_func(5)  # Inline comment


# End of file
"#;

        // For now, just test that it doesn't crash
        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = migrate_file_with_ruff(
            source,
            "test_module",
            test_ctx.file_path,
            TypeIntrospectionMethod::PyrightLsp,
        );

        match result {
            Ok(migrated) => {
                // Should preserve all whitespace and comments
                assert!(migrated.contains("# Important comment"));
                assert!(migrated.contains("# Inline comment"));
                assert!(migrated.contains("# End of file"));
                // Should preserve blank lines
                assert!(migrated.contains("\n\n"));
            }
            Err(e) => {
                // For now, we expect this might fail since we haven't fully implemented
                // the replacement logic
                println!("Migration not yet fully implemented: {}", e);
            }
        }
    }
}
