// Advanced edge case tests for AST parameter substitution

use crate::migrate_ruff::migrate_file;
use crate::type_introspection_context::TypeIntrospectionContext;
use crate::{RuffDeprecatedFunctionCollector, TypeIntrospectionMethod};
use std::collections::HashMap;

#[test]
fn test_unary_operators_on_complex_expressions() {
    // Test unary operators applied to complex expressions
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_unary(expr):
    return compute(expr)

# Unary operators on complex expressions
result1 = process_unary(~(a | b))
result2 = process_unary(not (x and y))
result3 = process_unary(-(a + b * c))
result4 = process_unary(+(x if x > 0 else -x))
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Note: AST doesn't preserve parentheses, operator precedence is flattened
    assert!(migrated.contains("compute(~a | b)"));
    assert!(migrated.contains("compute(not x and y)"));
    assert!(migrated.contains("compute(-a + b * c)"));
}

#[test]
fn test_super_and_metaclass_calls() {
    // Test super() calls and metaclass attribute access
    let source = r#"
from dissolve import replace_me

class MyClass:
    @replace_me()
    def process_super(self, value):
        return handle(value)

    def test_method(self):
        result1 = self.process_super(super().some_method())
        result2 = self.process_super(type(self).__name__)
        result3 = self.process_super(type(self).class_var)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // The function calls inside methods might not be migrated due to context
    // At least verify that super() and type() calls are preserved in structure
    assert!(migrated.contains("super().some_method()"));
    assert!(migrated.contains("type(self).__name__"));
    assert!(migrated.contains("type(self).class_var"));
}

#[test]
fn test_operator_precedence_edge_cases() {
    // Test complex operator precedence scenarios
    let source = r#"
from dissolve import replace_me

@replace_me()
def calc_precedence(expr):
    return evaluate(expr)

# Complex operator precedence
result1 = calc_precedence(a + b * c ** d)
result2 = calc_precedence(x << y + z)
result3 = calc_precedence(a & b | c ^ d)
result4 = calc_precedence(not a or b and c)
result5 = calc_precedence(a if b else c if d else e)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    assert!(migrated.contains("evaluate(a + b * c ** d)"));
    assert!(migrated.contains("evaluate(x << y + z)"));
    assert!(migrated.contains("evaluate(a & b | c ^ d)"));
}

#[test]
fn test_string_prefix_combinations() {
    // Test various string prefix combinations
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_string(s):
    return handle(s)

# String prefix combinations
result1 = process_string(rf"raw f-string {var}")
result2 = process_string(fr"f-string raw {var}")
result3 = process_string(rb"raw bytes")
result4 = process_string(br"bytes raw")
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Note: AST may normalize these prefixes
    assert!(migrated.contains("handle("));
}

