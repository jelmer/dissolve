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
5. Generating updated source code with perfect formatting preservation

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

import logging
from typing import Callable, Literal, Optional, Union

import libcst as cst

from .collector import DeprecatedFunctionCollector
from .replacer import FunctionCallReplacer, InteractiveFunctionCallReplacer


def migrate_file(
    file_path: str,
    content: Optional[str] = None,
    interactive: bool = False,
    prompt_func: Optional[Callable[[str, str], Literal["y", "n", "a", "q"]]] = None,
) -> Optional[str]:
    """Migrate a single Python source file.

    This function analyzes a Python source file, finds functions decorated with
    @replace_me, and replaces calls to those functions with their suggested
    alternatives. CST is used to preserve exact formatting, comments, and whitespace.

    Args:
        file_path: Path to the Python file to migrate.
        content: Optional source content. If not provided, reads from file_path.
        interactive: Whether to prompt for each replacement.
        prompt_func: Optional custom prompt function for interactive mode.

    Returns:
        The migrated source code if changes were made, None otherwise.

    Raises:
        IOError: If file cannot be read.
        SyntaxError: If the Python source code is invalid.
    """
    # Read source if not provided
    if content is None:
        try:
            with open(file_path, encoding="utf-8") as f:
                content = f.read()
        except OSError as e:
            logging.error(f"Failed to read {file_path}: {e}")
            raise

    try:
        result = migrate_source(
            content, interactive=interactive, prompt_func=prompt_func
        )
        if result == content:
            # No changes made
            return None
        return result
    except SyntaxError as e:
        logging.error(f"Failed to parse {file_path}: {e}")
        raise


def migrate_source(
    source: str,
    interactive: bool = False,
    prompt_func: Optional[Callable[[str, str], Literal["y", "n", "a", "q"]]] = None,
) -> str:
    """Migrate Python source code.

    This function analyzes Python source code, finds functions decorated with
    @replace_me, and replaces calls to those functions with their suggested
    alternatives.

    Args:
        source: The Python source code to migrate.
        interactive: Whether to prompt for each replacement.
        prompt_func: Optional custom prompt function for interactive mode.

    Returns:
        The migrated source code, or the original if no changes were made.

    Raises:
        SyntaxError: If the Python source code is invalid.
    """
    # Parse with CST
    try:
        module = cst.parse_module(source)
    except cst.ParserSyntaxError as e:
        raise SyntaxError(f"Failed to parse source: {e}")

    # Collect deprecated functions
    collector = DeprecatedFunctionCollector()
    wrapper = cst.MetadataWrapper(module)
    wrapper.visit(collector)

    # Report constructs that cannot be processed
    if collector.unreplaceable:
        for name, unreplaceable_node in collector.unreplaceable.items():
            construct_type = unreplaceable_node.construct_type_str()
            logging.warning(
                f"{construct_type} '{name}' cannot be processed: {unreplaceable_node.reason.value}"
                + (
                    f" ({unreplaceable_node.message})"
                    if unreplaceable_node.message
                    else ""
                )
            )

    if not collector.replacements:
        # No deprecated functions found
        return source

    # Create replacer
    replacer: Union[FunctionCallReplacer, InteractiveFunctionCallReplacer]
    if interactive:
        replacer = InteractiveFunctionCallReplacer(
            collector.replacements,
            prompt_func=prompt_func,
            source=source,
        )
    else:
        replacer = FunctionCallReplacer(collector.replacements)

    # Apply replacements
    if interactive:
        # For interactive mode, we need to wrap the replacer with metadata
        metadata_wrapper = cst.MetadataWrapper(module)
        modified_module = metadata_wrapper.visit(replacer)
    else:
        modified_module = module.visit(replacer)

    # Check if any replacements were made
    if not replacer.replaced_nodes:
        return source

    # Return the modified code with formatting preserved
    return modified_module.code


def migrate_file_simple(file_path: str) -> bool:
    """Simple interface to migrate a file in-place.

    Args:
        file_path: Path to the Python file to migrate.

    Returns:
        True if changes were made, False otherwise.
    """
    try:
        result = migrate_file(file_path)
        if result is not None:
            with open(file_path, "w", encoding="utf-8") as f:
                f.write(result)
            return True
        return False
    except (OSError, SyntaxError) as e:
        logging.error(f"Failed to migrate {file_path}: {e}")
        return False


def migrate_file_with_imports(
    file_path: str,
    interactive: bool = False,
    prompt_func: Optional[Callable[[str, str], Literal["y", "n", "a", "q"]]] = None,
    write: bool = False,
) -> Optional[str]:
    """Migrate a file with automatic import management.

    This is a wrapper around migrate_file that provides compatibility
    with the CLI interface. Import management is not currently implemented
    in the CST version.

    Args:
        file_path: Path to the Python file to migrate.
        interactive: Whether to prompt for each replacement.
        prompt_func: Optional custom prompt function for interactive mode.
        write: Whether to write changes back to the file.

    Returns:
        The migrated source code if changes were made and write=False,
        None otherwise.
    """
    result = migrate_file(
        file_path,
        interactive=interactive,
        prompt_func=prompt_func,
    )

    if result is not None:
        if write:
            with open(file_path, "w", encoding="utf-8") as f:
                f.write(result)
            return None
        else:
            return result
    return None
