"""Comprehensive tests for all replacement scenarios."""

from dissolve.migrate import migrate_source


class TestComprehensiveReplacements:
    """Test all different types of replacements."""

    def test_simple_function_replacement(self):
        """Test replacing a simple function call."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

result = old_func(10)
"""
        result = migrate_source(source.strip())
        assert "result = new_func(10 + 1)" in result

    def test_method_replacement(self):
        """Test replacing method calls."""
        source = """
from dissolve import replace_me

class MyClass:
    @replace_me()
    def old_method(self, x):
        return self.new_method(x * 2)

obj = MyClass()
result = obj.old_method(10)
"""
        result = migrate_source(source.strip())
        assert "result = obj.new_method(10 * 2)" in result

    def test_property_replacement(self):
        """Test replacing property access."""
        source = """
from dissolve import replace_me

class MyClass:
    @property
    @replace_me()
    def old_prop(self):
        return self.new_prop

obj = MyClass()
value = obj.old_prop
"""
        result = migrate_source(source.strip())
        assert "value = obj.new_prop" in result

    def test_multiple_parameters(self):
        """Test function with multiple parameters."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(a, b, c):
    return new_func(a + b, c)

result = old_func(1, 2, 3)
"""
        result = migrate_source(source.strip())
        assert "result = new_func(1 + 2, 3)" in result

    def test_chained_method_calls(self):
        """Test chained method calls."""
        source = """
from dissolve import replace_me

class MyClass:
    @replace_me()
    def old_method(self):
        return self.new_method()

obj = MyClass()
result = obj.old_method().something_else()
"""
        result = migrate_source(source.strip())
        assert "result = obj.new_method().something_else()" in result

    def test_static_method_replacement(self):
        """Test static method replacement."""
        source = """
from dissolve import replace_me

class MyClass:
    @staticmethod
    @replace_me()
    def old_static(x):
        return MyClass.new_static(x * 2)

result = MyClass.old_static(5)
"""
        result = migrate_source(source.strip())
        assert "result = MyClass.new_static(5 * 2)" in result

    def test_class_method_replacement(self):
        """Test class method replacement."""
        source = """
from dissolve import replace_me

class MyClass:
    @classmethod
    @replace_me()
    def old_class_method(cls, x):
        return cls.new_class_method(x + 10)

result = MyClass.old_class_method(5)
"""
        result = migrate_source(source.strip())
        assert "result = MyClass.new_class_method(5 + 10)" in result

    def test_nested_function_calls(self):
        """Test nested function calls."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x)

result = old_func(old_func(10))
"""
        result = migrate_source(source.strip())
        assert "result = new_func(new_func(10))" in result

    def test_keyword_arguments(self):
        """Test function with keyword arguments."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x, y=10):
    return new_func(x + y)

result = old_func(5, y=20)
"""
        result = migrate_source(source.strip())
        assert "result = new_func(5 + 20)" in result

    def test_property_with_expression(self):
        """Test property with complex expression."""
        source = """
from dissolve import replace_me

class MyClass:
    @property
    @replace_me()
    def old_prop(self):
        return self.data['new_key']

obj = MyClass()
value = obj.old_prop
"""
        result = migrate_source(source.strip())
        assert "value = obj.data['new_key']" in result

    def test_method_not_replaced_as_attribute(self):
        """Test that method references in calls aren't replaced by visit_Attribute."""
        source = """
from dissolve import replace_me

class MyClass:
    @replace_me()
    def old_method(self, x):
        return self.new_method(x * 2)

obj = MyClass()
# This should become obj.new_method(10 * 2), NOT obj.new_method({x} * 2)(10)
result = obj.old_method(10)
"""
        result = migrate_source(source.strip())
        assert "result = obj.new_method(10 * 2)" in result
        # Make sure we don't have the broken pattern (excluding comments)
        code_lines = [
            line for line in result.split("\n") if not line.strip().startswith("#")
        ]
        code_text = "\n".join(code_lines)
        assert "obj.new_method({x} * 2)(10)" not in code_text
        assert "obj.new_method(x * 2)(10)" not in code_text

    def test_async_function_replacement(self):
        """Test async function replacement."""
        source = """
from dissolve import replace_me

@replace_me()
async def old_async_func(x):
    return await new_async_func(x + 1)

result = await old_async_func(10)
"""
        result = migrate_source(source.strip())
        # The await is part of the replacement expression
        # Note: This creates double await which is a limitation
        assert (
            "result = await (await new_async_func(10 + 1))" in result
            or "result = await await new_async_func(10 + 1)" in result
        )

    def test_async_method_replacement(self):
        """Test async method replacement."""
        source = """
from dissolve import replace_me

class MyClass:
    @replace_me()
    async def old_async_method(self, x):
        return await self.new_async_method(x * 2)

obj = MyClass()
result = await obj.old_async_method(10)
"""
        result = migrate_source(source.strip())
        # The await is part of the replacement expression
        # Note: This creates double await which is a limitation
        assert (
            "result = await (await obj.new_async_method(10 * 2))" in result
            or "result = await await obj.new_async_method(10 * 2)" in result
        )

    def test_property_getter_and_setter(self):
        """Test that property getter is replaced correctly."""
        source = """
from dissolve import replace_me

class MyClass:
    @property
    @replace_me()
    def old_prop(self):
        return self._value
    
    @old_prop.setter
    def old_prop(self, value):
        self._value = value

obj = MyClass()
value = obj.old_prop  # Getter should be replaced
obj.old_prop = 42     # Setter should also be replaced
"""
        result = migrate_source(source.strip())
        assert "value = obj._value" in result
        # The setter assignment should also be replaced since old_prop is deprecated
        assert "obj._value = 42" in result

    def test_mixed_positional_and_keyword_args(self):
        """Test function with mixed positional and keyword arguments."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(a, b, c, d):
    return new_func(a + b, c * d)

result = old_func(1, 2, 10, d=30)
"""
        result = migrate_source(source.strip())
        assert "result = new_func(1 + 2, 10 * 30)" in result

    def test_args_and_kwargs(self):
        """Test function with *args and **kwargs - currently not fully supported."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x, *args, **kwargs):
    return new_func(x, *args, **kwargs)

result = old_func(1, 2, 3, x=4, y=5)
"""
        result = migrate_source(source.strip())
        # This is a limitation - keyword args override positional args
        assert "result = new_func(4, *args, **kwargs)" in result

    def test_lambda_replacement(self):
        """Test replacement in lambda expressions."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# Lambda using the old function
mapper = lambda x: old_func(x * 2)
result = mapper(5)
"""
        result = migrate_source(source.strip())
        assert "mapper = lambda x: new_func(x * 2 + 1)" in result

    def test_comprehension_replacement(self):
        """Test replacement in comprehensions."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

# List comprehension
results = [old_func(i) for i in range(3)]
# Generator expression
gen = (old_func(x) for x in [1, 2, 3])
# Dict comprehension
d = {i: old_func(i) for i in range(2)}
"""
        result = migrate_source(source.strip())
        assert "[new_func(i + 1) for i in range(3)]" in result
        assert "(new_func(x + 1) for x in [1, 2, 3])" in result
        assert "{i: new_func(i + 1) for i in range(2)}" in result

    def test_decorator_replacement(self):
        """Test replacement when old function is used as decorator with call."""
        source = """
from dissolve import replace_me

@replace_me()
def old_decorator(func):
    return new_decorator(func)

@old_decorator()
def my_function():
    pass
"""
        result = migrate_source(source.strip())
        # When @old_decorator() is called with no arguments, the {func} placeholder remains
        assert "@new_decorator({func})" in result

    def test_multiple_replacements_same_line(self):
        """Test multiple replacements on the same line."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func1(x):
    return new_func1(x)

@replace_me()
def old_func2(x):
    return new_func2(x)

result = old_func1(10) + old_func2(20)
"""
        result = migrate_source(source.strip())
        assert "result = new_func1(10) + new_func2(20)" in result

    def test_replacement_with_imports(self):
        """Test that necessary imports are preserved."""
        source = """
from dissolve import replace_me
import math

@replace_me()
def old_func(x):
    return math.sqrt(x)

result = old_func(16)
"""
        result = migrate_source(source.strip())
        assert "import math" in result
        assert "result = math.sqrt(16)" in result

    def test_qualified_name_replacement(self):
        """Test replacement of qualified names."""
        source = """
from dissolve import replace_me
import mymodule

@replace_me()
def old_func(x):
    return mymodule.new_func(x)

result = old_func(10)
"""
        result = migrate_source(source.strip())
        assert "result = mymodule.new_func(10)" in result

    def test_nested_class_method_replacement(self):
        """Test replacement in nested classes."""
        source = """
from dissolve import replace_me

class Outer:
    class Inner:
        @replace_me()
        def old_method(self, x):
            return self.new_method(x + 1)

obj = Outer.Inner()
result = obj.old_method(10)
"""
        result = migrate_source(source.strip())
        assert "result = obj.new_method(10 + 1)" in result

    def test_property_chain_replacement(self):
        """Test replacement of chained property access."""
        source = """
from dissolve import replace_me

class MyClass:
    @property
    @replace_me()
    def old_prop(self):
        return self.data
    
    @property
    def data(self):
        return {'key': 'value'}

obj = MyClass()
result = obj.old_prop['key']
"""
        result = migrate_source(source.strip())
        assert "result = obj.data['key']" in result

    def test_parameter_name_collision(self):
        """Test when parameter names might collide with other identifiers."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(format, formatter):
    return new_func(format + formatter)

# Parameter names that are substrings of each other
result = old_func("test", "_value")
"""
        result = migrate_source(source.strip())
        # Check for either quote style
        assert (
            'result = new_func("test" + "_value")' in result
            or "result = new_func('test' + '_value')" in result
        )

    def test_complex_expression_replacement(self):
        """Test replacement of complex expressions."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x, y):
    return new_func((x + y) * 2, x - y)

result = old_func(10 + 5, 3 * 2)
"""
        result = migrate_source(source.strip())
        assert "result = new_func((10 + 5 + 3 * 2) * 2, 10 + 5 - 3 * 2)" in result

    def test_conditional_expression_replacement(self):
        """Test replacement in conditional expressions."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x)

result = old_func(10) if condition else old_func(20)
"""
        result = migrate_source(source.strip())
        assert "result = new_func(10) if condition else new_func(20)" in result

    def test_walrus_operator_replacement(self):
        """Test replacement with walrus operator."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x + 1)

if (result := old_func(10)) > 5:
    pass
"""
        result = migrate_source(source.strip())
        assert "if (result := new_func(10 + 1)) > 5:" in result

    def test_f_string_replacement(self):
        """Test that replacements don't affect f-string contents."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return new_func(x)

# Function call outside f-string should be replaced
result = f"Value: {old_func(10)}"
# But the string content itself should not be affected
message = f"Call old_func with value"
"""
        result = migrate_source(source.strip())
        # Check for either quote style
        assert (
            'result = f"Value: {new_func(10)}"' in result
            or "result = f'Value: {new_func(10)}'" in result
        )
        assert (
            'message = f"Call old_func with value"' in result
            or "message = f'Call old_func with value'" in result
        )

    def test_multiline_call_replacement(self):
        """Test replacement of multiline function calls."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func(a, b, c):
    return new_func(a + b, c)

result = old_func(
    10,
    20,
    30
)
"""
        result = migrate_source(source.strip())
        # The replacement should maintain the structure
        assert "new_func(10 + 20, 30)" in result

    def test_no_parameters_replacement(self):
        """Test replacement of functions with no parameters."""
        source = """
from dissolve import replace_me

@replace_me()
def old_func():
    return new_func()

result = old_func()
"""
        result = migrate_source(source.strip())
        assert "result = new_func()" in result

    def test_single_line_class_property(self):
        """Test single-line property definitions."""
        source = """
from dissolve import replace_me

class MyClass:
    @property
    @replace_me()
    def old_prop(self): return self.new_prop

obj = MyClass()
value = obj.old_prop
"""
        result = migrate_source(source.strip())
        assert "value = obj.new_prop" in result
