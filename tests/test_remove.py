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

import os
import tempfile

from dissolve.remove import remove_decorators, remove_from_file


def test_remove_all_decorators():
    """Test removing all @replace_me decorators."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0")
def old_func(x):
    return x + 1

@replace_me(since="2.0.0")
def another_func(y):
    return y * 2

def regular_func(z):
    return z - 1
"""

    result = remove_decorators(source, remove_all=True)

    # Check that decorators are removed but functions remain
    assert "@replace_me" not in result
    assert "def old_func(x):" in result
    assert "def another_func(y):" in result
    assert "def regular_func(z):" in result
    assert "return x + 1" in result
    assert "return y * 2" in result
    assert "return z - 1" in result


def test_remove_before_version():
    """Test removing decorators before a specific version."""
    source = """
from dissolve import replace_me

@replace_me(since="0.5.0")
def very_old_func(x):
    return x + 1

@replace_me(since="1.0.0")
def old_func(y):
    return y * 2

@replace_me(since="2.0.0")
def newer_func(z):
    return z - 1

def regular_func(w):
    return w / 2
"""

    result = remove_decorators(source, before_version="1.5.0")

    # Check that only decorators before 1.5.0 are removed
    assert (
        '@replace_me(since="0.5.0")' not in result
        and "@replace_me(since='0.5.0')" not in result
    )
    assert (
        '@replace_me(since="1.0.0")' not in result
        and "@replace_me(since='1.0.0')" not in result
    )
    assert (
        '@replace_me(since="2.0.0")' in result or "@replace_me(since='2.0.0')" in result
    )
    assert "def very_old_func(x):" in result
    assert "def old_func(y):" in result
    assert "def newer_func(z):" in result
    assert "def regular_func(w):" in result


def test_remove_no_version_decorators():
    """Test behavior with decorators that have no version."""
    source = """
from dissolve import replace_me

@replace_me()
def func_no_version(x):
    return x + 1

@replace_me(since="1.0.0")
def func_with_version(y):
    return y * 2
"""

    # When remove_all=True, all decorators should be removed
    result = remove_decorators(source, remove_all=True)
    assert "@replace_me" not in result

    # When only before_version is specified, decorators without version remain
    result = remove_decorators(source, before_version="2.0.0")
    assert "@replace_me()" in result
    assert '@replace_me(since="1.0.0")' not in result


def test_remove_from_file():
    """Test removing decorators from a file."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0")
def old_func(x):
    return x + 1
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Test without writing
        result = remove_from_file(temp_path, remove_all=True, write=False)
        assert "@replace_me" not in result

        # Original file should be unchanged
        with open(temp_path) as f:
            assert f.read() == source

        # Test with writing
        remove_from_file(temp_path, remove_all=True, write=True)

        # File should be modified
        with open(temp_path) as f:
            modified_content = f.read()
            assert "@replace_me" not in modified_content
            assert "def old_func(x):" in modified_content
    finally:
        os.unlink(temp_path)


def test_preserve_other_decorators():
    """Test that other decorators are preserved."""
    source = """
from dissolve import replace_me
import functools

@functools.lru_cache()
@replace_me(since="1.0.0")
@property
def cached_func(x):
    return x + 1
"""

    result = remove_decorators(source, remove_all=True)

    # Check that only @replace_me is removed
    assert "@functools.lru_cache()" in result
    assert "@property" in result
    assert "@replace_me" not in result
    assert "def cached_func(x):" in result


def test_async_functions():
    """Test removing decorators from async functions."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0")
async def async_func(x):
    return x + 1
"""

    result = remove_decorators(source, remove_all=True)

    assert "@replace_me" not in result
    assert "async def async_func(x):" in result
    assert "return x + 1" in result


def test_remove_in_parameter():
    """Test removing decorators based on remove_in parameter."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0", remove_in="2.0.0")
def old_func(x):
    return x + 1

@replace_me(since="1.5.0", remove_in="3.0.0")
def newer_func(y):
    return y * 2

@replace_me(since="2.0.0")
def no_remove_in(z):
    return z - 1
