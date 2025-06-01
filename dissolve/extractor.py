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

"""Replacement expression extraction functionality.

This module provides functionality to extract replacement expressions from
function bodies decorated with @replace_me. It handles various patterns
including simple return statements and multi-statement functions with
assignment chains.
"""

import ast
from typing import Union

from .ast_helpers import (
    contains_local_imports,
    contains_recursive_call,
    filter_out_docstrings,
    substitute_variable_in_expr,
    uses_variable,
)
from .types import ReplacementExtractionError, ReplacementFailureReason


def extract_replacement_from_body(
    func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef],
) -> str:
    """Extract replacement expression from function body.

    Args:
        func_def: The function definition AST node.

    Returns:
        The replacement expression with parameter placeholders

    Raises:
        ReplacementExtractionError: If no valid replacement can be extracted
    """
    if not func_def.body:
        raise ReplacementExtractionError(
            func_def.name,
            ReplacementFailureReason.COMPLEX_BODY,
            "Function has no body",
        )

    # Check for **kwargs - functions with **kwargs should not be migrated
    if func_def.args.kwarg:
        raise ReplacementExtractionError(
            func_def.name,
            ReplacementFailureReason.COMPLEX_BODY,
            "Function uses **kwargs which is not supported",
        )

    # Filter out docstrings from the body
    body_without_docstring = filter_out_docstrings(func_def.body)

    if not body_without_docstring:
        return "None"

    # Handle special cases first
    if len(body_without_docstring) == 1:
        return _extract_from_single_statement(func_def, body_without_docstring[0])
    else:
        return _extract_from_multi_statement(func_def, body_without_docstring)


def _extract_from_single_statement(
    func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef],
    stmt: ast.stmt,
) -> str:
    """Extract replacement from a single-statement function body."""
    if isinstance(stmt, ast.Pass):
        # Function with only pass statement
        return "None"
    elif (
        isinstance(stmt, ast.Expr)
        and isinstance(stmt.value, ast.Constant)
        and stmt.value.value is ...
    ):
        # Function with only ellipsis (...)
        return "None"
    elif not isinstance(stmt, ast.Return):
        raise ReplacementExtractionError(
            func_def.name,
            ReplacementFailureReason.COMPLEX_BODY,
            "Function does not have a return statement",
        )

    # Single return statement - handle normally
    return_stmt = stmt
    if not return_stmt.value:
        raise ReplacementExtractionError(
            func_def.name,
            ReplacementFailureReason.COMPLEX_BODY,
            "Function has empty return statement",
        )

    replacement_expr = ast.unparse(return_stmt.value)
    return _add_parameter_placeholders(func_def, replacement_expr)


def _extract_from_multi_statement(
    func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef],
    body_without_docstring: list[ast.stmt],
) -> str:
    """Extract replacement from a multi-statement function body."""
    # Check for unsupported patterns
    if contains_recursive_call(
        ast.Module(body=body_without_docstring, type_ignores=[]), func_def.name
    ):
        raise ReplacementExtractionError(
            func_def.name,
            ReplacementFailureReason.RECURSIVE_CALL,
            "Function contains recursive calls",
        )

    if contains_local_imports(ast.Module(body=body_without_docstring, type_ignores=[])):
        raise ReplacementExtractionError(
            func_def.name,
            ReplacementFailureReason.LOCAL_IMPORTS,
            "Function contains local imports",
        )

    # Ensure the last statement is a return
    if not isinstance(body_without_docstring[-1], ast.Return):
        raise ReplacementExtractionError(
            func_def.name,
            ReplacementFailureReason.COMPLEX_BODY,
            "Function must end with a return statement",
        )

    return_stmt = body_without_docstring[-1]
    if not return_stmt.value:
        raise ReplacementExtractionError(
            func_def.name,
            ReplacementFailureReason.COMPLEX_BODY,
            "Function has empty return statement",
        )

    # Get all assignment statements before the return
    assignments = body_without_docstring[:-1]

    # Verify all assignments are simple variable assignments
    _validate_assignments(func_def, assignments)

    # Check if assignments form a proper chain or are simple enough to inline
    _validate_assignment_chainability(func_def, assignments, return_stmt)

    # Build inline expression by substituting variables
    result_expr = _substitute_assignments(func_def, assignments, return_stmt.value)

    # Convert to string with parameter placeholders
    replacement_expr = ast.unparse(result_expr)
    return _add_parameter_placeholders(func_def, replacement_expr)


