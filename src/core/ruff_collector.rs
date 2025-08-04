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

//! Collection functionality for @replace_me decorated functions using Ruff parser.

use crate::core::types::*;
use anyhow::Result;
use ruff_python_ast::{
    visitor::{self, Visitor},
    Decorator, Expr, Mod, Stmt, StmtClassDef, StmtFunctionDef,
};
use ruff_python_parser::{parse, Mode};
use ruff_text_size::Ranged;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct RuffDeprecatedFunctionCollector {
    module_name: String,
    _file_path: Option<PathBuf>,
    replacements: HashMap<String, ReplaceInfo>,
    unreplaceable: HashMap<String, UnreplaceableNode>,
    imports: Vec<ImportInfo>,
    inheritance_map: HashMap<String, Vec<String>>,
    class_methods: HashMap<String, HashSet<String>>,
    class_stack: Vec<String>,
    source: String,
    builtins: HashSet<String>,
}

impl RuffDeprecatedFunctionCollector {
    pub fn new(module_name: String, file_path: Option<&Path>) -> Self {
        Self {
            module_name,
            _file_path: file_path.map(Path::to_path_buf),
            replacements: HashMap::new(),
            unreplaceable: HashMap::new(),
            imports: Vec::new(),
            inheritance_map: HashMap::new(),
            class_methods: HashMap::new(),
            class_stack: Vec::new(),
            source: String::new(),
            builtins: Self::get_all_builtins(),
        }
    }

    /// Collect from source string
    pub fn collect_from_source(mut self, source: String) -> Result<CollectorResult> {
        self.source = source;
        let parsed = parse(&self.source, Mode::Module)?;

        match parsed.into_syntax() {
            Mod::Module(module) => {
                for stmt in &module.body {
                    self.visit_stmt(stmt);
                }
            }
            Mod::Expression(_) => {
                // Not handling expression mode
            }
        }

        Ok(CollectorResult {
            replacements: self.replacements,
            unreplaceable: self.unreplaceable,
            imports: self.imports,
            inheritance_map: self.inheritance_map,
            class_methods: self.class_methods,
        })
    }

    /// Build the full object path including module and class names
    fn build_full_path(&self, name: &str) -> String {
        let mut parts = Vec::with_capacity(2 + self.class_stack.len());
        parts.push(self.module_name.as_str());
        parts.extend(self.class_stack.iter().map(|s| s.as_str()));
        parts.push(name);
        parts.join(".")
    }

