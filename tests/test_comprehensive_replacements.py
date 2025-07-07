"""Comprehensive tests for all replacement scenarios."""

import pytest
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
        # Make sure we don't have the broken pattern
        assert "obj.new_method({x} * 2)(10)" not in result
        assert "obj.new_method(x * 2)(10)" not in result