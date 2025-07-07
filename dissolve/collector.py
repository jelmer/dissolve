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

import re
from typing import Optional, Union

import libcst as cst

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


class DeprecatedFunctionCollector(cst.CSTVisitor):
    """Collects information about functions decorated with @replace_me.

    This CST visitor traverses Python source code to find:
    - Functions decorated with @replace_me
    - Import statements for resolving external deprecated functions

    CST preserves exact formatting, comments, and whitespace.

    Attributes:
        replacements: Mapping from function names to their replacement info.
        imports: List of import information for module resolution.
    """

    def __init__(self) -> None:
        self.replacements: dict[str, ReplaceInfo] = {}
        self.unreplaceable: dict[str, UnreplaceableNode] = {}
        self.imports: list[ImportInfo] = []
        self._current_decorators: list[cst.Decorator] = []

    def visit_FunctionDef(self, node: cst.FunctionDef) -> None:
        """Store decorators for processing when we leave the function."""
        self._current_decorators = list(node.decorators)

    def leave_FunctionDef(self, original_node: cst.FunctionDef) -> None:
        """Process function definitions to find @replace_me decorators."""
        # Check decorator types
        is_property = any(
            self._is_decorator_named(d, "property") for d in self._current_decorators
        )
        is_classmethod = any(
            self._is_decorator_named(d, "classmethod") for d in self._current_decorators
        )
        is_staticmethod = any(
            self._is_decorator_named(d, "staticmethod")
            for d in self._current_decorators
        )

        # Check for @replace_me
        has_replace_me = any(
            self._is_replace_me_decorator(d) for d in self._current_decorators
        )

        if has_replace_me:
            func_name = original_node.name.value
            try:
                replacement_expr = self._extract_replacement_from_body(original_node)
                self.replacements[func_name] = ReplaceInfo(
                    func_name,
                    replacement_expr,
                    is_property=is_property,
                    is_classmethod=is_classmethod,
                    is_staticmethod=is_staticmethod,
                )
            except ReplacementExtractionError as e:
                self.unreplaceable[func_name] = UnreplaceableNode(
                    func_name, e.failure_reason, e.details or "No details provided"
                )

        self._current_decorators = []

    def visit_ImportFrom(self, node: cst.ImportFrom) -> None:
        """Collect import information for module resolution."""
        if node.module is None:
            return

        # Extract module name
        module_name = self._get_module_name(node.module)
        if not module_name:
            return

        # Extract imported names
        names: list[tuple[str, Optional[str]]] = []
        if isinstance(node.names, cst.ImportStar):
            names = [("*", None)]
        else:
            for name in node.names:
                if isinstance(name, cst.ImportAlias):
                    import_name = self._get_name_value(name.name)
                    alias = self._get_name_value(name.asname) if name.asname else None
                    if import_name:
                        names.append((import_name, alias))

        if names:
            self.imports.append(ImportInfo(module_name, names))

    def _is_decorator_named(self, decorator: cst.Decorator, name: str) -> bool:
        """Check if decorator has the given name."""
        dec = decorator.decorator

        # Handle @name
        if isinstance(dec, cst.Name):
            return dec.value == name
        # Handle @module.name
        elif isinstance(dec, cst.Attribute):
            return dec.attr.value == name
        # Handle @name() or @module.name()
        elif isinstance(dec, cst.Call):
            if isinstance(dec.func, cst.Name):
                return dec.func.value == name
            elif isinstance(dec.func, cst.Attribute):
                return dec.func.attr.value == name
        return False

    def _is_replace_me_decorator(self, decorator: cst.Decorator) -> bool:
        """Check if decorator is @replace_me."""
        return self._is_decorator_named(decorator, "replace_me")

    def _get_module_name(self, module: Union[cst.Name, cst.Attribute]) -> str:
        """Extract module name from a Name or Attribute node."""
        if isinstance(module, cst.Name):
            return module.value
        elif isinstance(module, cst.Attribute):
            parts = []
            current: cst.BaseExpression = module
            while isinstance(current, cst.Attribute):
                parts.append(current.attr.value)
                current = current.value
            if isinstance(current, cst.Name):
                parts.append(current.value)
            return ".".join(reversed(parts))
        return ""

    def _get_name_value(self, name: Union[cst.Name, cst.Attribute, cst.AsName]) -> str:
        """Extract string value from various name nodes."""
        if isinstance(name, cst.Name):
            return name.value
        elif isinstance(name, cst.AsName) and isinstance(name.name, cst.Name):
            return name.name.value
        elif isinstance(name, cst.Attribute):
            return self._get_module_name(name)
        return ""

    def _extract_replacement_from_body(self, func_def: cst.FunctionDef) -> str:
        """Extract replacement expression from function body.

        Args:
            func_def: The function definition CST node.

        Returns:
            The replacement expression with parameter placeholders

        Raises:
            ReplacementExtractionError: If no valid replacement can be extracted
        """
        if not func_def.body:
            raise ReplacementExtractionError(
                func_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function has no body",
            )

        # Handle single-line functions (SimpleStatementSuite) vs multi-line (IndentedBlock)
        if isinstance(func_def.body, cst.SimpleStatementSuite):
            # Single-line function like: def f(): return x
            body_stmts = list(func_def.body.body)  # type: ignore[arg-type]
        elif isinstance(func_def.body, cst.IndentedBlock):
            # Multi-line function with indented body
            body_stmts = list(func_def.body.body)  # type: ignore[arg-type]
            # Skip docstring if present
            if body_stmts and self._is_docstring(body_stmts[0]):
                body_stmts = body_stmts[1:]
        else:
            raise ReplacementExtractionError(
                func_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "Unexpected body type",
            )

        if not body_stmts:
            raise ReplacementExtractionError(
                func_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function has no body statements",
            )

        if len(body_stmts) != 1:
            raise ReplacementExtractionError(
                func_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "Function has multiple statements (excluding docstring)",
            )

        stmt = body_stmts[0]

        # Extract the return statement
        return_stmt = None

        # Handle different statement types
        if isinstance(stmt, cst.Return):
            # Direct return statement (from single-line functions)
            return_stmt = stmt
        elif isinstance(stmt, cst.SimpleStatementLine):
            # Return statement wrapped in SimpleStatementLine (from multi-line functions)
            if stmt.body and isinstance(stmt.body[0], cst.Return):
                return_stmt = stmt.body[0]
            elif stmt.body and isinstance(stmt.body[0], cst.Pass):
                # Special case: pass statement is valid
                return "None"

        if return_stmt:
            if not return_stmt.value:
                raise ReplacementExtractionError(
                    func_def.name.value,
                    ReplacementFailureReason.COMPLEX_BODY,
                    "Function has empty return statement",
                )
            # Get the exact code for the return value, preserving formatting
            replacement_expr = cst.Module([]).code_for_node(return_stmt.value)

            # Replace parameters with placeholders
            for param in func_def.params.params:
                if isinstance(param.name, cst.Name):
                    param_name = param.name.value
                    # Use word boundary regex to avoid replacing parts of other identifiers
                    replacement_expr = re.sub(
                        rf"\b{re.escape(param_name)}\b",
                        f"{{{param_name}}}",
                        replacement_expr,
                    )

            return replacement_expr

        raise ReplacementExtractionError(
            func_def.name.value,
            ReplacementFailureReason.COMPLEX_BODY,
            "Function does not have a return statement",
        )

    def _is_docstring(self, stmt: cst.BaseSmallStatement) -> bool:
        """Check if statement is a docstring."""
        if isinstance(stmt, cst.SimpleStatementLine):
            if stmt.body and isinstance(stmt.body[0], cst.Expr):
                expr = stmt.body[0]
                return isinstance(
                    expr.value, (cst.SimpleString, cst.ConcatenatedString)
                )
        return False
