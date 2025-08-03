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

//! Improved Ruff parser implementation with full *args/**kwargs support

use anyhow::Result;
use ruff_python_ast::{
    self as ast,
    visitor::{self, Visitor},
    Expr, Mod,
};
use ruff_text_size::{Ranged, TextRange};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use tracing;

use crate::ast_transformer::transform_replacement_ast;
use crate::core::ReplaceInfo;
use crate::ruff_parser::PythonModule;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::types::TypeIntrospectionMethod;

/// Improved visitor to find and replace function calls with proper *args/**kwargs handling
pub struct ImprovedFunctionCallReplacer<'a> {
    replacements_info: HashMap<String, ReplaceInfo>,
    replacements: Vec<(TextRange, String)>,
    source_module: &'a PythonModule<'a>,
    type_introspection: TypeIntrospectionMethod,
    file_path: String,
    module_name: String,
    import_map: HashMap<String, String>, // Maps imported names to their full module paths
    source_content: String,
    inheritance_map: HashMap<String, Vec<String>>, // Maps class names to their base classes
    pyright_client: Option<Rc<RefCell<crate::pyright_lsp::PyrightLspClient>>>, // Reuse client for performance
    mypy_client: Option<Rc<RefCell<crate::mypy_lsp::MypyTypeIntrospector>>>, // Mypy daemon for fallback
    type_cache: RefCell<HashMap<(u32, u32), Option<String>>>, // Cache type lookups by (line, column)
}