    /// Build a qualified name from an expression (e.g., module.Class)
    fn build_qualified_name_from_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::Name(name) => {
                // Simple name - assume it's in the current module
                format!("{}.{}", self.module_name, name.id)
            }
            Expr::Attribute(attr) => {
                // Handle nested attributes like a.b.c
                // Build the string from right to left to avoid reverse
                let mut result = attr.attr.to_string();
                let mut current = &*attr.value;

                loop {
                    match current {
                        Expr::Name(name) => {
                            // Prepend the final name
                            result = format!("{}.{}", name.id, result);
                            break;
                        }
                        Expr::Attribute(inner_attr) => {
                            // Prepend this attribute
                            result = format!("{}.{}", inner_attr.attr, result);
                            current = &*inner_attr.value;
                        }
                        _ => {
                            // Can't handle this expression type, just return the attribute
                            return attr.attr.to_string();
                        }
                    }
                }

                result
            }
            _ => {
                // Can't handle this expression type
                "Unknown".to_string()
            }
        }
    }

    /// Check if a decorator list contains @replace_me
    fn has_replace_me_decorator(decorators: &[Decorator]) -> bool {
        decorators.iter().any(|dec| match &dec.expression {
            Expr::Name(name) => name.id.as_str() == "replace_me",
            Expr::Call(call) => {
                matches!(&*call.func, Expr::Name(name) if name.id.as_str() == "replace_me")
            }
            _ => false,
        })
    }

    /// Extract the 'since' version from @replace_me decorator
    fn extract_since_version(&self, decorators: &[Decorator]) -> Option<String> {
        self.extract_decorator_version_arg(decorators, "since")
    }

    fn extract_remove_in_version(&self, decorators: &[Decorator]) -> Option<String> {
        self.extract_decorator_version_arg(decorators, "remove_in")
    }

    fn extract_message(decorators: &[Decorator]) -> Option<String> {
        Self::extract_decorator_string_arg(decorators, "message")
    }

    fn extract_decorator_version_arg(
        &self,
        decorators: &[Decorator],
        arg_name: &str,
    ) -> Option<String> {
        for dec in decorators {
            if let Expr::Call(call) = &dec.expression {
                if matches!(&*call.func, Expr::Name(name) if name.id.as_str() == "replace_me") {
                    for keyword in &call.arguments.keywords {
                        if let Some(arg) = &keyword.arg {
                            if arg.as_str() == arg_name {
                                match &keyword.value {
                                    // String literal: "1.2.3"
                                    Expr::StringLiteral(lit) => {
                                        return Some(lit.value.to_string());
                                    }
                                    // Tuple literal: (1, 2, 3) or (1, 2, "final")
                                    Expr::Tuple(tuple) => {
                                        let parts: Vec<String> = tuple
                                            .elts
                                            .iter()
                                            .filter_map(|elt| {
                                                match elt {
                                                    Expr::NumberLiteral(num) => {
                                                        // Extract the number from the source text
                                                        let range = num.range();
                                                        self.source
                                                            .get(
                                                                range.start().to_usize()
                                                                    ..range.end().to_usize(),
                                                            )
                                                            .map(|s| s.to_string())
                                                    }
                                                    Expr::StringLiteral(lit) => {
                                                        Some(lit.value.to_string())
                                                    }
                                                    _ => None,
                                                }
                                            })
                                            .collect();
                                        if !parts.is_empty() {
                                            return Some(parts.join("."));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn extract_decorator_string_arg(decorators: &[Decorator], arg_name: &str) -> Option<String> {
        for dec in decorators {
            if let Expr::Call(call) = &dec.expression {
                if matches!(&*call.func, Expr::Name(name) if name.id.as_str() == "replace_me") {
                    for keyword in &call.arguments.keywords {
                        if let Some(arg) = &keyword.arg {
                            if arg.as_str() == arg_name {
                                if let Expr::StringLiteral(lit) = &keyword.value {
                                    return Some(lit.value.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Extract parameters from a function
    fn extract_parameters(&self, func: &StmtFunctionDef) -> Vec<ParameterInfo> {
        let mut params = Vec::new();

        // Regular parameters
        for param in &func.parameters.args {
            let default_value = param.default.as_ref().map(|default| {
                // Extract the source code of the default value
                let range = default.range();
                self.source
                    .get(range.start().to_usize()..range.end().to_usize())
                    .unwrap_or("")
                    .to_string()
            });

            params.push(ParameterInfo {
                name: param.parameter.name.to_string(),
                has_default: param.default.is_some(),
                default_value,
                is_vararg: false,
                is_kwarg: false,
                is_kwonly: false,
            });
        }

        // *args
        if let Some(vararg) = &func.parameters.vararg {
            params.push(ParameterInfo {
                name: vararg.name.to_string(),
                has_default: false,
                default_value: None,
                is_vararg: true,
                is_kwarg: false,
                is_kwonly: false,
            });
        }

        // Keyword-only parameters
        for param in &func.parameters.kwonlyargs {
            let default_value = param.default.as_ref().map(|default| {
                // Extract the source code of the default value
                let range = default.range();
                self.source
                    .get(range.start().to_usize()..range.end().to_usize())
                    .unwrap_or("")
                    .to_string()
            });

            params.push(ParameterInfo {
                name: param.parameter.name.to_string(),
                has_default: param.default.is_some(),
                default_value,
                is_vararg: false,
                is_kwarg: false,
                is_kwonly: true,
            });
        }

        // **kwargs
        if let Some(kwarg) = &func.parameters.kwarg {
            params.push(ParameterInfo {
                name: kwarg.name.to_string(),
                has_default: false,
                default_value: None,
                is_vararg: false,
                is_kwarg: true,
                is_kwonly: false,
            });
        }

        params
    }

    /// Extract replacement expression from function body
    fn extract_replacement_from_function(
        &self,
        func: &StmtFunctionDef,
    ) -> Result<(String, Expr), ReplacementExtractionError> {
        // Skip docstring and pass statements
        let body_stmts: Vec<&Stmt> = func
            .body
            .iter()
            .skip_while(|stmt| {
                matches!(stmt, Stmt::Expr(expr_stmt) if matches!(&*expr_stmt.value,
                    Expr::StringLiteral(_) | Expr::FString(_)))
            })
            .filter(|stmt| !matches!(stmt, Stmt::Pass(_)))
            .collect();

        if body_stmts.is_empty() {
            // Empty body (possibly with just pass/docstring) is valid - it means remove the function completely
            // Return empty string and a dummy expression (won't be used)
            return Ok((
                "".to_string(),
                Expr::StringLiteral(ruff_python_ast::ExprStringLiteral {
                    value: ruff_python_ast::StringLiteralValue::single(
                        ruff_python_ast::StringLiteral {
                            value: "".into(),
                            flags: ruff_python_ast::StringLiteralFlags::default(),
                            range: ruff_text_size::TextRange::default(),
                        },
                    ),
                    range: ruff_text_size::TextRange::default(),
                }),
            ));
        }

        if body_stmts.len() > 1 {
            return Err(ReplacementExtractionError::new(
                func.name.to_string(),
                ReplacementFailureReason::MultipleStatements,
                "Function body contains multiple statements".to_string(),
            ));
        }

        // Extract return expression
        match body_stmts[0] {
            Stmt::Return(ret_stmt) => {
                if let Some(value) = &ret_stmt.value {
                    // Get function parameters for placeholder conversion
                    let param_names: HashSet<String> = self
                        .extract_parameters(func)
                        .into_iter()
                        .filter(|p| !p.is_vararg && !p.is_kwarg)
                        .map(|p| p.name)
                        .collect();

                    // Convert the AST expression to string with placeholders
                    let replacement_expr =
                        self.expr_to_string_with_placeholders(value, &param_names);

                    tracing::debug!("Extracted replacement expression: {}", replacement_expr);

                    // Also return the AST so we can store it
                    Ok((replacement_expr, (**value).clone()))
                } else {
                    Err(ReplacementExtractionError::new(
                        func.name.to_string(),
                        ReplacementFailureReason::NoReturnStatement,
                        "Return statement has no value".to_string(),
                    ))
                }
            }
            _ => Err(ReplacementExtractionError::new(
                func.name.to_string(),
                ReplacementFailureReason::NoReturnStatement,
                "Function body does not contain a return statement".to_string(),
            )),
        }
    }

    /// Get all builtin names from Python
    fn get_all_builtins() -> HashSet<String> {
        use pyo3::prelude::*;

        Python::with_gil(|py| {
            let mut builtin_names = HashSet::new();

            // Get the builtins module
            if let Ok(builtins) = py.import("builtins") {
                // Get all attributes of the builtins module
                if let Ok(dir_result) = builtins.dir() {
                    // Iterate through the dir() result
                    for item in dir_result.iter() {
                        if let Ok(name_str) = item.extract::<String>() {
                            builtin_names.insert(name_str);
                        }
                    }
                }
            }

            builtin_names
        })
    }

    /// Check if a name is a Python builtin
    fn is_builtin(&self, name: &str) -> bool {
        self.builtins.contains(name)
    }

    fn expr_to_string_with_placeholders(
        &self,
        expr: &Expr,
        param_names: &HashSet<String>,
    ) -> String {
        match expr {
            Expr::Name(name) => {
                let name_str = name.id.to_string();
                if param_names.contains(&name_str) {
                    format!("{{{}}}", name_str)
                } else {
                    name_str
                }
            }
            Expr::Call(call) => {
                // Handle function calls
                let func_str = match &*call.func {
                    // For simple function names, qualify them if needed
                    Expr::Name(name) => {
                        let name_str = name.id.to_string();
                        if param_names.contains(&name_str) {
                            format!("{{{}}}", name_str)
                        } else if self.is_builtin(&name_str) {
                            // Don't qualify builtins
                            name_str
                        } else if !name_str.contains('.') {
                            // Qualify unqualified function names with module name
                            format!("{}.{}", self.module_name, name_str)
                        } else {
                            name_str
                        }
                    }
                    // For attribute access (e.g., self.method or module.func), preserve as-is
                    _ => self.expr_to_string_with_placeholders(&call.func, param_names),
                };
                let mut args = Vec::new();

                // Handle positional arguments
                for arg in &call.arguments.args {
                    args.push(self.expr_to_string_with_placeholders(arg, param_names));
                }

                // Handle keyword arguments
                for keyword in &call.arguments.keywords {
                    if let Some(arg_name) = &keyword.arg {
                        // For keyword arguments, we don't replace the keyword name, only the value
                        let value_str =
                            self.expr_to_string_with_placeholders(&keyword.value, param_names);
                        args.push(format!("{}={}", arg_name, value_str));
                    } else {
                        // **kwargs expansion
                        args.push(format!(
                            "**{}",
                            self.expr_to_string_with_placeholders(&keyword.value, param_names)
                        ));
                    }
                }

                // Check if this is a multi-line call by looking at the original formatting
                let call_range = call.range();
                let original_text =
                    &self.source[call_range.start().to_usize()..call_range.end().to_usize()];

                if original_text.contains('\n') && args.len() > 1 {
                    // Multi-line formatting - preserve the style
                    format!(
                        "{}(\n            {}\n        )",
                        func_str,
                        args.join(",\n            ")
                    )
                } else {
                    // Single line
                    format!("{}({})", func_str, args.join(", "))
                }
            }
            Expr::Attribute(attr) => {
                let value_str = self.expr_to_string_with_placeholders(&attr.value, param_names);
                format!("{}.{}", value_str, attr.attr)
            }
            Expr::Starred(starred) => {
                // Handle *args
                format!(
                    "*{}",
                    self.expr_to_string_with_placeholders(&starred.value, param_names)
                )
            }
            Expr::BinOp(binop) => {
                // Handle binary operations like x * 2, y + 1
                let left = self.expr_to_string_with_placeholders(&binop.left, param_names);
                let right = self.expr_to_string_with_placeholders(&binop.right, param_names);

                // Get the operator string
                let op_str = binop.op.as_str();

                format!("{} {} {}", left, op_str, right)
            }
            Expr::Await(await_expr) => {
                // For await expressions, we extract the inner expression
                // The await will be added back by the migration if needed
                self.expr_to_string_with_placeholders(&await_expr.value, param_names)
            }
            _ => {
                // For other expression types, use the original source text
                let range = expr.range();
                self.source[range.start().to_usize()..range.end().to_usize()].to_string()
            }
        }
    }

    /// Visit a function definition
    fn visit_function(&mut self, func: &StmtFunctionDef) {
        if !Self::has_replace_me_decorator(&func.decorator_list) {
            return;
        }

        let full_path = self.build_full_path(&func.name);
        let parameters = self.extract_parameters(func);
        let since = self.extract_since_version(&func.decorator_list);
        let remove_in = self.extract_remove_in_version(&func.decorator_list);
        let message = Self::extract_message(&func.decorator_list);

        // Determine construct type
        let construct_type = if self.class_stack.is_empty() {
            ConstructType::Function
        } else {
            // Check decorators for special methods
            let decorator_names: Vec<&str> = func
                .decorator_list
                .iter()
                .filter_map(|dec| {
                    if let Expr::Name(name) = &dec.expression {
                        Some(name.id.as_str())
                    } else {
                        None
                    }
                })
                .collect();

            if decorator_names.contains(&"property") {
                ConstructType::Property
            } else if decorator_names.contains(&"classmethod") {
                ConstructType::ClassMethod
            } else if decorator_names.contains(&"staticmethod") {
                ConstructType::StaticMethod
            } else {
                ConstructType::Function
            }
        };

        // Try to extract replacement
        match self.extract_replacement_from_function(func) {
            Ok((replacement_expr, ast)) => {
                let mut replace_info =
                    ReplaceInfo::new(full_path.clone(), replacement_expr, construct_type);
                replace_info.replacement_ast = Some(Box::new(ast));
                replace_info.parameters = parameters;
                replace_info.since = since;
                replace_info.remove_in = remove_in;
                replace_info.message = message;
                self.replacements.insert(full_path, replace_info);
            }
            Err(e) => {
                let unreplaceable = UnreplaceableNode::new(
                    full_path.clone(),
                    e.reason(),
                    e.to_string(),
                    construct_type,
                );
                self.unreplaceable.insert(full_path, unreplaceable);
            }
        }
    }

    /// Visit a class definition
    fn visit_class(&mut self, class_def: &StmtClassDef) {
        let class_name = class_def.name.to_string();
        let full_class_name = self.build_full_path(&class_name);

        // Record base classes
        let mut bases = Vec::new();
        for base in class_def.bases() {
            match base {
                Expr::Name(name) => {
                    // Simple name like BaseRepo
                    // If it's a simple name and likely defined in the same module,
                    // we'll store the fully qualified name
                    let base_name = name.id.to_string();
                    // Check if this is likely a class from the same module
                    // by seeing if we have it in our current class definitions
                    let qualified_name = format!("{}.{}", self.module_name, base_name);
                    bases.push(qualified_name);
                }
                Expr::Attribute(_attr) => {
                    // Qualified name like module.BaseRepo
                    // We need to build the full qualified name from the attribute expression
                    let qualified_name = self.build_qualified_name_from_expr(base);
                    bases.push(qualified_name);
                }
                _ => {
                    // Other base class expressions not handled yet
                }
            }
        }

        if !bases.is_empty() {
            tracing::debug!("Class {} inherits from: {:?}", full_class_name, bases);
            self.inheritance_map.insert(full_class_name.clone(), bases);
        }

        // Check if class itself has @replace_me
        if Self::has_replace_me_decorator(&class_def.decorator_list) {
            // Try to extract replacement from __init__
            if let Some(init_replacement) = self.extract_class_replacement(class_def) {
                let mut replace_info = ReplaceInfo::new(
                    full_class_name.clone(),
                    init_replacement,
                    ConstructType::Class,
                );

                // Extract __init__ parameters
                for stmt in &class_def.body {
                    if let Stmt::FunctionDef(func) = stmt {
                        if func.name.as_str() == "__init__" {
                            replace_info.parameters = self
                                .extract_parameters(func)
                                .into_iter()
                                .filter(|p| p.name != "self")
                                .collect();
                            break;
                        }
                    }
                }

                self.replacements
                    .insert(full_class_name.clone(), replace_info);
            } else {
                // Class has @replace_me but no clear replacement pattern
                let unreplaceable = UnreplaceableNode::new(
                    full_class_name.clone(),
                    ReplacementFailureReason::NoInitMethod,
                    "Class has @replace_me decorator but no __init__ method with clear replacement pattern".to_string(),
                    ConstructType::Class,
                );
                self.unreplaceable
                    .insert(full_class_name.clone(), unreplaceable);
            }
        }

        // Visit class body
        self.class_stack.push(class_name);
        for stmt in &class_def.body {
            self.visit_stmt(stmt);
        }
        self.class_stack.pop();
    }

    /// Extract replacement from class __init__ method
    fn extract_class_replacement(&self, class_def: &StmtClassDef) -> Option<String> {
        // Look for __init__ method
        for stmt in &class_def.body {
            if let Stmt::FunctionDef(func) = stmt {
                if func.name.as_str() == "__init__" {
                    // Look for self.attr = SomeClass(...) pattern
                    for init_stmt in &func.body {
                        if let Stmt::Assign(assign) = init_stmt {
                            if assign.targets.len() == 1 {
                                if let Expr::Attribute(attr) = &assign.targets[0] {
                                    if let Expr::Name(name) = &*attr.value {
                                        if name.id.as_str() == "self" {
                                            // Found self.attr = expr
                                            let range = assign.value.range();
                                            let mut replacement_expr = self.source
                                                [range.start().to_usize()..range.end().to_usize()]
                                                .to_string();

                                            // Convert parameter names to placeholders (like function replacements)
                                            let params = self.extract_parameters(func);
                                            for param in params {
                                                if param.name != "self" {
                                                    let pattern = if param.is_vararg {
                                                        // Match *args
                                                        format!(
                                                            r"\*{}\b",
                                                            regex::escape(&param.name)
                                                        )
                                                    } else if param.is_kwarg {
                                                        // Match **kwargs
                                                        format!(
                                                            r"\*\*{}\b",
                                                            regex::escape(&param.name)
                                                        )
                                                    } else {
                                                        // Match regular parameter
                                                        format!(
                                                            r"\b{}\b",
                                                            regex::escape(&param.name)
                                                        )
                                                    };

                                                    let placeholder = if param.is_vararg {
                                                        format!("*{{{}}}", param.name)
                                                    } else if param.is_kwarg {
                                                        format!("**{{{}}}", param.name)
                                                    } else {
                                                        format!("{{{}}}", param.name)
                                                    };

                                                    replacement_expr = regex::Regex::new(&pattern)
                                                        .unwrap()
                                                        .replace_all(
                                                            &replacement_expr,
                                                            placeholder.as_str(),
                                                        )
                                                        .to_string();
                                                }
                                            }

                                            return Some(replacement_expr);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn visit_ann_assign(&mut self, ann_assign: &ruff_python_ast::StmtAnnAssign) {
        // Handle annotated assignments like DEFAULT_TIMEOUT: int = replace_me(30)
        if let Some(value) = &ann_assign.value {
            if let Expr::Name(name) = ann_assign.target.as_ref() {
                // Check if the assignment value is a replace_me call
                if let Expr::Call(call) = value.as_ref() {
                    if matches!(&*call.func, Expr::Name(func_name) if func_name.id.as_str() == "replace_me")
                    {
                        // Extract the replacement value
                        if let Some(arg) = call.arguments.args.first() {
                            let range = arg.range();
                            let replacement_expr = self.source
                                [range.start().to_usize()..range.end().to_usize()]
                                .to_string();

                            let full_name = if self.class_stack.is_empty() {
                                format!("{}.{}", self.module_name, name.id)
                            } else {
                                format!(
                                    "{}.{}.{}",
                                    self.module_name,
                                    self.class_stack.join("."),
                                    name.id
                                )
                            };

                            // Extract version information from keyword arguments
                            let since = self.extract_since_version(&[]);
                            let remove_in = self.extract_remove_in_version(&[]);
                            let message = Self::extract_message(&[]);

                            // Parse the replacement expression to get its AST
                            let replacement_ast = if let Ok(parsed) =
                                ruff_python_parser::parse_expression(&replacement_expr)
                            {
                                Some(Box::new(parsed.into_expr()))
                            } else {
                                None
                            };

                            let construct_type = if self.class_stack.is_empty() {
                                ConstructType::ModuleAttribute
                            } else {
                                ConstructType::ClassAttribute
                            };

                            let replace_info = ReplaceInfo {
                                old_name: full_name.clone(),
                                replacement_expr,
                                replacement_ast,
                                construct_type,
                                parameters: vec![], // Module attributes don't have parameters
                                return_type: None,
                                since,
                                remove_in,
                                message,
                            };

                            self.replacements.insert(full_name, replace_info);
                        }
                    }
                }
            }
        }
    }

    fn visit_assign(&mut self, assign: &ruff_python_ast::StmtAssign) {
        // Handle module-level assignments like OLD_CONSTANT = replace_me(42)
        if assign.targets.len() == 1 {
            if let Expr::Name(name) = &assign.targets[0] {
                // Check if the assignment value is a replace_me call
                if let Expr::Call(call) = assign.value.as_ref() {
                    if matches!(&*call.func, Expr::Name(func_name) if func_name.id.as_str() == "replace_me")
                    {
                        // Extract the replacement value
                        if let Some(arg) = call.arguments.args.first() {
                            let range = arg.range();
                            let replacement_expr = self.source
                                [range.start().to_usize()..range.end().to_usize()]
                                .to_string();

                            let full_name = if self.class_stack.is_empty() {
                                format!("{}.{}", self.module_name, name.id)
                            } else {
                                format!(
                                    "{}.{}.{}",
                                    self.module_name,
                                    self.class_stack.join("."),
                                    name.id
                                )
                            };

                            // Extract version information from keyword arguments
                            let since = self.extract_since_version(&[]);
                            let remove_in = self.extract_remove_in_version(&[]);
                            let message = Self::extract_message(&[]);

                            // Parse the replacement expression to get its AST
                            let replacement_ast = if let Ok(parsed) =
                                ruff_python_parser::parse_expression(&replacement_expr)
                            {
                                Some(Box::new(parsed.into_expr()))
                            } else {
                                None
                            };

                            let construct_type = if self.class_stack.is_empty() {
                                ConstructType::ModuleAttribute
                            } else {
                                ConstructType::ClassAttribute
                            };

                            let replace_info = ReplaceInfo {
                                old_name: full_name.clone(),
                                replacement_expr,
                                replacement_ast,
                                construct_type,
                                parameters: vec![], // Module/class attributes don't have parameters
                                return_type: None,
                                since,
                                remove_in,
                                message,
                            };

                            self.replacements.insert(full_name, replace_info);
                        }
                    }
                }
            }
        }
    }
}

impl Visitor<'_> for RuffDeprecatedFunctionCollector {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(func) => self.visit_function(func),
            Stmt::ClassDef(class) => self.visit_class(class),
            Stmt::Import(import) => {
                // Record imports
                for alias in &import.names {
                    self.imports.push(ImportInfo::new(
                        alias.name.to_string(),
                        vec![(
                            alias.name.to_string(),
                            alias.asname.as_ref().map(|n| n.to_string()),
                        )],
                    ));
                }
            }
            Stmt::ImportFrom(import) => {
                let names: Vec<(String, Option<String>)> = import
                    .names
                    .iter()
                    .map(|alias| {
                        (
                            alias.name.to_string(),
                            alias.asname.as_ref().map(|n| n.to_string()),
                        )
                    })
                    .collect();

                // Handle both absolute and relative imports
                let module_name = if let Some(module) = &import.module {
                    // Add dots for relative imports
                    let dots = ".".repeat(import.level as usize);
                    format!("{}{}", dots, module)
                } else {
                    // Pure relative import like "from . import x"
                    ".".repeat(import.level as usize)
                };

                self.imports.push(ImportInfo::new(module_name, names));
            }
            Stmt::Assign(assign) => {
                // Handle module-level assignments with replace_me calls
                self.visit_assign(assign);
                visitor::walk_stmt(self, stmt);
            }
            Stmt::AnnAssign(ann_assign) => {
                // Handle annotated assignments like DEFAULT_TIMEOUT: int = replace_me(30)
                self.visit_ann_assign(ann_assign);
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

impl ReplacementExtractionError {
    fn reason(&self) -> ReplacementFailureReason {
        match self {
            Self::ExtractionFailed { reason, .. } => reason.clone(),
        }
    }
}
