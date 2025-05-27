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
from typing import Optional
from packaging import version


class ReplaceRemover(ast.NodeTransformer):
    """Remove @replace_me decorators from function definitions."""

    def __init__(self, before_version: Optional[str] = None, remove_all: bool = False):
        self.before_version = before_version
        self.remove_all = remove_all

    def visit_FunctionDef(self, node: ast.FunctionDef) -> ast.FunctionDef:
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

    def visit_AsyncFunctionDef(
        self, node: ast.AsyncFunctionDef
    ) -> ast.AsyncFunctionDef:
        # Handle async functions the same way
        self.generic_visit(node)

        new_decorators = []
        for decorator in node.decorator_list:
            if self._should_remove_decorator(decorator):
                continue
            new_decorators.append(decorator)

        node.decorator_list = new_decorators
        return node

    def _should_remove_decorator(self, decorator: ast.AST) -> bool:
        """Check if a decorator should be removed."""
        if not self._is_replace_me_decorator(decorator):
            return False

        if self.remove_all:
            return True

        if self.before_version is None:
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

    def _extract_version(self, decorator: ast.AST) -> Optional[str]:
        """Extract the 'since' version from a @replace_me decorator."""
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


def remove_decorators(
    source: str, before_version: Optional[str] = None, remove_all: bool = False
) -> str:
    """Remove @replace_me decorators from Python source code.

    Args:
        source: Python source code
        before_version: Remove decorators with version older than this
        remove_all: Remove all @replace_me decorators regardless of version

    Returns:
        Modified source code with decorators removed
    """
    tree = ast.parse(source)

    remover = ReplaceRemover(before_version=before_version, remove_all=remove_all)
    new_tree = remover.visit(tree)

    return ast.unparse(new_tree)


def remove_from_file(
    filepath: str,
    before_version: Optional[str] = None,
    remove_all: bool = False,
    write: bool = False,
) -> str:
    """Remove @replace_me decorators from a Python file.

    Args:
        filepath: Path to the Python file
        before_version: Remove decorators with version older than this
        remove_all: Remove all @replace_me decorators regardless of version
        write: Whether to write changes back to the file

    Returns:
        Modified source code
    """
    with open(filepath, "r") as f:
        source = f.read()

    new_source = remove_decorators(
        source, before_version=before_version, remove_all=remove_all
    )

    if write and new_source != source:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source
