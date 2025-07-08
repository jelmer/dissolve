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

"""Collection functionality for @replace_me decorated functions.

This module provides tools to collect and analyze functions decorated with
@replace_me, extracting replacement expressions and import information.
"""

import ast
import re
from typing import Union

from .ast_helpers import is_replace_me_decorator
from .types import ReplacementExtractionError, ReplacementFailureReason


class ReplaceInfo:
    """Information about a function that should be replaced.

    Attributes:
        old_name: The name of the deprecated function.
        replacement_expr: The replacement expression template with parameter
            placeholders in the format {param_name}.
        is_property: Whether this is a property (attribute access) or a callable.
        is_classmethod: Whether this is a class method.
        is_staticmethod: Whether this is a static method.
    """

    def __init__(
        self,
        old_name: str,
        replacement_expr: str,
        is_property: bool = False,
        is_classmethod: bool = False,
        is_staticmethod: bool = False,
    ) -> None:
        self.old_name = old_name
        self.replacement_expr = replacement_expr
        self.is_property = is_property
        self.is_classmethod = is_classmethod
        self.is_staticmethod = is_staticmethod


class UnreplaceableNode:
    """Represents a node that cannot be replaced.

    This is used to indicate that a function or property cannot be replaced
    due to its complexity or structure.
    """

    def __init__(
        self, old_name: str, reason: ReplacementFailureReason, message: str
    ) -> None:
        self.old_name = old_name
        self.reason = reason
        self.message = message


class ImportInfo:
    """Information about imported names.

    Attributes:
        module: The module being imported from.
        names: List of (name, alias) tuples for imported names.
    """

    def __init__(self, module: str, names: list[tuple[str, Union[str, None]]]) -> None:
        self.module = module
        self.names = names  # List of (name, alias) tuples


class DeprecatedFunctionCollector(ast.NodeVisitor):
    """Collects information about functions decorated with @replace_me.

    This AST visitor traverses Python source code to find:
    - Functions decorated with @replace_me
    - Import statements for resolving external deprecated functions

    Attributes:
        replacements: Mapping from function names to their replacement info.
        imports: List of import information for module resolution.
    """

    def __init__(self) -> None:
        self.replacements: dict[str, ReplaceInfo] = {}
        self.unreplaceable: dict[str, UnreplaceableNode] = {}
        self.imports: list[ImportInfo] = []

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        """Process function definitions to find @replace_me decorators."""
        self._process_decorated_node(node)
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        """Process async function definitions to find @replace_me decorators."""
        self._process_decorated_node(node)
        self.generic_visit(node)

    def _process_decorated_node(
        self, node: Union[ast.FunctionDef, ast.AsyncFunctionDef]
    ) -> None:
        """Process any decorated node (function or property) to find @replace_me decorators."""
        # Check decorator types
        is_property = any(
            isinstance(d, ast.Name) and d.id == "property" for d in node.decorator_list
        )
        is_classmethod = any(
            isinstance(d, ast.Name) and d.id == "classmethod"
            for d in node.decorator_list
        )
        is_staticmethod = any(
            isinstance(d, ast.Name) and d.id == "staticmethod"
            for d in node.decorator_list
        )

        for decorator in node.decorator_list:
            if is_replace_me_decorator(decorator):
                # For the new format, extract replacement from function/property body
                try:
                    replacement_expr = self._extract_replacement_from_body(node)
                except ReplacementExtractionError as e:
                    # If extraction fails, mark as unreplaceable
                    self.unreplaceable[node.name] = UnreplaceableNode(
                        node.name, e.failure_reason, e.details or "No details provided"
                    )
                else:
                    # Create ReplaceInfo with appropriate flags
                    self.replacements[node.name] = ReplaceInfo(
                        node.name,
                        replacement_expr,
                        is_property=is_property,
                        is_classmethod=is_classmethod,
                        is_staticmethod=is_staticmethod,
                    )

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        """Collect import information for module resolution."""
        if node.module:
            names = [(alias.name, alias.asname) for alias in node.names]
            self.imports.append(ImportInfo(node.module, names))
        self.generic_visit(node)

    def _extract_replacement_from_body(
        self,
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

        # Helper to check if a statement is a docstring
        def is_docstring(stmt, index):
            return (
                index == 0
                and isinstance(stmt, ast.Expr)
                and isinstance(stmt.value, ast.Constant)
                and isinstance(stmt.value.value, str)
            )

        # Find the return statement, skipping docstrings
        return_stmt = None
        non_docstring_stmts = [
            stmt for i, stmt in enumerate(func_def.body) if not is_docstring(stmt, i)
        ]

        if len(non_docstring_stmts) == 0:
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function has no body statements",
            )
        elif len(non_docstring_stmts) == 1:
            stmt = non_docstring_stmts[0]
            if isinstance(stmt, ast.Return):
                return_stmt = stmt
            elif isinstance(stmt, ast.Pass):
                # Special case: pass statement is valid
                return "None"
        else:
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function has multiple statements (excluding docstring)",
            )

        if not return_stmt:
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function does not have a return statement",
            )

        if not return_stmt.value:
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function has empty return statement",
            )

        # Create a template with parameter placeholders
        replacement_expr = ast.unparse(return_stmt.value)

        # Replace parameter names with placeholders using word boundaries
        for arg in func_def.args.args:
            param_name = arg.arg
            # Use word boundary regex to avoid replacing parts of other identifiers
            replacement_expr = re.sub(
                rf"\b{re.escape(param_name)}\b", f"{{{param_name}}}", replacement_expr
            )

        return replacement_expr
