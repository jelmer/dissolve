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

"""Context analysis for understanding local definitions and import requirements."""

import ast
import builtins
from typing import Optional

from .import_utils import ImportRequirement


class ContextAnalyzer(ast.NodeVisitor):
    """Analyze a module's context to understand local definitions and imports."""

    def __init__(self) -> None:
        self.local_functions: set[str] = set()
        self.local_classes: set[str] = set()
        self.local_variables: set[str] = set()
        self.imported_names: dict[str, str] = {}  # name -> module
        self.import_aliases: dict[str, str] = {}  # alias -> original_name
        self.constants: set[str] = set()

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        """Track function definitions."""
        self.local_functions.add(node.name)
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        """Track async function definitions."""
        self.local_functions.add(node.name)
        self.generic_visit(node)

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        """Track class definitions."""
        self.local_classes.add(node.name)
        self.generic_visit(node)

    def visit_Assign(self, node: ast.Assign) -> None:
        """Track variable assignments."""
        for target in node.targets:
            if isinstance(target, ast.Name):
                self.local_variables.add(target.id)
                # Check if it's a constant (all uppercase)
                if target.id.isupper():
                    self.constants.add(target.id)
        self.generic_visit(node)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        """Track imports."""
        if node.module:
            for alias in node.names:
                name = alias.name
                asname = alias.asname or name
                self.imported_names[asname] = node.module
                if alias.asname:
                    self.import_aliases[alias.asname] = name
        self.generic_visit(node)

    def visit_Import(self, node: ast.Import) -> None:
        """Track import statements."""
        for alias in node.names:
            name = alias.name
            asname = alias.asname or name
            self.imported_names[asname] = name  # For import x, module is x
            if alias.asname:
                self.import_aliases[alias.asname] = name
        self.generic_visit(node)

    def is_local_reference(self, name: str) -> bool:
        """Check if a name refers to something defined locally."""
        return (
            name in self.local_functions
            or name in self.local_classes
            or name in self.local_variables
            or name in self.constants
        )

    def get_import_source(self, name: str) -> Optional[str]:
        """Get the module where a name is imported from."""
        return self.imported_names.get(name)

    def resolve_alias(self, alias: str) -> str:
        """Resolve an alias to its original name."""
        return self.import_aliases.get(alias, alias)


def analyze_imports_in_codebase(context: ContextAnalyzer) -> dict[str, set[str]]:
    """Analyze the codebase to discover import patterns from actual usage."""
    # This could be extended to analyze the entire codebase for patterns
    # For now, we return patterns based on what's already imported
    patterns: dict[str, set[str]] = {}
    for name, module in context.imported_names.items():
        if module not in patterns:
            patterns[module] = set()
        patterns[module].add(name)
    return patterns


def suggest_import_module(
    function_name: str, context: ContextAnalyzer
) -> Optional[str]:
    """Suggest which module a function might come from based on existing imports."""
    # Look at existing import patterns in this file
    patterns = analyze_imports_in_codebase(context)

    for module, names in patterns.items():
        if function_name in names:
            return module

    # If not found in existing patterns, return None
    # This encourages explicit import management rather than guessing
    return None


def analyze_replacement_context(
    replacement_expr: str, context: ContextAnalyzer
) -> list[ImportRequirement]:
    """Analyze a replacement expression in the context of a module."""
    try:
        tree = ast.parse(replacement_expr, mode="eval")
    except SyntaxError:
        return []

    analyzer = ReplacementAnalyzer(context)
    analyzer.visit(tree)

    return analyzer.requirements


class ReplacementAnalyzer(ast.NodeVisitor):
    """Analyze replacement expressions for import requirements."""

    def __init__(self, context: ContextAnalyzer) -> None:
        self.context = context
        self.requirements: list[ImportRequirement] = []
        # Get builtins dynamically from the builtins module
        self.builtins = set(dir(builtins))
        # Add commonly available constants
        self.builtins.update({"True", "False", "None", "Ellipsis", "NotImplemented"})

    def visit_Call(self, node: ast.Call) -> None:
        """Analyze function calls."""
        if isinstance(node.func, ast.Name):
            func_name = node.func.id
            self._analyze_name(func_name)
        elif isinstance(node.func, ast.Attribute):
            self._analyze_attribute_call(node.func)
        self.generic_visit(node)

    def visit_Name(self, node: ast.Name) -> None:
        """Analyze name references."""
        if isinstance(node.ctx, ast.Load):
            self._analyze_name(node.id)
        self.generic_visit(node)

    def visit_Attribute(self, node: ast.Attribute) -> None:
        """Analyze attribute access."""
        if isinstance(node.value, ast.Name):
            module_name = node.value.id
            self._analyze_name(module_name)
        self.generic_visit(node)

    def _analyze_name(self, name: str) -> None:
        """Analyze a name and determine if it needs an import."""
        # Skip builtins
        if name in self.builtins:
            return

        # Check if it's defined locally
        if self.context.is_local_reference(name):
            req = ImportRequirement(module="", name=name, is_local_reference=True)
            self.requirements.append(req)
            return

        # Check if it's already imported
        import_source = self.context.get_import_source(name)
        if import_source:
            req = ImportRequirement(
                module=import_source, name=name, is_local_reference=False
            )
            self.requirements.append(req)
            return

        # Suggest module based on existing patterns in this codebase
        suggested_module = suggest_import_module(name, self.context)
        req = ImportRequirement(
            module="",
            name=name,
            is_local_reference=False,
            suggested_module=suggested_module,
        )
        self.requirements.append(req)

    def _analyze_attribute_call(self, attr_node: ast.Attribute) -> None:
        """Analyze module.function style calls."""
        if isinstance(attr_node.value, ast.Name):
            module_name = attr_node.value.id
            attr_name = attr_node.attr

            # Check if module is imported
            import_source = self.context.get_import_source(module_name)
            if import_source:
                # This is a known import
                req = ImportRequirement(
                    module=import_source, name=module_name, is_local_reference=False
                )
                self.requirements.append(req)
            else:
                # Suggest import for the module based on existing patterns
                suggested_module = suggest_import_module(attr_name, self.context)
                if not suggested_module:
                    suggested_module = module_name

                req = ImportRequirement(
                    module="",
                    name=module_name,
                    is_local_reference=False,
                    suggested_module=suggested_module,
                )
                self.requirements.append(req)
