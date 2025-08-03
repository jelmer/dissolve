#!/usr/bin/env python3
"""Comprehensive tests for the @replace_me decorator."""

import warnings

import pytest

from dissolve import replace_me


class TestReplaceMeBasicFunctionality:
    """Test basic @replace_me decorator functionality."""

    def test_simple_function_deprecation(self):
        """Test basic function deprecation with replacement."""

        @replace_me(since="1.0.0")
        def old_func(x):
            return new_func(x)

        def new_func(x):
            return x * 2

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(5)

        assert len(w) == 1
        assert issubclass(w[0].category, DeprecationWarning)
        assert "old_func" in str(w[0].message)
        assert "since 1.0.0" in str(w[0].message)
        assert "new_func(5)" in str(w[0].message)
        assert result == 10

    def test_function_without_since_version(self):
        """Test function deprecation without version."""

        @replace_me()
        def old_func(x):
            return new_func(x)

        def new_func(x):
            return x * 2

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(3)

        assert len(w) == 1
        assert "has been deprecated;" in str(w[0].message)
        # Note: "since" may appear in the function name, so check more specifically
        assert "since 1.0.0" not in str(w[0].message) and "since None" not in str(
            w[0].message
        )
        assert "new_func(3)" in str(w[0].message)
        assert result == 6

    def test_function_with_remove_in_version(self):
        """Test function with removal version."""

        @replace_me(since="1.0.0", remove_in="2.0.0")
        def old_func(x):
            return new_func(x)

        def new_func(x):
            return x * 2

        # Check docstring includes removal info
        assert "deprecated" in old_func.__doc__
        assert "removed in version 2.0.0" in old_func.__doc__

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            old_func(5)

        assert len(w) == 1
        assert "since 1.0.0" in str(w[0].message)


class TestReplaceMeParameterSubstitution:
    """Test parameter substitution in replacement expressions."""

    def test_multiple_parameters(self):
        """Test function with multiple parameters."""

        @replace_me()
        def old_func(x, y, z=10):
            return new_func(x, y, default=z)

        def new_func(x, y, default=0):
            return x + y + default

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(1, 2, 3)

        assert len(w) == 1
        assert "new_func(1, 2, default=3)" in str(w[0].message)
        assert result == 6

    def test_keyword_arguments(self):
        """Test keyword argument substitution."""

        @replace_me()
        def old_func(x, y=5):
            return new_func(x, multiplier=y)

        def new_func(x, multiplier=1):
            return x * multiplier

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(3, y=7)

        assert len(w) == 1
        assert "new_func(3, multiplier=7)" in str(w[0].message)
        assert result == 21

    def test_mixed_args_and_kwargs(self):
        """Test mixed positional and keyword arguments."""

        @replace_me()
        def old_func(a, b, c=1, d=2):
            return new_func(a + b, option=c, extra=d)

        def new_func(value, option=0, extra=0):
            return value + option + extra

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(10, 20, d=5)

        assert len(w) == 1
        # The decorator substitutes parameters but may not simplify expressions
        warning_msg = str(w[0].message)
        assert "new_func" in warning_msg
        # Check that parameters are substituted properly
        assert "10 + 20" in warning_msg or "30" in warning_msg
        assert result == 36

    def test_string_parameter_substitution(self):
        """Test substitution with string parameters."""

        @replace_me()
        def old_func(name, prefix="Mr."):
            return new_func(f"{prefix} {name}")

        def new_func(full_name):
            return f"Hello, {full_name}"

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func("Smith", prefix="Dr.")

        assert len(w) == 1
        warning_msg = str(w[0].message)
        assert "new_func" in warning_msg
        # F-string parameters are substituted but not evaluated
        assert "'Dr.'" in warning_msg and "'Smith'" in warning_msg
        assert result == "Hello, Dr. Smith"


class TestReplaceMeComplexExpressions:
    """Test complex replacement expressions."""

    def test_method_call_replacement(self):
        """Test replacement with method calls."""

        @replace_me()
        def old_func(obj, key):
            return obj.get_value(key)

        class TestObj:
            def get_value(self, key):
                return f"value_{key}"

        obj = TestObj()

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(obj, "test")

        assert len(w) == 1
        # Note: obj representation will show in the warning
        assert "get_value('test')" in str(w[0].message)
        assert result == "value_test"

    def test_nested_function_calls(self):
        """Test replacement with nested function calls."""

        @replace_me()
        def old_func(x, y):
            return outer_func(inner_func(x), y)

        def inner_func(x):
            return x * 2

        def outer_func(x, y):
            return x + y

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(5, 3)

        assert len(w) == 1
        assert "outer_func(inner_func(5), 3)" in str(w[0].message)
        assert result == 13

    def test_conditional_expression(self):
        """Test replacement with conditional expressions."""

        @replace_me()
        def old_func(x, use_double=True):
            return new_func(x * 2 if use_double else x)

        def new_func(value):
            return value + 1

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(5, False)

        assert len(w) == 1
        warning_msg = str(w[0].message)
        assert "new_func" in warning_msg
        # The conditional expression should be preserved as-is
        assert "5 * 2 if False else 5" in warning_msg
        assert result == 6


