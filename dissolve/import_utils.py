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

"""Utilities for analyzing and managing imports in Python code."""

import builtins
from dataclasses import dataclass
from typing import Optional

import libcst as cst


@dataclass
class ImportInfo:
    """Information about an import statement."""

    module: str
    names: list[tuple[str, Optional[str]]]  # List of (name, alias) tuples
    level: int = 0  # For relative imports


@dataclass
class ImportRequirement:
    """A required import derived from replacement expressions."""

    module: str
    name: str
    alias: Optional[str] = None
    is_local_reference: bool = False  # True if this refers to something defined locally
    suggested_module: Optional[str] = None  # Suggested module based on common patterns

    def matches(self, import_info: ImportInfo) -> bool:
        """Check if this requirement is satisfied by an import."""
        if import_info.module != self.module:
            return False
        for name, alias in import_info.names:
            if name == self.name:
                # Check if alias matches (if we have one)
                if self.alias is not None:
                    return alias == self.alias
                return True
        return False


class ImportAnalyzer(cst.CSTVisitor):
    """Analyzes expressions to find required imports using CST."""

    def __init__(self) -> None:
        self.requirements: set[ImportRequirement] = set()
        self.in_attribute = False

    def visit_Name(self, node: cst.Name) -> None:
        """Visit Name nodes to find potential import requirements."""
        name = node.value

        # Skip if it's a Python builtin
        if hasattr(builtins, name):
            return

        # Skip if we're part of an attribute access
        if self.in_attribute:
            return

        # Common patterns for module names
        common_modules = {
            "np": "numpy",
            "pd": "pandas",
            "plt": "matplotlib.pyplot",
            "tf": "tensorflow",
            "torch": "torch",
            "cv2": "cv2",
        }

        # Check if it's a known alias
        if name in common_modules:
            self.requirements.add(
                ImportRequirement(
                    module=common_modules[name],
                    name=common_modules[name].split(".")[-1],
                    alias=name,
                    suggested_module=common_modules[name],
                )
            )
        else:
            # Mark as potentially a local reference
            self.requirements.add(
                ImportRequirement(
                    module="",  # Unknown module
                    name=name,
                    is_local_reference=True,
                )
            )

    def visit_Attribute(self, node: cst.Attribute) -> None:
        """Visit Attribute nodes to find module.function patterns."""
        # Set flag to skip the attribute name itself
        self.in_attribute = True
        # Visit the value part of the attribute
        node.value.visit(self)
        self.in_attribute = False

        # Check for common module patterns
        if isinstance(node.value, cst.Name):
            module_name = node.value.value
            attr_name = node.attr.value

            # Common module patterns
            if module_name in ["os", "sys", "re", "json", "math", "datetime"]:
                self.requirements.add(
                    ImportRequirement(
                        module=module_name,
                        name=attr_name,
                    )
                )
            elif module_name == "np" and attr_name in ["array", "zeros", "ones"]:
                self.requirements.add(
                    ImportRequirement(
                        module="numpy",
                        name=attr_name,
                        alias="np",
                        suggested_module="numpy",
                    )
                )


def analyze_import_requirements(expr: str) -> set[ImportRequirement]:
    """Analyze an expression to determine what imports it requires.

    This function parses a Python expression and identifies what names
    need to be imported for the expression to be valid.

    Args:
        expr: Python expression to analyze

    Returns:
        Set of ImportRequirement objects
    """
    try:
        # Try to parse as an expression first
        tree = cst.parse_expression(expr)
    except cst.ParserSyntaxError:
        try:
            # If that fails, try as a module (for statements)
            tree = cst.parse_module(expr)  # type: ignore[assignment]
        except cst.ParserSyntaxError:
            # If both fail, return empty set
            return set()

    analyzer = ImportAnalyzer()
    if isinstance(tree, cst.Module):
        wrapper = cst.MetadataWrapper(tree)
        wrapper.visit(analyzer)
    else:
        # For expressions, wrap in a module first
        module = cst.Module(body=[cst.SimpleStatementLine(body=[cst.Expr(tree)])])
        wrapper = cst.MetadataWrapper(module)
        wrapper.visit(analyzer)
    return analyzer.requirements


