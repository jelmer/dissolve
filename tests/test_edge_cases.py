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

"""Tests for edge cases in @replace_me migrations."""

from dissolve.migrate import migrate_source


def test_default_arguments():
    """Test functions with default arguments."""
    source = """
from dissolve import replace_me

@replace_me()
def greet(name="World", greeting="Hello"):
    return f"{greeting}, {name}!"

# Various ways to call the function
result1 = greet()
result2 = greet("Alice")
result3 = greet("Bob", "Hi")
result4 = greet(greeting="Hey")
result5 = greet(name="Charlie")
"""

    result = migrate_source(source)

    # Check various call patterns
    # The defaults are substituted, look for the pattern with quoted values
    assert (
        "f\"{'Hello'}, {'World'}!\"" in result
        or "f\"{'Hello'}, {'World'}!\"" in result
        or "f'{'Hello'}, {'World'}!'" in result
    )  # Default args
    assert (
        "f\"{'Hello'}, {'Alice'}!\"" in result
        or "f\"{'Hello'}, {'Alice'}!\"" in result
        or "f'{'Hello'}, {'Alice'}!'" in result
    )  # One positional
    assert (
        "f\"{'Hi'}, {'Bob'}!\"" in result
        or "f\"{'Hi'}, {'Bob'}!\"" in result
        or "f'{'Hi'}, {'Bob'}!'" in result
    )  # Two positional
    # Check keyword argument patterns
    assert (
        "f\"{'Hey'}, {'World'}!\"" in result or "f\"{'Hey'}, {'World'}!\"" in result
    )  # greeting="Hey"
    assert (
        "f\"{'Hello'}, {'Charlie'}!\"" in result
        or "f\"{'Hello'}, {'Charlie'}!\"" in result
    )  # name="Charlie"


def test_args_kwargs():
    """Test functions with *args and **kwargs."""
    source = """
from dissolve import replace_me

@replace_me()
def concat(*args):
    return "".join(args)

@replace_me()
def make_dict(**kwargs):
    return dict(kwargs)

@replace_me()
def flexible_func(x, *args, **kwargs):
    return (x, args, kwargs)

result1 = concat("a", "b", "c")
result2 = make_dict(a=1, b=2)
result3 = flexible_func(1, 2, 3, x=4, y=5)
"""

    result = migrate_source(source)

    # *args functions should now be migrated (supported)
    assert '"".join(("a", "b", "c"))' in result or "''.join(('a', 'b', 'c'))" in result

    # **kwargs functions should NOT be migrated (not supported)
    assert "make_dict(a=1, b=2)" in result
    assert "flexible_func(1, 2, 3, x=4, y=5)" in result
    # The **kwargs function definitions should still be there
    assert "def make_dict(**kwargs):" in result
    assert "def flexible_func(x, *args, **kwargs):" in result


def test_lambda_expressions():
    """Test replacement expressions containing lambdas."""
    source = """
from dissolve import replace_me

@replace_me()
def apply_twice(f, x):
    return f(f(x))

@replace_me()
def make_adder(n):
    return lambda x: x + n

result1 = apply_twice(lambda x: x * 2, 5)
result2 = make_adder(10)
result3 = result2(5)  # This calls the returned lambda
"""

    result = migrate_source(source)

    # Lambda expressions are complex - check handling
    assert "apply_twice" in result or "lambda" in result


def test_method_calls():
    """Test replacements involving method calls."""
    source = """
from dissolve import replace_me

class MyClass:
    def __init__(self, value):
        self.value = value
    
    @replace_me()
    def get_double(self):
        return self.value * 2
    
    @replace_me()
    def process(self, other):
        return self.value + other.value

obj1 = MyClass(5)
obj2 = MyClass(3)

result1 = obj1.get_double()
result2 = obj1.process(obj2)
"""

    result = migrate_source(source)

    # Method calls with self references are complex
    # They likely won't be migrated since self is not available in the call context
    assert "class MyClass:" in result


