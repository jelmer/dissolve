// AST transformer for applying replacements

use ruff_python_ast::{Arguments, Expr, ExprName, Operator};
use std::collections::{HashMap, HashSet};

/// Transform an AST expression by replacing parameter references with actual values
pub fn transform_replacement_ast(
    expr: &Expr,
    param_map: &HashMap<String, String>,
    provided_params: &[String],
    all_params: &[String],
) -> String {
    // Create sets for faster lookup - use references to avoid cloning strings
    let provided_set: HashSet<&str> = provided_params.iter().map(|s| s.as_str()).collect();
    let all_params_set: HashSet<&str> = all_params.iter().map(|s| s.as_str()).collect();

    tracing::debug!(
        "AST transform input - param_map: {:?}, provided_params: {:?}, all_params: {:?}",
        param_map,
        provided_params,
        all_params
    );

    // Clone and transform the expression
    let transformed =
        transform_expr_with_all_params(expr, param_map, &provided_set, &all_params_set);

    // Convert back to source code
    let result = ast_to_source(&transformed);
    tracing::debug!("AST transform result: {}", result);

    result
}

/// Convert AST expression to source code
fn ast_to_source(expr: &Expr) -> String {
    match expr {
        Expr::Name(name) => name.id.to_string(),

        Expr::Attribute(attr) => {
            format!("{}.{}", ast_to_source(&attr.value), attr.attr)
        }

        Expr::Call(call) => {
            let func = ast_to_source(&call.func);
            let mut args = Vec::new();

            // Positional arguments
            for arg in call.arguments.args.iter() {
                args.push(ast_to_source(arg));
            }

            // Keyword arguments
            for kw in call.arguments.keywords.iter() {
                if let Some(name) = &kw.arg {
                    args.push(format!("{}={}", name, ast_to_source(&kw.value)));
                } else {
                    args.push(format!("**{}", ast_to_source(&kw.value)));
                }
            }

            format!("{}({})", func, args.join(", "))
        }

        Expr::StringLiteral(s) => {
            // Use the to_str() method and properly escape the content
            let content = s.value.to_str();
            let mut escaped = String::with_capacity(content.len() * 2); // Pre-allocate with reasonable capacity

            for c in content.chars() {
                match c {
                    '"' => escaped.push_str("\\\""),
                    '\\' => escaped.push_str("\\\\"),
                    '\n' => escaped.push_str("\\n"),
                    '\r' => escaped.push_str("\\r"),
                    '\t' => escaped.push_str("\\t"),
                    c if c.is_control() => escaped.push_str(&format!("\\u{{{:04x}}}", c as u32)),
                    c => escaped.push(c),
                }
            }
            format!("\"{}\"", escaped)
        }

        Expr::NumberLiteral(n) => match &n.value {
            ruff_python_ast::Number::Int(i) => i.to_string(),
            ruff_python_ast::Number::Float(f) => f.to_string(),
            ruff_python_ast::Number::Complex { real, imag } => {
                format!("{}+{}j", real, imag)
            }
        },

        Expr::BooleanLiteral(b) => if b.value { "True" } else { "False" }.to_string(),

        Expr::NoneLiteral(_) => "None".to_string(),

        Expr::List(list) => {
            let elements: Vec<String> = list.elts.iter().map(ast_to_source).collect();
            format!("[{}]", elements.join(", "))
        }

        Expr::Tuple(tuple) => {
            let elements: Vec<String> = tuple.elts.iter().map(ast_to_source).collect();
            if elements.len() == 1 {
                format!("({},)", elements[0])
            } else {
                format!("({})", elements.join(", "))
            }
        }

        Expr::Dict(dict) => {
            let mut items = Vec::new();
            for item in &dict.items {
                if let Some(key) = &item.key {
                    items.push(format!(
                        "{}: {}",
                        ast_to_source(key),
                        ast_to_source(&item.value)
                    ));
                } else {
                    items.push(format!("**{}", ast_to_source(&item.value)));
                }
            }
            format!("{{{}}}", items.join(", "))
        }

        Expr::BinOp(binop) => {
            let op = match binop.op {
                Operator::Add => "+",
                Operator::Sub => "-",
                Operator::Mult => "*",
                Operator::Div => "/",
                Operator::Mod => "%",
                Operator::Pow => "**",
                Operator::LShift => "<<",
                Operator::RShift => ">>",
                Operator::BitOr => "|",
                Operator::BitXor => "^",
                Operator::BitAnd => "&",
                Operator::FloorDiv => "//",
                Operator::MatMult => "@",
            };
            format!(
                "{} {} {}",
                ast_to_source(&binop.left),
                op,
                ast_to_source(&binop.right)
            )
        }

        Expr::Starred(starred) => {
            // Handle *args expressions
            format!("*{}", ast_to_source(&starred.value))
        }

        Expr::Await(await_expr) => {
            // Handle await expressions
            format!("await {}", ast_to_source(&await_expr.value))
        }

        Expr::UnaryOp(unary) => {
            // Handle unary operations like -1, not x, ~x
            use ruff_python_ast::UnaryOp;
            let op = match unary.op {
                UnaryOp::Invert => "~",
                UnaryOp::Not => "not ",
                UnaryOp::UAdd => "+",
                UnaryOp::USub => "-",
            };
            format!("{}{}", op, ast_to_source(&unary.operand))
        }

        Expr::Subscript(sub) => {
            // Handle indexing like dict[key] or list[0]
            format!(
                "{}[{}]",
                ast_to_source(&sub.value),
                ast_to_source(&sub.slice)
            )
        }

        Expr::Slice(slice) => {
            // Handle slice operations like [start:stop:step]
            let start = slice
                .lower
                .as_ref()
                .map(|e| ast_to_source(e))
                .unwrap_or_default();
            let stop = slice
                .upper
                .as_ref()
                .map(|e| ast_to_source(e))
                .unwrap_or_default();
            let step = slice
                .step
                .as_ref()
                .map(|e| format!(":{}", ast_to_source(e)))
                .unwrap_or_default();
            format!("{}:{}{}", start, stop, step)
        }

        Expr::Compare(cmp) => {
            // Handle comparison operations
            let mut result = ast_to_source(&cmp.left);
            for (op, comparator) in cmp.ops.iter().zip(cmp.comparators.iter()) {
                use ruff_python_ast::CmpOp;
                let op_str = match op {
                    CmpOp::Eq => "==",
                    CmpOp::NotEq => "!=",
                    CmpOp::Lt => "<",
                    CmpOp::LtE => "<=",
                    CmpOp::Gt => ">",
                    CmpOp::GtE => ">=",
                    CmpOp::Is => "is",
                    CmpOp::IsNot => "is not",
                    CmpOp::In => "in",
                    CmpOp::NotIn => "not in",
                };
                result.push_str(&format!(" {} {}", op_str, ast_to_source(comparator)));
            }
            result
        }

        Expr::BoolOp(boolop) => {
            // Handle boolean operations (and, or)
            use ruff_python_ast::BoolOp;
            let op = match boolop.op {
                BoolOp::And => " and ",
                BoolOp::Or => " or ",
            };
            let values: Vec<String> = boolop.values.iter().map(ast_to_source).collect();
            values.join(op)
        }

        Expr::If(ifexp) => {
            // Handle conditional expressions (ternary)
            format!(
                "{} if {} else {}",
                ast_to_source(&ifexp.body),
                ast_to_source(&ifexp.test),
                ast_to_source(&ifexp.orelse)
            )
        }

        Expr::Lambda(lambda) => {
            // Handle lambda expressions
            let params = lambda
                .parameters
                .as_ref()
                .map(|p| {
                    p.posonlyargs
                        .iter()
                        .chain(p.args.iter())
                        .map(|param| param.parameter.name.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            format!("lambda {}: {}", params, ast_to_source(&lambda.body))
        }

        Expr::ListComp(comp) => {
            // Handle list comprehensions
            format!(
                "[{} {}]",
                ast_to_source(&comp.elt),
                generators_to_string(&comp.generators)
            )
        }

        Expr::SetComp(comp) => {
            // Handle set comprehensions
            format!(
                "{{{} {}}}",
                ast_to_source(&comp.elt),
                generators_to_string(&comp.generators)
            )
        }

        Expr::DictComp(comp) => {
            // Handle dict comprehensions
            format!(
                "{{{}: {} {}}}",
                ast_to_source(&comp.key),
                ast_to_source(&comp.value),
                generators_to_string(&comp.generators)
            )
        }

        Expr::Generator(gen) => {
            // Handle generator expressions
            format!(
                "({} {})",
                ast_to_source(&gen.elt),
                generators_to_string(&gen.generators)
            )
        }

        Expr::Set(set) => {
            // Handle set literals
            let elements: Vec<String> = set.elts.iter().map(ast_to_source).collect();
            format!("{{{}}}", elements.join(", "))
        }

        Expr::BytesLiteral(b) => {
            // Handle bytes literals
            let mut result = String::from("b\"");
            for byte in b.value.bytes() {
                match byte {
                    b'\\' => result.push_str("\\\\"),
                    b'"' => result.push_str("\\\""),
                    b'\n' => result.push_str("\\n"),
                    b'\r' => result.push_str("\\r"),
                    b'\t' => result.push_str("\\t"),
                    b'\0' => result.push_str("\\x00"),
                    0x20..=0x7E => result.push(byte as char), // Printable ASCII
                    _ => result.push_str(&format!("\\x{:02x}", byte)),
                }
            }
            result.push('"');
            result
        }

        Expr::FString(fstring) => {
            // Handle f-strings
            let mut result = String::new();
            result.push_str("f\"");

            // Process each element of the f-string
            for part in fstring.value.elements() {
                match part {
                    ruff_python_ast::FStringElement::Literal(lit) => {
                        // Escape special characters in the literal part
                        let escaped = lit
                            .value
                            .chars()
                            .map(|c| match c {
                                '"' => "\\\"".to_string(),
                                '\\' => "\\\\".to_string(),
                                '{' => "{{".to_string(),
                                '}' => "}}".to_string(),
                                c => c.to_string(),
                            })
                            .collect::<String>();
                        result.push_str(&escaped);
                    }
                    ruff_python_ast::FStringElement::Expression(expr) => {
                        result.push('{');
                        result.push_str(&ast_to_source(&expr.expression));
                        if let Some(spec) = &expr.format_spec {
                            result.push(':');
                            for spec_elem in &spec.elements {
                                match spec_elem {
                                    ruff_python_ast::FStringElement::Literal(lit) => {
                                        result.push_str(&lit.value);
                                    }
                                    ruff_python_ast::FStringElement::Expression(e) => {
                                        result.push('{');
                                        result.push_str(&ast_to_source(&e.expression));
                                        result.push('}');
                                    }
                                }
                            }
                        }
                        result.push('}');
                    }
                }
            }

            result.push('"');
            result
        }

        Expr::Named(named) => {
            // Handle walrus operator :=
            format!(
                "{} := {}",
                ast_to_source(&named.target),
                ast_to_source(&named.value)
            )
        }

        Expr::EllipsisLiteral(_) => {
            // Handle ellipsis literal (...)
            "...".to_string()
        }

        Expr::YieldFrom(yield_from) => {
            // Handle yield from expressions
            format!("yield from {}", ast_to_source(&yield_from.value))
        }

        Expr::Yield(yield_expr) => {
            // Handle yield expressions
            if let Some(value) = &yield_expr.value {
                format!("yield {}", ast_to_source(value))
            } else {
                "yield".to_string()
            }
        }

        _ => {
            // Log error for unsupported expression types
            tracing::error!("Unsupported expression type in AST transformer: {:?}", expr);
            panic!("AST transformer does not support this expression type yet");
        }
    }
}

/// Helper function to convert generator expressions to string
fn generators_to_string(generators: &[ruff_python_ast::Comprehension]) -> String {
    generators
        .iter()
        .map(|gen| {
            let mut result = format!(
                "for {} in {}",
                ast_to_source(&gen.target),
                ast_to_source(&gen.iter)
            );
            for if_clause in &gen.ifs {
                result.push_str(&format!(" if {}", ast_to_source(if_clause)));
            }
            result
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn transform_expr_with_all_params(
    expr: &Expr,
    param_map: &HashMap<String, String>,
    provided_params: &HashSet<&str>,
    all_params: &HashSet<&str>,
) -> Expr {
    match expr {
        Expr::Name(name) => {
            // Replace parameter names with actual values
            if let Some(value) = param_map.get(name.id.as_str()) {
                // Parse the replacement value as an expression
                if let Ok(parsed) = ruff_python_parser::parse_expression(value) {
                    parsed.into_expr()
                } else {
                    // Fallback for complex expressions or if parsing fails
                    // For simple names, create a Name expression
                    if value.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        Expr::Name(ExprName {
                            id: value.clone().into(),
                            ctx: name.ctx,
                            range: name.range,
                        })
                    } else {
                        // For more complex expressions, we'd need to parse them
                        // For now, just return the original
                        expr.clone()
                    }
                }
            } else {
                expr.clone()
            }
        }
        Expr::Call(call) => {
            // Transform function calls
            let mut new_call = call.clone();

            // Transform the function expression
            new_call.func = Box::new(transform_expr_with_all_params(
                &call.func,
                param_map,
                provided_params,
                all_params,
            ));

            // Transform arguments, filtering out unprovided parameters
            let mut new_args = Vec::new();
            for arg in &call.arguments.args {
                match arg {
                    Expr::Name(name) => {
                        let param_name = name.id.as_str();
                        // Include if: parameter was provided OR it's not a parameter at all
                        if provided_params.contains(param_name) || !all_params.contains(param_name)
                        {
                            new_args.push(transform_expr_with_all_params(
                                arg,
                                param_map,
                                provided_params,
                                all_params,
                            ));
                        }
                        // Otherwise, it's an unprovided parameter - skip it
                    }
                    Expr::Starred(starred) => {
                        // Handle *args - check if we have a value for it
                        if let Expr::Name(name) = &*starred.value {
                            let starred_param = format!("*{}", name.id);
                            if let Some(args_value) = param_map.get(&starred_param) {
                                // We have values for *args, expand them inline
                                // Parse the comma-separated values and add them as individual arguments
                                if !args_value.is_empty() {
                                    // Split by comma and parse each as an expression
                                    for arg_str in args_value.split(", ") {
                                        if let Ok(parsed) =
                                            ruff_python_parser::parse_expression(arg_str)
                                        {
                                            new_args.push(parsed.into_expr());
                                        }
                                    }
                                }
                            }
                            // Otherwise skip the *args entirely
                        } else {
                            // Not a simple *args pattern, keep it
                            new_args.push(transform_expr_with_all_params(
                                arg,
                                param_map,
                                provided_params,
                                all_params,
                            ));
                        }
                    }
                    _ => {
                        // Other expressions are always kept
                        new_args.push(transform_expr_with_all_params(
                            arg,
                            param_map,
                            provided_params,
                            all_params,
                        ));
                    }
                }
            }

            // Transform keyword arguments
            let mut new_keywords = Vec::new();
            for keyword in &call.arguments.keywords {
                if let Some(_arg_name) = &keyword.arg {
                    // For regular keyword arguments, we need to check if the VALUE contains a provided parameter
                    // The keyword name itself is not a parameter, so we always include the keyword
                    // but we need to check if we should include it based on the value
                    let mut should_include = true;

                    // Check if the keyword value is a simple parameter name that wasn't provided
                    if let Expr::Name(name) = &keyword.value {
                        if all_params.contains(name.id.as_str())
                            && !provided_params.contains(name.id.as_str())
                        {
                            // This is an unprovided parameter used as a keyword value, skip it
                            should_include = false;
                        }
                    }

                    if should_include {
                        let mut new_keyword = keyword.clone();
                        new_keyword.value = transform_expr_with_all_params(
                            &keyword.value,
                            param_map,
                            provided_params,
                            all_params,
                        );
                        new_keywords.push(new_keyword);
                    }
                } else {
                    // **kwargs expansion - check if we have values for it
                    if let Expr::Name(name) = &keyword.value {
                        let kwarg_param = format!("**{}", name.id);
                        if let Some(kwargs_value) = param_map.get(&kwarg_param) {
                            // We have values for **kwargs, expand them inline
                            if !kwargs_value.is_empty() {
                                // Parse the kwargs (format: "key1=value1, key2=value2")
                                for kwarg_str in kwargs_value.split(", ") {
                                    if let Some((key, value)) = kwarg_str.split_once('=') {
                                        if let Ok(value_expr) =
                                            ruff_python_parser::parse_expression(value)
                                        {
                                            // Create a keyword argument
                                            let keyword = ruff_python_ast::Keyword {
                                                arg: Some(ruff_python_ast::Identifier::new(
                                                    key.to_string(),
                                                    ruff_text_size::TextRange::default(),
                                                )),
                                                value: value_expr.into_expr(),
                                                range: ruff_text_size::TextRange::default(),
                                            };
                                            new_keywords.push(keyword);
                                        }
                                    } else if let Some(stripped) = kwarg_str.strip_prefix("**") {
                                        // Handle **dict expansion
                                        if let Ok(value_expr) =
                                            ruff_python_parser::parse_expression(stripped)
                                        {
                                            let keyword = ruff_python_ast::Keyword {
                                                arg: None,
                                                value: value_expr.into_expr(),
                                                range: ruff_text_size::TextRange::default(),
                                            };
                                            new_keywords.push(keyword);
                                        }
                                    }
                                }
                            }
                        }
                        // Otherwise skip the **kwargs entirely
                    } else {
                        // Not a simple **kwargs pattern, keep it
                        let mut new_keyword = keyword.clone();
                        new_keyword.value = transform_expr_with_all_params(
                            &keyword.value,
                            param_map,
                            provided_params,
                            all_params,
                        );
                        new_keywords.push(new_keyword);
                    }
                }
            }

            new_call.arguments = Arguments {
                args: new_args.into_boxed_slice(),
                keywords: new_keywords.into_boxed_slice(),
                range: call.arguments.range,
            };

            Expr::Call(new_call)
        }
        Expr::Attribute(attr) => {
            // Transform attribute access
            let mut new_attr = attr.clone();
            new_attr.value = Box::new(transform_expr_with_all_params(
                &attr.value,
                param_map,
                provided_params,
                all_params,
            ));
            Expr::Attribute(new_attr)
        }
        Expr::BinOp(binop) => {
            // Transform binary operations (like x * 2, y + 1)
            let mut new_binop = binop.clone();
            new_binop.left = Box::new(transform_expr_with_all_params(
                &binop.left,
                param_map,
                provided_params,
                all_params,
            ));
            new_binop.right = Box::new(transform_expr_with_all_params(
                &binop.right,
                param_map,
                provided_params,
                all_params,
            ));
            Expr::BinOp(new_binop)
        }
        Expr::Starred(starred) => {
            // Transform *args expressions
            let mut new_starred = starred.clone();
            new_starred.value = Box::new(transform_expr_with_all_params(
                &starred.value,
                param_map,
                provided_params,
                all_params,
            ));
            Expr::Starred(new_starred)
        }
        Expr::Await(await_expr) => {
            // Transform await expressions
            let mut new_await = await_expr.clone();
            new_await.value = Box::new(transform_expr_with_all_params(
                &await_expr.value,
                param_map,
                provided_params,
                all_params,
            ));
            Expr::Await(new_await)
        }
        Expr::UnaryOp(unary) => {
            // Transform unary operations
            let mut new_unary = unary.clone();
            new_unary.operand = Box::new(transform_expr_with_all_params(
                &unary.operand,
                param_map,
                provided_params,
                all_params,
            ));
            Expr::UnaryOp(new_unary)
        }
        Expr::Subscript(sub) => {
            // Transform subscript operations
            let mut new_sub = sub.clone();
            new_sub.value = Box::new(transform_expr_with_all_params(
                &sub.value,
                param_map,
                provided_params,
                all_params,
            ));
            new_sub.slice = Box::new(transform_expr_with_all_params(
                &sub.slice,
                param_map,
                provided_params,
                all_params,
            ));
            Expr::Subscript(new_sub)
        }

        Expr::Compare(cmp) => {
            // Transform comparison operations
            let mut new_cmp = cmp.clone();
            new_cmp.left = Box::new(transform_expr_with_all_params(
                &cmp.left,
                param_map,
                provided_params,
                all_params,
            ));
            new_cmp.comparators = cmp
                .comparators
                .iter()
                .map(|c| transform_expr_with_all_params(c, param_map, provided_params, all_params))
                .collect();
            Expr::Compare(new_cmp)
        }

        Expr::BoolOp(boolop) => {
            // Transform boolean operations
            let mut new_boolop = boolop.clone();
            new_boolop.values = boolop
                .values
                .iter()
                .map(|v| transform_expr_with_all_params(v, param_map, provided_params, all_params))
                .collect();
            Expr::BoolOp(new_boolop)
        }

        Expr::If(ifexp) => {
            // Transform conditional expressions
            let mut new_ifexp = ifexp.clone();
            new_ifexp.test = Box::new(transform_expr_with_all_params(
                &ifexp.test,
                param_map,
                provided_params,
                all_params,
            ));
            new_ifexp.body = Box::new(transform_expr_with_all_params(
                &ifexp.body,
                param_map,
                provided_params,
                all_params,
            ));
            new_ifexp.orelse = Box::new(transform_expr_with_all_params(
                &ifexp.orelse,
                param_map,
                provided_params,
                all_params,
            ));
            Expr::If(new_ifexp)
        }

        // Add more cases as needed - for now, recursively transform any other expressions
        _ => {
            // For unsupported expression types, just clone them
            // Most of these (like comprehensions, lambdas) are complex and less likely
            // to contain simple parameter references that need substitution
            expr.clone()
        }
    }
}
