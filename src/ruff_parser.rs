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

//! Python parser and CST manipulation using Ruff's parser.
//!
//! This module provides a Rust implementation that preserves formatting
//! and integrates with mypy for type inference.

use anyhow::{anyhow, Result};
use ruff_python_ast::{
    visitor::{self, Visitor},
    Expr, Mod,
};
use ruff_python_parser::{parse, Mode, Parsed, Token};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::collections::HashMap;

use crate::core::{CollectorResult, ReplaceInfo};
use crate::types::TypeIntrospectionMethod;

/// Parse Python source code preserving all formatting information
pub struct PythonModule<'a> {
    source: &'a str,
    parsed: Parsed<Mod>,
    /// Map from byte offset to line/column for mypy integration
    position_map: HashMap<u32, (u32, u32)>,
}

impl<'a> PythonModule<'a> {
    /// Parse Python source code
    pub fn parse(source: &'a str) -> Result<Self> {
        let parsed = parse(source, Mode::Module).map_err(|e| anyhow!("Parse error: {:?}", e))?;

        // Build position map for byte offset -> line/column conversion
        let position_map = Self::build_position_map(source);

        Ok(Self {
            source,
            parsed,
            position_map,
        })
    }

    /// Build a map from byte offset to (line, column)
    fn build_position_map(source: &str) -> HashMap<u32, (u32, u32)> {
        let mut map = HashMap::new();
        let mut line = 1;
        let mut col = 0;

        for (offset, ch) in source.char_indices() {
            map.insert(offset as u32, (line, col));
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }

        // Add end position
        map.insert(source.len() as u32, (line, col));

        map
    }

    /// Get AST
    pub fn ast(&self) -> &Mod {
        self.parsed.syntax()
    }

    /// Get all tokens including formatting
    pub fn tokens(&self) -> &[Token] {
        self.parsed.tokens()
    }

    /// Convert byte offset to line/column for mypy
    pub fn offset_to_position(&self, offset: TextSize) -> Option<(u32, u32)> {
        self.position_map.get(&offset.to_u32()).copied()
    }

    /// Convert byte offset to line/column (alias for compatibility)
    pub fn line_col_at_offset(&self, offset: TextSize) -> (u32, u32) {
        self.offset_to_position(offset).unwrap_or((1, 0))
    }

    /// Get text for a range
    pub fn text_at_range(&self, range: TextRange) -> &str {
        &self.source[range.start().to_usize()..range.end().to_usize()]
    }
}

/// Collect deprecated functions using Ruff's AST
/// For now, we delegate to LibCST collector until we implement full extraction
pub fn collect_deprecated_functions(source: &str, module_name: &str) -> Result<CollectorResult> {
    // For now, use LibCST collector
    let collector =
        crate::core::RuffDeprecatedFunctionCollector::new(module_name.to_string(), None);
    collector.collect_from_source(source.to_string())
}

/// Visitor to find and replace function calls
pub struct FunctionCallReplacer<'a> {
    replacements_info: HashMap<String, ReplaceInfo>,
    replacements: Vec<(TextRange, String)>,
    source_module: &'a PythonModule<'a>,
}

impl<'a> FunctionCallReplacer<'a> {
    pub fn new(
        replacements: HashMap<String, ReplaceInfo>,
        source_module: &'a PythonModule<'a>,
        _type_introspection: TypeIntrospectionMethod,
        _file_path: String,
        _module_name: String,
    ) -> Self {
        Self {
            replacements_info: replacements,
            replacements: Vec::new(),
            source_module,
        }
    }

    pub fn get_replacements(self) -> Vec<(TextRange, String)> {
        self.replacements
    }
}

impl<'a> Visitor<'a> for FunctionCallReplacer<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            // Extract the function name being called
            let func_name = match &*call.func {
                Expr::Name(name) => Some(name.id.as_str()),
                Expr::Attribute(attr) => Some(attr.attr.as_str()),
                _ => None,
            };

            if let Some(name) = func_name {
                // Check if this function is deprecated
                if let Some(replace_info) = self.replacements_info.get(name) {
                    // Extract arguments for substitution
                    let mut arg_map = HashMap::new();

                    // Map positional arguments
                    for (i, arg) in call.arguments.args.iter().enumerate() {
                        if let Some(param) = replace_info.parameters.get(i) {
                            let arg_text = self.source_module.text_at_range(arg.range());
                            arg_map.insert(param.name.clone(), arg_text.to_string());
                        }
                    }

                    // Map keyword arguments
                    for keyword in &call.arguments.keywords {
                        if let Some(arg_name) = &keyword.arg {
                            let arg_text = self.source_module.text_at_range(keyword.value.range());
                            arg_map.insert(arg_name.as_str().to_string(), arg_text.to_string());
                        }
                    }

                    // Handle self for method calls
                    if let Expr::Attribute(attr) = &*call.func {
                        let obj_text = self.source_module.text_at_range(attr.value.range());
                        arg_map.insert("self".to_string(), obj_text.to_string());
                    }

                    // Substitute parameters in replacement expression
                    let mut replacement = replace_info.replacement_expr.clone();
                    for (param_name, arg_value) in &arg_map {
                        // Use {param_name} format, not $param_name
                        replacement =
                            replacement.replace(&format!("{{{}}}", param_name), arg_value);
                    }

                    // Add replacement
                    self.replacements.push((call.range(), replacement));
                }
            }
        }

        visitor::walk_expr(self, expr);
    }
}

/// Apply replacements to source code preserving formatting
pub fn apply_replacements(source: &str, mut replacements: Vec<(TextRange, String)>) -> String {
    // Sort replacements by start position (reverse order for applying)
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start()));

    let mut result = source.to_string();

    for (range, replacement) in replacements {
        let start = range.start().to_usize();
        let end = range.end().to_usize();
        let original_text = &source[start..end];
        tracing::debug!(
            "Applying replacement at {}..{}: '{}' -> '{}'",
            start,
            end,
            original_text,
            replacement
        );
        result.replace_range(start..end, &replacement);
    }

    result
}

/// Main entry point for migrating a file using Ruff parser
pub fn migrate_file_with_ruff(
    source: &str,
    module_name: &str,
    file_path: String,
    type_introspection: TypeIntrospectionMethod,
) -> Result<String> {
    // Parse source
    let parsed_module = PythonModule::parse(source)?;

    // Collect deprecated functions
    let collector_result = collect_deprecated_functions(source, module_name)?;

    // Find and replace calls
    let mut replacer = FunctionCallReplacer::new(
        collector_result.replacements,
        &parsed_module,
        type_introspection,
        file_path,
        module_name.to_string(),
    );

    // Visit the AST to find replacements
    match parsed_module.ast() {
        Mod::Module(module) => {
            for stmt in &module.body {
                replacer.visit_stmt(stmt);
            }
        }
        Mod::Expression(_) => {
            // Not handling expression mode
        }
    }

    let replacements = replacer.get_replacements();

    // Apply replacements
    Ok(apply_replacements(source, replacements))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let source = "x = 1\ny = 2";
        let module = PythonModule::parse(source).unwrap();
        assert_eq!(module.ast().as_module().unwrap().body.len(), 2);
    }

    #[test]
    fn test_position_mapping() {
        let source = "x = 1\ny = 2";
        let module = PythonModule::parse(source).unwrap();

        // First line, first column
        assert_eq!(module.offset_to_position(TextSize::new(0)), Some((1, 0)));

        // Second line start
        assert_eq!(module.offset_to_position(TextSize::new(6)), Some((2, 0)));
    }
}
