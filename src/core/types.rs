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

use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructType {
    Function,
    Property,
    ClassMethod,
    StaticMethod,
    AsyncFunction,
    Class,
    ClassAttribute,
    ModuleAttribute,
}

impl ConstructType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConstructType::Function => "Function",
            ConstructType::Property => "Property",
            ConstructType::ClassMethod => "Class method",
            ConstructType::StaticMethod => "Static method",
            ConstructType::AsyncFunction => "Async function",
            ConstructType::Class => "Class",
            ConstructType::ClassAttribute => "Class attribute",
            ConstructType::ModuleAttribute => "Module attribute",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub has_default: bool,
    pub default_value: Option<String>, // The actual default value as source code
    pub is_vararg: bool,               // *args
    pub is_kwarg: bool,                // **kwargs
    pub is_kwonly: bool,               // keyword-only parameter
}

impl ParameterInfo {
    pub fn new(name: String) -> Self {
        Self {
            name,
            has_default: false,
            default_value: None,
            is_vararg: false,
            is_kwarg: false,
            is_kwonly: false,
        }
    }

    /// Create from a string slice to avoid unnecessary allocations when possible
    pub fn from_name(name: &str) -> Self {
        Self::new(name.to_string())
    }

    /// Create a vararg parameter (*args)
    pub fn vararg(name: &str) -> Self {
        Self {
            name: name.to_string(),
            has_default: false,
            default_value: None,
            is_vararg: true,
            is_kwarg: false,
            is_kwonly: false,
        }
    }

    /// Create a kwarg parameter (**kwargs)
    pub fn kwarg(name: &str) -> Self {
        Self {
            name: name.to_string(),
            has_default: false,
            default_value: None,
            is_vararg: false,
            is_kwarg: true,
            is_kwonly: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReplaceInfo {
    pub old_name: String,
    pub replacement_expr: String, // String representation with placeholders (for backward compatibility)
    pub replacement_ast: Option<Box<ruff_python_ast::Expr>>, // The actual AST expression
    pub construct_type: ConstructType,
    pub parameters: Vec<ParameterInfo>,
    pub return_type: Option<String>,
    pub since: Option<String>,
    pub remove_in: Option<String>,
    pub message: Option<String>,
}

impl ReplaceInfo {
    pub fn new(old_name: String, replacement_expr: String, construct_type: ConstructType) -> Self {
        Self {
            old_name,
            replacement_expr,
            replacement_ast: None,
            construct_type,
            parameters: Vec::new(),
            return_type: None,
            since: None,
            remove_in: None,
            message: None,
        }
    }

    /// Create from string slices to avoid unnecessary allocations when possible
    pub fn from_strs(old_name: &str, replacement_expr: &str, construct_type: ConstructType) -> Self {
        Self::new(old_name.to_string(), replacement_expr.to_string(), construct_type)
    }

    /// Builder pattern for setting optional fields
    pub fn with_since(mut self, since: &str) -> Self {
        self.since = Some(since.to_string());
        self
    }

    pub fn with_remove_in(mut self, remove_in: &str) -> Self {
        self.remove_in = Some(remove_in.to_string());
        self
    }

    pub fn with_message(mut self, message: &str) -> Self {
        self.message = Some(message.to_string());
        self
    }

    pub fn with_parameters(mut self, parameters: Vec<ParameterInfo>) -> Self {
        self.parameters = parameters;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplacementFailureReason {
    ComplexBody,
    NoReturnStatement,
    EmptyBody,
    MultipleStatements,
    InvalidPattern,
    NoInitMethod,
}

#[derive(Debug, Clone)]
pub struct UnreplaceableNode {
    pub old_name: String,
    pub reason: ReplacementFailureReason,
    pub message: String,
    pub construct_type: ConstructType,
}

impl UnreplaceableNode {
    pub fn new(
        old_name: String,
        reason: ReplacementFailureReason,
        message: String,
        construct_type: ConstructType,
    ) -> Self {
        Self {
            old_name,
            reason,
            message,
            construct_type,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub module: String,
    pub names: Vec<(String, Option<String>)>, // (name, alias)
}

impl ImportInfo {
    pub fn new(module: String, names: Vec<(String, Option<String>)>) -> Self {
        Self { module, names }
    }
}

#[derive(Error, Debug)]
pub enum ReplacementExtractionError {
    #[error("Failed to extract replacement for {name}: {details}")]
    ExtractionFailed {
        name: String,
        reason: ReplacementFailureReason,
        details: String,
    },
}

impl ReplacementExtractionError {
    pub fn new(name: String, reason: ReplacementFailureReason, details: String) -> Self {
        Self::ExtractionFailed {
            name,
            reason,
            details,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CollectorResult {
    pub replacements: HashMap<String, ReplaceInfo>,
    pub unreplaceable: HashMap<String, UnreplaceableNode>,
    pub imports: Vec<ImportInfo>,
    pub inheritance_map: HashMap<String, Vec<String>>,
    pub class_methods: HashMap<String, HashSet<String>>,
}

impl Default for CollectorResult {
    fn default() -> Self {
        Self::new()
    }
}

impl CollectorResult {
    pub fn new() -> Self {
        Self {
            replacements: HashMap::new(),
            unreplaceable: HashMap::new(),
            imports: Vec::new(),
            inheritance_map: HashMap::new(),
            class_methods: HashMap::new(),
        }
    }
}