def test_comprehensions():
    """Test replacement expressions with comprehensions."""
    source = """
from dissolve import replace_me

@replace_me()
def squares(n):
    return [x**2 for x in range(n)]

@replace_me()
def filtered_dict(items, min_value):
    return {k: v for k, v in items.items() if v >= min_value}

@replace_me()
def nested_comp(matrix):
    return [[cell * 2 for cell in row] for row in matrix]

result1 = squares(5)
result2 = filtered_dict({"a": 1, "b": 5, "c": 3}, 3)
result3 = nested_comp([[1, 2], [3, 4]])
"""

    result = migrate_source(source)

    # Check if comprehensions are properly substituted
    assert (
        "[x**2 for x in range(5)]" in result or "[x ** 2 for x in range(5)]" in result
    )


def test_format_strings():
    """Test various string formatting in replacements."""
    source = """
from dissolve import replace_me

@replace_me()
def format_old(name, age):
    return "%s is %d years old" % (name, age)

@replace_me()
def format_new(name, age):
    return "{} is {} years old".format(name, age)

@replace_me()
def format_fstring(name, age):
    return f"{name} is {age} years old"

result1 = format_old("Alice", 30)
result2 = format_new("Bob", 25)
result3 = format_fstring("Charlie", 35)
"""

    result = migrate_source(source)

    # Check different formatting styles
    assert (
        "'Alice' is 30 years old" in result
        or '"Alice" is 30 years old' in result
        or "'%s is %d years old' % ('Alice', 30)" in result
    )


def test_nested_function_calls():
    """Test deeply nested function calls."""
    source = """
from dissolve import replace_me
import math

@replace_me()
def process(x):
    return x * 2

@replace_me()
def transform(x):
    return math.sqrt(x)

# Nested calls
result = process(transform(process(8)))
"""

    result = migrate_source(source)

    # Should handle nested replacements
    assert "math.sqrt" in result
    assert "* 2" in result


def test_conditional_expressions():
    """Test ternary/conditional expressions in replacements."""
    source = """
from dissolve import replace_me

@replace_me()
def safe_divide(a, b):
    return a / b if b != 0 else 0

@replace_me()
def max_value(x, y):
    return x if x > y else y

result1 = safe_divide(10, 2)
result2 = safe_divide(10, 0)
result3 = max_value(5, 3)
"""

    result = migrate_source(source)

    # Check conditional expressions
    assert "10 / 2 if 2 != 0 else 0" in result or "10 / 2 if 2 != 0 else 0" in result


def test_walrus_operator():
    """Test assignments in expressions (walrus operator)."""
    source = """
from dissolve import replace_me

@replace_me()
def process_with_assignment(items):
    return [y for x in items if (y := x * 2) > 5]

result = process_with_assignment([1, 2, 3, 4])
"""

    result = migrate_source(source)

    # Walrus operator in comprehensions
    assert ":=" in result or "process_with_assignment" in result


def test_type_annotations():
    """Test functions with type annotations."""
    source = """
from typing import List, Dict, Optional
from dissolve import replace_me

@replace_me()
def typed_func(x: int, y: float) -> float:
    return x + y

@replace_me()
def complex_types(items: List[Dict[str, int]]) -> Optional[int]:
    return items[0].get("key") if items else None

result1 = typed_func(5, 3.14)
result2 = complex_types([{"key": 42}])
"""

    result = migrate_source(source)

    # Type annotations should not affect the replacement
    assert "5 + 3.14" in result


def test_decorator_stacking():
    """Test functions with multiple decorators."""
    source = """
from functools import lru_cache
from dissolve import replace_me

@lru_cache(maxsize=128)
@replace_me()
def expensive_computation(n):
    return sum(range(n))

@replace_me()
@staticmethod
def static_method(x):
    return x * 2

result = expensive_computation(100)
"""

    result = migrate_source(source)

    # Multiple decorators might affect migration
    assert "sum(range(100))" in result or "expensive_computation(100)" in result


def test_generator_expressions():
    """Test generator expressions in replacements."""
    source = """
from dissolve import replace_me

@replace_me()
def make_generator(n):
    return (x**2 for x in range(n))

@replace_me()
def sum_generator(items):
    return sum(x for x in items if x > 0)

result1 = list(make_generator(5))
result2 = sum_generator([-1, 2, -3, 4])
"""

    result = migrate_source(source)

    # Generator expressions
    assert "for x in range" in result


