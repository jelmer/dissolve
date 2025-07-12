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


import asyncio

import pytest

from dissolve import replace_me


def test_replace_me():
    @replace_me(since="0.1.0")
    def inc(x):
        return x + 1

    with pytest.deprecated_call():
        result = inc(3)

    assert result == 4


def test_replace_me_with_substring_params():
    """Test that parameter names that are substrings don't cause issues."""

    @replace_me(since="1.0.0")
    def process_range(n):
        return list(range(n))

    with pytest.deprecated_call() as warning_info:
        result = process_range(5)

    assert result == [0, 1, 2, 3, 4]
    # Check that the warning message has correct substitution
    warning_msg = str(warning_info.list[0].message)
    assert "range(5)" in warning_msg  # Should be range(5), not ra5ge(5)


def test_replace_me_with_complex_expression():
    """Test replacement with complex expressions."""

    @replace_me(since="2.0.0")
    def old_api(data, timeout):
        return {"data": data, "timeout": timeout * 1000, "mode": "legacy"}

    with pytest.deprecated_call() as warning_info:
        result = old_api([1, 2, 3], 30)

    assert result == {"data": [1, 2, 3], "timeout": 30000, "mode": "legacy"}
    warning_msg = str(warning_info.list[0].message)
    # Should properly show the list and number in the warning
    assert "[1, 2, 3]" in warning_msg
    assert "30" in warning_msg


def test_deprecation_message_includes_migrate_suggestion():
    """Test that deprecation warnings include 'dissolve migrate' suggestion."""

    @replace_me(since="1.5.0")
    def deprecated_func(x, y):
        return x + y

    with pytest.deprecated_call() as warning_info:
        deprecated_func(1, 2)

    warning_msg = str(warning_info.list[0].message)
    assert "dissolve migrate" in warning_msg
    assert "update your code automatically" in warning_msg


def test_deprecation_message_without_since_includes_migrate():
    """Test that deprecation warnings without 'since' still include migrate suggestion."""

    @replace_me()
    def another_deprecated_func(value):
        return value * 2

    with pytest.deprecated_call() as warning_info:
        another_deprecated_func(5)

    warning_msg = str(warning_info.list[0].message)
    assert "dissolve migrate" in warning_msg
    assert "update your code automatically" in warning_msg


def test_deprecation_message_for_non_analyzable_function():
    """Test migrate suggestion for functions that can't be analyzed."""

    @replace_me(since="3.0.0")
    def complex_func(x):
        # Multiple statements make this non-analyzable
        if x > 0:
            return x * 2
        else:
            return 0

    with pytest.deprecated_call() as warning_info:
        complex_func(3)

    warning_msg = str(warning_info.list[0].message)
    assert "dissolve migrate" in warning_msg
    assert "update your code automatically" in warning_msg


def test_async_replace_me():
    """Test @replace_me decorator on async functions."""

    async def new_async_api(x):
        """New async API."""
        return x * 2

    @replace_me(since="1.0.0")
    async def old_async_api(x):
        """Old async API that should be replaced."""
        return await new_async_api(x + 1)

    async def run_test():
        with pytest.deprecated_call() as warning_info:
            result = await old_async_api(10)

        assert result == 22  # (10 + 1) * 2
        warning_msg = str(warning_info.list[0].message)
        assert "has been deprecated since 1.0.0" in warning_msg
        assert "use 'await new_async_api(10 + 1)' instead" in warning_msg

    asyncio.run(run_test())


def test_async_replace_me_with_args():
    """Test async decorator with multiple arguments."""

    async def new_process(data, *, log_level="INFO"):
        """New async process function."""
        return f"Processing {data} with {log_level}"

    @replace_me()
    async def process_data(data, verbose=False):
        """Old async process function."""
        return await new_process(data, log_level="DEBUG" if verbose else "INFO")

    async def run_test():
        with pytest.deprecated_call() as warning_info:
            result = await process_data("test_data", verbose=True)

        assert result == "Processing test_data with DEBUG"
        warning_msg = str(warning_info.list[0].message)
        # The AST preserves the conditional expression
        assert (
            "use 'await new_process('test_data', log_level='DEBUG' if True else 'INFO')' instead"
            in warning_msg
        )

    asyncio.run(run_test())


def test_async_without_return():
    """Test async function without a clear replacement."""

    @replace_me(since="2.0.0")
    async def old_async_void():
        """Old async function without return."""
        await asyncio.sleep(0.001)

    async def run_test():
        with pytest.deprecated_call() as warning_info:
            await old_async_void()

        warning_msg = str(warning_info.list[0].message)
        assert "old_async_void has been deprecated since 2.0.0" in warning_msg
        assert "dissolve migrate" in warning_msg

    asyncio.run(run_test())
