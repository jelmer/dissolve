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

"""Functionality for removing @replace_me decorators from source code.

This module provides tools to clean up source code by removing @replace_me
decorators after migration is complete. It supports selective removal based
on version constraints.

The removal process can:
- Remove all @replace_me decorators
- Remove only decorators with versions older than a specified version
- Preserve the decorated functions while removing only the decorators

Example:
    Remove all decorators::

        source = remove_decorators(source, remove_all=True)

    Remove decorators older than version 2.0.0::

        source = remove_decorators(source, before_version="2.0.0")
"""

import ast
from typing import Optional, Union

from packaging import version

from .ast_helpers import is_replace_me_decorator


class ReplaceRemover(ast.NodeTransformer):
    """Remove @replace_me decorators from function definitions.

    This AST transformer selectively removes @replace_me decorators based on
    version constraints while preserving the decorated functions.

    Attributes:
        before_version: Remove decorators with versions older than this.
        remove_all: If True, remove all @replace_me decorators regardless of version.
        current_version: Current package version for remove_in comparison.
    """

    def __init__(
        self,
        before_version: Optional[str] = None,
        remove_all: bool = False,
        current_version: Optional[str] = None,
    ) -> None:
        self.before_version = before_version
        self.remove_all = remove_all
        self.current_version = current_version

    def visit_FunctionDef(self, node: ast.FunctionDef) -> ast.FunctionDef:
        """Process function definitions to remove @replace_me decorators."""
        result = self._process_decorated_node(node)
        assert isinstance(result, ast.FunctionDef)
        return result

    def visit_AsyncFunctionDef(
        self, node: ast.AsyncFunctionDef
    ) -> ast.AsyncFunctionDef:
        """Process async function definitions to remove @replace_me decorators."""
        result = self._process_decorated_node(node)
        assert isinstance(result, ast.AsyncFunctionDef)
        return result

    def _process_decorated_node(
        self, node: Union[ast.FunctionDef, ast.AsyncFunctionDef]
    ) -> Union[ast.FunctionDef, ast.AsyncFunctionDef]:
        """Process any decorated node (function or property) to remove @replace_me decorators."""
        # Process the function body first
        self.generic_visit(node)

        # Filter decorators
        new_decorators = []
        for decorator in node.decorator_list:
            if self._should_remove_decorator(decorator):
                continue
            new_decorators.append(decorator)

        node.decorator_list = new_decorators
        return node

    def _should_remove_decorator(self, decorator: ast.AST) -> bool:
        """Check if a decorator should be removed.

        Args:
            decorator: The decorator AST node to check.

        Returns:
            True if the decorator should be removed, False otherwise.
        """
        if not is_replace_me_decorator(decorator):
            return False

        if self.remove_all:
            return True

        # Check remove_in parameter first
        remove_in_version = self._extract_remove_in_version(decorator)
        if remove_in_version is not None and self.current_version is not None:
            try:
                # Only remove if current version >= remove_in version
                return version.parse(self.current_version) >= version.parse(
                    remove_in_version
                )
            except Exception:
                # If version parsing fails, fall through to other checks
                pass

        if self.before_version is None:
            # Default behavior: don't remove unless remove_in condition is met or remove_all is True
            return False

        # Extract version from decorator
        decorator_version = self._extract_version(decorator)
        if decorator_version is None:
            # No version specified, remove if remove_all is True
            return self.remove_all

        # Compare versions
        try:
            return version.parse(decorator_version) < version.parse(self.before_version)
        except Exception:
            # If version parsing fails, don't remove
            return False

    def _extract_version(self, decorator: ast.AST) -> Optional[str]:
        """Extract the 'since' version from a @replace_me decorator.

        Args:
            decorator: The decorator AST node.

        Returns:
            The version string if found, None otherwise.
        """
        if not isinstance(decorator, ast.Call):
            return None

        # Check keyword arguments
        for keyword in decorator.keywords:
            if keyword.arg == "since":
                if isinstance(keyword.value, ast.Constant):
                    return str(keyword.value.value)
                elif isinstance(keyword.value, ast.Str):  # Python < 3.8
                    return keyword.value.s

        # Check positional arguments (since is the first argument)
        if decorator.args:
            arg = decorator.args[0]
            if isinstance(arg, ast.Constant):
                return str(arg.value)
            elif isinstance(arg, ast.Str):  # Python < 3.8
                return arg.s

        return None

    def _extract_remove_in_version(self, decorator: ast.AST) -> Optional[str]:
        """Extract the 'remove_in' version from a @replace_me decorator.

        Args:
            decorator: The decorator AST node.

        Returns:
            The remove_in version string if found, None otherwise.
        """
        if not isinstance(decorator, ast.Call):
            return None

        # Check keyword arguments
        for keyword in decorator.keywords:
            if keyword.arg == "remove_in":
                if isinstance(keyword.value, ast.Constant):
                    return str(keyword.value.value)
                elif isinstance(keyword.value, ast.Str):  # Python < 3.8
                    return keyword.value.s

        return None


