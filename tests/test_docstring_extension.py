# Copyright (C) 2025 Jelmer Vernooij <jelmer@samba.org>
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

"""Tests for docstring extension functionality in @replace_me decorator."""

import asyncio

from dissolve import replace_me


def test_docstring_extension_with_existing_docstring():
    """Test that deprecation notice is appended to existing docstring."""

    @replace_me(since="1.0.0")
    def func_with_docstring(x):
        """This function does something useful."""
        return x + 1

    assert func_with_docstring.__doc__ is not None
    assert "This function does something useful." in func_with_docstring.__doc__
    assert ".. deprecated:: 1.0.0" in func_with_docstring.__doc__
    assert "This function is deprecated." in func_with_docstring.__doc__


def test_docstring_extension_without_existing_docstring():
    """Test that deprecation notice is created when no docstring exists."""

    @replace_me(since="2.0.0")
    def func_without_docstring(x):
        return x * 2

    assert func_without_docstring.__doc__ is not None
    assert ".. deprecated:: 2.0.0" in func_without_docstring.__doc__
    assert "This function is deprecated." in func_without_docstring.__doc__


def test_docstring_extension_with_remove_in():
    """Test that remove_in version is included in deprecation notice."""

    @replace_me(since="1.0.0", remove_in="3.0.0")
    def func_with_removal(x):
        """A function that will be removed."""
        return x - 1

    assert func_with_removal.__doc__ is not None
    assert "A function that will be removed." in func_with_removal.__doc__
    assert ".. deprecated:: 1.0.0" in func_with_removal.__doc__
    assert "This function is deprecated." in func_with_removal.__doc__
    assert "It will be removed in version 3.0.0." in func_with_removal.__doc__


def test_docstring_extension_without_since():
    """Test deprecation notice without since version."""

    @replace_me()
    def func_no_version(x):
        """A deprecated function."""
        return x

    assert func_no_version.__doc__ is not None
    assert "A deprecated function." in func_no_version.__doc__
    assert ".. deprecated::" in func_no_version.__doc__
    assert "This function is deprecated." in func_no_version.__doc__


def test_docstring_extension_with_tuple_versions():
    """Test deprecation notice with tuple version format."""

    @replace_me(since=(2, 1, 0), remove_in=(3, 0, 0))
    def func_tuple_version(x):
        """Function with tuple versions."""
        return x**2

    assert func_tuple_version.__doc__ is not None
    assert "Function with tuple versions." in func_tuple_version.__doc__
    assert ".. deprecated:: (2, 1, 0)" in func_tuple_version.__doc__
    assert "This function is deprecated." in func_tuple_version.__doc__
    assert "It will be removed in version (3, 0, 0)." in func_tuple_version.__doc__


def test_async_function_docstring_extension():
    """Test that async functions get docstring extension."""

    @replace_me(since="1.5.0")
    async def async_func(x):
        """An async function that's deprecated."""
        return await asyncio.sleep(0.001) or x

    assert async_func.__doc__ is not None
    assert "An async function that's deprecated." in async_func.__doc__
    assert ".. deprecated:: 1.5.0" in async_func.__doc__
    assert "This function is deprecated." in async_func.__doc__


def test_class_docstring_extension():
    """Test that classes get docstring extension."""

    @replace_me(since="2.0.0", remove_in="4.0.0")
    class OldClass:
        """A deprecated class."""

        def __init__(self, value):
            self.value = value

    # Classes remain classes, check actual class
    assert OldClass.__doc__ is not None
    assert "A deprecated class." in OldClass.__doc__
    assert ".. deprecated:: 2.0.0" in OldClass.__doc__
    assert "This function is deprecated." in OldClass.__doc__
    assert "It will be removed in version 4.0.0." in OldClass.__doc__


def test_multiline_docstring_preservation():
    """Test that multiline docstrings are preserved properly."""

    @replace_me(since="1.2.3")
    def func_multiline_doc(x, y):
        """This is a function with a multiline docstring.

        Args:
            x: First parameter
            y: Second parameter

        Returns:
            The sum of x and y
        """
        return x + y

    assert func_multiline_doc.__doc__ is not None
    # Check original content is preserved
    assert (
        "This is a function with a multiline docstring." in func_multiline_doc.__doc__
    )
    assert "Args:" in func_multiline_doc.__doc__
    assert "Returns:" in func_multiline_doc.__doc__
    # Check deprecation notice is added
    assert ".. deprecated:: 1.2.3" in func_multiline_doc.__doc__
    assert "This function is deprecated." in func_multiline_doc.__doc__
