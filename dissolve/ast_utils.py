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

"""AST utilities for parameter substitution in expressions."""

import ast
from collections.abc import Mapping
from typing import Any


class ParameterSubstitutor(ast.NodeTransformer):
    """Replace parameter names in an AST with their actual values."""

    def __init__(self, param_map: Mapping[str, ast.AST]):
        """Initialize with a mapping of parameter names to AST nodes.

        Args:
            param_map: Dictionary mapping parameter names to their AST representations
        """
        self.param_map = param_map

    def visit_Name(self, node: ast.Name) -> ast.AST:
        """Replace Name nodes that match parameters."""
        if node.id in self.param_map:
            # Return a copy of the replacement node
            replacement = self.param_map[node.id]
            if isinstance(replacement, ast.AST):
                return ast.copy_location(replacement, node)
            else:
                # If it's a value, create a Constant node
                return ast.copy_location(ast.Constant(value=replacement), node)
        return self.generic_visit(node)


def substitute_parameters(
    expr_ast: ast.AST, param_map: Mapping[str, ast.AST]
) -> ast.AST:
    """Substitute parameters in an AST expression.

    Args:
        expr_ast: The AST expression containing parameter references
        param_map: Dictionary mapping parameter names to their AST representations

    Returns:
        New AST with parameters substituted
    """
    substitutor = ParameterSubstitutor(param_map)
    return substitutor.visit(expr_ast)


def create_ast_from_value(value: Any) -> ast.AST:
    """Create an AST node from a Python value.

    Args:
        value: Python value to convert to AST

    Returns:
        AST representation of the value
    """
    return ast.Constant(value=value)
