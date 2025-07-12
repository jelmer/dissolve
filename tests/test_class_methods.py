"""Comprehensive tests for class method support in dissolve.

This test file verifies that issue #11 is fully implemented by testing
all aspects of class method deprecation and replacement.
"""

from dissolve.migrate import migrate_source


class TestClassMethodsComprehensive:
    """Comprehensive tests for all class method scenarios."""

    def test_basic_classmethod_replacement(self):
        """Test basic @classmethod replacement."""
        source = """
from dissolve import replace_me

class MyClass:
    @classmethod
    @replace_me()
    def old_class_method(cls, x):
        return cls.new_class_method(x + 1)

result = MyClass.old_class_method(10)
"""
        result = migrate_source(source.strip())
        assert "result = MyClass.new_class_method(10 + 1)" in result

    def test_classmethod_with_inheritance(self):
        """Test @classmethod replacement with inheritance."""
        source = """
from dissolve import replace_me

class BaseClass:
    @classmethod
    @replace_me()
    def old_method(cls, value):
        return cls.new_method(value * 2)

class DerivedClass(BaseClass):
    pass

result = DerivedClass.old_method(5)
"""
        result = migrate_source(source.strip())
        assert "result = DerivedClass.new_method(5 * 2)" in result

    def test_classmethod_with_complex_return(self):
        """Test @classmethod with complex return expression."""
        source = """
from dissolve import replace_me

class Factory:
    @classmethod
    @replace_me()
    def old_create(cls, name, config):
        return cls.new_create(name.upper(), config.get('type', 'default'))

instance = Factory.old_create("test", {"type": "special"})
"""
        result = migrate_source(source.strip())
        assert (
            'instance = Factory.new_create("test".upper(), {"type": "special"}.get(\'type\', \'default\'))'
            in result
            or "instance = Factory.new_create('test'.upper(), {'type': 'special'}.get('type', 'default'))"
            in result
        )

    def test_classmethod_decorator_order(self):
        """Test that decorator order doesn't matter."""
        source = """
from dissolve import replace_me

class MyClass:
    @replace_me()
    @classmethod
    def old_method1(cls, x):
        return cls.new_method1(x)
    
    @classmethod
    @replace_me()
    def old_method2(cls, x):
        return cls.new_method2(x)

result1 = MyClass.old_method1(5)
result2 = MyClass.old_method2(10)
"""
        result = migrate_source(source.strip())
        assert "result1 = MyClass.new_method1(5)" in result
        assert "result2 = MyClass.new_method2(10)" in result

    def test_classmethod_with_multiple_args(self):
        """Test @classmethod with multiple arguments."""
        source = """
from dissolve import replace_me

class Calculator:
    @classmethod
    @replace_me()
    def old_compute(cls, a, b, c, operation='add'):
        return cls.new_compute(a + b + c, operation)

result = Calculator.old_compute(1, 2, 3, operation='sum')
"""
        result = migrate_source(source.strip())
        assert (
            "result = Calculator.new_compute(1 + 2 + 3, 'sum')" in result
            or 'result = Calculator.new_compute(1 + 2 + 3, "sum")' in result
        )

    def test_classmethod_with_kwargs(self):
        """Test @classmethod with keyword arguments."""
        source = """
from dissolve import replace_me

class Builder:
    @classmethod
    @replace_me()
    def old_build(cls, name, **kwargs):
        return cls.new_build(name.title(), **kwargs)

result = Builder.old_build("test", debug=True, verbose=False)
"""
        result = migrate_source(source.strip())
        assert (
            'result = Builder.new_build("test".title(), **kwargs)' in result
            or "result = Builder.new_build('test'.title(), **kwargs)" in result
        )

    def test_classmethod_chained_calls(self):
        """Test @classmethod in chained method calls."""
        source = """
from dissolve import replace_me

class ChainClass:
    @classmethod
    @replace_me()
    def old_chain(cls):
        return cls.new_chain()

result = ChainClass.old_chain().process().finish()
"""
        result = migrate_source(source.strip())
        assert "result = ChainClass.new_chain().process().finish()" in result

    def test_classmethod_nested_in_other_calls(self):
        """Test @classmethod nested within other function calls."""
        source = """
from dissolve import replace_me

class Processor:
    @classmethod
    @replace_me()
    def old_process(cls, data):
        return cls.new_process(data.strip())

result = some_function(Processor.old_process("  test  "), other_arg=True)
"""
        result = migrate_source(source.strip())
        assert (
            'result = some_function(Processor.new_process("  test  ".strip()), other_arg=True)'
            in result
            or "result = some_function(Processor.new_process('  test  '.strip()), other_arg=True)"
            in result
        )

    def test_classmethod_in_comprehensions(self):
        """Test @classmethod replacement in comprehensions."""
        source = """
from dissolve import replace_me

class Converter:
    @classmethod
    @replace_me()
    def old_convert(cls, value):
        return cls.new_convert(value * 10)

results = [Converter.old_convert(x) for x in range(3)]
gen = (Converter.old_convert(x) for x in [1, 2, 3])
"""
        result = migrate_source(source.strip())
        assert "results = [Converter.new_convert(x * 10) for x in range(3)]" in result
        assert "gen = (Converter.new_convert(x * 10) for x in [1, 2, 3])" in result

    def test_classmethod_vs_staticmethod_distinction(self):
        """Test that @classmethod and @staticmethod are handled differently."""
        source = """
from dissolve import replace_me

class Utils:
    @classmethod
    @replace_me()
    def old_class_util(cls, x):
        return cls.new_class_util(x)
    
    @staticmethod
    @replace_me()
    def old_static_util(x):
        return new_static_util(x)

result1 = Utils.old_class_util(5)
result2 = Utils.old_static_util(10)
"""
        result = migrate_source(source.strip())
        assert "result1 = Utils.new_class_util(5)" in result
        assert "result2 = new_static_util(10)" in result

    def test_classmethod_with_async(self):
        """Test async @classmethod (if supported in Python)."""
        source = """
from dissolve import replace_me

class AsyncClass:
    @classmethod
    @replace_me()
    async def old_async_class_method(cls, x):
        return await cls.new_async_class_method(x + 1)

result = await AsyncClass.old_async_class_method(10)
"""
        result = migrate_source(source.strip())
        # Should handle async class methods correctly
        assert (
            "result = await (await AsyncClass.new_async_class_method(10 + 1))" in result
            or "result = await await AsyncClass.new_async_class_method(10 + 1)"
            in result
        )

    def test_classmethod_property_combination(self):
        """Test that @classmethod works when class also has properties."""
        source = """
from dissolve import replace_me

class MixedClass:
    @classmethod
    @replace_me()
    def old_class_method(cls, x):
        return cls.new_class_method(x)
    
    @property
    @replace_me()
    def old_property(self):
        return self.new_property

obj = MixedClass()
result1 = MixedClass.old_class_method(5)
result2 = obj.old_property
"""
        result = migrate_source(source.strip())
        assert "result1 = MixedClass.new_class_method(5)" in result
        assert "result2 = obj.new_property" in result

    def test_classmethod_with_version_info(self):
        """Test @classmethod with version information in decorator."""
        source = """
from dissolve import replace_me

class VersionedClass:
    @classmethod
    @replace_me(since="2.0.0", remove_in="3.0.0")
    def old_versioned_method(cls, data):
        return cls.new_versioned_method(data.upper())

result = VersionedClass.old_versioned_method("hello")
"""
        result = migrate_source(source.strip())
        assert (
            'result = VersionedClass.new_versioned_method("hello".upper())' in result
            or "result = VersionedClass.new_versioned_method('hello'.upper())" in result
        )

    def test_multiple_classmethods_same_class(self):
        """Test multiple @classmethod replacements in the same class."""
        source = """
from dissolve import replace_me

class MultiClass:
    @classmethod
    @replace_me()
    def old_method_a(cls, x):
        return cls.new_method_a(x + 1)
    
    @classmethod
    @replace_me()
    def old_method_b(cls, y):
        return cls.new_method_b(y * 2)
    
    def regular_method(self):
        return "normal"

result_a = MultiClass.old_method_a(5)
result_b = MultiClass.old_method_b(10)
"""
        result = migrate_source(source.strip())
        assert "result_a = MultiClass.new_method_a(5 + 1)" in result
        assert "result_b = MultiClass.new_method_b(10 * 2)" in result
        # Ensure regular method is not affected
        assert "def regular_method(self):" in result

    def test_classmethod_called_on_instance(self):
        """Test @classmethod called on instance (should still work)."""
        source = """
from dissolve import replace_me

class MyClass:
    @classmethod
    @replace_me()
    def old_class_method(cls, value):
        return cls.new_class_method(value + 100)

obj = MyClass()
result = obj.old_class_method(5)  # Called on instance
"""
        result = migrate_source(source.strip())
        assert "result = obj.new_class_method(5 + 100)" in result
