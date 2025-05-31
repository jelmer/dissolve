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

"""Tests for import management functionality."""

import ast

from dissolve.import_utils import (
    ImportManager,
    ImportRequirement,
    extract_imports_from_expression,
)
from dissolve.migrate import migrate_source


def test_extract_imports_from_expression():
    """Test extracting potential imports from expressions."""
    # Simple function call
    expr = "new_func(x)"
    imports = extract_imports_from_expression(expr)
    assert len(imports) == 1
    assert imports[0].name == "new_func"

    # Module.function call
    expr = "math.sqrt(x)"
    imports = extract_imports_from_expression(expr)
    # Should identify math as a potential import
    assert any(imp.name == "math" for imp in imports)

    # No imports needed for builtins
    expr = "len(x) + sum(y)"
    imports = extract_imports_from_expression(expr)
    assert len(imports) == 0


def test_import_manager_basic():
    """Test basic ImportManager functionality."""
    source = """
from old_module import old_func
import math

def use_it():
    return old_func(5)
"""
    tree = ast.parse(source)
    manager = ImportManager(tree)

    # Check existing imports are detected
    assert len(manager.imports) == 2

    # Check has_import
    req = ImportRequirement(module="old_module", name="old_func")
    assert manager.has_import(req)

    # Check adding new import
    new_req = ImportRequirement(module="new_module", name="new_func")
    manager.add_import(new_req)
    assert manager.has_import(new_req)


def test_function_call_replacer_tracks_new_functions():
    """Test that FunctionCallReplacer tracks new functions used."""
    source = """
from dissolve import replace_me

@replace_me()
def old_api(x):
    return new_api(x, mode='legacy')

result = old_api(42)
"""

    result = migrate_source(source)

    # The migration should replace old_api(42) with new_api(42, mode='legacy')
    assert "new_api(42, mode='legacy')" in result
    assert "old_api(42)" not in result


def test_module_function_replacement():
    """Test replacement involving module.function patterns."""
    source = """
from dissolve import replace_me

@replace_me()
def calculate(x):
    return math.sqrt(x)

result = calculate(16)
"""

    result = migrate_source(source)

    # Should replace calculate(16) with math.sqrt(16)
    assert "math.sqrt(16)" in result
    assert "calculate(16)" not in result


def test_import_preserved_when_still_needed():
    """Test that imports are preserved when the imported name is still used elsewhere."""
    source = """
from old_module import old_func, other_func

from dissolve import replace_me

@replace_me()
def wrapper(x):
    return new_func(x)

result1 = wrapper(5)
result2 = other_func(10)
"""

    result = migrate_source(source)

    # wrapper(5) should be replaced with new_func(5)
    assert "new_func(5)" in result
    # The import should still include other_func
    assert "other_func" in result


def test_complex_replacement_with_imports():
    """Test complex replacements that might need import updates."""
    source = """
from dissolve import replace_me

@replace_me()
def process_data(data):
    return pd.DataFrame(data).apply(transform)

result = process_data([1, 2, 3])
"""

    result = migrate_source(source)

    # Should replace the function call
    assert "pd.DataFrame([1, 2, 3]).apply(transform)" in result
    assert "process_data([1, 2, 3])" not in result
