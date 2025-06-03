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
from typing import Optional, Union

from .ast_helpers import is_replace_me_decorator
from .extractor import extract_replacement_from_body
from .types import ReplacementExtractionError, ReplacementFailureReason


class ReplaceInfo:
    """Information about a function that should be replaced.

    Attributes:
        old_name: The name of the deprecated function.
        replacement_expr: The replacement expression template with parameter
            placeholders in the format {param_name}.
        func_def: Optional AST node of the function definition.
        source_module: Optional module name where the function is defined.
    """

    def __init__(
        self, old_name: str, replacement_expr: str, func_def=None, source_module=None
    ) -> None:
        self.old_name = old_name
        self.replacement_expr = replacement_expr
        self.func_def = func_def
        self.source_module = source_module


class UnreplaceableNode:
    """Represents a node that cannot be replaced.

    This is used to indicate that a function or property cannot be replaced
    due to its complexity or structure.
    """

    def __init__(
        self,
        old_name: str,
        reason: ReplacementFailureReason,
        message: str,
        node: Optional[ast.AST] = None,
    ) -> None:
        self.old_name = old_name
        self.reason = reason
        self.message = message
        self.node = node


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
                    # Make a copy to avoid mutating the original AST
                    import copy

                    node_copy = copy.deepcopy(node)
                    replacement_expr = extract_replacement_from_body(node_copy)
                except ReplacementExtractionError as e:
                    # If extraction fails, mark as unreplaceable
                    self.unreplaceable[node.name] = UnreplaceableNode(
                        node.name,
                        e.failure_reason,
                        e.details or "No details provided",
                        node,
                    )
                else:
                    # For properties, we need to handle them as attribute access
                    if is_property:
                        # Property access is obj.property_name, no parentheses
                        self.replacements[node.name] = ReplaceInfo(
                            node.name, replacement_expr, func_def=node
                        )
                    else:
                        self.replacements[node.name] = ReplaceInfo(
                            node.name, replacement_expr, func_def=node
                        )

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        """Collect import information for module resolution."""
        if node.module:
            names = [(alias.name, alias.asname) for alias in node.names]
            self.imports.append(ImportInfo(node.module, names))
        self.generic_visit(node)

    def _get_non_parameter_names(self, func_def, replacement_expr: str) -> set[str]:
        """Get names in replacement expression that are not parameters."""
        import ast

        from .ast_helpers import extract_module_names, extract_names_from_ast

        # Get parameter names
        param_names = {arg.arg for arg in func_def.args.args}
        if func_def.args.vararg:
            param_names.add(func_def.args.vararg.arg)

        # Replace placeholders with dummy parameter names for parsing
        temp_expr = replacement_expr
        for param in param_names:
            temp_expr = temp_expr.replace(f"{{{param}}}", param)

        # Parse replacement expression
        try:
            tree = ast.parse(temp_expr, mode="eval")
        except SyntaxError:
            return set()

        # Get all names used as variables (not modules)
        all_names = extract_names_from_ast(
            tree, context_filter=lambda ctx: isinstance(ctx, ast.Load)
        )

        # Get module names separately
        module_names = extract_module_names(tree)

        # Return names that are not parameters and not modules
        return (all_names - param_names) - module_names
