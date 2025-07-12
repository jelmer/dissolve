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

from dataclasses import dataclass

import libcst as cst

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


class ReplacementChecker(cst.CSTVisitor):
    """Validates @replace_me decorated functions for correctness."""

    def __init__(self) -> None:
        self.errors: list[str] = []
        self.checked_functions: list[str] = []
        self._collector = DeprecatedFunctionCollector()

    def leave_FunctionDef(self, original_node: cst.FunctionDef) -> None:
        """Check function definitions with @replace_me decorators."""
        # Check if this function has @replace_me decorator
        has_replace_me = any(
            self._collector._is_replace_me_decorator(d)
            for d in original_node.decorators
        )

        if has_replace_me:
            func_name = original_node.name.value
            self.checked_functions.append(func_name)

            try:
                # Try to extract replacement - this validates the function body
                self._collector._extract_replacement_from_body(original_node)
            except ReplacementExtractionError as e:
                self.errors.append(
                    f"Function '{func_name}': {e.details or 'Invalid replacement'}"
                )


def check_file(file_path: str) -> CheckResult:
    """Check all @replace_me decorated functions in a file.

    Args:
        file_path: Path to Python file to check.

    Returns:
        CheckResult containing validation results.
    """
    try:
        with open(file_path, encoding="utf-8") as f:
            source = f.read()
        return check_replacements(source)
    except OSError as e:
        return CheckResult(
            success=False,
            errors=[f"Failed to read file: {e}"],
            checked_functions=[],
        )


def check_replacements(source: str) -> CheckResult:
    """Check all @replace_me decorated functions in source code.

    This function validates that all functions decorated with @replace_me
    have valid replacement expressions that can be extracted.

    Args:
        source: Python source code to check.

    Returns:
        CheckResult containing validation results.
    """
    try:
        module = cst.parse_module(source)
    except cst.ParserSyntaxError as e:
        return CheckResult(
            success=False,
            errors=[f"Failed to parse source: {e}"],
            checked_functions=[],
        )

    checker = ReplacementChecker()
    wrapper = cst.MetadataWrapper(module)
    wrapper.visit(checker)

    return CheckResult(
        success=len(checker.errors) == 0,
        errors=checker.errors,
        checked_functions=checker.checked_functions,
    )