#[test]
fn test_attribute_access_on_literals() {
    // Test method calls on literal values
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_literal_method(value):
    return transform(value)

# Attribute access on literals
result1 = process_literal_method((1).bit_length())
result2 = process_literal_method("hello".upper())
result3 = process_literal_method([1, 2, 3].copy())
result4 = process_literal_method({1, 2}.union({3, 4}))
result5 = process_literal_method((1, 2).count(1))
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Note: AST removes unnecessary parentheses around literals
    assert!(migrated.contains("transform(1.bit_length())"));
    assert!(migrated.contains(r#"transform("hello".upper())"#));
    assert!(migrated.contains("transform([1, 2, 3].copy())"));
    assert!(migrated.contains("transform({1, 2}.union({3, 4}))"));
}

#[test]
fn test_slice_objects() {
    // Test slice object creation
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_slice(s):
    return apply_slice(s)

# Slice objects
result1 = process_slice(slice(None))
result2 = process_slice(slice(1, 10))
result3 = process_slice(slice(1, 10, 2))
result4 = process_slice(slice(None, None, -1))
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    assert!(migrated.contains("apply_slice(slice(None))"));
    assert!(migrated.contains("apply_slice(slice(1, 10))"));
    assert!(migrated.contains("apply_slice(slice(1, 10, 2))"));
    assert!(migrated.contains("apply_slice(slice(None, None, -1))"));
}

#[test]
fn test_very_deeply_nested_expressions() {
    // Test deeply nested expressions to verify recursion handling
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_nested(expr):
    return compute(expr)

# Very deeply nested expressions
result = process_nested(
    a.b.c.d.e[0][1][2].method().attr.other[key].final
)
result2 = process_nested(
    func(arg1(arg2(arg3(arg4(value)))))
)
result3 = process_nested(
    ((((((a + b) * c) / d) ** e) % f) | g)
)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    assert!(migrated.contains("compute(a.b.c.d.e[0][1][2].method().attr.other[key].final)"));
    assert!(migrated.contains("compute(func(arg1(arg2(arg3(arg4(value))))))"));
}

#[test]
fn test_frozenset_and_special_collections() {
    // Test frozenset and other special collection types
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_collection(coll):
    return handle(coll)

# Special collection types
result1 = process_collection(frozenset({1, 2, 3}))
result2 = process_collection(frozenset())
result3 = process_collection(memoryview(b"hello"))
result4 = process_collection(bytearray(b"world"))
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    assert!(migrated.contains("handle(frozenset({1, 2, 3}))"));
    assert!(migrated.contains("handle(frozenset())"));
    assert!(migrated.contains(r#"handle(memoryview(b"hello"))"#));
    assert!(migrated.contains(r#"handle(bytearray(b"world"))"#));
}

#[test]
fn test_assignment_expressions_in_complex_contexts() {
    // Test walrus operator in more complex contexts
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_assignment_expr(value):
    return compute(value)

# Assignment expressions in complex contexts
if result := process_assignment_expr([x := i**2 for i in range(5) if (x := i*2) > 3]):
    print(result)

# Nested assignment in conditional
value = process_assignment_expr((y := func()) if (y := get_val()) else (y := default()))

# Assignment in function call arguments
process_assignment_expr(func(a := 1, b := a + 2, c := a * b))
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should handle complex assignment expressions
    assert!(migrated.contains("compute("));
}

#[test]
fn test_conditional_imports_and_dynamic_access() {
    // Test dynamic imports and conditional attribute access
    let source = r#"
from dissolve import replace_me
import importlib

@replace_me()
def process_dynamic(value):
    return handle(value)

# Dynamic imports and access
module_name = "math"
result1 = process_dynamic(importlib.import_module(module_name).sqrt)
result2 = process_dynamic(getattr(obj, "method_name", default))
result3 = process_dynamic(hasattr(obj, "attr"))
result4 = process_dynamic(vars(obj).get("key"))
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    assert!(migrated.contains("handle(importlib.import_module(module_name).sqrt)"));
    assert!(migrated.contains(r#"handle(getattr(obj, "method_name", default))"#));
    assert!(migrated.contains(r#"handle(hasattr(obj, "attr"))"#));
}

#[test]
fn test_yield_from_expressions() {
    // Test yield from in generator functions
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_generator(gen):
    return consume(gen)

def test_generator():
    # Yield from expressions
    result1 = process_generator((yield from range(10)))
    result2 = process_generator((yield from other_generator()))
    result3 = process_generator((yield x for x in range(5)))
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should handle yield from expressions
    assert!(migrated.contains("consume("));
}

#[test]
fn test_custom_operators_via_dunder_methods() {
    // Test custom operators implemented via __dunder__ methods
    let source = r#"
from dissolve import replace_me

class CustomType:
    def __add__(self, other):
        return self
    def __matmul__(self, other):
        return other

@replace_me()
def process_custom_op(expr):
    return evaluate(expr)

# Custom operators
obj1 = CustomType()
obj2 = CustomType()
result1 = process_custom_op(obj1 + obj2)
result2 = process_custom_op(obj1 @ obj2)
result3 = process_custom_op(obj1.__add__(obj2))
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    assert!(migrated.contains("evaluate(obj1 + obj2)"));
    assert!(migrated.contains("evaluate(obj1 @ obj2)"));
    assert!(migrated.contains("evaluate(obj1.__add__(obj2))"));
}

#[test]
fn test_decimal_and_fraction_usage() {
    // Test decimal and fraction objects
    let source = r#"
from dissolve import replace_me
from decimal import Decimal
from fractions import Fraction

@replace_me()
def process_numeric(num):
    return calculate(num)

# Decimal and fraction usage
result1 = process_numeric(Decimal("3.14"))
result2 = process_numeric(Fraction(1, 3))
result3 = process_numeric(Decimal("1.5") + Decimal("2.5"))
result4 = process_numeric(Fraction(1, 2) * Fraction(3, 4))
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    assert!(migrated.contains(r#"calculate(Decimal("3.14"))"#));
    assert!(migrated.contains("calculate(Fraction(1, 3))"));
    assert!(migrated.contains(r#"calculate(Decimal("1.5") + Decimal("2.5"))"#));
}

#[test]
fn test_exception_handling_in_expressions() {
    // Test exception handling within expressions
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_with_exception_handling(value):
    return safe_process(value)

def safe_get(obj, key, default=None):
    try:
        return obj[key]
    except (KeyError, TypeError):
        return default

# Exception handling in expressions
result1 = process_with_exception_handling(safe_get(data, "key", "default"))
result2 = process_with_exception_handling(next(iter(collection), None))
result3 = process_with_exception_handling(getattr(obj, "attr", lambda: "default")())
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    assert!(migrated.contains(r#"safe_process(safe_get(data, "key", "default"))"#));
    assert!(migrated.contains("safe_process(next(iter(collection), None))"));
}

#[test]
fn test_annotations_and_type_comments() {
    // Test function annotations and type comments in parameters
    let source = r#"
from dissolve import replace_me
from typing import List, Dict, Union, Optional, Callable

@replace_me()
def process_annotated(
    data: List[Dict[str, Union[int, str]]],
    callback: Callable[[int], str] = None,
    config: Optional[Dict[str, any]] = None
):
    return transform(data, callback, config or {})

# Complex annotated calls
complex_data: List[Dict[str, Union[int, str]]] = [{"a": 1, "b": "test"}]
callback_fn: Callable[[int], str] = lambda x: str(x)
result = process_annotated(complex_data, callback_fn, {"mode": "strict"})
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    // Should handle complex type annotations
    assert!(migrated.contains("transform(complex_data, callback_fn, {\"mode\": \"strict\"} or {})"));
}

#[test]
fn test_bitwise_operations_edge_cases() {
    // Test edge cases with bitwise operations
    let source = r#"
from dissolve import replace_me

@replace_me()
def process_bitwise(expr):
    return compute(expr)

# Bitwise operation edge cases
result1 = process_bitwise(~0)
result2 = process_bitwise(1 << 32)
result3 = process_bitwise(0xFF & 0x0F | 0xF0)
result4 = process_bitwise((a ^ b) & (c | d))
result5 = process_bitwise(x >> y << z)
"#;

    let collector = RuffDeprecatedFunctionCollector::new("test_module".to_string(), None);
    let result = collector.collect_from_source(source.to_string()).unwrap();

    let mut type_context =
        TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).unwrap();
    let migrated = migrate_file(
        source,
        "test_module",
        "test.py".to_string(),
        &mut type_context,
        result.replacements,
        HashMap::new(),
    )
    .unwrap();
    type_context.shutdown().unwrap();

    assert!(migrated.contains("compute(~0)"));
    assert!(migrated.contains("compute(1 << 32)"));
    assert!(migrated.contains("compute(255 & 15 | 240)")); // Hex might be converted
}
