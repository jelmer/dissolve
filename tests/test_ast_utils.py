# Copyright (C) 2022 Jelmer Vernooij <jelmer@samba.org>
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

import ast
from dissolve.ast_utils import substitute_in_expression, substitute_parameters, create_ast_from_value


def test_substitute_simple_parameters():
    """Test basic parameter substitution."""
    expr = "x + y"
    param_map = {"x": 5, "y": 10}
    result = substitute_in_expression(expr, param_map)
    assert result == "5 + 10"


def test_substitute_parameter_in_function_call():
    """Test parameter substitution in function calls."""
    expr = "func(x, y=y)"
    param_map = {"x": 42, "y": "hello"}
    result = substitute_in_expression(expr, param_map)
    assert result == "func(42, y='hello')"


def test_substitute_parameter_with_substring_names():
    """Test that parameters with substring names are handled correctly."""
    # This is the main bug we're fixing - 'n' should not replace the 'n' in 'range'
    expr = "range(n)"
    param_map = {"n": 5}
    result = substitute_in_expression(expr, param_map)
    assert result == "range(5)"
    
    # More complex case
    expr = "func(name, namespace)"
    param_map = {"name": "'test'", "namespace": "'global'"}
    result = substitute_in_expression(expr, param_map)
    # Should handle the actual values properly
    assert "func(" in result and "'test'" in result and "'global'" in result


def test_substitute_nested_expressions():
    """Test substitution in nested expressions."""
    expr = "func(x + y, z * 2)"
    param_map = {"x": 1, "y": 2, "z": 3}
    result = substitute_in_expression(expr, param_map)
    assert result == "func(1 + 2, 3 * 2)"


def test_substitute_with_string_values():
    """Test substitution with string values."""
    expr = "print(msg)"
    param_map = {"msg": "Hello, World!"}
    result = substitute_in_expression(expr, param_map)
    assert result == "print('Hello, World!')"


def test_substitute_with_list_values():
    """Test substitution with list values."""
    expr = "sum(items)"
    param_map = {"items": [1, 2, 3]}
    result = substitute_in_expression(expr, param_map)
    assert result == "sum([1, 2, 3])"


def test_substitute_with_dict_values():
    """Test substitution with dict values."""
    expr = "process(config)"
    param_map = {"config": {"key": "value"}}
    result = substitute_in_expression(expr, param_map)
    assert result == "process({'key': 'value'})"


def test_substitute_preserves_non_parameters():
    """Test that non-parameter identifiers are preserved."""
    expr = "math.sqrt(x) + len(items)"
    param_map = {"x": 16}
    result = substitute_in_expression(expr, param_map)
    assert result == "math.sqrt(16) + len(items)"


def test_substitute_with_attribute_access():
    """Test substitution with attribute access."""
    expr = "obj.method(x)"
    param_map = {"x": 42, "obj": "self"}
    result = substitute_in_expression(expr, param_map)
    # 'obj' gets replaced but attribute access is preserved
    assert "'self'.method(42)" in result or "self.method(42)" in result


def test_substitute_parameters_with_ast_nodes():
    """Test direct AST substitution."""
    # Create an expression AST
    expr_ast = ast.parse("x + y", mode='eval').body
    
    # Create parameter map with AST nodes
    param_map = {
        "x": ast.Constant(value=5),
        "y": ast.Constant(value=10)
    }
    
    result_ast = substitute_parameters(expr_ast, param_map)
    result = ast.unparse(result_ast)
    assert result == "5 + 10"


def test_no_substitution_for_missing_params():
    """Test that missing parameters are left unchanged."""
    expr = "func(x, y, z)"
    param_map = {"x": 1, "y": 2}  # z is missing
    result = substitute_in_expression(expr, param_map)
    assert result == "func(1, 2, z)"


def test_complex_expression_substitution():
    """Test substitution in a complex real-world expression."""
    expr = "new_api(data=data, mode='legacy', timeout=timeout * 2)"
    param_map = {"data": ["a", "b", "c"], "timeout": 30}
    result = substitute_in_expression(expr, param_map)
    assert result == "new_api(data=['a', 'b', 'c'], mode='legacy', timeout=30 * 2)"