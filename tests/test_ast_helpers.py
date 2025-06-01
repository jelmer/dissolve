# Copyright (C) 2024 Jelmer Vernooij <jelmer@samba.org>
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#    http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Tests for dissolve.ast_helpers module."""

import ast

from dissolve.ast_helpers import (
    contains_local_imports,
    contains_recursive_call,
    expr_to_replacement_string,
    extract_module_names,
    extract_names_from_ast,
    filter_out_docstrings,
    get_variables_used,
    substitute_variable_in_expr,
    uses_variable,
)


class TestFilterOutDocstrings:
    """Tests for filter_out_docstrings function."""

    def test_empty_body(self):
        """Test filtering empty function body."""
        result = filter_out_docstrings([])
        assert result == []

    def test_no_docstring(self):
        """Test filtering body with no docstring."""
        body = [
            ast.Return(value=ast.Constant(value=42)),
        ]
        result = filter_out_docstrings(body)
        assert len(result) == 1
        assert isinstance(result[0], ast.Return)

    def test_with_docstring(self):
        """Test filtering body with docstring."""
        body = [
            ast.Expr(value=ast.Constant(value="This is a docstring")),
            ast.Return(value=ast.Constant(value=42)),
        ]
        result = filter_out_docstrings(body)
        assert len(result) == 1
        assert isinstance(result[0], ast.Return)

    def test_docstring_only(self):
        """Test filtering body with only docstring."""
        body = [
            ast.Expr(value=ast.Constant(value="This is a docstring")),
        ]
        result = filter_out_docstrings(body)
        assert result == []

    def test_multiple_strings_only_first_is_docstring(self):
        """Test that only the first string literal is treated as docstring."""
        body = [
            ast.Expr(value=ast.Constant(value="This is a docstring")),
            ast.Expr(value=ast.Constant(value="This is not a docstring")),
            ast.Return(value=ast.Constant(value=42)),
        ]
        result = filter_out_docstrings(body)
        assert len(result) == 2
        assert isinstance(result[0], ast.Expr)
        assert isinstance(result[1], ast.Return)


class TestContainsRecursiveCall:
    """Tests for contains_recursive_call function."""

    def test_no_recursive_call(self):
        """Test AST without recursive calls."""
        node = ast.parse("other_func(x)").body[0]
        result = contains_recursive_call(node, "my_func")
        assert result is False

    def test_has_recursive_call(self):
        """Test AST with recursive call."""
        node = ast.parse("my_func(x - 1)").body[0]
        result = contains_recursive_call(node, "my_func")
        assert result is True

    def test_nested_recursive_call(self):
        """Test AST with nested recursive call."""
        node = ast.parse("x + my_func(y)").body[0]
        result = contains_recursive_call(node, "my_func")
        assert result is True

    def test_similar_name_not_recursive(self):
        """Test that similar but different names don't match."""
        node = ast.parse("my_function(x)").body[0]
        result = contains_recursive_call(node, "my_func")
        assert result is False


class TestContainsLocalImports:
    """Tests for contains_local_imports function."""

    def test_no_imports(self):
        """Test AST without imports."""
        node = ast.parse("x + y").body[0]
        result = contains_local_imports(node)
        assert result is False

    def test_has_import(self):
        """Test AST with import statement."""
        node = ast.parse("import os").body[0]
        result = contains_local_imports(node)
        assert result is True

    def test_has_import_from(self):
        """Test AST with import from statement."""
        node = ast.parse("from os import path").body[0]
        result = contains_local_imports(node)
        assert result is True


class TestUsesVariable:
    """Tests for uses_variable function."""

    def test_variable_not_used(self):
        """Test expression that doesn't use the variable."""
        expr = ast.parse("x + y", mode="eval").body
        result = uses_variable(expr, "z")
        assert result is False

    def test_variable_used(self):
        """Test expression that uses the variable."""
        expr = ast.parse("x + z", mode="eval").body
        result = uses_variable(expr, "z")
        assert result is True

    def test_variable_in_function_call(self):
        """Test variable used in function call."""
        expr = ast.parse("func(z)", mode="eval").body
        result = uses_variable(expr, "z")
        assert result is True

    def test_variable_assigned_not_loaded(self):
        """Test that assignment context doesn't count as usage."""
        # This would be a statement, not an expression, but let's test the Name node directly
        name_node = ast.Name(id="z", ctx=ast.Store())
        result = uses_variable(name_node, "z")
        assert result is False


class TestSubstituteVariableInExpr:
    """Tests for substitute_variable_in_expr function."""

    def test_simple_substitution(self):
        """Test simple variable substitution."""
        expr = ast.parse("x + y", mode="eval").body
        replacement = ast.Constant(value=42)
        result = substitute_variable_in_expr(expr, "x", replacement)

        assert result is not None
        # Check that x was replaced with 42
        assert isinstance(result, ast.BinOp)
        assert isinstance(result.left, ast.Constant)
        assert result.left.value == 42

    def test_no_variable_to_substitute(self):
        """Test when variable to substitute is not present."""
        expr = ast.parse("x + y", mode="eval").body
        replacement = ast.Constant(value=42)
        result = substitute_variable_in_expr(expr, "z", replacement)

        assert result is not None
        # Original expression should be unchanged
        assert isinstance(result, ast.BinOp)
        assert isinstance(result.left, ast.Name)
        assert result.left.id == "x"

    def test_multiple_occurrences(self):
        """Test substitution of multiple occurrences."""
        expr = ast.parse("x + x * 2", mode="eval").body
        replacement = ast.Constant(value=5)
        result = substitute_variable_in_expr(expr, "x", replacement)

        assert result is not None
        # Both x's should be replaced
        assert isinstance(result, ast.BinOp)
        assert isinstance(result.left, ast.Constant)
        assert result.left.value == 5