def add_import_to_module(
    module: cst.Module,
    module_name: str,
    import_names: list[tuple[str, Optional[str]]],
    level: int = 0,
) -> cst.Module:
    """Add an import to a module, avoiding duplicates and maintaining order.

    Args:
        module: CST module to add import to
        module_name: Module to import from
        import_names: List of (name, alias) tuples to import
        level: Import level for relative imports

    Returns:
        Modified module with import added
    """
    # Create the new import
    aliases = []
    for name, alias in import_names:
        aliases.append(
            cst.ImportAlias(
                name=cst.Name(name),
                asname=cst.AsName(
                    name=cst.Name(alias),
                    whitespace_before_as=cst.SimpleWhitespace(" "),
                    whitespace_after_as=cst.SimpleWhitespace(" "),
                )
                if alias
                else None,
            )
        )

    if level > 0:
        # Relative import
        new_import = cst.ImportFrom(
            module=cst.Attribute(value=cst.Name(module_name), attr=cst.Name(""))
            if "." in module_name and module_name
            else (cst.Name(module_name) if module_name else None),
            names=aliases,
            relative=[cst.Dot() for _ in range(level)],
        )
    else:
        # Absolute import
        new_import = cst.ImportFrom(
            module=cst.Attribute(value=cst.Name(module_name), attr=cst.Name(""))
            if "." in module_name
            else cst.Name(module_name),
            names=aliases,
        )

    # Check if import already exists
    class ImportChecker(cst.CSTVisitor):
        def __init__(
            self, target_module: str, target_names: list[tuple[str, Optional[str]]]
        ):
            self.target_module = target_module
            self.target_names = set(name for name, _ in target_names)
            self.exists = False

        def visit_ImportFrom(self, node: cst.ImportFrom) -> None:
            if node.module and self._get_module_name(node.module) == self.target_module:
                if isinstance(node.names, cst.ImportStar):
                    self.exists = True
                else:
                    imported_names = {
                        alias.name.value if isinstance(alias.name, cst.Name) else ""
                        for alias in node.names
                    }
                    if self.target_names.issubset(imported_names):
                        self.exists = True

        def _get_module_name(self, module: cst.BaseExpression) -> str:
            if isinstance(module, cst.Name):
                return module.value
            elif isinstance(module, cst.Attribute):
                # Handle dotted imports - simplified for now
                return str(module)
            return ""

    checker = ImportChecker(module_name, import_names)
    wrapper = cst.MetadataWrapper(module)
    wrapper.visit(checker)

    if checker.exists:
        return module

    # Add import at the top, after any module docstring
    body = list(module.body)

    # Find the position to insert (after docstring and other imports)
    insert_pos = 0

    # Skip module docstring
    if body and isinstance(body[0], cst.SimpleStatementLine):
        if body[0].body and isinstance(body[0].body[0], cst.Expr):
            if isinstance(
                body[0].body[0].value, (cst.SimpleString, cst.ConcatenatedString)
            ):
                insert_pos = 1

    # Find last import position
    for i in range(insert_pos, len(body)):
        stmt = body[i]
        if isinstance(stmt, (cst.SimpleStatementLine, cst.BaseCompoundStatement)):
            if isinstance(stmt, cst.SimpleStatementLine) and stmt.body:
                first_stmt = stmt.body[0]
                if not isinstance(first_stmt, (cst.Import, cst.ImportFrom)):
                    break
            else:
                break
        insert_pos = i + 1

    # Create import statement line
    import_line = cst.SimpleStatementLine(body=[new_import])

    # Insert the import
    new_body = [*body[:insert_pos], import_line, *body[insert_pos:]]

    return module.with_changes(body=new_body)


def remove_unused_imports(module: cst.Module) -> cst.Module:
    """Remove unused imports from a module.

    This is a simplified version that could be expanded with proper usage analysis.

    Args:
        module: CST module to clean up

    Returns:
        Module with unused imports removed
    """
    # For now, just return the module unchanged
    # A full implementation would analyze name usage throughout the module
    return module