def _validate_assignments(
    func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef],
    assignments: list[ast.stmt],
) -> None:
    """Validate that all assignments are simple variable assignments."""
    for stmt in assignments:
        if not isinstance(stmt, ast.Assign):
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function contains non-assignment statements",
            )
        # Type check: stmt is now guaranteed to be ast.Assign
        assign_stmt = stmt
        if len(assign_stmt.targets) != 1 or not isinstance(
            assign_stmt.targets[0], ast.Name
        ):
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function contains complex assignment patterns",
            )


def _validate_assignment_chainability(
    func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef],
    assignments: list[ast.stmt],
    return_stmt: ast.Return,
) -> None:
    """Validate that assignments can be properly chained or inlined."""
    # For now, only allow single assignment + return pattern
    # Multiple independent assignments are too complex
    if len(assignments) > 1:
        # Check if assignments form a dependency chain
        assignment_vars = set()
        for stmt in assignments:
            if isinstance(stmt, ast.Assign):
                assignment_vars.add(stmt.targets[0].id)  # type: ignore

        # Check if any assignment uses variables from previous assignments
        has_dependency_chain = False
        for i, stmt in enumerate(assignments):
            if isinstance(stmt, ast.Assign):
                var_name = stmt.targets[0].id  # type: ignore
                # Check if this assignment uses any previously assigned variables
                for j in range(i):
                    prev_stmt = assignments[j]
                    if isinstance(prev_stmt, ast.Assign):
                        prev_var = prev_stmt.targets[0].id  # type: ignore
                        if uses_variable(stmt.value, prev_var):
                            has_dependency_chain = True
                            break
                if has_dependency_chain:
                    break

        # If no dependency chain, reject multi-assignment functions
        if not has_dependency_chain:
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function has multiple independent assignments",
            )

    # Check that all assignment variables are used somewhere
    for stmt in assignments:
        if isinstance(stmt, ast.Assign):
            var_name = stmt.targets[0].id  # type: ignore
            is_used = False

            # Check if variable is used in return statement
            if return_stmt.value and uses_variable(return_stmt.value, var_name):
                is_used = True
            else:
                # Check if variable is used in any later assignment
                stmt_index = assignments.index(stmt)
                for later_stmt in assignments[stmt_index + 1 :]:
                    if isinstance(later_stmt, ast.Assign) and uses_variable(
                        later_stmt.value, var_name
                    ):
                        is_used = True
                        break

            if not is_used:
                raise ReplacementExtractionError(
                    func_def.name,
                    ReplacementFailureReason.COMPLEX_BODY,
                    f"Assignment variable '{var_name}' is not used",
                )


def _substitute_assignments(
    func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef],
    assignments: list[ast.stmt],
    return_expr: ast.expr,
) -> ast.expr:
    """Substitute assignment variables in the return expression."""
    result_expr = return_expr

    # Substitute each assignment in reverse order
    for stmt in reversed(assignments):
        if isinstance(stmt, ast.Assign):
            var_name = stmt.targets[0].id  # type: ignore
            new_expr = substitute_variable_in_expr(result_expr, var_name, stmt.value)
            if new_expr is None:
                raise ReplacementExtractionError(
                    func_def.name,
                    ReplacementFailureReason.COMPLEX_BODY,
                    f"Failed to substitute variable '{var_name}'",
                )
            # Type assertion: we know it's an expression since we started with one
            assert isinstance(new_expr, ast.expr)
            result_expr = new_expr

    return result_expr


def _add_parameter_placeholders(
    func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef],
    replacement_expr: str,
) -> str:
    """Add parameter placeholders to the replacement expression."""
    # Replace parameter names with placeholders
    for arg in func_def.args.args:
        param_name = arg.arg
        replacement_expr = replacement_expr.replace(param_name, f"{{{param_name}}}")

    # Handle *args parameter
    if func_def.args.vararg:
        vararg_name = func_def.args.vararg.arg
        replacement_expr = replacement_expr.replace(vararg_name, f"{{{vararg_name}}}")

    return replacement_expr
