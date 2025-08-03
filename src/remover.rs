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

//! Functionality for removing deprecated functions from source code.
//!
//! This module provides wrappers around the Ruff-based implementation
//! for backward compatibility.

use anyhow::{Context, Result};
use std::fs;

/// Remove entire functions decorated with @replace_me from source code.
///
/// This function completely removes functions that are decorated with @replace_me,
/// not just the decorators. This should only be used after migration is complete
/// and all calls to deprecated functions have been updated.
pub fn remove_decorators(
    source: &str,
    before_version: Option<&str>,
    remove_all: bool,
    current_version: Option<&str>,
) -> Result<String> {
    if !remove_all && before_version.is_none() && current_version.is_none() {
        // No removal criteria specified, return source unchanged
        return Ok(source.to_string());
    }

    // Use Ruff-based remover
    let (_removed_count, result) = crate::ruff_remover::remove_deprecated_functions(
        source,
        before_version,
        remove_all,
        current_version,
    )?;

    Ok(result)
}

/// Remove functions decorated with @replace_me from a file
pub fn remove_decorators_from_file(
    file_path: &str,
    before_version: Option<&str>,
    remove_all: bool,
    write: bool,
    current_version: Option<&str>,
) -> Result<(usize, String)> {
    let source = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path))?;

    // Use Ruff-based remover
    let (removed_count, result) = crate::ruff_remover::remove_deprecated_functions(
        &source,
        before_version,
        remove_all,
        current_version,
    )?;

    if write && removed_count > 0 {
        fs::write(file_path, &result)
            .with_context(|| format!("Failed to write file: {}", file_path))?;
    }

    Ok((removed_count, result))
}

/// Alias for CLI compatibility
pub fn remove_from_file(
    file_path: &str,
    before_version: Option<&str>,
    remove_all: bool,
    write: bool,
    current_version: Option<&str>,
) -> Result<(usize, String)> {
    remove_decorators_from_file(
        file_path,
        before_version,
        remove_all,
        write,
        current_version,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_all() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_function():
    return new_function()

def regular_function():
    return 42

@replace_me(since="1.0.0")
def another_old():
    return new_api()
"#;

        let result = remove_decorators(source, None, true, None).unwrap();
        assert!(!result.contains("def old_function"));
        assert!(!result.contains("def another_old"));
        assert!(result.contains("def regular_function"));
    }

    #[test]
    fn test_no_removal_criteria() {
        let source = r#"
@replace_me()
def old_function():
    return new_function()
"#;

        let result = remove_decorators(source, None, false, None).unwrap();
        assert_eq!(result, source);
    }

    #[test]
    fn test_remove_before_version() {
        let source = r#"
from dissolve import replace_me

@replace_me(since="1.0.0")
def old_v1():
    return new_v1()

@replace_me(since="2.0.0")
def old_v2():
    return new_v2()

def regular_function():
    return 42
"#;

        let result = remove_decorators(source, Some("1.5.0"), false, None).unwrap();
        // Functions with version < 1.5.0 should be removed
        assert!(!result.contains("def old_v1"));
        // Functions with version >= 1.5.0 should remain
        assert!(result.contains("def old_v2"));
        assert!(result.contains("def regular_function"));
    }
}