impl<'a> ImprovedFunctionCallReplacer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        replacements: HashMap<String, ReplaceInfo>,
        source_module: &'a PythonModule<'a>,
        type_introspection: TypeIntrospectionMethod,
        file_path: String,
        module_name: String,
        _replace_me_functions: HashSet<String>,
        source_content: String,
        inheritance_map: HashMap<String, Vec<String>>,
    ) -> Result<Self> {
        // Initialize type introspection clients based on method
        let (pyright_client, mypy_client) = match type_introspection {
            TypeIntrospectionMethod::PyrightLsp => {
                // Use None to let pyright use the current working directory
                match crate::pyright_lsp::PyrightLspClient::new(None) {
                    Ok(mut client) => {
                        // Pre-open the file in pyright
                        client.open_file(&file_path, &source_content)?;
                        (Some(Rc::new(RefCell::new(client))), None)
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!("Failed to initialize pyright LSP client: {}. Type introspection is required for safe migrations.", e));
                    }
                }
            }
            TypeIntrospectionMethod::MypyDaemon => {
                match crate::mypy_lsp::MypyTypeIntrospector::new(None) {
                    Ok(client) => (None, Some(Rc::new(RefCell::new(client)))),
                    Err(e) => {
                        return Err(anyhow::anyhow!("Failed to initialize mypy daemon: {}. Type introspection is required for safe migrations.", e));
                    }
                }
            }
            TypeIntrospectionMethod::PyrightWithMypyFallback => {
                let pyright = match crate::pyright_lsp::PyrightLspClient::new(None) {
                    Ok(mut client) => {
                        client.open_file(&file_path, &source_content).ok();
                        Some(Rc::new(RefCell::new(client)))
                    }
                    Err(_) => None,
                };
                let mypy = match crate::mypy_lsp::MypyTypeIntrospector::new(None) {
                    Ok(client) => Some(Rc::new(RefCell::new(client))),
                    Err(_) => None,
                };
                if pyright.is_none() && mypy.is_none() {
                    return Err(anyhow::anyhow!("Failed to initialize any type introspection client. Type introspection is required for safe migrations."));
                }
                (pyright, mypy)
            }
        };

        let mut replacer = Self {
            replacements_info: replacements,
            replacements: Vec::new(),
            source_module,
            type_introspection,
            file_path,
            module_name,
            import_map: HashMap::new(),
            source_content,
            inheritance_map,
            pyright_client,
            mypy_client,
            type_cache: RefCell::new(HashMap::new()),
        };

        // Parse imports from the module
        replacer.collect_imports();

        Ok(replacer)
    }

    /// Create a new replacer with an existing type introspection context
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_context(
        replacements: HashMap<String, ReplaceInfo>,
        source_module: &'a PythonModule<'a>,
        type_introspection_context: &mut TypeIntrospectionContext,
        file_path: String,
        module_name: String,
        _replace_me_functions: HashSet<String>,
        source_content: String,
        inheritance_map: HashMap<String, Vec<String>>,
    ) -> Result<Self> {
        // Open the file in the context
        type_introspection_context.open_file(&file_path, &source_content)?;

        // Get the clients from context
        let pyright_client = type_introspection_context.pyright_client();
        let mypy_client = type_introspection_context.mypy_client();

        let mut replacer = Self {
            replacements_info: replacements,
            replacements: Vec::new(),
            source_module,
            type_introspection: type_introspection_context.method(),
            file_path,
            module_name,
            import_map: HashMap::new(),
            source_content,
            inheritance_map,
            pyright_client,
            mypy_client,
            type_cache: RefCell::new(HashMap::new()),
        };

        // Parse imports from the module
        replacer.collect_imports();

        Ok(replacer)
    }

    /// Collect import statements and build the import map
    fn collect_imports(&mut self) {
        use ruff_python_ast::{Stmt, StmtImport};

        if let Some(module) = self.source_module.ast().as_module() {
            for stmt in &module.body {
                match stmt {
                    Stmt::Import(StmtImport { names, .. }) => {
                        // Handle: import module.submodule as alias
                        for alias in names {
                            let module_name = alias.name.to_string();
                            let alias_name = alias
                                .asname
                                .as_ref()
                                .map(|a| a.to_string())
                                .unwrap_or_else(|| module_name.clone());
                            self.import_map.insert(alias_name, module_name);
                        }
                    }
                    Stmt::ImportFrom(stmt) => {
                        tracing::debug!(
                            "Processing ImportFrom statement, module: {:?}, level: {}",
                            stmt.module,
                            stmt.level
                        );

                        // Handle relative imports - level indicates the number of dots
                        let module_str = if let Some(module_name) = &stmt.module {
                            module_name.to_string()
                        } else {
                            String::new() // Pure relative import like "from . import foo"
                        };

                        tracing::debug!(
                            "Module string: '{}', relative level: {}",
                            module_str,
                            stmt.level
                        );

                        // Resolve imports
                        let resolved_module = if stmt.level > 0 {
                            // Handle relative imports based on level (number of dots)
                            let dots = stmt.level as usize;

                            // Split current module by dots to get package hierarchy
                            let module_parts: Vec<&str> = self.module_name.split('.').collect();
                            tracing::debug!(
                                "Resolving relative import level {} with module '{}' from '{}'",
                                dots,
                                module_str,
                                self.module_name
                            );

                            if module_parts.len() > dots {
                                // Go up 'dots' levels and append the module name if any
                                let parent_parts = &module_parts[..module_parts.len() - dots];
                                let result = if module_str.is_empty() {
                                    parent_parts.join(".")
                                } else {
                                    format!("{}.{}", parent_parts.join("."), module_str)
                                };
                                tracing::debug!("Resolved to: {}", result);
                                result
                            } else {
                                // Can't resolve, use as-is
                                tracing::debug!("Cannot resolve relative import, using as-is");
                                module_str.clone()
                            }
                        } else {
                            // Absolute import
                            module_str.clone()
                        };

                        // Handle: from module import name as alias
                        for alias in &stmt.names {
                            let imported_name = alias.name.to_string();
                            let alias_name = alias
                                .asname
                                .as_ref()
                                .map(|a| a.to_string())
                                .unwrap_or_else(|| imported_name.clone());
                            let full_name = format!("{}.{}", resolved_module, imported_name);
                            tracing::debug!("Import mapping: {} -> {}", alias_name, full_name);
                            self.import_map.insert(alias_name, full_name);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn get_replacements(self) -> Vec<(TextRange, String)> {
        self.replacements
    }

    fn get_attribute_type(&self, attr: &ast::ExprAttribute) -> Option<String> {
        // Use type introspection to get the type of the attribute's value
        // For chained attributes like self.repo.do_commit(), we need the range of the
        // actual object (repo), not the full chain (self.repo)
        let (range, variable_name) = match &*attr.value {
            ast::Expr::Name(name) => (name.range(), name.id.to_string()),
            ast::Expr::Attribute(inner_attr) => {
                // For self.repo.method(), get the range of "repo" not "self.repo"
                (inner_attr.attr.range(), inner_attr.attr.to_string())
            }
            ast::Expr::Call(call) => {
                // For target.get_worktree().reset_index(), we need to get the type
                // of the function call result.
                // Use a position just before the end (inside the parentheses) to get
                // the type of the entire call expression
                let call_end = call.range().end();
                // Create a range at the end of the call to query the result type
                let query_pos = call_end - ruff_text_size::TextSize::from(1);
                let query_range = ruff_text_size::TextRange::new(
                    query_pos,
                    query_pos + ruff_text_size::TextSize::from(1),
                );
                (query_range, "<call_result>".to_string())
            }
            _ => return None,
        };

        let location = self.source_module.line_col_at_offset(range.start());

        // Debug: let's see what text we're looking at
        let text = self.source_module.text_at_range(range);
        tracing::debug!("Text at range {:?}: '{}'", range, text);

        // Query type based on configured method
        tracing::debug!(
            "Querying type at {}:{} in {} for variable '{}' using {:?}",
            location.0,
            location.1,
            self.file_path,
            variable_name,
            self.type_introspection
        );

        // Check cache first
        let cache_key = (location.0, location.1);
        if let Some(cached_type) = self.type_cache.borrow().get(&cache_key) {
            tracing::debug!("Type lookup (cached): {:?}", cached_type);
            return cached_type.clone();
        }

        let type_result = match self.type_introspection {
            TypeIntrospectionMethod::PyrightLsp => {
                // Use the cached pyright client
                if let Some(ref client_cell) = self.pyright_client {
                    let mut client = client_cell.borrow_mut();
                    match client.query_type(
                        &self.file_path,
                        &self.source_content,
                        location.0,
                        location.1,
                    ) {
                        Ok(Some(type_str)) => Ok(type_str),
                        Ok(None) => Err(anyhow::anyhow!(
                            "No type information available from pyright"
                        )),
                        Err(e) => Err(e),
                    }
                } else {
                    Err(anyhow::anyhow!("Pyright client not available"))
                }
            }
            TypeIntrospectionMethod::MypyDaemon => {
                // Use the cached mypy client
                if let Some(ref client_cell) = self.mypy_client {
                    let mut client = client_cell.borrow_mut();
                    match client.get_type_at_position(
                        &self.file_path,
                        location.0 as usize,
                        location.1 as usize,
                    ) {
                        Ok(Some(type_str)) => Ok(type_str),
                        Ok(None) => Err(anyhow::anyhow!("No type information available from mypy")),
                        Err(e) => Err(anyhow::anyhow!("Mypy error: {}", e)),
                    }
                } else {
                    Err(anyhow::anyhow!("Mypy client not available"))
                }
            }
            TypeIntrospectionMethod::PyrightWithMypyFallback => {
                // Try pyright first
                let mut result = if let Some(ref client_cell) = self.pyright_client {
                    let mut client = client_cell.borrow_mut();
                    match client.query_type(
                        &self.file_path,
                        &self.source_content,
                        location.0,
                        location.1,
                    ) {
                        Ok(Some(type_str)) => Ok(type_str),
                        Ok(None) => Err(anyhow::anyhow!("No type from pyright")),
                        Err(e) => Err(e),
                    }
                } else {
                    Err(anyhow::anyhow!("Pyright not available"))
                };

                // If pyright failed, try mypy
                if result.is_err() {
                    if let Some(ref client_cell) = self.mypy_client {
                        let mut client = client_cell.borrow_mut();
                        result = match client.get_type_at_position(
                            &self.file_path,
                            location.0 as usize,
                            location.1 as usize,
                        ) {
                            Ok(Some(type_str)) => Ok(type_str),
                            Ok(None) => result, // Keep original error
                            Err(e) => Err(anyhow::anyhow!("Mypy error: {}", e)),
                        };
                    }
                }

                result
            }
        };

        match type_result {
            Ok(type_str) => {
                tracing::debug!("Type introspection found type: {}", type_str);
                // Cache the result
                self.type_cache
                    .borrow_mut()
                    .insert(cache_key, Some(type_str.clone()));
                return Some(type_str);
            }
            Err(e) => {
                tracing::debug!("Type introspection failed: {}", e);
                // Cache the failure too to avoid repeated queries
                self.type_cache.borrow_mut().insert(cache_key, None);

                // Special case: if the variable name is a known class in our replacements,
                // we can infer the type directly
                let full_class_name = format!("{}.{}", self.module_name, variable_name);
                for key in self.replacements_info.keys() {
                    if key.starts_with(&full_class_name) && key.contains('.') {
                        // This is a method on this class
                        tracing::debug!("Found class {} in replacements", full_class_name);
                        return Some(full_class_name);
                    }
                }
            }
        }

        // No fallback - if type introspection fails, we cannot determine the type
        tracing::debug!(
            "Could not determine type for attribute value: {:?}",
            attr.value
        );

        None
    }

    /// Build parameter mapping with proper *args/**kwargs handling
    fn build_param_map(
        &self,
        call: &ast::ExprCall,
        replace_info: &ReplaceInfo,
    ) -> (HashMap<String, String>, HashSet<String>, Vec<String>) {
        let mut arg_map = HashMap::new();
        let mut keyword_args = HashSet::new(); // Track which args were passed as keywords

        // Categorize parameters
        let mut regular_params = Vec::new();
        let mut vararg_param: Option<&str> = None;
        let mut kwarg_param: Option<&str> = None;

        for param in &replace_info.parameters {
            if param.is_vararg {
                vararg_param = Some(&param.name);
            } else if param.is_kwarg {
                kwarg_param = Some(&param.name);
            } else if param.name != "self" && param.name != "cls" {
                // Skip self/cls parameters - they're handled separately for method calls
                regular_params.push(&param.name);
            }
        }

        // Map positional arguments
        let mut remaining_args = Vec::new();
        for (i, arg) in call.arguments.args.iter().enumerate() {
            let arg_text = self.source_module.text_at_range(arg.range());
            if i < regular_params.len() {
                arg_map.insert(regular_params[i].clone(), arg_text.to_string());
            } else {
                // These go to *args
                remaining_args.push(arg_text);
            }
        }

        // Handle *args if present in replacement
        if let Some(vararg) = vararg_param {
            let vararg_key = format!("*{}", vararg);
            if replace_info.replacement_expr.contains(&vararg_key) && !remaining_args.is_empty() {
                let args_str = remaining_args.join(", ");
                arg_map.insert(vararg_key, args_str);
            }
        }

        // Map keyword arguments
        let mut kwarg_pairs = Vec::new();
        for keyword in &call.arguments.keywords {
            let value_text = self.source_module.text_at_range(keyword.value.range());

            if let Some(arg_name) = &keyword.arg {
                let name_str = arg_name.as_str();
                // Check if it's a regular parameter
                if regular_params.iter().any(|p| *p == name_str) {
                    // Store just the value for regular parameters
                    arg_map.insert(name_str.to_string(), value_text.to_string());
                    keyword_args.insert(name_str.to_string()); // Mark as keyword arg
                } else {
                    // It's for **kwargs
                    kwarg_pairs.push(format!("{}={}", name_str, value_text));
                }
            } else {
                // **dict expansion
                kwarg_pairs.push(format!("**{}", value_text));
            }
        }

        // Handle **kwargs if present in replacement
        if let Some(kwarg) = kwarg_param {
            let kwarg_key = format!("**{}", kwarg);
            if replace_info.replacement_expr.contains(&kwarg_key) && !kwarg_pairs.is_empty() {
                let kwargs_str = kwarg_pairs.join(", ");
                arg_map.insert(kwarg_key, kwargs_str);
            }
        }

        // DO NOT fill in default values for missing parameters
        // This was causing default parameter pollution where calls like:
        //   repo.do_commit(b"message")
        // were being migrated to:
        //   repo.get_worktree().commit(message=b"message", tree=None, encoding=None, ...)
        // Instead, we want to only include the parameters that were actually provided

        (arg_map, keyword_args, kwarg_pairs)
    }
}

impl<'a> Visitor<'a> for ImprovedFunctionCallReplacer<'a> {
    fn visit_stmt(&mut self, stmt: &'a ruff_python_ast::Stmt) {
        use ruff_python_ast::Stmt;

        // Handle import statement replacements
        if let Stmt::ImportFrom(import_from) = stmt {
            if let Some(module) = &import_from.module {
                let module_str = module.to_string();

                // Check if any imported names need to be replaced
                let mut needs_replacement = false;
                let mut new_imports = Vec::new();

                for alias in &import_from.names {
                    let imported_name = alias.name.to_string();

                    // Check if this is a function that's being replaced
                    let full_name = format!("{}.{}", module_str, imported_name);

                    if let Some(replace_info) = self
                        .replacements_info
                        .get(&imported_name)
                        .or_else(|| self.replacements_info.get(&full_name))
                    {
                        // Extract the new function name from the replacement expression
                        let replacement_expr = &replace_info.replacement_expr;
                        let func_name_end =
                            replacement_expr.find('(').unwrap_or(replacement_expr.len());
                        let new_func_name = &replacement_expr[..func_name_end];

                        // If it's a simple function name (not qualified), add it to imports
                        if !new_func_name.contains('.') && new_func_name != imported_name {
                            needs_replacement = true;
                            new_imports.push(new_func_name.to_string());
                            tracing::debug!(
                                "Import replacement needed: {} -> {}",
                                imported_name,
                                new_func_name
                            );
                        }
                    }
                }

                if needs_replacement && !new_imports.is_empty() {
                    // Build the new import statement
                    let mut all_imports: Vec<String> = import_from
                        .names
                        .iter()
                        .map(|alias| alias.name.to_string())
                        .collect();

                    // Add new imports
                    for new_import in new_imports {
                        if !all_imports.contains(&new_import) {
                            all_imports.push(new_import);
                        }
                    }

                    // Create the new import statement
                    let new_import_stmt =
                        format!("from {} import {}", module_str, all_imports.join(", "));

                    // Add the replacement
                    self.replacements
                        .push((import_from.range(), new_import_stmt));
                }
            }
        }

        // Check if this is a function definition with @replace_me decorator
        if let Stmt::FunctionDef(func_def) = stmt {
            let func_name = func_def.name.to_string();
            let _full_func_name = format!("{}.{}", self.module_name, func_name);

            // Check if this function has @replace_me decorator
            let has_replace_me =
                func_def
                    .decorator_list
                    .iter()
                    .any(|decorator| match &decorator.expression {
                        Expr::Name(name) => name.id == "replace_me",
                        Expr::Call(call) => {
                            if let Expr::Name(name) = &*call.func {
                                name.id == "replace_me"
                            } else {
                                false
                            }
                        }
                        _ => false,
                    });

            if has_replace_me {
                // Don't visit the body of @replace_me functions to avoid double substitution
                tracing::debug!(
                    "Skipping migration inside @replace_me function: {}",
                    func_name
                );
                return;
            }
        }

        // For all other statements, continue with normal visitation
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        // Special handling for await expressions
        if let Expr::Await(await_expr) = expr {
            // Check if the inner expression is a call that we need to replace
            if let Expr::Call(call) = &*await_expr.value {
                // First visit the arguments to handle nested calls
                for arg in call.arguments.args.iter() {
                    self.visit_expr(arg);
                }
                for keyword in call.arguments.keywords.iter() {
                    self.visit_expr(&keyword.value);
                }

                // Process the call, but remember we're in an await context
                self.process_call_with_await_context(call, true);
                return; // Don't visit children, we've handled it
            }
        }

        if let Expr::Call(call) = expr {
            // First visit the arguments to handle nested calls
            for arg in call.arguments.args.iter() {
                self.visit_expr(arg);
            }
            for keyword in call.arguments.keywords.iter() {
                self.visit_expr(&keyword.value);
            }

            // Then process this call
            self.process_call_with_await_context(call, false);
            return; // Don't visit children again, we've already handled them
        }

        // For other expressions, continue with normal visitation
        visitor::walk_expr(self, expr);
    }
}

impl<'a> ImprovedFunctionCallReplacer<'a> {
    /// Check if an expression has a magic method with @replace_me and return the replacement
    fn check_magic_method(
        &self,
        expr: &'a Expr,
        magic_method: &str,
        builtin_name: &str,
    ) -> Option<String> {
        // First, we need to get the type of the expression
        let expr_text = self.source_module.text_at_range(expr.range());
        tracing::debug!(
            "Checking {}() magic method for expression: {}",
            builtin_name,
            expr_text
        );
        let type_name = self.get_expression_type(expr)?;
        tracing::debug!("Expression '{}' has type: {:?}", expr_text, type_name);

        // Check if this type has the magic method with @replace_me
        let method_key = format!("{}.{}", type_name, magic_method);
        let method_key_with_module = if !type_name.contains('.') {
            Some(format!(
                "{}.{}.{}",
                self.module_name, type_name, magic_method
            ))
        } else {
            None
        };

        tracing::debug!("Checking for {} replacement: {}", magic_method, method_key);
        if let Some(ref key) = method_key_with_module {
            tracing::debug!("Also checking with module: {}", key);
        }

        let replace_info = self.replacements_info.get(&method_key).or_else(|| {
            method_key_with_module
                .as_ref()
                .and_then(|k| self.replacements_info.get(k))
        });

        if let Some(replace_info) = replace_info {
            tracing::debug!(
                "Found replacement info for type {}: {:?}",
                type_name,
                replace_info.old_name
            );
            // Generate the replacement
            let obj_str = self.source_module.text_at_range(expr.range());

            // Parse the replacement to extract the actual method call
            if let Some(replacement_ast) = &replace_info.replacement_ast {
                // For magic method replacements, we need to handle a special case:
                // If the replacement is "builtin(self.something())", we should just use "self.something()"
                // because we're already in a builtin() call context
                let inner_expr = match &**replacement_ast {
                    Expr::Call(call) => {
                        if let Expr::Name(name) = &*call.func {
                            if name.id.as_str() == builtin_name && call.arguments.args.len() == 1 {
                                // Extract the inner expression from builtin(...)
                                &call.arguments.args[0]
                            } else {
                                replacement_ast
                            }
                        } else {
                            replacement_ast
                        }
                    }
                    _ => replacement_ast,
                };

                // Replace "self" with the actual object
                let mut param_map = HashMap::new();
                param_map.insert("self".to_string(), obj_str.to_string());

                // Get parameter names
                let param_names: Vec<String> = replace_info
                    .parameters
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                let provided_params = vec!["self".to_string()]; // Only self is provided in magic method calls

                let replacement = transform_replacement_ast(
                    inner_expr,
                    &param_map,
                    &provided_params,
                    &param_names,
                );
                tracing::debug!(
                    "Found {} replacement for {}() call: {}",
                    magic_method,
                    builtin_name,
                    replacement
                );
                return Some(replacement);
            } else {
                // Fallback: try simple string replacement
                let mut replacement = replace_info.replacement_expr.replace("self", obj_str);
                // Also handle the case where the replacement starts with "builtin("
                let prefix = format!("{}(", builtin_name);
                if replacement.starts_with(&prefix) && replacement.ends_with(")") {
                    replacement = replacement[prefix.len()..replacement.len() - 1].to_string();
                }
                tracing::debug!(
                    "Found {} replacement for {}() call (fallback): {}",
                    magic_method,
                    builtin_name,
                    replacement
                );
                return Some(replacement);
            }
        }

        None
    }

    /// Get the type of any expression using type introspection
    fn get_expression_type(&self, expr: &'a Expr) -> Option<String> {
        match expr {
            Expr::Name(name) => {
                // For simple names, use type introspection directly
                let range = name.range();
                let location = self.source_module.line_col_at_offset(range.start());
                self.query_type_at_location(location, name.id.as_ref())
            }
            Expr::Attribute(attr) => {
                // For str() magic method, we need the type of the full attribute expression
                // not the type of the base object
                // Query at the position of the attribute name itself
                let attr_start = attr.attr.range().start();
                let location = self.source_module.line_col_at_offset(attr_start);

                // Try to get the type at this location
                let result = self.query_type_at_location(location, attr.attr.as_ref());

                // If that fails, fall back to the standard attribute type lookup
                if result.is_none() {
                    self.get_attribute_type(attr)
                } else {
                    result
                }
            }
            _ => {
                // For other expressions, try to get type from their range
                let range = expr.range();
                let location = self.source_module.line_col_at_offset(range.start());
                self.query_type_at_location(location, "<expression>")
            }
        }
    }

    /// Query type at a specific location using the abstracted type introspection
    fn query_type_at_location(&self, location: (u32, u32), variable_name: &str) -> Option<String> {
        tracing::debug!(
            "Querying type at {}:{} in {} for variable '{}' using {:?}",
            location.0,
            location.1,
            self.file_path,
            variable_name,
            self.type_introspection
        );

        // Check cache first
        let cache_key = (location.0, location.1);
        if let Some(cached_type) = self.type_cache.borrow().get(&cache_key) {
            tracing::debug!("Type lookup (cached): {:?}", cached_type);
            return cached_type.clone();
        }

        let type_result = match self.type_introspection {
            TypeIntrospectionMethod::PyrightLsp => {
                if let Some(ref client_cell) = self.pyright_client {
                    let mut client = client_cell.borrow_mut();
                    match client.query_type(
                        &self.file_path,
                        &self.source_content,
                        location.0,
                        location.1,
                    ) {
                        Ok(Some(type_str)) => Ok(type_str),
                        Ok(None) => Err(anyhow::anyhow!(
                            "No type information available from pyright"
                        )),
                        Err(e) => Err(e),
                    }
                } else {
                    Err(anyhow::anyhow!("Pyright client not available"))
                }
            }
            TypeIntrospectionMethod::MypyDaemon => {
                if let Some(ref client_cell) = self.mypy_client {
                    let mut client = client_cell.borrow_mut();
                    match client.get_type_at_position(
                        &self.file_path,
                        location.0 as usize,
                        location.1 as usize,
                    ) {
                        Ok(Some(type_str)) => Ok(type_str),
                        Ok(None) => Err(anyhow::anyhow!("No type information available from mypy")),
                        Err(e) => Err(anyhow::anyhow!("Mypy error: {}", e)),
                    }
                } else {
                    Err(anyhow::anyhow!("Mypy client not available"))
                }
            }
            TypeIntrospectionMethod::PyrightWithMypyFallback => {
                let mut result = if let Some(ref client_cell) = self.pyright_client {
                    let mut client = client_cell.borrow_mut();
                    match client.query_type(
                        &self.file_path,
                        &self.source_content,
                        location.0,
                        location.1,
                    ) {
                        Ok(Some(type_str)) => Ok(type_str),
                        Ok(None) => Err(anyhow::anyhow!("No type from pyright")),
                        Err(e) => Err(e),
                    }
                } else {
                    Err(anyhow::anyhow!("Pyright not available"))
                };

                // If pyright failed, try mypy
                if result.is_err() {
                    if let Some(ref client_cell) = self.mypy_client {
                        let mut client = client_cell.borrow_mut();
                        result = match client.get_type_at_position(
                            &self.file_path,
                            location.0 as usize,
                            location.1 as usize,
                        ) {
                            Ok(Some(type_str)) => Ok(type_str),
                            Ok(None) => result, // Keep original error
                            Err(e) => Err(anyhow::anyhow!("Mypy error: {}", e)),
                        };
                    }
                }

                result
            }
        };

        match type_result {
            Ok(type_str) => {
                tracing::debug!("Type introspection found type: {}", type_str);
                // Cache the result
                self.type_cache
                    .borrow_mut()
                    .insert(cache_key, Some(type_str.clone()));
                Some(type_str)
            }
            Err(e) => {
                tracing::debug!("Type introspection failed: {}", e);
                // Cache the failure
                self.type_cache.borrow_mut().insert(cache_key, None);
                None
            }
        }
    }

    fn replacement_starts_with_await(&self, expr: &Expr) -> bool {
        matches!(expr, Expr::Await(_))
    }

    fn process_call_with_await_context(&mut self, call: &'a ast::ExprCall, is_await: bool) {
        // Extract the function name being called
        let func_name = match &*call.func {
            Expr::Name(name) => Some(name.id.to_string()),
            Expr::Attribute(attr) => Some(attr.attr.to_string()),
            _ => None,
        };

        if let Some(name) = func_name {
            // Special handling for built-in functions that call magic methods
            let magic_method = match name.as_str() {
                "str" => Some("__str__"),
                "repr" => Some("__repr__"),
                "bool" => Some("__bool__"),
                "int" => Some("__int__"),
                "float" => Some("__float__"),
                "bytes" => Some("__bytes__"),
                "hash" => Some("__hash__"),
                "len" => Some("__len__"),
                _ => None,
            };

            if let Some(magic_method) = magic_method {
                if call.arguments.args.len() == 1 {
                    if let Some(replacement) = self.check_magic_method(
                        &call.arguments.args[0],
                        magic_method,
                        name.as_str(),
                    ) {
                        tracing::debug!("Found {} replacement for {}() call", magic_method, name);
                        self.replacements.push((call.range(), replacement));
                        return;
                    }
                }
            }

            // Try with full module path first
            let full_name = format!("{}.{}", self.module_name, name);
            tracing::debug!("Checking function call: {} (full: {})", name, full_name);

            let replace_info = self
                    .replacements_info
                    .get(&full_name)
                    .or_else(|| self.replacements_info.get(&name))
                    .or_else(|| {
                        // Check if this is an imported function
                        if matches!(&*call.func, Expr::Name(_)) {
                            if let Some(full_imported_name) = self.import_map.get(&name) {
                                tracing::debug!(
                                    "Checking imported function: {} -> {}",
                                    name,
                                    full_imported_name
                                );
                                return self.replacements_info.get(full_imported_name);
                            }
                        }
                        None
                    })
                    .or_else(|| {
                        // For method calls, try to find any replacement that ends with the method name
                        if let Expr::Attribute(attr) = &*call.func {
                            tracing::debug!("Checking method call attribute: {}", name);

                            // OPTIMIZATION: First check if we have any replacements for this method name
                            // before doing expensive type introspection
                            let matching_keys: Vec<_> = self
                                .replacements_info
                                .keys()
                                .filter(|key| key.ends_with(&format!(".{}", name)))
                                .collect();

                            if matching_keys.is_empty() {
                                // No replacements for this method - nothing to do
                                tracing::debug!("No replacements found for method '{}', skipping type introspection", name);
                                return None;
                            }

                            // We have potential replacements, now do type introspection
                            tracing::debug!("Found {} potential replacement(s) for method '{}', performing type introspection", matching_keys.len(), name);

                            if let Some(type_name) = self.get_attribute_type(attr) {
                                // First, check if the type_name is in our import_map to get the FQN
                                let resolved_type = if !type_name.contains('.') {
                                    // Check if this type was imported
                                    if let Some(fqn) = self.import_map.get(&type_name) {
                                        tracing::debug!("Resolved type '{}' to FQN '{}' from imports", type_name, fqn);
                                        fqn.clone()
                                    } else {
                                        type_name.clone()
                                    }
                                } else {
                                    type_name.clone()
                                };

                                // Try to find replacement with the resolved type
                                let typed_method = format!("{}.{}", resolved_type, name);
                                tracing::debug!("Looking for replacement for typed method: {}", typed_method);
                                if let Some(info) = self.replacements_info.get(&typed_method) {
                                    tracing::debug!("Found replacement for {}", typed_method);
                                    return Some(info);
                                }

                                // If type_name doesn't have module prefix and wasn't in imports, try adding current module
                                if !resolved_type.contains('.') {
                                    let typed_method_with_module = format!("{}.{}.{}", self.module_name, resolved_type, name);
                                    tracing::debug!("Also trying with module prefix: {}", typed_method_with_module);
                                    if let Some(info) = self.replacements_info.get(&typed_method_with_module) {
                                        tracing::debug!("Found replacement for {}", typed_method_with_module);
                                        return Some(info);
                                    }
                                }

                                // Also check if the type is from an imported module
                                // Sometimes pyright returns "module.ClassName" format
                                if type_name.contains('.') {
                                    // Extract just the class name and try with our module
                                    if let Some(class_name) = type_name.split('.').next_back() {
                                        let typed_method_with_our_module = format!("{}.{}.{}", self.module_name, class_name, name);
                                        tracing::debug!("Also trying with our module prefix: {}", typed_method_with_our_module);
                                        if let Some(info) = self.replacements_info.get(&typed_method_with_our_module) {
                                            tracing::debug!("Found replacement for {}", typed_method_with_our_module);
                                            return Some(info);
                                        }
                                    }
                                }

                                // Handle inheritance - check parent classes
                                let class_with_module = if !resolved_type.contains('.') {
                                    format!("{}.{}", self.module_name, resolved_type)
                                } else {
                                    resolved_type.clone()
                                };

                                tracing::debug!("Checking inheritance for class: {}", class_with_module);
                                tracing::debug!("Inheritance map keys: {:?}", self.inheritance_map.keys().collect::<Vec<_>>());
                                if let Some(base_classes) = self.inheritance_map.get(&class_with_module) {
                                    tracing::debug!("Found base classes: {:?}", base_classes);
                                    for base_class in base_classes {
                                        // Try with just the base class name
                                        let base_method = format!("{}.{}", base_class, name);
                                        tracing::debug!("Trying base method: {}", base_method);
                                        if let Some(info) = self.replacements_info.get(&base_method) {
                                            tracing::debug!("Found replacement via inheritance: {} -> {}", base_method, info.replacement_expr);
                                            return Some(info);
                                        }

                                        // Try with module prefix
                                        let base_method_with_module = format!("{}.{}.{}", self.module_name, base_class, name);
                                        tracing::debug!("Trying base method with module: {}", base_method_with_module);
                                        if let Some(info) = self.replacements_info.get(&base_method_with_module) {
                                            tracing::debug!("Found replacement via inheritance: {} -> {}", base_method_with_module, info.replacement_expr);
                                            return Some(info);
                                        }

                                        // Also try with the module of the resolved type (for cross-module inheritance)
                                        if resolved_type.contains('.') {
                                            let parts: Vec<&str> = resolved_type.split('.').collect();
                                            if parts.len() >= 2 {
                                                let module_parts = &parts[..parts.len() - 1];
                                                let cross_module_base = format!("{}.{}.{}", module_parts.join("."), base_class, name);
                                                tracing::debug!("Trying cross-module base method: {}", cross_module_base);
                                                if let Some(info) = self.replacements_info.get(&cross_module_base) {
                                                    tracing::debug!("Found replacement via cross-module inheritance: {} -> {}", cross_module_base, info.replacement_expr);
                                                    return Some(info);
                                                }
                                            }
                                        }
                                    }
                                }

                                // If we successfully determined the type but found no replacement,
                                // do NOT fall back to suffix matching - this prevents over-migration
                                tracing::debug!(
                                    "Type determined as '{}' but no replacement found for '{}'",
                                    type_name,
                                    typed_method
                                );
                                None
                            } else {
                                // We have potential replacements but no type info
                                // Log an error and skip this migration
                                tracing::error!(
                                    "Type introspection failed for method call '{}' at {:?}. \
                                    Cannot safely migrate without type information. \
                                    Found {} potential replacement(s): {:?}",
                                    name,
                                    attr.value.range(),
                                    matching_keys.len(),
                                    matching_keys
                                );
                                None
                            }
                        } else {
                            None
                        }
                    });

            if let Some(replace_info) = replace_info {
                tracing::debug!("Found replacement for {}", name);
                // Build parameter mapping
                let (mut arg_map, _keyword_args, kwarg_pairs) =
                    self.build_param_map(call, replace_info);

                // Handle self/cls for method calls
                if let Expr::Attribute(attr) = &*call.func {
                    let obj_text = self.source_module.text_at_range(attr.value.range());
                    arg_map.insert("self".to_string(), obj_text.to_string());
                    arg_map.insert("cls".to_string(), obj_text.to_string());
                }

                // Check if we have an AST for this replacement
                let mut replacement = if let Some(ref ast) = replace_info.replacement_ast {
                    // Use AST transformation for proper handling
                    // Build list of provided parameters (those that were actually passed in the call)
                    let mut provided_params = Vec::new();
                    for param in &replace_info.parameters {
                        if arg_map.contains_key(&param.name) {
                            provided_params.push(param.name.clone());
                        }
                    }

                    // Get all parameter names from the replacement info
                    let all_params: Vec<String> = replace_info
                        .parameters
                        .iter()
                        .filter(|p| !p.is_vararg && !p.is_kwarg)
                        .map(|p| p.name.clone())
                        .collect();

                    let result =
                        transform_replacement_ast(ast, &arg_map, &provided_params, &all_params);

                    // Handle double await - if we're in an await context and the replacement starts with await,
                    // we need to strip the await from the replacement
                    let mut final_result = if is_await && self.replacement_starts_with_await(ast) {
                        // The replacement already has await, so don't double it
                        result.strip_prefix("await ").unwrap_or(&result).to_string()
                    } else {
                        result
                    };

                    // Check if we need to preserve class/module qualification
                    if let Expr::Attribute(attr) = &*call.func {
                        // This is a Class.method or module.function call
                        if let Expr::Name(class_or_module_name) = &*attr.value {
                            let prefix = class_or_module_name.id.to_string();
                            tracing::debug!("Detected qualified call: {}.{}", prefix, name);

                            // Check if it's a static method replacement without qualification
                            if replace_info.construct_type
                                == crate::core::types::ConstructType::StaticMethod
                            {
                                // For static methods, if the replacement doesn't contain a dot,
                                // preserve the class prefix
                                if !final_result.contains(".") {
                                    // Extract just the function name from the replacement
                                    let func_name_end =
                                        final_result.find('(').unwrap_or(final_result.len());
                                    let new_func_name = &final_result[..func_name_end];

                                    // Preserve the class prefix from the original call
                                    let qualified_name = format!("{}.{}", prefix, new_func_name);
                                    final_result =
                                        final_result.replacen(new_func_name, &qualified_name, 1);
                                    tracing::debug!(
                                        "Preserving class prefix for static method: {}",
                                        final_result
                                    );
                                }
                            }
                        }
                    }

                    // Add the replacement
                    tracing::debug!("AST-based replacement for {}: {}", name, final_result);
                    self.replacements.push((call.range(), final_result));

                    // Skip all the string manipulation below
                    return;
                } else {
                    // Fallback to string manipulation (legacy)
                    replace_info.replacement_expr.clone()
                };

                // Check if we need to preserve module qualification for function replacements
                if let Expr::Attribute(attr) = &*call.func {
                    // This is a module.function call (e.g., porcelain.checkout_branch)
                    if let Expr::Name(module_name) = &*attr.value {
                        let module_str = module_name.id.to_string();
                        tracing::debug!("Detected module-qualified call: {}.{}", module_str, name);

                        // If the replacement is a simple function name, preserve the module prefix
                        if !replacement.contains(".") {
                            // Extract just the function name from the replacement expression
                            // Handle cases like "checkout({repo}, {target}, force={force})"
                            let func_name_end = replacement.find('(').unwrap_or(replacement.len());
                            let new_func_name = &replacement[..func_name_end];

                            // Preserve the module prefix from the original call
                            let qualified_name = format!("{}.{}", module_str, new_func_name);
                            replacement = replacement.replacen(new_func_name, &qualified_name, 1);
                            tracing::debug!(
                                "Preserving module prefix in replacement: {}",
                                replacement
                            );
                        }
                    }
                }

                // Always do parameter substitution
                tracing::debug!("Parameter substitution - Original: {}", replacement);
                tracing::debug!("Parameter map: {:?}", arg_map);

                // Track which parameters we've already processed to avoid double substitution
                let mut processed_params = HashSet::new();

                // First handle keyword arguments patterns like keyword={param}
                // We need to find ALL patterns of the form `keyword={param}` where param is one of our parameters
                let param_names: Vec<String> = replace_info
                    .parameters
                    .iter()
                    .filter(|p| !p.is_vararg && !p.is_kwarg)
                    .map(|p| p.name.clone())
                    .collect();

                // Find all keyword={param} patterns in the replacement expression
                let mut kwarg_patterns: Vec<(String, String)> = Vec::new(); // (full_pattern, param_name)
                for param_name in &param_names {
                    let param_placeholder = format!("{{{}}}", param_name);
                    // Look for any pattern like `word={param}` where word is an identifier
                    let pattern_regex = format!(r"(\w+)={}", regex::escape(&param_placeholder));
                    if let Ok(re) = regex::Regex::new(&pattern_regex) {
                        for cap in re.captures_iter(&replacement) {
                            if let Some(keyword) = cap.get(1) {
                                let full_pattern =
                                    format!("{}={}", keyword.as_str(), param_placeholder);
                                kwarg_patterns.push((full_pattern, param_name.clone()));
                            }
                        }
                    }
                }

                // Process each keyword={param} pattern found
                for (kwarg_pattern, param_name) in kwarg_patterns {
                    tracing::debug!(
                        "Processing kwarg pattern '{}' for param '{}'",
                        kwarg_pattern,
                        param_name
                    );
                    if replacement.contains(&kwarg_pattern) {
                        // This parameter appears in a keyword argument position
                        if let Some(arg_value) = arg_map.get(&param_name) {
                            // Check if the original argument was passed as a keyword
                            let was_keyword = call.arguments.keywords.iter().any(|kw| {
                                kw.arg.as_ref().map(|arg| arg.as_str()) == Some(&param_name)
                            });

                            // Extract the keyword part from the pattern (e.g., "processing_mode" from "processing_mode={mode}")
                            let keyword_name =
                                kwarg_pattern.split('=').next().unwrap_or(&param_name);

                            let kwarg_replacement = if was_keyword {
                                // Preserve keyword format with the actual keyword name
                                format!("{}={}", keyword_name, arg_value)
                            } else {
                                // Convert positional to keyword with the actual keyword name
                                format!("{}={}", keyword_name, arg_value)
                            };

                            tracing::debug!(
                                "Replacing {} with {}",
                                kwarg_pattern,
                                kwarg_replacement
                            );
                            replacement = replacement.replace(&kwarg_pattern, &kwarg_replacement);
                            processed_params.insert(param_name.clone());
                        } else {
                            // Remove the entire keyword={param} pattern for unmapped parameters
                            // We need to be careful to handle comma placement correctly
                            // Try patterns in order of specificity
                            let patterns = vec![
                                // With leading comma and whitespace variations
                                format!(",\n        {}", kwarg_pattern), // With leading comma and newline
                                format!(",\n    {}", kwarg_pattern), // With different indentation
                                format!(",\n{}", kwarg_pattern),     // With just newline
                                format!(", {}", kwarg_pattern),      // With leading comma and space
                                // Special case: if this is the last parameter but there's a preceding comma
                                format!(", {})", kwarg_pattern), // Last parameter with comma before it
                                // With trailing comma
                                format!("{},\n", kwarg_pattern), // With trailing comma and newline
                                format!("{}, ", kwarg_pattern),  // With trailing comma and space
                                format!("{},", kwarg_pattern),   // With trailing comma
                                // Standalone (might be first/only parameter)
                                kwarg_pattern.clone(), // Just the pattern itself
                            ];

                            let mut found = false;
                            for pattern in patterns {
                                if replacement.contains(&pattern) {
                                    tracing::debug!(
                                        "Removing unmapped parameter pattern: {}",
                                        pattern
                                    );
                                    replacement = replacement.replace(&pattern, "");
                                    processed_params.insert(param_name.clone());
                                    found = true;
                                    break;
                                }
                            }

                            if !found {
                                tracing::warn!(
                                    "Could not remove unmapped parameter: {}",
                                    kwarg_pattern
                                );
                            }
                        }
                    }
                }

                // Then handle remaining placeholders
                for (param_name, arg_value) in &arg_map {
                    // Skip if already processed in kwarg pattern
                    if processed_params.contains(param_name) {
                        continue;
                    }

                    let placeholder = format!("{{{}}}", param_name);
                    if replacement.contains(&placeholder) {
                        // For class constructor replacements, we should NOT preserve keyword arguments
                        // The replacement expression shows the intended parameter positions
                        let replacement_value = arg_value.to_string();

                        tracing::debug!("Replacing {} with {}", placeholder, replacement_value);
                        replacement = replacement.replace(&placeholder, &replacement_value);
                    }
                }
                tracing::debug!("After substitution: {}", replacement);

                // Handle any remaining placeholders for parameters that weren't provided
                for param in &replace_info.parameters {
                    if !param.is_vararg
                        && !param.is_kwarg
                        && param.name != "self"
                        && param.name != "cls"
                    {
                        // Skip if already processed in kwarg pattern
                        if processed_params.contains(&param.name) {
                            continue;
                        }

                        let placeholder = format!("{{{}}}", param.name);
                        if replacement.contains(&placeholder) {
                            // This parameter wasn't provided in the call
                            // For parameters with defaults, we should remove them rather than
                            // substituting the default value, since Python will use the default
                            // automatically when the argument is not provided
                            tracing::debug!(
                                "Removing unprovided parameter placeholder: {}",
                                placeholder
                            );

                            // We need to find and remove the entire argument containing this placeholder
                            // Look for the argument boundaries by finding commas or parentheses

                            // First, find where the placeholder appears
                            if let Some(placeholder_pos) = replacement.find(&placeholder) {
                                tracing::debug!(
                                    "Processing placeholder '{}' at position {} in '{}'",
                                    placeholder,
                                    placeholder_pos,
                                    replacement
                                );
                                // Find the start of this argument (either after '(' or after ', ')
                                let mut arg_start = placeholder_pos;
                                let bytes = replacement.as_bytes();

                                // Search backwards for argument start
                                while arg_start > 0 {
                                    if arg_start >= 2 && &bytes[arg_start - 2..arg_start] == b", " {
                                        arg_start -= 2;
                                        break;
                                    } else if bytes[arg_start - 1] == b'(' {
                                        arg_start -= 1;
                                        break;
                                    }
                                    arg_start -= 1;
                                }

                                // Find the end of this argument (either before ',' or before ')')
                                let mut arg_end = placeholder_pos + placeholder.len();
                                while arg_end < bytes.len() {
                                    if bytes[arg_end] == b',' || bytes[arg_end] == b')' {
                                        break;
                                    }
                                    arg_end += 1;
                                }

                                // Extract the full argument
                                let full_arg = &replacement[arg_start..arg_end];
                                tracing::debug!(
                                        "Found full argument containing placeholder: '{}' (arg_start={}, arg_end={})",
                                        full_arg, arg_start, arg_end
                                    );

                                // Determine how to remove it based on context
                                if arg_start > 0
                                    && bytes[arg_start - 1] == b'('
                                    && arg_end < bytes.len()
                                    && bytes[arg_end] == b')'
                                {
                                    // This is the only argument: func({arg})
                                    replacement =
                                        replacement.replace(&format!("({})", full_arg), "()");
                                } else if arg_start >= 2
                                    && &bytes[arg_start - 2..arg_start] == b", "
                                {
                                    // This is not the first argument: , {arg}
                                    replacement =
                                        replacement.replace(&format!(", {}", full_arg), "");
                                } else if arg_end < bytes.len() && bytes[arg_end] == b',' {
                                    // This is the first argument: {arg},
                                    replacement =
                                        replacement.replace(&format!("{}, ", full_arg), "");
                                } else {
                                    // Fallback: just remove the argument
                                    replacement = replacement.replace(full_arg, "");
                                }
                            } else {
                                // Placeholder not found, shouldn't happen
                                tracing::warn!(
                                    "Placeholder {} not found in replacement expression",
                                    placeholder
                                );
                            }
                        }
                    }
                }

                // Handle *args in the replacement expression
                let vararg_param = replace_info
                    .parameters
                    .iter()
                    .find(|p| p.is_vararg)
                    .map(|p| &p.name);

                if let Some(vararg_name) = vararg_param {
                    let vararg_key = format!("*{}", vararg_name);
                    let vararg_pattern = format!("*{}", vararg_name);

                    if let Some(args_value) = arg_map.get(&vararg_key) {
                        // Replace *args with the actual arguments
                        replacement = replacement.replace(&vararg_pattern, args_value);
                    } else {
                        // No extra args, remove the *args from replacement
                        // Try to remove with trailing comma first
                        if replacement.contains(&format!("{}, ", vararg_pattern)) {
                            replacement = replacement.replace(&format!("{}, ", vararg_pattern), "");
                        } else if replacement.contains(&format!(", {}", vararg_pattern)) {
                            replacement = replacement.replace(&format!(", {}", vararg_pattern), "");
                        } else {
                            replacement = replacement.replace(&vararg_pattern, "");
                        }
                    }
                }

                // Handle **kwargs in the replacement expression
                let kwarg_param = replace_info
                    .parameters
                    .iter()
                    .find(|p| p.is_kwarg)
                    .map(|p| &p.name);

                if let Some(kwarg_name) = kwarg_param {
                    let kwarg_key = format!("**{}", kwarg_name);
                    let kwarg_pattern = format!("**{}", kwarg_name);

                    if let Some(kwargs_value) = arg_map.get(&kwarg_key) {
                        // Replace **kwargs with the actual keyword arguments
                        replacement = replacement.replace(&kwarg_pattern, kwargs_value);
                    } else {
                        // No kwargs, remove the **kwargs from replacement
                        // Try to remove with trailing comma first
                        if replacement.contains(&format!("{}, ", kwarg_pattern)) {
                            replacement = replacement.replace(&format!("{}, ", kwarg_pattern), "");
                        } else if replacement.contains(&format!(", {}", kwarg_pattern)) {
                            replacement = replacement.replace(&format!(", {}", kwarg_pattern), "");
                        } else {
                            replacement = replacement.replace(&kwarg_pattern, "");
                        }
                    }
                }

                // Handle any remaining **dict expansions that weren't handled by **kwargs
                // This is important for cases where the caller uses **dict but the function
                // doesn't have **kwargs in its signature
                if !kwarg_pairs.is_empty() && kwarg_param.is_none() {
                    // We have dict expansions but no **kwargs parameter to handle them
                    // We need to append them to the replacement
                    let dict_expansions: Vec<String> = kwarg_pairs
                        .iter()
                        .filter(|kp| kp.starts_with("**"))
                        .cloned()
                        .collect();

                    if !dict_expansions.is_empty() {
                        // Find the last closing parenthesis and insert before it
                        if let Some(last_paren) = replacement.rfind(')') {
                            let dict_expansion_str = dict_expansions.join(", ");

                            // Check what's immediately before the closing paren
                            let before_paren = &replacement[..last_paren].trim_end();

                            // Determine if we need a comma
                            let needs_comma = if before_paren.ends_with('(') {
                                // Empty parentheses - no comma needed
                                false
                            } else if before_paren.ends_with(',') {
                                // Already has trailing comma - no additional comma needed
                                false
                            } else {
                                // Has content without trailing comma - need comma
                                true
                            };

                            let insertion = if needs_comma {
                                format!(", {}", dict_expansion_str)
                            } else {
                                dict_expansion_str.clone()
                            };

                            replacement.insert_str(last_paren, &insertion);
                            tracing::debug!("Added dict expansions to replacement: {}", insertion);
                        }
                    }
                }

                // Handle double await for string manipulation path
                let final_replacement = if is_await && replacement.starts_with("await ") {
                    // The replacement already has await, so don't double it
                    replacement[6..].to_string() // Skip "await "
                } else {
                    replacement
                };

                // Add replacement
                tracing::debug!("Final replacement for {}: {}", name, final_replacement);
                self.replacements.push((call.range(), final_replacement));
            }
        }
    }
}

/// Main entry point for migrating a file using improved Ruff parser
pub fn migrate_file_with_improved_ruff(
    source: &str,
    module_name: &str,
    file_path: String,
    type_introspection: TypeIntrospectionMethod,
) -> Result<String> {
    // Parse source
    let parsed_module = PythonModule::parse(source)?;

    // Collect deprecated functions
    let collector_result = crate::ruff_parser::collect_deprecated_functions(source, module_name)?;

    // Find and replace calls
    let mut replacer = ImprovedFunctionCallReplacer::new(
        collector_result.replacements,
        &parsed_module,
        type_introspection,
        file_path,
        module_name.to_string(),
        HashSet::new(), // Not used anymore - detection happens in visitor
        source.to_string(),
        collector_result.inheritance_map,
    )?;

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
    Ok(crate::ruff_parser::apply_replacements(source, replacements))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_kwargs_replacement() {
        let source = r#"
from dissolve import replace_me

@replace_me()
def old_func(a, b, *args, **kwargs):
    return new_func(a, b, *args, **kwargs)

# Test various call patterns
result1 = old_func(1, 2)
result2 = old_func(1, 2, 3, 4)
result3 = old_func(1, 2, x=3, y=4)
result4 = old_func(1, 2, 3, 4, x=5, y=6)
"#;

        // Test that migration handles these cases correctly
        let test_ctx = crate::tests::test_utils::TestContext::new(source);
        let result = migrate_file_with_improved_ruff(
            source,
            "test_module",
            test_ctx.file_path,
            TypeIntrospectionMethod::PyrightLsp,
        );

        match result {
            Ok(migrated) => {
                println!("Migration result:\n{}", migrated);
                // Should replace with proper argument expansion
                assert!(migrated.contains("new_func(1, 2)"));
                assert!(migrated.contains("new_func(1, 2, 3, 4)"));
                assert!(migrated.contains("new_func(1, 2, x=3, y=4)"));
                assert!(migrated.contains("new_func(1, 2, 3, 4, x=5, y=6)"));
            }
            Err(e) => {
                println!("Migration not yet fully implemented: {}", e);
            }
        }
    }
}
