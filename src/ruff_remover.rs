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

//! Functionality for removing deprecated functions from source code using Ruff parser.

use anyhow::Result;
use ruff_python_ast::{
    self as ast,
    visitor::{self, Visitor},
    Decorator, Mod, Stmt,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashSet;

/// Remove entire functions decorated with @replace_me using Ruff parser
pub struct RuffReplaceRemover<'a> {
    /// Only remove functions with decorators with versions before this.
    before_version: Option<&'a str>,
    /// If true, remove all functions with @replace_me decorators regardless of version.
    remove_all: bool,
    /// Current version to check against remove_in parameter.
    current_version: Option<&'a str>,
    /// Ranges to remove
    ranges_to_remove: Vec<TextRange>,
    /// Track removed function names
    removed_functions: HashSet<String>,
}

impl<'a> RuffReplaceRemover<'a> {
    pub fn new(
        before_version: Option<&'a str>,
        remove_all: bool,
        current_version: Option<&'a str>,
    ) -> Self {
        Self {
            before_version,
            remove_all,
            current_version,
            ranges_to_remove: Vec::new(),
            removed_functions: HashSet::new(),
        }
    }

    pub fn removed_count(&self) -> usize {
        self.removed_functions.len()
    }

    fn should_remove_decorator(&self, decorator: &Decorator) -> bool {
        // Check if this is a replace_me decorator
        let is_replace_me = match &decorator.expression {
            ast::Expr::Name(name) => name.id.as_str() == "replace_me",
            ast::Expr::Call(call) => match &*call.func {
                ast::Expr::Name(name) => name.id.as_str() == "replace_me",
                _ => false,
            },
            _ => false,
        };

        if !is_replace_me {
            return false;
        }

        if self.remove_all {
            return true;
        }

        // Check version constraints if provided
        if let Some(before_version) = self.before_version {
            // Extract version from decorator arguments
            if let ast::Expr::Call(call) = &decorator.expression {
                for keyword in &call.arguments.keywords {
                    if let Some(arg_name) = &keyword.arg {
                        if arg_name.as_str() == "since" {
                            if let ast::Expr::StringLiteral(s) = &keyword.value {
                                let version = s.value.to_str();
                                // Compare versions (simple string comparison for now)
                                return version < before_version;
                            }
                        }
                    }
                }
            }
        }

        // Check remove_in parameter against current version
        if let Some(current_version) = self.current_version {
            if let ast::Expr::Call(call) = &decorator.expression {
                for keyword in &call.arguments.keywords {
                    if let Some(arg_name) = &keyword.arg {
                        if arg_name.as_str() == "remove_in" {
                            if let ast::Expr::StringLiteral(s) = &keyword.value {
                                let remove_in = s.value.to_str();
                                // Compare versions (simple string comparison for now)
                                return current_version >= remove_in;
                            }
                        }
                    }
                }
            }
        }

        false
    }
}

impl<'a> Visitor<'a> for RuffReplaceRemover<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(func_def) => {
                // Check if any decorator is @replace_me
                let has_replace_me = func_def
                    .decorator_list
                    .iter()
                    .any(|dec| self.should_remove_decorator(dec));

                if has_replace_me {
                    // Mark this function for removal
                    self.ranges_to_remove.push(stmt.range());
                    self.removed_functions.insert(func_def.name.to_string());
                    // Don't visit children since we're removing the whole function
                    return;
                }
            }
            Stmt::ClassDef(class_def) => {
                // Visit methods inside the class
                for stmt in &class_def.body {
                    self.visit_stmt(stmt);
                }
                return;
            }
            _ => {}
        }

        visitor::walk_stmt(self, stmt);
    }
}

/// Remove functions decorated with @replace_me from source
pub fn remove_deprecated_functions(
    source: &str,
    before_version: Option<&str>,
    remove_all: bool,
    current_version: Option<&str>,
) -> Result<(usize, String)> {
    use crate::ruff_parser::PythonModule;

    // Parse source with Ruff
    let parsed_module = PythonModule::parse(source)?;

    // Find functions to remove
    let mut remover = RuffReplaceRemover::new(before_version, remove_all, current_version);

    match parsed_module.ast() {
        Mod::Module(module) => {
            for stmt in &module.body {
                remover.visit_stmt(stmt);
            }
        }
        Mod::Expression(_) => {
            // Not handling expression mode
        }
    }

    let removed_count = remover.removed_count();

    if remover.ranges_to_remove.is_empty() {
        return Ok((0, source.to_string()));
    }

    // Sort ranges in reverse order so we can remove from end to start
    let mut ranges = remover.ranges_to_remove;
    ranges.sort_by_key(|b| std::cmp::Reverse(b.start()));

    // Apply removals
    let mut result = source.to_string();
    for range in ranges {
        let start = range.start().to_usize();
        let end = range.end().to_usize();

        // Find the actual line boundaries to remove complete lines
        let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = source[end..]
            .find('\n')
            .map(|i| end + i + 1)
            .unwrap_or(source.len());

        result.replace_range(line_start..line_end, "");
    }

    Ok((removed_count, result))
}
