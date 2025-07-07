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
from typing import Callable, Literal, Optional, Union

from .collector import DeprecatedFunctionCollector
from .replacer import FunctionCallReplacer, InteractiveFunctionCallReplacer


def migrate_source(
    source: str,
    module_resolver: Union[
        Callable[[str, Union[str, None]], Union[str, None]], None
    ] = None,
    interactive: bool = False,
    prompt_func: Union[Callable[[str, str], Literal["y", "n", "a", "q"]], None] = None,
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
        interactive: Whether to prompt for confirmation before each replacement.
        prompt_func: Optional custom prompt function for interactive mode.

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

        Interactive migration::

            migrated = migrate_source(source, interactive=True)
            # Will prompt: Found deprecated call: old_func(5)
            # Replace with: new_func(5 * 2)?
            # [Y]es / [N]o / [A]ll / [Q]uit:
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
    if interactive:
        replacer: FunctionCallReplacer = InteractiveFunctionCallReplacer(
            collector.replacements, prompt_func, source
        )
    else:
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
    with open(filepath) as f:
        source = f.read()

    new_source = migrate_source(source)

    if write and new_source != source:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source


def migrate_file_with_imports(
    filepath: str,
    write: bool = False,
    interactive: bool = False,
    prompt_func: Union[Callable[[str, str], Literal["y", "n", "a", "q"]], None] = None,
) -> str:
    """Migrate a Python file, considering imported deprecated functions.

    This enhanced version analyzes imports and attempts to fetch replacement
    information from imported modules in the same directory structure.
    It can handle cases where deprecated functions are imported from other
    local modules.

    Args:
        filepath: Path to the Python file to migrate.
        write: Whether to write changes back to the file.
        interactive: Whether to prompt for confirmation before each replacement.
        prompt_func: Optional custom prompt function for interactive mode.

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

    with open(filepath) as f:
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
                    with open(path) as f:
                        return f.read()
                except BaseException as e:
                    logging.warning('Failed to read module "%s", ignoring: %s', path, e)
                    continue
        return None

    new_source = migrate_source(
        source,
        module_resolver=local_module_resolver,
        interactive=interactive,
        prompt_func=prompt_func,
    )

    if write and new_source != source:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source