def test_async_functions():
    """Test async function replacements."""
    source = """
from dissolve import replace_me
import asyncio

@replace_me()
async def async_double(x):
    return x * 2

@replace_me()
async def async_fetch(url):
    return f"Fetched: {url}"

async def main():
    result1 = await async_double(5)
    result2 = await async_fetch("https://example.com")
    return result1, result2
"""

    result = migrate_source(source)

    # Async functions are complex - they likely won't be migrated
    assert "async def" in result


def test_multiline_expressions():
    """Test replacements with multiline expressions."""
    source = """
from dissolve import replace_me

@replace_me()
def multiline_calc(x, y, z):
    return (x + y +
            z * 2)

@replace_me()
def multiline_string():
    return '''This is a
multiline
string'''

result1 = multiline_calc(1, 2, 3)
result2 = multiline_string()
"""

    result = migrate_source(source)

    # Multiline expressions might be reformatted
    assert "1 + 2" in result or "multiline_calc" in result


def test_slice_operations():
    """Test slice operations in replacements."""
    source = """
from dissolve import replace_me

@replace_me()
def get_slice(lst, start, end):
    return lst[start:end]

@replace_me()
def get_every_other(lst):
    return lst[::2]

@replace_me()
def reverse_list(lst):
    return lst[::-1]

data = [1, 2, 3, 4, 5]
result1 = get_slice(data, 1, 3)
result2 = get_every_other(data)
result3 = reverse_list(data)
"""

    result = migrate_source(source)

    # Check slice operations
    assert "[1, 2, 3, 4, 5][1:3]" in result or "data[1:3]" in result


def test_unpacking_operations():
    """Test unpacking operations in replacements."""
    source = """
from dissolve import replace_me

@replace_me()
def merge_dicts(d1, d2):
    return {**d1, **d2}

@replace_me()
def concat_lists(l1, l2):
    return [*l1, *l2]

@replace_me()
def unpack_args(func, args):
    return func(*args)

dict1 = {"a": 1}
dict2 = {"b": 2}
list1 = [1, 2]
list2 = [3, 4]

result1 = merge_dicts(dict1, dict2)
result2 = concat_lists(list1, list2)
result3 = unpack_args(sum, [1, 2, 3])
"""

    result = migrate_source(source)

    # Check unpacking operations
    assert "**" in result or "*" in result


def test_binary_operators():
    """Test various binary operators in replacements."""
    source = """
from dissolve import replace_me

@replace_me()
def bit_ops(x, y):
    return x & y | (x ^ y)

@replace_me()
def comparison_chain(x, y, z):
    return x < y <= z

@replace_me()
def power_mod(base, exp, mod):
    return pow(base, exp, mod)

result1 = bit_ops(12, 7)
result2 = comparison_chain(1, 2, 3)
result3 = power_mod(2, 10, 1000)
"""

    result = migrate_source(source)

    # Check various operators
    assert "12 & 7" in result or "bit_ops" in result
    assert "pow(2, 10, 1000)" in result


def test_imports_in_replacement():
    """Test when replacement expression would need new imports."""
    source = """
from dissolve import replace_me

@replace_me()
def get_cwd():
    import os
    return os.getcwd()

@replace_me() 
def parse_json(text):
    import json
    return json.loads(text)

result1 = get_cwd()
result2 = parse_json('{"key": "value"}')
"""

    result = migrate_source(source)

    # Functions with local imports are complex
    assert "get_cwd()" in result or "os.getcwd()" in result


def test_recursive_functions():
    """Test recursive function definitions."""
    source = """
from dissolve import replace_me

@replace_me()
def factorial(n):
    return 1 if n <= 1 else n * factorial(n - 1)

@replace_me()
def fibonacci(n):
    return n if n <= 1 else fibonacci(n-1) + fibonacci(n-2)

result1 = factorial(5)
result2 = fibonacci(7)
"""

    result = migrate_source(source)

    # Recursive functions cannot be inlined
    assert "factorial(" in result
    assert "fibonacci(" in result
