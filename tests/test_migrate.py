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

from typing import Literal

from dissolve.migrate import migrate_source


class TestMigrateSource:
    def test_migrate_with_substring_params(self):
        """Test that migration handles parameter names that are substrings correctly."""
        source = """
from dissolve import replace_me

@replace_me()
def process_range(n):
    return list(range(n))

# Usage
result = process_range(5)
items = process_range(n=10)
"""

        result = migrate_source(source.strip())

        # Check that range(n) is properly replaced with range(5) and range(10)
        assert "list(range(5))" in result
        assert "list(range(10))" in result
        # Make sure it didn't do substring replacement like "ra5ge"
        assert "ra5ge" not in result
        assert "ra10ge" not in result

    def test_simple_function_replacement(self):
        source = """
from dissolve import replace_me

@replace_me(since="0.1.0")
def inc(x):
    return x + 1

result = inc(5)
"""
        result = migrate_source(source.strip())
        assert "result = 5 + 1" in result or "result = (5 + 1)" in result

    def test_multiple_calls(self):
        source = """
from dissolve import replace_me

@replace_me()
def add_numbers(a, b):
    return a + b

x = add_numbers(1, 2)
y = add_numbers(3, 4)
z = add_numbers(a=5, b=6)
"""
        result = migrate_source(source.strip())
        assert "x = 1 + 2" in result or "x = (1 + 2)" in result
        assert "y = 3 + 4" in result or "y = (3 + 4)" in result
        assert "z = 5 + 6" in result or "z = (5 + 6)" in result

    def test_keyword_arguments(self):
        source = """
from dissolve import replace_me

@replace_me()
def mult(x, y):
    return x * y

result = mult(x=3, y=4)
"""
        result = migrate_source(source.strip())
        assert "result = 3 * 4" in result or "result = (3 * 4)" in result

    def test_no_replacement_decorator(self):
        source = """
def regular_function(x):
    return x + 1

result = regular_function(5)
"""
        result = migrate_source(source.strip())
        assert result == source.strip()

    def test_complex_expression(self):
        source = """
from dissolve import replace_me

@replace_me()
def power(base, exp):
    return base ** exp

result = power(2, 3)
"""
        result = migrate_source(source.strip())
        assert "result = 2 ** 3" in result or "result = (2 ** 3)" in result

    def test_nested_calls(self):
        source = """
from dissolve import replace_me

@replace_me()
def inc(x):
    return x + 1

result = inc(inc(5))
"""
        result = migrate_source(source.strip())
        # Should expand nested calls
        assert (
            "result = (5 + 1) + 1" in result
            or "result = ((5 + 1) + 1)" in result
            or "result = 5 + 1 + 1" in result
        )

    def test_imports_with_module_resolver(self):
        main_source = """
from mymodule import old_func

result = old_func(10)
"""

        module_source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return x * 2
"""

        def test_resolver(module_name, _):
            if module_name == "mymodule":
                return module_source
            return None

        result = migrate_source(main_source.strip(), module_resolver=test_resolver)
        assert "result = 10 * 2" in result or "result = (10 * 2)" in result
        # The import is preserved but the function call is replaced
        assert "from mymodule import old_func" in result

    def test_import_with_alias(self):
        main_source = """
from mymodule import old_func as of

result = of(20)
"""

        module_source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return x * 2
"""

        def test_resolver(module_name, _):
            if module_name == "mymodule":
                return module_source
            return None

        result = migrate_source(main_source.strip(), module_resolver=test_resolver)
        assert "result = 20 * 2" in result or "result = (20 * 2)" in result

    def test_decorator_variations(self):
        # Test different decorator syntax variations
        source = """
from dissolve import replace_me
import dissolve

@replace_me()
def f1(x):
    return x

@dissolve.replace_me()
def f2(x):
    return x

a = f1(1)
b = f2(2)
"""
        result = migrate_source(source.strip())
        assert "a = 1" in result
        assert "b = 2" in result

    def test_preserve_other_decorators(self):
        source = """
from dissolve import replace_me

@property
@replace_me()
def value():
    return 42

x = value()
"""
        result = migrate_source(source.strip())
        assert "@property" in result
        assert "x = 42" in result

    def test_non_simple_function(self):
        # Test that functions with complex bodies are not replaced
        source = """
from dissolve import replace_me

@replace_me()
def complex_func(x):
    y = x + 1
    return y * 2

result = complex_func(5)
"""
        result = migrate_source(source.strip())
        # Should keep the original call when function body is not simple
        assert "complex_func(5)" in result

    def test_empty_source(self):
        result = migrate_source("")
        assert result == ""

    def test_no_imports(self):
        source = """
def normal_func(x):
    return x + 1

print(normal_func(5))
"""
        result = migrate_source(source.strip())
        assert result == source.strip()