"""

    # Current version is 2.0.0, so old_func should be removed
    result = remove_decorators(source, current_version="2.0.0")
    assert (
        '@replace_me(since="1.0.0", remove_in="2.0.0")' not in result
        and "@replace_me(since='1.0.0', remove_in='2.0.0')" not in result
    )
    assert "def old_func(x):" in result
    assert (
        '@replace_me(since="1.5.0", remove_in="3.0.0")' in result
        or "@replace_me(since='1.5.0', remove_in='3.0.0')" in result
    )
    assert (
        '@replace_me(since="2.0.0")' in result or "@replace_me(since='2.0.0')" in result
    )

    # Current version is 3.0.0, so both old_func and newer_func should be removed
    result = remove_decorators(source, current_version="3.0.0")
    assert (
        '@replace_me(since="1.0.0", remove_in="2.0.0")' not in result
        and "@replace_me(since='1.0.0', remove_in='2.0.0')" not in result
    )
    assert (
        '@replace_me(since="1.5.0", remove_in="3.0.0")' not in result
        and "@replace_me(since='1.5.0', remove_in='3.0.0')" not in result
    )
    assert "def old_func(x):" in result
    assert "def newer_func(y):" in result
    assert (
        '@replace_me(since="2.0.0")' in result or "@replace_me(since='2.0.0')" in result
    )

    # Current version is 1.0.0, so nothing should be removed
    result = remove_decorators(source, current_version="1.0.0")
    assert (
        '@replace_me(since="1.0.0", remove_in="2.0.0")' in result
        or "@replace_me(since='1.0.0', remove_in='2.0.0')" in result
    )
    assert (
        '@replace_me(since="1.5.0", remove_in="3.0.0")' in result
        or "@replace_me(since='1.5.0', remove_in='3.0.0')" in result
    )
    assert (
        '@replace_me(since="2.0.0")' in result or "@replace_me(since='2.0.0')" in result
    )


def test_remove_in_with_all_flag():
    """Test that --all flag overrides remove_in logic."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0", remove_in="2.0.0")
def old_func(x):
    return x + 1

@replace_me(since="1.5.0", remove_in="3.0.0")
def newer_func(y):
    return y * 2
"""

    # With remove_all=True, all decorators should be removed regardless of current_version
    result = remove_decorators(source, current_version="1.0.0", remove_all=True)
    assert "@replace_me" not in result
    assert "def old_func(x):" in result
    assert "def newer_func(y):" in result


def test_remove_in_without_current_version():
    """Test that remove_in is ignored when no current_version is provided."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0", remove_in="2.0.0")
def old_func(x):
    return x + 1
"""

    # Without current_version, remove_in should be ignored
    result = remove_decorators(source)
    assert (
        '@replace_me(since="1.0.0", remove_in="2.0.0")' in result
        or "@replace_me(since='1.0.0', remove_in='2.0.0')" in result
    )

    # With before_version but no current_version, should use before_version logic
    result = remove_decorators(source, before_version="2.0.0")
    assert (
        '@replace_me(since="1.0.0", remove_in="2.0.0")' not in result
        and "@replace_me(since='1.0.0', remove_in='2.0.0')" not in result
    )
    assert "def old_func(x):" in result


def test_mixed_remove_in_and_before_version():
    """Test behavior when both remove_in and before_version logic could apply."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0", remove_in="2.0.0")
def func_with_remove_in(x):
    return x + 1

@replace_me(since="0.5.0")
def func_with_only_since(y):
    return y * 2
"""

    # Current version 2.0.0, before_version 1.5.0
    # func_with_remove_in should be removed due to remove_in condition
    # func_with_only_since should be removed due to before_version condition
    result = remove_decorators(source, current_version="2.0.0", before_version="1.5.0")
    assert (
        '@replace_me(since="1.0.0", remove_in="2.0.0")' not in result
        and "@replace_me(since='1.0.0', remove_in='2.0.0')" not in result
    )
    assert (
        '@replace_me(since="0.5.0")' not in result
        and "@replace_me(since='0.5.0')" not in result
    )
    assert "def func_with_remove_in(x):" in result
    assert "def func_with_only_since(y):" in result
