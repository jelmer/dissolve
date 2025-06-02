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
from typing import Union

from .ast_helpers import is_replace_me_decorator
from .types import ReplacementExtractionError, ReplacementFailureReason


class ReplaceInfo:
    """Information about a function that should be replaced.

    Attributes:
        old_name: The name of the deprecated function.
        replacement_expr: The replacement expression template with parameter
            placeholders in the format {param_name}.
    """

    def __init__(self, old_name: str, replacement_expr: str) -> None:
        self.old_name = old_name
        self.replacement_expr = replacement_expr


class UnreplaceableNode:
    """Represents a node that cannot be replaced.

    This is used to indicate that a function or property cannot be replaced
    due to its complexity or structure.
    """

    def __init__(self, old_name: str, reason: ReplacementFailureReason, message: str) -> None:
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
        # Check if this is a property getter
        is_property = any(
            isinstance(d, ast.Name) and d.id == "property" for d in node.decorator_list
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
                    # For properties, we need to handle them as attribute access
                    if is_property:
                        # Property access is obj.property_name, no parentheses
                        self.replacements[node.name] = ReplaceInfo(
                            node.name, replacement_expr
                        )
                    else:
                        self.replacements[node.name] = ReplaceInfo(
                            node.name, replacement_expr
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

        if len(func_def.body) != 1:
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function has multiple statements",
            )

        stmt = func_def.body[0]
        if not isinstance(stmt, ast.Return):
            # Special case: pass statement is valid but not extractable
            if isinstance(stmt, ast.Pass):
                return "None"
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function does not have a return statement",
            )

        if not stmt.value:
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function has empty return statement",
            )

        # Create a template with parameter placeholders
        replacement_expr = ast.unparse(stmt.value)

        # Replace parameter names with placeholders
        for arg in func_def.args.args:
            param_name = arg.arg
            replacement_expr = replacement_expr.replace(param_name, f"{{{param_name}}}")

        return replacement_expr