class TestGetVariablesUsed:
    """Tests for get_variables_used function."""

    def test_simple_expression(self):
        """Test simple expression with variables."""
        expr = ast.parse("x + y", mode="eval").body
        result = get_variables_used(expr)
        assert result == {"x", "y"}

    def test_with_builtins(self):
        """Test that builtins are excluded."""
        expr = ast.parse("len(x) + int(y)", mode="eval").body
        result = get_variables_used(expr)
        assert result == {"x", "y"}  # len and int should be excluded

    def test_constants_excluded(self):
        """Test that constants like True, False, None are excluded."""
        expr = ast.parse("x and True or None", mode="eval").body
        result = get_variables_used(expr)
        assert result == {"x"}

    def test_function_calls(self):
        """Test function calls and their arguments."""
        expr = ast.parse("func(x, y=z)", mode="eval").body
        result = get_variables_used(expr)
        assert result == {"func", "x", "z"}


class TestExtractModuleNames:
    """Tests for extract_module_names function."""

    def test_simple_attribute_access(self):
        """Test simple module.attribute pattern."""
        tree = ast.parse("os.path.join(x)")
        result = extract_module_names(tree)
        assert "os" in result

    def test_nested_attribute_access(self):
        """Test nested attribute access."""
        tree = ast.parse("a.b.c.method()")
        result = extract_module_names(tree)
        assert "a" in result

    def test_no_attribute_access(self):
        """Test expression without attribute access."""
        tree = ast.parse("func(x)")
        result = extract_module_names(tree)
        assert result == set()

    def test_multiple_modules(self):
        """Test multiple module accesses."""
        tree = ast.parse("os.path.join(sys.argv[0])")
        result = extract_module_names(tree)
        assert "os" in result
        assert "sys" in result


class TestExprToReplacementString:
    """Tests for expr_to_replacement_string function."""

    def test_simple_expression(self):
        """Test simple expression with parameters."""
        func_def = ast.parse("def func(x, y): pass").body[0]
        expr = ast.parse("x + y", mode="eval").body
        result = expr_to_replacement_string(expr, func_def)
        assert result == "{x} + {y}"

    def test_with_constants(self):
        """Test expression with constants and parameters."""
        func_def = ast.parse("def func(x): pass").body[0]
        expr = ast.parse("x * 2 + 1", mode="eval").body
        result = expr_to_replacement_string(expr, func_def)
        assert result == "{x} * 2 + 1"

    def test_function_call(self):
        """Test function call with parameters."""
        func_def = ast.parse("def func(x, y): pass").body[0]
        expr = ast.parse("some_func(x, y)", mode="eval").body
        result = expr_to_replacement_string(expr, func_def)
        assert result == "some_func({x}, {y})"

    def test_varargs(self):
        """Test function with *args."""
        func_def = ast.parse("def func(x, *args): pass").body[0]
        expr = ast.parse("new_func(x, args)", mode="eval").body
        result = expr_to_replacement_string(expr, func_def)
        assert result == "new_func({x}, {args})"

    def test_parameter_name_ordering(self):
        """Test that longer parameter names are replaced first."""
        func_def = ast.parse("def func(a, ab): pass").body[0]
        expr = ast.parse("ab + a", mode="eval").body
        result = expr_to_replacement_string(expr, func_def)
        assert result == "{ab} + {a}"  # Should not become "{a}b + {a}"


class TestExtractNamesFromAst:
    """Tests for extract_names_from_ast function."""

    def test_simple_names(self):
        """Test extracting names from simple expression."""
        node = ast.parse("x + y")
        result = extract_names_from_ast(node)
        assert "x" in result
        assert "y" in result

    def test_exclude_builtins_default(self):
        """Test that builtins are excluded by default."""
        node = ast.parse("len(x)")
        result = extract_names_from_ast(node)
        assert "x" in result
        assert "len" not in result

    def test_include_builtins(self):
        """Test including builtins when requested."""
        node = ast.parse("len(x)")
        result = extract_names_from_ast(node, include_builtins=True)
        assert "x" in result
        assert "len" in result

    def test_context_filter_load_only(self):
        """Test filtering by Load context only."""
        # Create an assignment statement: x = y
        node = ast.parse("x = y")
        result = extract_names_from_ast(
            node, context_filter=lambda ctx: isinstance(ctx, ast.Load)
        )
        assert "y" in result  # y is in Load context
        assert "x" not in result  # x is in Store context

    def test_context_filter_all(self):
        """Test without context filter (includes all contexts)."""
        node = ast.parse("x = y")
        result = extract_names_from_ast(node)
        assert "y" in result
        assert "x" in result
