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

"""Verification functionality for @replace_me decorated functions.

This module provides tools to verify that all @replace_me decorated functions
can be successfully replaced according to their replacement expressions.
"""

import ast
from dataclasses import dataclass

from .ast_helpers import is_replace_me_decorator
from .collector import DeprecatedFunctionCollector
from .types import ReplacementExtractionError


@dataclass
class CheckResult:
    """Result of checking @replace_me decorated functions.

    Attributes:
        success: True if all replacements are valid, False otherwise.
        errors: List of error messages for invalid replacements.
        checked_functions: List of function names that were checked.
    """

    success: bool
    errors: list[str]
    checked_functions: list[str]


class ReplacementChecker(ast.NodeVisitor):
    """Validates @replace_me decorated functions for correctness."""

    def __init__(self) -> None:
        self.errors: list[str] = []
        self.checked_functions: list[str] = []

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        """Check function definitions with @replace_me decorators."""
        self._check_decorated_node(node)
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        """Check async function definitions with @replace_me decorators."""
        self._check_decorated_node(node)
        self.generic_visit(node)

    def _check_decorated_node(self, node) -> None:
        """Check any decorated node (function or property) with @replace_me decorators."""
        for decorator in node.decorator_list:
            if is_replace_me_decorator(decorator):
                self.checked_functions.append(node.name)

                # Use the same logic as migrate to test extraction
                collector = DeprecatedFunctionCollector()
                try:
                    collector._extract_replacement_from_body(node)
                    # If we get here, the function can be processed successfully
                except ReplacementExtractionError as e:
                    # Capture the detailed error message from the exception
                    self.errors.append(str(e))
                break


def check_replacements(source: str) -> CheckResult:
    """Check all @replace_me decorated functions in source code.

    Args:
        source: Python source code to check.

    Returns:
        CheckResult with validation results.
    """
    try:
        tree = ast.parse(source)
    except SyntaxError as e:
        return CheckResult(
            success=False,
            errors=[f"Syntax error in source code: {e}"],
            checked_functions=[],
        )

    checker = ReplacementChecker()
    checker.visit(tree)

    success = len(checker.errors) == 0
    return CheckResult(
        success=success,
        errors=checker.errors,
        checked_functions=checker.checked_functions,
    )


def check_file(filepath: str) -> CheckResult:
    """Check @replace_me decorated functions in a Python file.

    Args:
        filepath: Path to the Python file to check.

    Returns:
        CheckResult with validation results.
    """
    try:
        with open(filepath) as f:
            source = f.read()
    except OSError as e:
        return CheckResult(
            success=False,
            errors=[f"Failed to read file: {e}"],
            checked_functions=[],
        )

    return check_replacements(source)
