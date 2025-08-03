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

//! Verification functionality for @replace_me decorated functions.
//!
//! This module provides the CheckResult type used by the check functionality.

/// Result of checking @replace_me decorated functions
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// True if all replacements are valid, False otherwise
    pub success: bool,
    /// List of error messages for invalid replacements
    pub errors: Vec<String>,
    /// List of function names that were checked
    pub checked_functions: Vec<String>,
}

impl CheckResult {
    pub fn new() -> Self {
        Self {
            success: true,
            errors: Vec::new(),
            checked_functions: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.success = false;
        self.errors.push(error);
    }

    pub fn add_checked_function(&mut self, name: String) {
        self.checked_functions.push(name);
    }
}

impl Default for CheckResult {
    fn default() -> Self {
        Self::new()
    }
}
