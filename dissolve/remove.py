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

"""Functionality for removing deprecated functions from source code.

This module provides tools to clean up source code by removing entire functions
that are decorated with @replace_me after migration is complete. It supports
selective removal based on version constraints.

The removal process can:
- Remove all functions decorated with @replace_me
- Remove only functions with decorators older than a specified version
- Completely remove deprecated functions, not just their decorators

Note: This should only be used AFTER all calls to deprecated functions have
been migrated using 'dissolve migrate', as removing the functions will break
any remaining calls to them.

Example:
    Remove all deprecated functions::

        source = remove_decorators(source, remove_all=True)

    Remove functions with decorators older than version 2.0.0::

        source = remove_decorators(source, before_version="2.0.0")
"""

from typing import Optional, Union

import libcst as cst
from packaging import version


class ReplaceRemover(cst.CSTTransformer):
    """Remove entire functions decorated with @replace_me.

    This CST transformer selectively removes complete function definitions that
    are decorated with @replace_me based on version constraints. This completely
    removes deprecated functions from the codebase after migration is complete.

    Attributes:
        before_version: Only remove functions with decorators with versions before this.
        remove_all: If True, remove all functions with @replace_me decorators regardless of version.
        removed_count: Number of functions removed.
    """

    def __init__(
        self,
        before_version: Optional[str] = None,
        remove_all: bool = False,
        current_version: Optional[str] = None,
    ) -> None:
        self.before_version = version.parse(before_version) if before_version else None
        self.remove_all = remove_all
        self.current_version = (
            version.parse(current_version) if current_version else None
        )
        self.removed_count = 0

    def leave_FunctionDef(
        self, original_node: cst.FunctionDef, updated_node: cst.FunctionDef
    ) -> Union[cst.FunctionDef, cst.RemovalSentinel]:
        """Process function definitions to remove entire functions with @replace_me decorators."""
        # Check if any decorator should be removed
        for decorator in updated_node.decorators:
            if self._should_remove_decorator(decorator):
                self.removed_count += 1
                # Remove the entire function, not just the decorator
                return cst.RemovalSentinel.REMOVE

        return updated_node

    def _should_remove_decorator(self, decorator: cst.Decorator) -> bool:
        """Check if a decorator should be removed."""
        if not self._is_replace_me_decorator(decorator):
            return False

        if self.remove_all:
            return True

        # Check remove_in version
        if self.current_version:
            remove_in_version = self._extract_remove_in_version(decorator)
            if remove_in_version and self.current_version >= remove_in_version:
                return True

        # Extract version from decorator if present
        decorator_version = self._extract_version_from_decorator(decorator)
        if decorator_version and self.before_version:
            return decorator_version < self.before_version

        return False

    def _is_replace_me_decorator(self, decorator: cst.Decorator) -> bool:
        """Check if decorator is @replace_me."""
        dec = decorator.decorator

        # Handle @replace_me or @module.replace_me
        if isinstance(dec, cst.Name):
            return dec.value == "replace_me"
        elif isinstance(dec, cst.Attribute):
            return dec.attr.value == "replace_me"
        # Handle @replace_me() or @module.replace_me()
        elif isinstance(dec, cst.Call):
            if isinstance(dec.func, cst.Name):
                return dec.func.value == "replace_me"
            elif isinstance(dec.func, cst.Attribute):
                return dec.func.attr.value == "replace_me"
        return False

    def _extract_version_from_decorator(
        self, decorator: cst.Decorator
    ) -> Optional[version.Version]:
        """Extract version from @replace_me(since="x.y.z") decorator."""
        dec = decorator.decorator

        # Only handle Call forms
        if not isinstance(dec, cst.Call):
            return None

        # Look for 'since' keyword argument
        for arg in dec.args:
            if arg.keyword and arg.keyword.value == "since":
                if isinstance(arg.value, cst.SimpleString):
                    # Remove quotes and parse version
                    version_str = arg.value.value.strip("\"'")
                    try:
                        return version.parse(version_str)
                    except version.InvalidVersion:
                        pass

        return None

    def _extract_remove_in_version(
        self, decorator: cst.Decorator
    ) -> Optional[version.Version]:
        """Extract remove_in version from @replace_me(remove_in="x.y.z") decorator."""
        dec = decorator.decorator

        # Only handle Call forms
        if not isinstance(dec, cst.Call):
            return None

        # Look for 'remove_in' keyword argument
        for arg in dec.args:
            if arg.keyword and arg.keyword.value == "remove_in":
                if isinstance(arg.value, cst.SimpleString):
                    # Remove quotes and parse version
                    version_str = arg.value.value.strip("\"'")
                    try:
                        return version.parse(version_str)
                    except version.InvalidVersion:
                        pass

        return None


def remove_decorators(
    source: str,
    before_version: Optional[str] = None,
    remove_all: bool = False,
    current_version: Optional[str] = None,
) -> str:
    """Remove entire functions decorated with @replace_me from source code.

    This function completely removes functions that are decorated with @replace_me,
    not just the decorators. This should only be used after migration is complete
    and all calls to deprecated functions have been updated.

    Args:
        source: Python source code to process.
        before_version: Only remove functions with decorators with versions before this.
            Version should be a string like "2.0.0".
        remove_all: If True, remove all functions with @replace_me decorators regardless of version.
        current_version: Current version to check against remove_in parameter.
            If a decorator has remove_in="x.y.z" and current_version >= x.y.z,
            the function will be removed.

    Returns:
        Modified source code with deprecated functions removed.

    Raises:
        cst.ParserSyntaxError: If the source code is invalid Python.
    """
    if not remove_all and not before_version and not current_version:
        # No removal criteria specified, return source unchanged
        return source

    module = cst.parse_module(source)
    remover = ReplaceRemover(
        before_version=before_version,
        remove_all=remove_all,
        current_version=current_version,
    )
    modified = module.visit(remover)

    return modified.code


def remove_decorators_from_file(
    file_path: str,
    before_version: Optional[str] = None,
    remove_all: bool = False,
    write: bool = True,
    current_version: Optional[str] = None,
) -> Union[str, int]:
    """Remove functions decorated with @replace_me from a file.

    Args:
        file_path: Path to the Python file to process.
        before_version: Only remove functions with decorators with versions before this.
        remove_all: If True, remove all functions with @replace_me decorators.
        write: If True, write changes back to the file.

    Returns:
        If write is True, returns the number of functions removed.
        If write is False, returns the modified source code.

    Raises:
        IOError: If the file cannot be read or written.
        cst.ParserSyntaxError: If the file contains invalid Python.
    """
    with open(file_path, encoding="utf-8") as f:
        source = f.read()

    module = cst.parse_module(source)
    remover = ReplaceRemover(
        before_version=before_version,
        remove_all=remove_all,
        current_version=current_version,
    )
    modified = module.visit(remover)

    if write:
        if remover.removed_count > 0:
            with open(file_path, "w", encoding="utf-8") as f:
                f.write(modified.code)
        return remover.removed_count
    else:
        return modified.code


# Alias for CLI compatibility
remove_from_file = remove_decorators_from_file
