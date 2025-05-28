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

"""Tests for local variable handling in @replace_me migrations."""

import os
import tempfile

from dissolve.migrate import migrate_file_with_imports, migrate_source


def test_simple_module_constant():
    """Test migration with simple module constants."""
    source = """
from dissolve import replace_me

SCALE_FACTOR = 2.5

@replace_me()
def scale_value(x):
    return x * SCALE_FACTOR

result = scale_value(10)
"""

    result = migrate_source(source)

    # Should inline with the constant reference preserved
    assert "result = 10 * SCALE_FACTOR" in result
    assert "SCALE_FACTOR = 2.5" in result


def test_multiple_module_variables():
    """Test migration with multiple module variables."""
    source = """
from dissolve import replace_me

PREFIX = "Hello"
SUFFIX = "!"
DEFAULT_NAME = "World"

@replace_me()
def greet(name=None):
    return f"{PREFIX}, {name or DEFAULT_NAME}{SUFFIX}"

result1 = greet()
result2 = greet("Alice")
"""

    result = migrate_source(source)

    # Check that all variables are preserved in the inlined code
    assert "PREFIX" in result
    assert "DEFAULT_NAME" in result
    assert "SUFFIX" in result
    assert (
        'f"{PREFIX}, {None or DEFAULT_NAME}{SUFFIX}"' in result
        or "f'{PREFIX}, {None or DEFAULT_NAME}{SUFFIX}'" in result
    )


def test_cross_module_variable_imports():
    """Test that variables from other modules are imported when needed."""
    with tempfile.TemporaryDirectory() as tmpdir:
        # Create module with constants
        module_a = os.path.join(tmpdir, "constants.py")
        with open(module_a, "w") as f:
            f.write("""
from dissolve import replace_me

MULTIPLIER = 3
DEFAULT_VALUE = 100

@replace_me()
def process(x=None):
    return (x or DEFAULT_VALUE) * MULTIPLIER
""")

        # Create module that uses the function
        module_b = os.path.join(tmpdir, "usage.py")
        with open(module_b, "w") as f:
            f.write("""
from constants import process

result1 = process(50)
result2 = process()
""")

        # Migrate the usage module
        result = migrate_file_with_imports(module_b)

        # Should import the necessary constants
        assert (
            "from constants import process, MULTIPLIER, DEFAULT_VALUE" in result
            or "from constants import process, DEFAULT_VALUE, MULTIPLIER" in result
        )
        assert "(50 or DEFAULT_VALUE) * MULTIPLIER" in result
        assert "(None or DEFAULT_VALUE) * MULTIPLIER" in result


def test_builtin_names_not_imported():
    """Test that builtin names are not treated as variables to import."""
    source = """
from dissolve import replace_me

@replace_me()
def get_type_name(obj):
    return type(obj).__name__

@replace_me()
def safe_len(obj):
    return len(obj) if obj is not None else 0

result1 = get_type_name("test")
result2 = safe_len([1, 2, 3])
"""

    result = migrate_source(source)

    # Builtins should be used directly, not imported
    assert "import type" not in result
    assert "import len" not in result
    assert "import None" not in result
    assert 'type("test").__name__' in result or "type('test').__name__" in result


def test_local_variable_inside_function():
    """Test that simple local variables inside functions can now be inlined."""
    source = """
from dissolve import replace_me

MODULE_VAR = 10

@replace_me()
def process(x):
    # This can now be inlined with multi-statement support
    local_var = x * 2
    return local_var + MODULE_VAR

result = process(5)
"""

    result = migrate_source(source)

    # Multi-statement functions with simple assignments can now be migrated
    assert "result = 5 * 2 + MODULE_VAR" in result
    assert "MODULE_VAR = 10" in result


def test_imported_module_attributes():
    """Test handling of module attributes like math.pi."""
    source = """
import math
from dissolve import replace_me

@replace_me()
def circle_area(radius):
    return math.pi * radius ** 2

result = circle_area(5)
"""

    result = migrate_source(source)

    # Module attributes should be preserved
    assert "math.pi * 5 ** 2" in result or "math.pi * 5**2" in result
    # The import should be preserved (though it might be reformatted)
    assert "math" in result and (
        "import math" in result or "from math import" in result
    )


def test_aliased_imports():
    """Test handling of aliased imports and module variables."""
    with tempfile.TemporaryDirectory() as tmpdir:
        # Create module with constants
        module_a = os.path.join(tmpdir, "config.py")
        with open(module_a, "w") as f:
            f.write("""
from dissolve import replace_me

MAX_SIZE = 1000
MIN_SIZE = 10

@replace_me()
def clamp_size(size):
    return max(MIN_SIZE, min(size, MAX_SIZE))
""")

        # Create module that uses aliased import
        module_b = os.path.join(tmpdir, "app.py")
        with open(module_b, "w") as f:
            f.write("""
from config import clamp_size as limit_size

result = limit_size(1500)
""")

        # Migrate the usage module
        result = migrate_file_with_imports(module_b)

        # Should import the necessary constants from the original module
        assert "from config import" in result
        assert "MIN_SIZE" in result
        assert "MAX_SIZE" in result
        assert "max(MIN_SIZE, min(1500, MAX_SIZE))" in result


def test_name_conflicts():
    """Test handling of potential name conflicts."""
    source = """
from dissolve import replace_me

# Local variable with common name
result = 42

@replace_me()
def compute():
    return result * 2

# This 'result' shadows the module variable
result = compute()
"""

    migrated = migrate_source(source)

    # The migration should preserve the variable reference
    # The actual value substitution happens at runtime
    assert "result = result * 2" in migrated


def test_complex_expressions_with_variables():
    """Test complex expressions involving module variables."""
    source = """
from dissolve import replace_me

FACTOR_A = 2
FACTOR_B = 3
OFFSET = 10

@replace_me()
def complex_calc(x, y):
    return (x * FACTOR_A + y * FACTOR_B) / 2 + OFFSET

result = complex_calc(5, 7)
"""

    result = migrate_source(source)

    # All variables should be preserved in the complex expression
    assert "(5 * FACTOR_A + 7 * FACTOR_B) / 2 + OFFSET" in result
    assert "FACTOR_A = 2" in result
    assert "FACTOR_B = 3" in result
    assert "OFFSET = 10" in result