class TestReplaceMeAsyncFunctions:
    """Test @replace_me with async functions."""

    @pytest.mark.asyncio
    async def test_async_function_deprecation(self):
        """Test async function deprecation."""

        @replace_me(since="1.0.0")
        async def old_async_func(x):
            return await new_async_func(x)

        async def new_async_func(x):
            return x * 2

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = await old_async_func(4)

        assert len(w) == 1
        assert "old_async_func" in str(w[0].message)
        assert "since 1.0.0" in str(w[0].message)
        assert result == 8


class TestReplaceMeClasses:
    """Test @replace_me with classes."""

    def test_class_deprecation(self):
        """Test class deprecation."""

        @replace_me(since="1.0.0")
        class OldClass:
            def __init__(self, value):
                self.wrapped = NewClass(value)

            def get_value(self):
                return self.wrapped.get_value()

        class NewClass:
            def __init__(self, value):
                self.value = value

            def get_value(self):
                return self.value * 2

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            obj = OldClass(5)

        assert len(w) == 1
        assert "OldClass" in str(w[0].message)
        assert "since 1.0.0" in str(w[0].message)
        assert obj.get_value() == 10


class TestReplaceMeEdgeCases:
    """Test edge cases and error handling."""

    def test_function_without_return_statement(self):
        """Test function without a clear return statement."""

        @replace_me()
        def old_func(x):
            print(f"Processing {x}")
            # No return statement

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(5)

        assert len(w) == 1
        assert "old_func has been deprecated" in str(w[0].message)
        # Should not include specific replacement since no clear return
        assert "use '" not in str(w[0].message) or "Run 'dissolve migrate'" in str(
            w[0].message
        )
        assert result is None

    def test_function_with_multiple_statements(self):
        """Test function with multiple statements."""

        @replace_me()
        def old_func(x):
            y = x * 2
            return new_func(y)

        def new_func(value):
            return value + 1

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(3)

        assert len(w) == 1
        # Should fall back to generic deprecation message
        assert "old_func has been deprecated" in str(w[0].message)
        assert result == 7

    def test_function_with_docstring_and_return(self):
        """Test function with docstring followed by return."""

        @replace_me()
        def old_func(x):
            """This is a deprecated function."""
            return new_func(x)

        def new_func(x):
            return x * 2

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = old_func(4)

        assert len(w) == 1
        assert "new_func(4)" in str(w[0].message)
        assert result == 8

        # Check docstring was updated
        assert "deprecated" in old_func.__doc__

    def test_fallback_behavior_when_no_substitution(self):
        """Test behavior when parameter substitution cannot be performed."""

        # Test with complex expression that references undefined variable
        @replace_me()
        def old_func(x):
            return new_func(x + undefined_var)  # noqa: F821

        def new_func(value):
            return value

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            try:
                old_func(5)
            except NameError:
                # Expected - undefined_var is not defined
                pass

        assert len(w) == 1
        assert "old_func" in str(w[0].message)

    def test_version_as_tuple(self):
        """Test version specified as tuple."""

        @replace_me(since=(1, 2, 3))
        def old_func(x):
            return new_func(x)

        def new_func(x):
            return x * 2

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            old_func(5)

        assert len(w) == 1
        assert "since (1, 2, 3)" in str(w[0].message)


class TestReplaceMeDocstringHandling:
    """Test docstring handling by the decorator."""

    def test_adds_deprecation_to_existing_docstring(self):
        """Test that deprecation notice is added to existing docstring."""

        @replace_me(since="1.0.0", remove_in="2.0.0")
        def old_func(x):
            """Original docstring."""
            return new_func(x)

        def new_func(x):
            return x

        docstring = old_func.__doc__
        assert "Original docstring." in docstring
        assert ".. deprecated:: 1.0.0" in docstring
        assert "This function is deprecated." in docstring
        assert "removed in version 2.0.0" in docstring

    def test_creates_docstring_if_none_exists(self):
        """Test that deprecation docstring is created if none exists."""

        @replace_me(since="1.0.0")
        def old_func(x):
            return new_func(x)

        def new_func(x):
            return x

        docstring = old_func.__doc__
        assert ".. deprecated:: 1.0.0" in docstring
        assert "This function is deprecated." in docstring


def test_comprehensive_integration():
    """Integration test covering multiple features together."""

    @replace_me(since="2.1.0", remove_in="3.0.0")
    def process_data(data, format="json", validate=True, **options):
        """Process data in the old way."""
        return new_processor.process(
            data, output_format=format, validation=validate, **options
        )

    class NewProcessor:
        def process(self, data, output_format="json", validation=True, **kwargs):
            return f"processed_{data}_{output_format}_{validation}_{len(kwargs)}"

    new_processor = NewProcessor()

    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")
        result = process_data("test_data", format="xml", validate=False, extra=True)

    assert len(w) == 1
    warning_msg = str(w[0].message)
    assert "process_data" in warning_msg
    assert "since 2.1.0" in warning_msg
    assert "new_processor.process" in warning_msg
    assert "'test_data'" in warning_msg
    assert "output_format='xml'" in warning_msg
    assert "validation=False" in warning_msg

    # Check docstring
    docstring = process_data.__doc__
    assert "Process data in the old way." in docstring
    assert ".. deprecated:: 2.1.0" in docstring
    assert "removed in version 3.0.0" in docstring

    assert result == "processed_test_data_xml_False_1"


if __name__ == "__main__":
    pytest.main([__file__])