class TestInteractiveMigration:
    def test_interactive_yes(self):
        """Test interactive mode with 'yes' responses."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x * 2)

result = old_func(5)
"""
        responses = ["y"]
        response_iter = iter(responses)

        def mock_prompt(old_call: str, new_call: str) -> Literal["y", "n", "a", "q"]:
            return next(response_iter)

        result = migrate_source(
            source.strip(), interactive=True, prompt_func=mock_prompt
        )
        assert (
            "result = new_func(5 * 2)" in result
            or "result = new_func((5 * 2))" in result
        )

    def test_interactive_no(self):
        """Test interactive mode with 'no' responses."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x * 2)

result = old_func(5)
"""
        responses = ["n"]
        response_iter = iter(responses)

        def mock_prompt(old_call: str, new_call: str) -> Literal["y", "n", "a", "q"]:
            return next(response_iter)

        result = migrate_source(
            source.strip(), interactive=True, prompt_func=mock_prompt
        )
        assert "old_func(5)" in result
        assert (
            "new_func" not in result or "@replace_me()" in result
        )  # new_func only in decorator

    def test_interactive_all(self):
        """Test interactive mode with 'all' response."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x * 2)

a = old_func(1)
b = old_func(2)
c = old_func(3)
"""
        responses = ["a"]  # Only need one response for 'all'
        response_iter = iter(responses)

        def mock_prompt(old_call: str, new_call: str) -> Literal["y", "n", "a", "q"]:
            return next(response_iter)

        result = migrate_source(
            source.strip(), interactive=True, prompt_func=mock_prompt
        )
        # All calls should be replaced
        assert "a = new_func(1 * 2)" in result or "a = new_func((1 * 2))" in result
        assert "b = new_func(2 * 2)" in result or "b = new_func((2 * 2))" in result
        assert "c = new_func(3 * 2)" in result or "c = new_func((3 * 2))" in result

    def test_interactive_quit(self):
        """Test interactive mode with 'quit' response."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x * 2)

a = old_func(1)
b = old_func(2)
c = old_func(3)
"""
        responses = ["y", "q"]  # Replace first, quit on second
        response_iter = iter(responses)

        def mock_prompt(old_call: str, new_call: str) -> Literal["y", "n", "a", "q"]:
            return next(response_iter)

        result = migrate_source(
            source.strip(), interactive=True, prompt_func=mock_prompt
        )
        # First call should be replaced
        assert "a = new_func(1 * 2)" in result or "a = new_func((1 * 2))" in result
        # Remaining calls should not be replaced
        assert "old_func(2)" in result
        assert "old_func(3)" in result

    def test_interactive_mixed_responses(self):
        """Test interactive mode with mixed responses."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x * 2)

a = old_func(1)
b = old_func(2)
c = old_func(3)
d = old_func(4)
"""
        responses = ["y", "n", "y", "n"]
        response_iter = iter(responses)

        def mock_prompt(old_call: str, new_call: str) -> Literal["y", "n", "a", "q"]:
            return next(response_iter)

        result = migrate_source(
            source.strip(), interactive=True, prompt_func=mock_prompt
        )
        # First and third calls should be replaced
        assert "a = new_func(1 * 2)" in result or "a = new_func((1 * 2))" in result
        assert "old_func(2)" in result
        assert "c = new_func(3 * 2)" in result or "c = new_func((3 * 2))" in result
        assert "old_func(4)" in result

    def test_property_replacement(self):
        """Test that @replace_me works with properties."""
        source = """
from dissolve import replace_me

class MyClass:
    def __init__(self, value):
        self._value = value
        
    @property
    @replace_me(since="1.0.0")
    def old_property(self):
        return self.new_property
        
    @property
    def new_property(self):
        return self._value * 2

obj = MyClass(5)
result = obj.old_property
"""

        result = migrate_source(source.strip())

        # Check that property access is replaced
        assert "result = obj.new_property" in result
        # Check that the deprecated property definition remains
        assert "@replace_me" in result
        assert "def old_property" in result

    def test_property_with_complex_replacement(self):
        """Test property replacement with more complex expressions."""
        source = """
from dissolve import replace_me

class Calculator:
    def __init__(self, x, y):
        self.x = x
        self.y = y
        
    @property
    @replace_me()
    def old_sum(self):
        return self.x + self.y + 10

calc = Calculator(3, 7)
total = calc.old_sum
"""

        result = migrate_source(source.strip())

        # Check that property access is replaced with the complex expression
        assert "total = calc.x + calc.y + 10" in result
