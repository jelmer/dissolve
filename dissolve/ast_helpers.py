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

"""Shared AST helper utilities for the dissolve package."""

import ast
from collections.abc import Callable
from typing import Union


def is_replace_me_decorator(decorator: ast.AST) -> bool:
    """Check if a decorator is @replace_me.

    Args:
        decorator: The decorator AST node to check.

    Returns:
        True if the decorator is @replace_me, False otherwise.
    """
    if isinstance(decorator, ast.Name) and decorator.id == "replace_me":
        return True
    if isinstance(decorator, ast.Call):
        if isinstance(decorator.func, ast.Name) and decorator.func.id == "replace_me":
            return True
        if (
            isinstance(decorator.func, ast.Attribute)
            and decorator.func.attr == "replace_me"
        ):
            return True
    return False


def get_single_return_value(
    func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef],
) -> Union[ast.AST, None]:
    """Extract return value from single-statement function.

    Args:
        func_def: The function definition to analyze.

    Returns:
        The return value AST node if function has single return statement, None otherwise.
    """
    if not func_def.body or len(func_def.body) != 1:
        return None

    stmt = func_def.body[0]
    if isinstance(stmt, ast.Return):
        return stmt.value
    return None


def contains_ast_pattern(
    node: ast.AST, pattern_checker: Callable[[ast.AST], bool]
) -> bool:
    """Check if an AST node contains a specific pattern.

    Args:
        node: The AST node to search.
        pattern_checker: A function that takes an AST node and returns True if it matches.

    Returns:
        True if the pattern is found, False otherwise.
    """

    class PatternVisitor(ast.NodeVisitor):
        def __init__(self) -> None:
            self.found = False

        def visit(self, node) -> None:
            if pattern_checker(node):
                self.found = True
                return
            self.generic_visit(node)

    visitor = PatternVisitor()
    visitor.visit(node)
    return visitor.found
