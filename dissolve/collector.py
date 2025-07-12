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

"""Collection functionality for @replace_me decorated functions and attributes.

This module provides tools to collect and analyze functions decorated with
@replace_me and attributes marked with replace_me(value), extracting 
replacement expressions and import information.
"""

import re
from enum import Enum
from typing import Optional, Union

import libcst as cst

from .types import ReplacementExtractionError, ReplacementFailureReason


class ConstructType(Enum):
    """Enum representing the type of construct being replaced."""

    FUNCTION = "function"
    PROPERTY = "property"
    CLASSMETHOD = "classmethod"
    STATICMETHOD = "staticmethod"
    ASYNC_FUNCTION = "async_function"
    CLASS = "class"
    CLASS_ATTRIBUTE = "class_attribute"
    MODULE_ATTRIBUTE = "module_attribute"


class ReplaceInfo:
    """Information about a function or class that should be replaced.

    Attributes:
        old_name: The name of the deprecated function or class.
        replacement_expr: The replacement expression template with parameter
            placeholders in the format {param_name}.
        construct_type: The type of construct being replaced.
    """

    def __init__(
        self,
        old_name: str,
        replacement_expr: str,
        construct_type: ConstructType = ConstructType.FUNCTION,
    ) -> None:
        self.old_name = old_name
        self.replacement_expr = replacement_expr
        self.construct_type = construct_type


