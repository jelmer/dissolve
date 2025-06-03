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
from dataclasses import dataclass
from typing import Callable, Literal, Optional

from .collector import DeprecatedFunctionCollector, ReplaceInfo
from .context_analyzer import ContextAnalyzer, analyze_replacement_context
from .import_utils import ImportManager, ImportRequirement
from .replacer import FunctionCallReplacer, InteractiveFunctionCallReplacer


@dataclass
class MigrationResult:
    """Result of a migration operation."""

    source: str
    has_unmigrated_calls: bool
    unmigrated_count: int = 0
    migrated_count: int = 0


def migrate_source(
    source: str,
    module_resolver: Optional[Callable[[str, Optional[str]], Optional[str]]] = None,
    interactive: bool = False,
    prompt_func: Optional[Callable[[str, str], Literal["y", "n", "a", "q"]]] = None,
    verbose: bool = False,
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
        verbose: Whether to enable verbose logging of successful inlinings.

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

    # First pass: analyze context (imports, local definitions)
    context = ContextAnalyzer()
    context.visit(tree)

    # Second pass: collect imports and local deprecations
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
                            # Create a new ReplaceInfo with the source module information
                            key = alias if alias else name
                            collector.replacements[key] = ReplaceInfo(
                                old_name=key,
                                replacement_expr=replacement_info.replacement_expr,
                                func_def=replacement_info.func_def,
                                source_module=import_info.module,
                            )
            except BaseException as e:
                logging.warning(
                    'Failed to resolve module "%s", ignoring: %s', import_info.module, e
                )

    # Combine replacements and unreplaceable functions for comprehensive tracking
    all_deprecated = {}
    all_deprecated.update(collector.replacements)

    # Add unreplaceable functions with dummy ReplaceInfo
    for func_name, unreplaceable in collector.unreplaceable.items():
        # Create a ReplaceInfo with the function definition but no replacement
        all_deprecated[func_name] = ReplaceInfo(
            func_name,
            "",  # Empty replacement expression
            func_def=unreplaceable.node if hasattr(unreplaceable, "node") else None,
        )

    if not all_deprecated:
        return source

    # Filter out non-inlinable functions (but don't warn yet - wait until we see if they're called)
    from .extractor import extract_replacement_from_body
    from .types import ReplacementExtractionError

    inlinable_replacements = {}

    for func_name, replacement in collector.replacements.items():
        if replacement.func_def:
            try:
                extract_replacement_from_body(replacement.func_def)
                inlinable_replacements[func_name] = replacement
            except ReplacementExtractionError:
                # Don't warn here - wait to see if the function is actually called
                pass
        else:
            # If no func_def, assume it's inlinable (might be from older format)
            inlinable_replacements[func_name] = replacement

    # Third pass: replace function calls (only for inlinable functions)
    # Even if there are no inlinable replacements, we still need to check for calls to non-inlinable functions
    if interactive:
        replacer: FunctionCallReplacer = InteractiveFunctionCallReplacer(
            inlinable_replacements, prompt_func, all_deprecated, verbose=verbose
        )
    else:
        replacer = FunctionCallReplacer(
            inlinable_replacements, all_deprecated, verbose=verbose
        )
    new_tree = replacer.visit(tree)

    # Issue warnings for specific calls to non-migratable functions
    warned_functions = set()
    for func_name, line_num, reason in replacer.non_migratable_calls:
        # First, issue a general warning about the function if we haven't already
        if func_name not in warned_functions:
            logging.warning(
                'Deprecated function "%s" cannot be automatically migrated: %s',
                func_name,
                reason,
            )
            warned_functions.add(func_name)

        # Then issue a specific warning about this call
        logging.warning(
            'Call to deprecated function "%s" at line %d cannot be automatically migrated: %s',
            func_name,
            line_num,
            reason,
        )

    # Log successful inlinings in verbose mode
    if verbose:
        for func_name, line_num in replacer.successful_inlinings:
            logging.info(
                'Successfully inlined deprecated function "%s" at line %d',
                func_name,
                line_num,
            )

    # Fourth pass: intelligent import management
    if replacer.new_functions_used or inlinable_replacements:
        import_manager = ImportManager(new_tree)

        # Analyze replacement expressions for import requirements
        for old_func, replacement_info in inlinable_replacements.items():
            requirements = analyze_replacement_context(
                replacement_info.replacement_expr, context
            )

            for req in requirements:
                if req.is_local_reference:
                    # This references something defined locally, no import needed
                    continue

                if req.suggested_module:
                    # We have a suggestion for where this should be imported from
                    actual_req = ImportRequirement(
                        module=req.suggested_module, name=req.name, alias=req.alias
                    )
                    import_manager.add_import(actual_req)
                elif req.module:
                    # We know the exact module
                    import_manager.add_import(req)

            # If this replacement comes from an imported module, check for local variable references
            if replacement_info.source_module and replacement_info.func_def:
                non_param_names = collector._get_non_parameter_names(
                    replacement_info.func_def, replacement_info.replacement_expr
                )

                # Add imports for non-parameter names from the source module
                for name in non_param_names:
                    # Skip if already imported or defined locally
                    if not context.is_local_reference(
                        name
                    ) and not context.get_import_source(name):
                        import_req = ImportRequirement(
                            module=replacement_info.source_module, name=name, alias=None
                        )
                        import_manager.add_import(import_req)

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
    prompt_func: Optional[Callable[[str, str], Literal["y", "n", "a", "q"]]] = None,
    verbose: bool = False,
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
        verbose: Whether to enable verbose logging of successful inlinings.

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
        verbose=verbose,
    )

    if write and new_source != source:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source
