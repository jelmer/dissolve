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

import ast
import sys
from typing import Dict, List, Tuple, Optional


class ReplaceInfo:
    """Information about a function that should be replaced."""

    def __init__(self, old_name: str, replacement_expr: str):
        self.old_name = old_name
        self.replacement_expr = replacement_expr


class ImportInfo:
    """Information about imported names."""

    def __init__(self, module: str, names: List[Tuple[str, Optional[str]]]):
        self.module = module
        self.names = names  # List of (name, alias) tuples


class DeprecatedFunctionCollector(ast.NodeVisitor):
    """Collects information about functions decorated with @replace_me."""

    def __init__(self):
        self.replacements: Dict[str, ReplaceInfo] = {}
        self.imports: List[ImportInfo] = []

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        for decorator in node.decorator_list:
            if self._is_replace_me_decorator(decorator):
                replacement_expr = self._extract_replacement_expr(decorator)
                if replacement_expr:
                    self.replacements[node.name] = ReplaceInfo(
                        node.name, replacement_expr
                    )
        self.generic_visit(node)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        if node.module:
            names = [(alias.name, alias.asname) for alias in node.names]
            self.imports.append(ImportInfo(node.module, names))
        self.generic_visit(node)

    def _is_replace_me_decorator(self, decorator: ast.AST) -> bool:
        if isinstance(decorator, ast.Name) and decorator.id == "replace_me":
            return True
        if isinstance(decorator, ast.Call):
            if (
                isinstance(decorator.func, ast.Name)
                and decorator.func.id == "replace_me"
            ):
                return True
            if (
                isinstance(decorator.func, ast.Attribute)
                and decorator.func.attr == "replace_me"
            ):
                return True
        return False

    def _extract_replacement_expr(self, decorator: ast.AST) -> Optional[str]:
        if isinstance(decorator, ast.Call) and decorator.args:
            first_arg = decorator.args[0]
            if isinstance(first_arg, ast.Constant):
                return first_arg.value
        return None


class FunctionCallReplacer(ast.NodeTransformer):
    """Replaces function calls with their replacement expressions."""

    def __init__(self, replacements: Dict[str, ReplaceInfo]):
        self.replacements = replacements

    def visit_Call(self, node: ast.Call) -> ast.AST:
        self.generic_visit(node)

        func_name = self._get_function_name(node)
        if func_name and func_name in self.replacements:
            replacement = self.replacements[func_name]
            return self._create_replacement_node(node, replacement)
        return node

    def _get_function_name(self, node: ast.Call) -> Optional[str]:
        if isinstance(node.func, ast.Name):
            return node.func.id
        return None

    def _create_replacement_node(
        self, original_call: ast.Call, replacement: ReplaceInfo
    ) -> ast.AST:
        # Build a mapping of parameter names to their values
        param_map = self._build_param_map(original_call, replacement)

        # Parse the replacement expression
        replacement_expr = replacement.replacement_expr

        # Replace placeholders in the expression
        for param, value in param_map.items():
            placeholder = f"{{{param}}}"
            if placeholder in replacement_expr:
                # Convert AST node back to source code
                value_str = ast.unparse(value)
                replacement_expr = replacement_expr.replace(placeholder, value_str)

        # Parse the modified expression as an AST node
        try:
            result = ast.parse(replacement_expr, mode="eval").body
            # Copy location information from original call
            ast.copy_location(result, original_call)
            return result
        except SyntaxError:
            # If parsing fails, return the original call
            return original_call

    def _build_param_map(
        self, call: ast.Call, replacement: ReplaceInfo
    ) -> Dict[str, ast.AST]:
        # For now, we'll do a simple mapping based on position
        # This could be enhanced to handle keyword arguments properly
        param_map = {}

        # Extract parameter names from replacement expression
        import re

        param_names = re.findall(r"\{(\w+)\}", replacement.replacement_expr)

        # Map positional arguments
        for i, (param_name, arg) in enumerate(zip(param_names, call.args)):
            param_map[param_name] = arg

        # Map keyword arguments
        for keyword in call.keywords:
            if keyword.arg and keyword.arg in param_names:
                param_map[keyword.arg] = keyword.value

        return param_map


def migrate_file(filepath: str, write: bool = False) -> str:
    """Migrate a Python file by inlining replace_me decorated functions.

    Args:
        filepath: Path to the Python file to migrate
        write: Whether to write changes back to the file

    Returns:
        The migrated source code
    """
    with open(filepath, "r") as f:
        source = f.read()

    # Parse the source code
    tree = ast.parse(source)

    # First pass: collect all functions decorated with @replace_me
    collector = DeprecatedFunctionCollector()
    collector.visit(tree)

    if not collector.replacements:
        return source

    # Second pass: replace function calls
    replacer = FunctionCallReplacer(collector.replacements)
    new_tree = replacer.visit(tree)

    # Convert back to source code
    new_source = ast.unparse(new_tree)

    if write:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source


def migrate_file_with_imports(filepath: str, write: bool = False) -> str:
    """Migrate a Python file, considering imported deprecated functions.

    This version analyzes imports and attempts to fetch replacement
    information from imported modules.
    """
    import os
    import importlib.util

    with open(filepath, "r") as f:
        source = f.read()

    # Parse the source code
    tree = ast.parse(source)

    # First pass: collect imports and local deprecations
    collector = DeprecatedFunctionCollector()
    collector.visit(tree)

    # Try to analyze imported modules for deprecated functions
    file_dir = os.path.dirname(os.path.abspath(filepath))

    for import_info in collector.imports:
        # Try to find and analyze the imported module
        module_file = None

        # Check if it's a local module (relative to current file)
        module_path = import_info.module.replace(".", "/")
        potential_paths = [
            os.path.join(file_dir, f"{module_path}.py"),
            os.path.join(file_dir, module_path, "__init__.py"),
        ]

        for path in potential_paths:
            if os.path.exists(path):
                module_file = path
                break

        if module_file:
            try:
                # Parse the imported module
                with open(module_file, "r") as f:
                    module_source = f.read()
                module_tree = ast.parse(module_source)

                # Collect deprecated functions from the module
                module_collector = DeprecatedFunctionCollector()
                module_collector.visit(module_tree)

                # Add imported deprecated functions to our replacements
                for name, alias in import_info.names:
                    if name in module_collector.replacements:
                        replacement_info = module_collector.replacements[name]
                        # Use alias if provided, otherwise use original name
                        key = alias if alias else name
                        collector.replacements[key] = replacement_info
            except:
                # If we can't analyze the module, skip it
                pass

    if not collector.replacements:
        return source

    # Second pass: replace function calls
    replacer = FunctionCallReplacer(collector.replacements)
    new_tree = replacer.visit(tree)

    # Convert back to source code
    new_source = ast.unparse(new_tree)

    if write:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source
