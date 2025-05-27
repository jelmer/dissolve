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

"""Migration functionality for replacing deprecated function calls.

This module provides the core logic for analyzing Python source code,
identifying calls to functions decorated with @replace_me, and replacing
those calls with their suggested alternatives.

The migration process involves:
1. Parsing source code to find @replace_me decorated functions
2. Extracting replacement expressions from function bodies
3. Locating calls to deprecated functions
4. Substituting actual arguments into replacement expressions
5. Generating updated source code

Example:
    Given a source file with::

        @replace_me()
        def old_api(x, y):
            return new_api(x, y, mode="legacy")

        result = old_api(5, 10)

    The migration will transform it to::

        @replace_me()
        def old_api(x, y):
            return new_api(x, y, mode="legacy")

        result = new_api(5, 10, mode="legacy")
"""

import ast
import logging
from typing import Dict, List, Tuple, Optional, Callable
from .ast_utils import substitute_parameters


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


class ImportInfo:
    """Information about imported names.

    Attributes:
        module: The module being imported from.
        names: List of (name, alias) tuples for imported names.
    """

    def __init__(self, module: str, names: List[Tuple[str, Optional[str]]]) -> None:
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
        self.replacements: Dict[str, ReplaceInfo] = {}
        self.imports: List[ImportInfo] = []

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        """Process function definitions to find @replace_me decorators."""
        for decorator in node.decorator_list:
            if self._is_replace_me_decorator(decorator):
                # For the new format, extract replacement from function body
                replacement_expr = self._extract_replacement_from_body(node)
                if replacement_expr:
                    self.replacements[node.name] = ReplaceInfo(
                        node.name, replacement_expr
                    )
        self.generic_visit(node)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        """Collect import information for module resolution."""
        if node.module:
            names = [(alias.name, alias.asname) for alias in node.names]
            self.imports.append(ImportInfo(node.module, names))
        self.generic_visit(node)

    def _is_replace_me_decorator(self, decorator: ast.AST) -> bool:
        """Check if a decorator is @replace_me."""
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

    def _extract_replacement_from_body(
        self, func_def: ast.FunctionDef
    ) -> Optional[str]:
        """Extract replacement expression from function body.

        Args:
            func_def: The function definition AST node.

        Returns:
            The replacement expression with parameter placeholders, or None
            if no valid replacement can be extracted.
        """
        if func_def.body and len(func_def.body) == 1:
            stmt = func_def.body[0]
            if isinstance(stmt, ast.Return) and stmt.value:
                # Create a template with parameter placeholders
                replacement_expr = ast.unparse(stmt.value)

                # Replace parameter names with placeholders
                for arg in func_def.args.args:
                    param_name = arg.arg
                    replacement_expr = replacement_expr.replace(
                        param_name, f"{{{param_name}}}"
                    )

                return replacement_expr
        return None


class FunctionCallReplacer(ast.NodeTransformer):
    """Replaces function calls with their replacement expressions.

    This AST transformer visits function calls and replaces calls to
    deprecated functions with their suggested replacements, substituting
    actual argument values.

    Attributes:
        replacements: Mapping from function names to their replacement info.
    """

    def __init__(self, replacements: Dict[str, ReplaceInfo]) -> None:
        self.replacements = replacements

    def visit_Call(self, node: ast.Call) -> ast.AST:
        """Visit Call nodes and replace deprecated function calls."""
        self.generic_visit(node)

        func_name = self._get_function_name(node)
        if func_name and func_name in self.replacements:
            replacement = self.replacements[func_name]
            return self._create_replacement_node(node, replacement)
        return node

    def _get_function_name(self, node: ast.Call) -> Optional[str]:
        """Extract the function name from a Call node."""
        if isinstance(node.func, ast.Name):
            return node.func.id
        return None

    def _create_replacement_node(
        self, original_call: ast.Call, replacement: ReplaceInfo
    ) -> ast.AST:
        """Create an AST node for the replacement expression.

        Args:
            original_call: The original function call to replace.
            replacement: Information about the replacement expression.

        Returns:
            AST node representing the replacement expression with arguments
            substituted.
        """
        # Build a mapping of parameter names to their AST values
        param_map = self._build_param_map(original_call, replacement)

        # Parse the replacement expression with placeholders
        # First, we need to convert {param} placeholders to valid Python identifiers
        temp_expr = replacement.replacement_expr
        for param in param_map.keys():
            temp_expr = temp_expr.replace(f"{{{param}}}", param)

        try:
            # Parse the expression
            replacement_ast = ast.parse(temp_expr, mode="eval").body

            # Substitute parameters using AST transformation
            result = substitute_parameters(replacement_ast, param_map)

            # Copy location information from original call
            ast.copy_location(result, original_call)
            return result
        except SyntaxError:
            # If parsing fails, return the original call
            return original_call

    def _build_param_map(
        self, call: ast.Call, replacement: ReplaceInfo
    ) -> Dict[str, ast.expr]:
        """Build a mapping of parameter names to their AST values.

        Args:
            call: The function call with arguments.
            replacement: Information about the replacement expression.

        Returns:
            Dictionary mapping parameter names to their AST representations.
        """
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


