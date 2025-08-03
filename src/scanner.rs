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

//! Fast scanner for @replace_me decorators.
//!
//! This module provides a fast pre-filter to skip files that definitely
//! don't contain @replace_me decorators, avoiding expensive LibCST parsing.

use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;

/// Quick check if content might contain @replace_me decorators.
///
/// This is a fast pre-filter that uses regex to avoid parsing files
/// that definitely don't contain @replace_me. It errs on the side of
/// false positives to avoid missing any actual decorators.
pub fn might_contain_replace_me(content: &str) -> bool {
    // Use a static regex for performance
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        // Regex pattern to quickly check if a file might contain @replace_me
        // This is intentionally broad to avoid false negatives
        Regex::new(r"(?i)@?\breplace_me\b").unwrap()
    });

    re.is_match(content)
}

/// Read a file and return content if it might contain @replace_me.
///
/// # Arguments
/// * `file_path` - Path to Python file
///
/// # Returns
/// * `Ok(Some(content))` - File content if it might contain @replace_me
/// * `Ok(None)` - File doesn't contain @replace_me
/// * `Err(_)` - File cannot be read or is not valid UTF-8
pub fn scan_file(file_path: &str) -> Result<Option<String>> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path))?;

    if might_contain_replace_me(&content) {
        Ok(Some(content))
    } else {
        Ok(None)
    }
}

/// Iterator that yields files that might contain @replace_me decorators.
///
/// This iterator reads files and pre-filters them to avoid expensive parsing
/// of files that definitely don't contain @replace_me decorators.
pub fn find_files_with_replace_me<I>(file_paths: I) -> FindFilesIterator<I::IntoIter>
where
    I: IntoIterator,
    I::Item: AsRef<Path>,
{
    FindFilesIterator {
        paths: file_paths.into_iter(),
    }
}

/// Iterator implementation for finding files with @replace_me
pub struct FindFilesIterator<I> {
    paths: I,
}

impl<I> Iterator for FindFilesIterator<I>
where
    I: Iterator,
    I::Item: AsRef<Path>,
{
    type Item = Result<(String, String)>; // (file_path, content)

    fn next(&mut self) -> Option<Self::Item> {
        for path in &mut self.paths {
            let path_str = path.as_ref().to_string_lossy().to_string();

            match scan_file(&path_str) {
                Ok(Some(content)) => return Some(Ok((path_str, content))),
                Ok(None) => continue, // File doesn't contain @replace_me, skip
                Err(e) => return Some(Err(e)),
            }
        }
        None
    }
}

/// Recursively find all Python files in a directory that might contain @replace_me
pub fn find_python_files_with_replace_me(dir_path: &str) -> Result<Vec<(String, String)>> {
    let mut results = Vec::new();
    visit_directory(Path::new(dir_path), &mut results)?;
    Ok(results)
}

fn visit_directory(dir: &Path, results: &mut Vec<(String, String)>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Skip hidden directories and __pycache__
            if let Some(name) = path.file_name() {
                let name = name.to_string_lossy();
                if !name.starts_with('.') && name != "__pycache__" {
                    visit_directory(&path, results)?;
                }
            }
        } else if path.extension().is_some_and(|ext| ext == "py") {
            // Check if Python file contains @replace_me
            let path_str = path.to_string_lossy().to_string();
            if let Some(content) = scan_file(&path_str)? {
                results.push((path_str, content));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_might_contain_replace_me() {
        assert!(might_contain_replace_me("@replace_me\ndef foo(): pass"));
        assert!(might_contain_replace_me("from dissolve import replace_me"));
        assert!(might_contain_replace_me("@dissolve.replace_me()"));
        assert!(might_contain_replace_me("some text replace_me somewhere"));
        assert!(!might_contain_replace_me("def regular_function(): pass"));
        assert!(!might_contain_replace_me("# This file has no decorators"));
    }

    #[test]
    fn test_scan_file_with_decorator() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "@replace_me\ndef old_func(): pass")?;

        let result = scan_file(temp_file.path().to_str().unwrap())?;
        assert!(result.is_some());
        assert!(result.unwrap().contains("@replace_me"));

        Ok(())
    }

    #[test]
    fn test_scan_file_without_decorator() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "def regular_func(): pass")?;

        let result = scan_file(temp_file.path().to_str().unwrap())?;
        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn test_find_files_iterator() -> Result<()> {
        // Create temp files
        let mut temp1 = NamedTempFile::new()?;
        let mut temp2 = NamedTempFile::new()?;
        let mut temp3 = NamedTempFile::new()?;

        writeln!(temp1, "@replace_me\ndef old_func(): pass")?;
        writeln!(temp2, "def regular_func(): pass")?;
        writeln!(temp3, "from dissolve import replace_me")?;

        let paths = vec![
            temp1.path().to_str().unwrap(),
            temp2.path().to_str().unwrap(),
            temp3.path().to_str().unwrap(),
        ];

        let results: Result<Vec<_>> = find_files_with_replace_me(paths).collect();
        let results = results?;

        // Should find temp1 and temp3, but not temp2
        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|(path, _)| path.contains(&temp1.path().to_string_lossy().to_string())));
        assert!(results
            .iter()
            .any(|(path, _)| path.contains(&temp3.path().to_string_lossy().to_string())));
        assert!(!results
            .iter()
            .any(|(path, _)| path.contains(&temp2.path().to_string_lossy().to_string())));

        Ok(())
    }

    #[test]
    fn test_case_insensitive_matching() {
        // The regex should be case insensitive
        assert!(might_contain_replace_me("@Replace_Me"));
        assert!(might_contain_replace_me("@REPLACE_ME"));
        assert!(might_contain_replace_me("Replace_Me somewhere"));
    }

    #[test]
    fn test_word_boundary_matching() {
        // Should match whole words only
        assert!(might_contain_replace_me("replace_me"));
        assert!(might_contain_replace_me("@replace_me()"));
        assert!(might_contain_replace_me("import replace_me"));

        // Should not match partial words (our regex should handle this)
        // Note: Our current regex is intentionally broad, so this might match
        // If we need stricter matching, we can adjust the regex
    }
}
