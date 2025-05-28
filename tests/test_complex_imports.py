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

"""Tests for complex import scenarios."""

import ast

from dissolve.context_analyzer import (
    ContextAnalyzer,
    analyze_replacement_context,
    suggest_import_module,
)
from dissolve.migrate import migrate_source


def test_context_analyzer_local_definitions():
    """Test that ContextAnalyzer correctly identifies local definitions."""
    source = """
import math
from collections import defaultdict as dd
from typing import List

CONSTANT = 42

def helper_function(x):
    return x * 2

class HelperClass:
    pass

local_var = "test"
"""

    tree = ast.parse(source)
    context = ContextAnalyzer()
    context.visit(tree)

    # Check local definitions
    assert "helper_function" in context.local_functions
    assert "HelperClass" in context.local_classes
    assert "local_var" in context.local_variables
    assert "CONSTANT" in context.constants

    # Check imports
    assert context.get_import_source("math") == "math"
    assert context.get_import_source("dd") == "collections"
    assert context.get_import_source("List") == "typing"

    # Check if names are local
    assert context.is_local_reference("helper_function")
    assert context.is_local_reference("HelperClass")
    assert context.is_local_reference("CONSTANT")
    assert not context.is_local_reference("math")


def test_local_reference_in_replacement():
    """Test replacement that references local functions/variables."""
    source = """
from dissolve import replace_me

def helper(x):
    return x * MULTIPLIER

MULTIPLIER = 10

@replace_me()
def old_process(data):
    return helper(data)

result = old_process(5)
"""

    result = migrate_source(source)

    # Should replace old_process(5) with helper(5)
    assert "helper(5)" in result
    assert "old_process(5)" not in result
    # The helper function and MULTIPLIER should still be there
    assert "def helper(x):" in result
    assert "MULTIPLIER = 10" in result


def test_imported_alias_in_replacement():
    """Test replacement using imported aliases."""
    source = """
from collections import defaultdict as dd
from dissolve import replace_me

@replace_me()
def create_dict():
    return dd(list)

result = create_dict()
"""

    result = migrate_source(source)

    # Should replace create_dict() with dd(list)
    assert "dd(list)" in result
    # The function definition should still be there, but the call should be replaced
    assert "result = dd(list)" in result
    # The import should be preserved
    assert "from collections import defaultdict as dd" in result


def test_complex_replacement_with_multiple_references():
    """Test complex replacement with multiple types of references."""
    source = """
import math
from typing import List
from dissolve import replace_me

PRECISION = 2

def round_to_precision(x):
    return round(x, PRECISION)

@replace_me()
def calculate_distance(points: List[tuple]):
    return [math.sqrt(x**2 + y**2) for x, y in points]

result = calculate_distance([(3, 4), (1, 1)])
"""

    result = migrate_source(source)

    # Should replace the function call (allowing for spacing differences)
    # Note: Python 3.9 uses (x, y) while newer versions use x, y
    assert "math.sqrt(x" in result and (
        "for x, y in [(3, 4), (1, 1)]" in result  # Python 3.10+
        or "for (x, y) in [(3, 4), (1, 1)]" in result  # Python 3.9
    )
    assert "result = [math.sqrt(" in result
    # All imports and local definitions should be preserved
    assert "import math" in result
    assert "from typing import List" in result
    assert "PRECISION = 2" in result
    assert "def round_to_precision" in result


def test_suggest_import_module():
    """Test import module suggestion functionality."""
    # Create a context with some existing imports
    source = """
import math
import json
from pandas import DataFrame
"""
    tree = ast.parse(source)
    context = ContextAnalyzer()
    context.visit(tree)

    # Functions from existing imports should be suggested
    assert suggest_import_module("math", context) == "math"
    assert suggest_import_module("json", context) == "json"
    assert suggest_import_module("DataFrame", context) == "pandas"

    # Unknown functions should return None (no hardcoded patterns)
    assert suggest_import_module("unknown_func", context) is None
    assert suggest_import_module("sqrt", context) is None  # Not in existing imports


def test_replacement_with_smart_import_suggestion():
    """Test that replacement works without hardcoded import suggestions."""
    source = """
import json
from dissolve import replace_me

@replace_me()
def parse_data(text):
    return json.loads(text)  # Use existing import

result = parse_data('{"key": "value"}')
"""

    result = migrate_source(source)

    # Should replace parse_data call
    assert 'json.loads(\'{"key": "value"}\')' in result
    # Should preserve json import
    assert "import json" in result


def test_module_attribute_replacement():
    """Test replacement involving module.attribute patterns."""
    source = """
import os
from dissolve import replace_me

@replace_me()  
def get_home_dir():
    return os.path.expanduser("~")

result = get_home_dir()
"""

    result = migrate_source(source)

    # Should replace with os.path.expanduser("~")
    assert 'os.path.expanduser("~")' in result or "os.path.expanduser('~')" in result
    # Check that the call was replaced
    assert "result = os.path.expanduser(" in result
    # os import should be preserved
    assert "import os" in result


def test_replacement_with_constants_and_functions():
    """Test replacement that uses both constants and function calls."""
    source = """
import math
from dissolve import replace_me

PI = 3.14159

def calculate_area(radius):
    return PI * radius ** 2

@replace_me()
def circle_area(r):
    return math.pi * r * r

result = circle_area(5)
"""

    result = migrate_source(source)

    # Should replace circle_area(5) with math.pi * 5 * 5
    assert "math.pi * 5 * 5" in result
    assert "circle_area(5)" not in result
    # Local definitions should be preserved
    assert "PI = 3.14159" in result
    assert "def calculate_area" in result


def test_analyze_replacement_context():
    """Test the analyze_replacement_context function directly."""
    source = """
import math
from collections import defaultdict as dd

def helper():
    pass

LOCAL_VAR = 42
"""

    tree = ast.parse(source)
    context = ContextAnalyzer()
    context.visit(tree)

    # Test local reference
    reqs = analyze_replacement_context("helper(LOCAL_VAR)", context)
    local_reqs = [r for r in reqs if r.is_local_reference]
    assert len(local_reqs) >= 2  # helper and LOCAL_VAR
    assert any(r.name == "helper" for r in local_reqs)
    assert any(r.name == "LOCAL_VAR" for r in local_reqs)

    # Test imported reference
    reqs = analyze_replacement_context("math.sqrt(x)", context)
    imported_reqs = [r for r in reqs if not r.is_local_reference and r.module]
    assert any(r.name == "math" and r.module == "math" for r in imported_reqs)

    # Test alias reference
    reqs = analyze_replacement_context("dd(list)", context)
    alias_reqs = [r for r in reqs if not r.is_local_reference and r.module]
    assert any(r.name == "dd" and r.module == "collections" for r in alias_reqs)
