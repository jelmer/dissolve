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

"""Tests for multi-statement function support in @replace_me migrations."""

from dissolve.migrate import migrate_source


def test_simple_assignment_return():
    """Test basic assignment + return pattern."""
    source = """
from dissolve import replace_me

@replace_me()
def double_and_add(x):
    doubled = x * 2
    return doubled + 1

result = double_and_add(5)
"""

    result = migrate_source(source)

    # Should inline by substituting the variable
    assert "result = 5 * 2 + 1" in result


def test_multiple_variables_assignment():
    """Test assignment with multiple variables used."""
    source = """
from dissolve import replace_me

@replace_me()
def calculate(x, y):
    product = x * y
    return product + product

@replace_me()
def complex_calc(a, b):
    temp = a + b
    return temp * temp + temp

result1 = calculate(3, 4)
result2 = complex_calc(2, 3)
"""

    result = migrate_source(source)

    # Should substitute the variables
    assert "result1 = 3 * 4 + 3 * 4" in result
    assert "result2 = (2 + 3) * (2 + 3) + (2 + 3)" in result


def test_assignment_with_method_calls():
    """Test assignment with method calls."""
    source = """
from dissolve import replace_me

@replace_me()
def process_string(s):
    upper = s.upper()
    return upper + '!'

@replace_me()
def get_length_doubled(text):
    length = len(text)
    return length * 2

result1 = process_string("hello")
result2 = get_length_doubled("world")
"""

    result = migrate_source(source)

    assert (
        "result1 = 'hello'.upper() + '!'" in result
        or 'result1 = "hello".upper() + "!"' in result
    )
    assert (
        "result2 = len('world') * 2" in result or 'result2 = len("world") * 2' in result
    )


def test_assignment_with_parameters():
    """Test assignment using function parameters."""
    source = """
from dissolve import replace_me

@replace_me()
def scale_and_offset(value, scale, offset):
    scaled = value * scale
    return scaled + offset

@replace_me()
def combine_strings(a, b, sep):
    joined = a + sep + b
    return joined.upper()

result1 = scale_and_offset(10, 2, 5)
result2 = combine_strings("hello", "world", " ")
"""

    result = migrate_source(source)

    assert "result1 = 10 * 2 + 5" in result
    assert (
        "result2 = ('hello' + ' ' + 'world').upper()" in result
        or 'result2 = ("hello" + " " + "world").upper()' in result
    )


def test_unused_assignment_not_simplified():
    """Test that assignments not used in return are not simplified."""
    source = """
from dissolve import replace_me

@replace_me()
def weird_function(x):
    unused = x * 2  # This variable is not used in return
    return x + 1

result = weird_function(5)
"""

    result = migrate_source(source)

    # Should not be migrated because the assignment isn't used
    assert "weird_function(5)" in result
    assert "def weird_function(x):" in result


def test_complex_multi_statement_not_simplified():
    """Test that complex multi-statement functions are not simplified."""
    source = """
from dissolve import replace_me

@replace_me()
def too_complex(x):
    if x > 0:
        result = x * 2
    else:
        result = x * -1
    return result

@replace_me()
def with_loop(items):
    total = 0
    for item in items:
        total += item
    return total

result1 = too_complex(5)
result2 = with_loop([1, 2, 3])
"""

    result = migrate_source(source)

    # These should NOT be migrated (too complex)
    assert "too_complex(5)" in result
    assert "with_loop([1, 2, 3])" in result
    assert "def too_complex(x):" in result
    assert "def with_loop(items):" in result


def test_chained_assignments():
    """Test that simple chained assignments can be handled."""
    source = """
from dissolve import replace_me

@replace_me()
def process_string(text):
    upper = text.upper()
    stripped = upper.strip()
    return stripped

@replace_me()
def chain_operations(x):
    doubled = x * 2
    squared = doubled ** 2
    return squared + 1

result1 = process_string("  hello  ")
result2 = chain_operations(3)
"""

    result = migrate_source(source)

    # Should chain the operations
    assert (
        "result1 = '  hello  '.upper().strip()" in result
        or 'result1 = "  hello  ".upper().strip()' in result
    )
    assert (
        "result2 = (3 * 2) ** 2 + 1" in result or "result2 = 3 * 2 ** 2 + 1" in result
    )


def test_multiple_assignments_not_chainable():
    """Test that non-chainable multiple assignments are not handled."""
    source = """
from dissolve import replace_me

@replace_me()
def complex_multiple(x, y):
    step1 = x * 2
    step2 = y + 1  # Uses different variable, not chainable
    return step1 + step2

result = complex_multiple(5, 3)
"""

    result = migrate_source(source)

    # This should not be migrated (not chainable)
    assert "complex_multiple(5, 3)" in result
    assert "def complex_multiple(x, y):" in result


def test_assignment_with_args():
    """Test assignment + return with *args functions."""
    source = """
from dissolve import replace_me

@replace_me()
def sum_and_double(*args):
    total = sum(args)
    return total * 2

result = sum_and_double(1, 2, 3, 4)
"""

    result = migrate_source(source)

    # Should handle *args with assignment
    assert "result = sum((1, 2, 3, 4)) * 2" in result


def test_assignment_with_defaults():
    """Test assignment + return with default parameters."""
    source = """
from dissolve import replace_me

@replace_me()
def scale_with_default(value, multiplier=2):
    scaled = value * multiplier
    return scaled + 10

result1 = scale_with_default(5)
result2 = scale_with_default(5, 3)
"""

    result = migrate_source(source)

    # Should handle default parameters
    assert "result1 = 5 * 2 + 10" in result
    assert "result2 = 5 * 3 + 10" in result
