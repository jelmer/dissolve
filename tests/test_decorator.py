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


from dissolve import replace_me
import pytest


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