class UnreplaceableNode:
    """Represents a node that cannot be replaced.

    This is used to indicate that a function, class, or property cannot be replaced
    due to its complexity or structure.
    """

    def __init__(
        self,
        old_name: str,
        reason: ReplacementFailureReason,
        message: str,
        construct_type: ConstructType = ConstructType.FUNCTION,
    ) -> None:
        self.old_name = old_name
        self.reason = reason
        self.message = message
        self.construct_type = construct_type

    def construct_type_str(self) -> str:
        """Return a human-readable description of the construct type."""
        type_map = {
            ConstructType.CLASS: "Class",
            ConstructType.PROPERTY: "Property",
            ConstructType.CLASSMETHOD: "Class method",
            ConstructType.STATICMETHOD: "Static method",
            ConstructType.ASYNC_FUNCTION: "Async function",
            ConstructType.FUNCTION: "Function",
            ConstructType.CLASS_ATTRIBUTE: "Class attribute",
            ConstructType.MODULE_ATTRIBUTE: "Module attribute",
        }
        return type_map[self.construct_type]


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
        self._current_class_decorators: list[cst.Decorator] = []
        self._inside_class: bool = False
        self._current_class_name: Optional[str] = None

    def visit_FunctionDef(self, node: cst.FunctionDef) -> None:
        """Store decorators for processing when we leave the function."""
        self._current_decorators = list(node.decorators)

    def leave_FunctionDef(self, original_node: cst.FunctionDef) -> None:
        """Process function definitions to find @replace_me decorators."""
        # Determine construct type
        construct_type = self._determine_construct_type(
            original_node, self._current_decorators
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
                    construct_type=construct_type,
                )
            except ReplacementExtractionError as e:
                self.unreplaceable[func_name] = UnreplaceableNode(
                    func_name,
                    e.failure_reason,
                    e.details or "No details provided",
                    construct_type=construct_type,
                )

        self._current_decorators = []

    def visit_ClassDef(self, node: cst.ClassDef) -> None:
        """Store decorators for processing when we leave the class."""
        self._current_class_decorators = list(node.decorators)
        self._inside_class = True
        self._current_class_name = node.name.value

    def leave_ClassDef(self, original_node: cst.ClassDef) -> None:
        """Process class definitions to find @replace_me decorators."""
        # Check for @replace_me
        has_replace_me = any(
            self._is_replace_me_decorator(d) for d in self._current_class_decorators
        )

        if has_replace_me:
            class_name = original_node.name.value
            try:
                replacement_expr = self._extract_replacement_from_class(original_node)
                self.replacements[class_name] = ReplaceInfo(
                    class_name,
                    replacement_expr,
                    construct_type=ConstructType.CLASS,
                )
            except ReplacementExtractionError as e:
                self.unreplaceable[class_name] = UnreplaceableNode(
                    class_name,
                    e.failure_reason,
                    e.details or "No details provided",
                    construct_type=ConstructType.CLASS,
                )

        self._current_class_decorators = []
        self._inside_class = False
        self._current_class_name = None

    def _determine_construct_type(
        self, node: cst.FunctionDef, decorators: list[cst.Decorator]
    ) -> ConstructType:
        """Determine the construct type based on decorators and function properties."""
        if any(self._is_decorator_named(d, "property") for d in decorators):
            return ConstructType.PROPERTY
        elif any(self._is_decorator_named(d, "classmethod") for d in decorators):
            return ConstructType.CLASSMETHOD
        elif any(self._is_decorator_named(d, "staticmethod") for d in decorators):
            return ConstructType.STATICMETHOD
        elif (
            isinstance(node, cst.FunctionDef)
            and hasattr(node, "asynchronous")
            and node.asynchronous is not None
        ):
            return ConstructType.ASYNC_FUNCTION
        else:
            return ConstructType.FUNCTION

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

    def visit_SimpleStatementLine(self, node: cst.SimpleStatementLine) -> None:
        """Process simple statements to find replace_me() decorated attributes."""
        # Look for assignments with replace_me() call
        for stmt in node.body:
            if isinstance(stmt, (cst.Assign, cst.AnnAssign)):
                # Check if the value is a replace_me() call
                if self._is_replace_me_call(stmt):
                    self._process_replace_me_attribute(stmt, node)

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

    def _extract_replacement_from_class(self, class_def: cst.ClassDef) -> str:
        """Extract replacement expression from class __init__ method wrapper pattern.

        Args:
            class_def: The class definition CST node.

        Returns:
            The replacement expression with parameter placeholders

        Raises:
            ReplacementExtractionError: If no valid replacement can be extracted
        """
        if not class_def.body:
            raise ReplacementExtractionError(
                class_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "Class has no body",
            )

        # Handle single-line classes vs multi-line
        if isinstance(class_def.body, cst.SimpleStatementSuite):
            body_stmts = list(class_def.body.body)  # type: ignore[arg-type]
        elif isinstance(class_def.body, cst.IndentedBlock):
            body_stmts = list(class_def.body.body)  # type: ignore[arg-type]
        else:
            raise ReplacementExtractionError(
                class_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "Unexpected body type",
            )

        # Look for __init__ method
        init_method = None
        for stmt in body_stmts:
            if isinstance(stmt, cst.SimpleStatementLine):
                continue  # Skip simple statements
            elif isinstance(stmt, cst.FunctionDef) and stmt.name.value == "__init__":
                init_method = stmt
                break

        if not init_method:
            raise ReplacementExtractionError(
                class_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "Class does not have __init__ method for wrapper pattern",
            )

        # Extract wrapper pattern from __init__ method body
        if not init_method.body:
            raise ReplacementExtractionError(
                class_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "__init__ method has no body",
            )

        # Handle single-line vs multi-line __init__ method
        if isinstance(init_method.body, cst.SimpleStatementSuite):
            body_stmts = list(init_method.body.body)  # type: ignore[arg-type]
        elif isinstance(init_method.body, cst.IndentedBlock):
            body_stmts = list(init_method.body.body)  # type: ignore[arg-type]
            # Skip docstring if present
            if body_stmts and self._is_docstring(body_stmts[0]):
                body_stmts = body_stmts[1:]
        else:
            raise ReplacementExtractionError(
                class_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "__init__ method has unexpected body type",
            )

        if not body_stmts:
            raise ReplacementExtractionError(
                class_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "__init__ method has no body statements",
            )

        # Look for wrapper assignment pattern: self._attr = TargetClass(args)
        wrapper_assignment = None
        for stmt in body_stmts:
            if isinstance(stmt, cst.SimpleStatementLine):
                for simple_stmt in stmt.body:
                    if isinstance(simple_stmt, cst.Assign):
                        # Check if this is self._something = SomeClass(...)
                        if len(simple_stmt.targets) == 1 and isinstance(
                            simple_stmt.targets[0].target, cst.Attribute
                        ):
                            attr_node = simple_stmt.targets[0].target
                            if (
                                isinstance(attr_node.value, cst.Name)
                                and attr_node.value.value == "self"
                                and isinstance(simple_stmt.value, cst.Call)
                            ):
                                wrapper_assignment = simple_stmt
                                break

                if wrapper_assignment:
                    break

        if not wrapper_assignment:
            raise ReplacementExtractionError(
                class_def.name.value,
                ReplacementFailureReason.COMPLEX_BODY,
                "__init__ method does not contain wrapper assignment pattern (self._attr = TargetClass(...))",
            )

        # Extract the right-hand side (the constructor call)
        constructor_call = wrapper_assignment.value
        replacement_expr = cst.Module([]).code_for_node(constructor_call)

        # Replace parameters with placeholders (skip 'self' parameter)
        if init_method.params and init_method.params.params:
            for param in init_method.params.params[1:]:  # Skip 'self'
                if isinstance(param.name, cst.Name):
                    param_name = param.name.value
                    # Use word boundary regex to avoid replacing parts of other identifiers
                    replacement_expr = re.sub(
                        rf"\b{re.escape(param_name)}\b",
                        f"{{{param_name}}}",
                        replacement_expr,
                    )

        return replacement_expr

    def _is_docstring(self, stmt: cst.BaseSmallStatement) -> bool:
        """Check if statement is a docstring."""
        if isinstance(stmt, cst.SimpleStatementLine):
            if stmt.body and isinstance(stmt.body[0], cst.Expr):
                expr = stmt.body[0]
                return isinstance(
                    expr.value, (cst.SimpleString, cst.ConcatenatedString)
                )
        return False

    def _extract_replacement_from_value(self, value: cst.BaseExpression) -> str:
        """Extract replacement expression from an attribute value.

        Args:
            value: The value expression of the assignment.

        Returns:
            The replacement expression as a string.

        Raises:
            ReplacementExtractionError: If the value is too complex.
        """
        # For attributes, we simply use the value expression as the replacement
        # This works for simple values like strings, numbers, other attributes, etc.
        replacement_expr = cst.Module([]).code_for_node(value)
        return replacement_expr

    def _is_replace_me_call(self, stmt: Union[cst.Assign, cst.AnnAssign]) -> bool:
        """Check if an assignment's value is a replace_me() call."""
        if isinstance(stmt, cst.Assign):
            value = stmt.value
        else:  # AnnAssign
            if stmt.value is None:
                return False
            value = stmt.value

        # Check if value is a Call node
        if not isinstance(value, cst.Call):
            return False

        # Check if the function being called is 'replace_me'
        func = value.func
        if isinstance(func, cst.Name):
            return func.value == "replace_me"
        elif isinstance(func, cst.Attribute):
            return func.attr.value == "replace_me"

        return False

    def _process_replace_me_attribute(
        self, stmt: Union[cst.Assign, cst.AnnAssign], node: cst.SimpleStatementLine
    ) -> None:
        """Process an attribute assignment using replace_me(value) pattern."""
        # Get target and value
        if isinstance(stmt, cst.Assign):
            if not stmt.targets:
                return
            target = stmt.targets[0].target
            value = stmt.value
        else:  # AnnAssign
            target = stmt.target
            if stmt.value is None:
                return
            value = stmt.value

        # Get the attribute name
        if isinstance(target, cst.Name):
            attr_name = target.value
        else:
            # Complex target (e.g., obj.attr), not supported yet
            return

        # Determine if it's a class or module attribute
        if self._inside_class:
            construct_type = ConstructType.CLASS_ATTRIBUTE
            full_name = f"{self._current_class_name}.{attr_name}"
        else:
            construct_type = ConstructType.MODULE_ATTRIBUTE
            full_name = attr_name

        # Extract the replacement value from the replace_me() call
        if isinstance(value, cst.Call) and value.args:
            # Get the first argument to replace_me()
            first_arg = value.args[0]
            if isinstance(first_arg, cst.Arg) and first_arg.value:
                try:
                    replacement_expr = self._extract_replacement_from_value(
                        first_arg.value
                    )
                    self.replacements[full_name] = ReplaceInfo(
                        full_name,
                        replacement_expr,
                        construct_type=construct_type,
                    )
                except ReplacementExtractionError as e:
                    self.unreplaceable[full_name] = UnreplaceableNode(
                        full_name,
                        e.failure_reason,
                        e.details or "No details provided",
                        construct_type=construct_type,
                    )
