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

import ast
import builtins
from dataclasses import dataclass
from typing import Optional


@dataclass
class ImportInfo:
    """Information about an import statement."""

    module: str
    names: list[tuple[str, Optional[str]]]  # List of (name, alias) tuples
    level: int = 0  # For relative imports

    def to_ast(self) -> ast.ImportFrom:
        """Convert to an AST ImportFrom node."""
        aliases = [ast.alias(name=name, asname=alias) for name, alias in self.names]
        return ast.ImportFrom(module=self.module, names=aliases, level=self.level)


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
                # If we need a specific alias, check it matches
                if self.alias is not None:
                    return alias == self.alias
                return True
        return False


class ImportAnalyzer(ast.NodeVisitor):
    """Analyze AST to find required imports."""

    def __init__(self) -> None:
        self.function_names: set[str] = set()
        self.attribute_accesses: dict[str, set[str]] = {}  # module -> attributes
        self.all_names: set[str] = set()

    def visit_Call(self, node: ast.Call) -> None:
        """Track function calls specifically."""
        if isinstance(node.func, ast.Name):
            self.function_names.add(node.func.id)
        elif isinstance(node.func, ast.Attribute):
            if isinstance(node.func.value, ast.Name):
                module_name = node.func.value.id
                if module_name not in self.attribute_accesses:
                    self.attribute_accesses[module_name] = set()
                self.attribute_accesses[module_name].add(node.func.attr)
        self.generic_visit(node)

    def visit_Name(self, node: ast.Name) -> None:
        """Track all name references."""
        if isinstance(node.ctx, ast.Load):
            self.all_names.add(node.id)
        self.generic_visit(node)


def extract_imports_from_expression(expr: str) -> list[ImportRequirement]:
    """Extract potential import requirements from a replacement expression.

    Args:
        expr: Python expression that may contain references to imported names

    Returns:
        List of import requirements found in the expression
    """
    try:
        tree = ast.parse(expr, mode="eval")
    except SyntaxError:
        return []

    analyzer = ImportAnalyzer()
    analyzer.visit(tree)

    requirements = []

    # Process function calls as potential imports
    builtins_set = set(dir(builtins))

    for name in analyzer.function_names:
        # Skip common builtins
        if name in builtins_set:
            continue
        # Create a requirement (we don't know the module yet)
        requirements.append(ImportRequirement(module="", name=name))

    # Process module.function patterns
    for module, attrs in analyzer.attribute_accesses.items():
        # The module itself might need to be imported
        requirements.append(ImportRequirement(module="", name=module))

    return requirements


class ImportManager:
    """Manages imports in Python source code."""

    def __init__(self, tree: ast.Module):
        self.tree = tree
        self.imports: list[ImportInfo] = []
        self._collect_imports()

    def _collect_imports(self) -> None:
        """Collect existing imports from the AST."""
        for node in self.tree.body:
            if isinstance(node, ast.ImportFrom):
                names = [(alias.name, alias.asname) for alias in node.names]
                self.imports.append(
                    ImportInfo(module=node.module or "", names=names, level=node.level)
                )
            elif isinstance(node, ast.Import):
                # Convert Import to ImportFrom format for consistency
                for alias in node.names:
                    self.imports.append(
                        ImportInfo(
                            module=alias.name,
                            names=[(alias.name, alias.asname)],
                            level=0,
                        )
                    )

    def has_import(self, requirement: ImportRequirement) -> bool:
        """Check if a required import already exists."""
        for imp in self.imports:
            if requirement.matches(imp):
                return True
        return False

    def add_import(self, requirement: ImportRequirement) -> None:
        """Add a new import if it doesn't already exist."""
        if self.has_import(requirement):
            return

        # Find existing import from the same module
        for imp in self.imports:
            if imp.module == requirement.module:
                # Add to existing import
                imp.names.append((requirement.name, requirement.alias))
                # Need to rebuild AST nodes when modifying existing imports
                self._rebuild_import_nodes()
                return

        # Create new import
        new_import = ImportInfo(
            module=requirement.module,
            names=[(requirement.name, requirement.alias)],
            level=0,
        )
        self.imports.append(new_import)

        # Add to AST
        import_node = new_import.to_ast()
        # Insert after other imports
        insert_idx = 0
        for i, node in enumerate(self.tree.body):
            if isinstance(node, (ast.Import, ast.ImportFrom)):
                insert_idx = i + 1
            else:
                break
        self.tree.body.insert(insert_idx, import_node)

    def update_import(self, old_name: str, new_requirement: ImportRequirement) -> None:
        """Update an existing import to use a new module/name."""
        # Remove old import
        for imp in self.imports:
            new_names = []
            for name, alias in imp.names:
                if name != old_name:
                    new_names.append((name, alias))
            imp.names = new_names

        # Add new import
        self.add_import(new_requirement)

        # Update AST
        self._rebuild_import_nodes()

    def _rebuild_import_nodes(self) -> None:
        """Rebuild import nodes in the AST based on current imports."""
        # Remove all import nodes
        new_body = []
        for node in self.tree.body:
            if not isinstance(node, (ast.Import, ast.ImportFrom)):
                new_body.append(node)

        # Add imports back
        import_nodes = []
        for imp in self.imports:
            if imp.names:  # Only add if there are names to import
                import_nodes.append(imp.to_ast())

        # Reconstruct body with imports first
        self.tree.body = import_nodes + new_body