def migrate_source(
    source: str,
    module_resolver: Optional[Callable[[str, Optional[str]], Optional[str]]] = None,
) -> str:
    """Migrate Python source code by inlining replace_me decorated functions.

    This function analyzes the source code for calls to functions decorated
    with @replace_me and replaces those calls with their suggested replacements.
    It can also resolve imports to find deprecated functions in other modules.

    Args:
        source: Python source code to migrate.
        module_resolver: Optional callable that takes (module_name, file_dir)
            and returns the module's source code as a string, or None if the
            module cannot be resolved.

    Returns:
        The migrated source code with deprecated function calls replaced.

    Example:
        Basic migration::

            source = '''
            @replace_me()
            def old_func(x):
                return new_func(x * 2)

            result = old_func(5)
            '''

            migrated = migrate_source(source)
            # result = new_func(5 * 2)
    """
    # Parse the source code
    tree = ast.parse(source)

    # First pass: collect imports and local deprecations
    collector = DeprecatedFunctionCollector()
    collector.visit(tree)

    # If module_resolver is provided, analyze imported modules
    if module_resolver:
        for import_info in collector.imports:
            try:
                module_source = module_resolver(import_info.module, None)
                if module_source:
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
            except BaseException as e:
                logging.warning(
                    'Failed to resolve module "%s", ignoring: %s', import_info.module, e
                )

    if not collector.replacements:
        return source

    # Second pass: replace function calls
    replacer = FunctionCallReplacer(collector.replacements)
    new_tree = replacer.visit(tree)

    # Convert back to source code
    return ast.unparse(new_tree)


def migrate_file(filepath: str, write: bool = False) -> str:
    """Migrate a Python file by inlining replace_me decorated functions.

    This is a simple wrapper that reads a file, migrates its content,
    and optionally writes it back. It only processes deprecations defined
    within the same file.

    Args:
        filepath: Path to the Python file to migrate.
        write: Whether to write changes back to the file.

    Returns:
        The migrated source code.

    Raises:
        IOError: If the file cannot be read or written.
    """
    with open(filepath, "r") as f:
        source = f.read()

    new_source = migrate_source(source)

    if write and new_source != source:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source


def migrate_file_with_imports(filepath: str, write: bool = False) -> str:
    """Migrate a Python file, considering imported deprecated functions.

    This enhanced version analyzes imports and attempts to fetch replacement
    information from imported modules in the same directory structure.
    It can handle cases where deprecated functions are imported from other
    local modules.

    Args:
        filepath: Path to the Python file to migrate.
        write: Whether to write changes back to the file.

    Returns:
        The migrated source code.

    Raises:
        IOError: If the file cannot be read or written.

    Example:
        If module_a.py contains::

            from module_b import old_func
            result = old_func(10)

        And module_b.py contains::

            @replace_me()
            def old_func(x):
                return new_func(x, mode="legacy")

        The migration will update module_a.py to::

            from module_b import old_func
            result = new_func(10, mode="legacy")
    """
    import os

    with open(filepath, "r") as f:
        source = f.read()

    file_dir = os.path.dirname(os.path.abspath(filepath))

    # Create a module resolver for local files
    def local_module_resolver(module_name: str, _: Optional[str]) -> Optional[str]:
        module_path = module_name.replace(".", "/")
        potential_paths = [
            os.path.join(file_dir, f"{module_path}.py"),
            os.path.join(file_dir, module_path, "__init__.py"),
        ]

        for path in potential_paths:
            if os.path.exists(path):
                try:
                    with open(path, "r") as f:
                        return f.read()
                except BaseException as e:
                    logging.warning('Failed to read module "%s", ignoring: %s', path, e)
                    continue
        return None

    new_source = migrate_source(source, module_resolver=local_module_resolver)

    if write and new_source != source:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source
