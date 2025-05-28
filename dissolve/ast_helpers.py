# Copyright (C) 2024 Jelmer Vernooij <jelmer@samba.org>
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

"""Shared AST helper utilities for the dissolve package."""

import ast


def is_replace_me_decorator(decorator: ast.AST) -> bool:
    """Check if a decorator is @replace_me.

    Args:
        decorator: The decorator AST node to check.

    Returns:
        True if the decorator is @replace_me, False otherwise.
    """
    if isinstance(decorator, ast.Name) and decorator.id == "replace_me":
        return True
    if isinstance(decorator, ast.Call):
        if isinstance(decorator.func, ast.Name) and decorator.func.id == "replace_me":
            return True
        if (
            isinstance(decorator.func, ast.Attribute)
            and decorator.func.attr == "replace_me"
        ):
            return True
    return False