def remove_decorators(
    source: str,
    before_version: Optional[str] = None,
    remove_all: bool = False,
    current_version: Optional[str] = None,
) -> str:
    """Remove @replace_me decorators from Python source code.

    This function parses the source code and selectively removes @replace_me
    decorators based on the provided criteria. The decorated functions remain
    intact; only the decorators are removed.

    Args:
        source: Python source code to process.
        before_version: Remove decorators with version older than this.
            Version comparison uses standard semantic versioning rules.
        remove_all: Remove all @replace_me decorators regardless of version.
            If True, before_version is ignored.
        current_version: Current package version for remove_in comparison.
            Used to determine if decorators with remove_in should be removed.

    Returns:
        Modified source code with decorators removed.

    Example:
        Remove all decorators::

            source = '''
            @replace_me(since="1.0.0")
            def old_func():
                return new_func()
            '''

            result = remove_decorators(source, remove_all=True)
            # def old_func():
            #     return new_func()

        Remove old decorators::

            result = remove_decorators(source, before_version="2.0.0")
            # Removes decorators with since < 2.0.0
    """
    tree = ast.parse(source)

    remover = ReplaceRemover(
        before_version=before_version,
        remove_all=remove_all,
        current_version=current_version,
    )
    new_tree = remover.visit(tree)

    result = ast.unparse(new_tree)

    # Preserve trailing newline if original had one
    if source.endswith("\n") and not result.endswith("\n"):
        result += "\n"

    return result


def remove_from_file(
    filepath: str,
    before_version: Optional[str] = None,
    remove_all: bool = False,
    write: bool = False,
    current_version: Optional[str] = None,
) -> str:
    """Remove @replace_me decorators from a Python file.

    This is a convenience wrapper that reads a file, removes decorators
    according to the specified criteria, and optionally writes it back.

    Args:
        filepath: Path to the Python file to process.
        before_version: Remove decorators with version older than this.
            Version comparison uses standard semantic versioning rules.
        remove_all: Remove all @replace_me decorators regardless of version.
            If True, before_version is ignored.
        write: Whether to write changes back to the file.
        current_version: Current package version for remove_in comparison.
            Used to determine if decorators with remove_in should be removed.

    Returns:
        Modified source code with decorators removed.

    Raises:
        IOError: If the file cannot be read or written.

    Example:
        Remove all decorators from a file::

            result = remove_from_file("mymodule.py", remove_all=True, write=True)

        Remove old decorators and preview changes::

            result = remove_from_file("mymodule.py", before_version="2.0.0")
            print(result)  # Preview changes without writing
    """
    with open(filepath) as f:
        source = f.read()

    new_source = remove_decorators(
        source,
        before_version=before_version,
        remove_all=remove_all,
        current_version=current_version,
    )

    if write and new_source != source:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source
