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

from dissolve.check import check_replacements


class TestCheckReplacements:
    def test_valid_replacement_function(self):
        source = """
@replace_me()
def old_func(x, y):
    return new_func(x, y, mode="legacy")
        """
        result = check_replacements(source)
        assert result.success
        assert result.checked_functions == ["old_func"]
        assert result.errors == []

    def test_empty_body_function(self):
        source = """
@replace_me()
def old_func(x, y):
    pass
        """
        result = check_replacements(source)
        assert not result.success
        assert result.checked_functions == ["old_func"]
        assert "cannot be processed by migrate" in result.errors[0]

    def test_multiple_statements(self):
        source = """
@replace_me()
def old_func(x, y):
    print("hello")
    return new_func(x, y)
        """
        result = check_replacements(source)
        assert not result.success
        assert result.checked_functions == ["old_func"]
        assert "cannot be processed by migrate" in result.errors[0]

    def test_no_return_statement(self):
        source = """
@replace_me()
def old_func(x, y):
    x + y
        """
        result = check_replacements(source)
        assert not result.success
        assert result.checked_functions == ["old_func"]
        assert "cannot be processed by migrate" in result.errors[0]

    def test_empty_return(self):
        source = """
@replace_me()
def old_func(x, y):
    return
        """
        result = check_replacements(source)
        assert not result.success
        assert result.checked_functions == ["old_func"]
        assert "cannot be processed by migrate" in result.errors[0]

    def test_no_replace_me_functions(self):
        source = """
def normal_func(x, y):
    return x + y
        """
        result = check_replacements(source)
        assert result.success
        assert result.checked_functions == []
        assert result.errors == []

    def test_syntax_error(self):
        source = """
@replace_me()
def old_func(x, y):
    return new_func(x, y
        """
        result = check_replacements(source)
        assert not result.success
        assert "Syntax error in source code" in result.errors[0]

    def test_multiple_functions(self):
        source = """
@replace_me()
def old_func1(x):
    return new_func1(x)

@replace_me()
def old_func2(y):
    return new_func2(y, default=True)

def normal_func(z):
    return z * 2
        """
        result = check_replacements(source)
        assert result.success
        assert set(result.checked_functions) == {"old_func1", "old_func2"}
        assert result.errors == []
