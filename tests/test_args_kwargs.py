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

"""Tests for *args and **kwargs support in @replace_me migrations."""

from dissolve.migrate import migrate_source


def test_simple_args():
    """Test basic *args functionality."""
    source = """
from dissolve import replace_me

@replace_me()
def sum_all(*args):
    return sum(args)

result1 = sum_all(1, 2, 3, 4)
result2 = sum_all()
result3 = sum_all(10)
"""

    result = migrate_source(source)

    assert "result1 = sum((1, 2, 3, 4))" in result
    assert "result2 = sum(())" in result
    assert "result3 = sum((10,))" in result or "result3 = sum((10))" in result


def test_args_with_operations():
    """Test *args with various operations."""
    source = """
from dissolve import replace_me

@replace_me()
def first_or_default(*args):
    return args[0] if args else None

@replace_me()
def count_args(*args):
    return len(args)

@replace_me()
def join_all(*args):
    return ', '.join(str(x) for x in args)

result1 = first_or_default(10, 20, 30)
result2 = first_or_default()
result3 = count_args(1, 2, 3)
result4 = join_all('a', 'b', 'c')
"""

    result = migrate_source(source)

    assert "(10, 20, 30)[0] if (10, 20, 30) else None" in result
    assert "()[0] if () else None" in result
    assert "len((1, 2, 3))" in result
    # Note: AST may add extra parentheses around generator expressions
    assert "', '.join((str(x) for x in ('a', 'b', 'c')))" in result


def test_mixed_params_and_args():
    """Test functions with both regular parameters and *args."""
    source = """
from dissolve import replace_me

@replace_me()
def process(x, y, *rest):
    return x + y + sum(rest)

@replace_me()
def format_message(prefix, *parts):
    return prefix + ': ' + ', '.join(parts)

result1 = process(10, 20, 1, 2, 3)
result2 = process(5, 3)
result3 = format_message("Error", "file not found", "permission denied")
"""

    result = migrate_source(source)

    assert "10 + 20 + sum((1, 2, 3))" in result
    assert "5 + 3 + sum(())" in result
    assert (
        "'Error' + ': ' + ', '.join(('file not found', 'permission denied'))" in result
    )


def test_args_with_defaults():
    """Test *args with functions that have default parameters."""
    source = """
from dissolve import replace_me

@replace_me()
def join_with_sep(sep=', ', *items):
    return sep.join(items)

result1 = join_with_sep('-', 'a', 'b', 'c')
result2 = join_with_sep('a', 'b')  # 'a' becomes sep, 'b' goes to *items
"""

    result = migrate_source(source)

    assert "'-'.join(('a', 'b', 'c'))" in result
    assert "'a'.join(('b',))" in result or "'a'.join(('b'))" in result


def test_kwargs_still_rejected():
    """Test that **kwargs functions are still not migrated."""
    source = """
from dissolve import replace_me

@replace_me()
def with_kwargs(**kwargs):
    return kwargs.get('x', 0)

@replace_me()
def mixed_args_kwargs(*args, **kwargs):
    return len(args) + len(kwargs)

result1 = with_kwargs(x=5, y=10)
result2 = mixed_args_kwargs(1, 2, x=3, y=4)
"""

    result = migrate_source(source)

    # These should NOT be migrated
    assert "with_kwargs(x=5, y=10)" in result
    assert "mixed_args_kwargs(1, 2, x=3, y=4)" in result
    assert "def with_kwargs(**kwargs):" in result
    assert "def mixed_args_kwargs(*args, **kwargs):" in result


def test_args_in_comprehensions():
    """Test *args used in comprehensions."""
    source = """
from dissolve import replace_me

@replace_me()
def double_all(*nums):
    return [x * 2 for x in nums]

@replace_me()
def filter_positive(*values):
    return tuple(v for v in values if v > 0)

result1 = double_all(1, 2, 3)
result2 = filter_positive(-1, 2, -3, 4)
"""

    result = migrate_source(source)

    assert "[x * 2 for x in (1, 2, 3)]" in result
    # Note: AST may add extra parentheses around generator expressions
    assert "tuple((v for v in (-1, 2, -3, 4) if v > 0))" in result


def test_nested_args_calls():
    """Test nested function calls with *args."""
    source = """
from dissolve import replace_me

@replace_me()
def add_all(*nums):
    return sum(nums)

@replace_me()
def multiply_sum(*nums):
    return add_all(*nums) * 2

# This will first inline multiply_sum, then add_all
result = multiply_sum(1, 2, 3)
"""

    result = migrate_source(source)

    # The multiply_sum function uses add_all(*nums), which creates a complex pattern
    # The result shows the inlining happens in stages
    assert "add_all(*(1, 2, 3)) * 2" in result
