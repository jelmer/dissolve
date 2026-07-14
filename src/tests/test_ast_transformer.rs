// Tests for rendering AST expressions back to source

use crate::ast_transformer::transform_replacement_ast;
use std::collections::HashMap;

fn to_source(expr: &str) -> String {
    let parsed = ruff_python_parser::parse_expression(expr).expect("expression should parse");
    transform_replacement_ast(parsed.expr(), &HashMap::new(), &[], &[])
}

#[test]
fn test_dict_comprehension() {
    assert_eq!("{k: v for k in items}", to_source("{k: v for k in items}"));
}

#[test]
fn test_dict_comprehension_unpacking() {
    // PEP 798 dict unpacking has no key: {**d for d in dicts}
    assert_eq!("{**d for d in dicts}", to_source("{**d for d in dicts}"));
}
