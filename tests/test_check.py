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
        assert result.success
        assert result.checked_functions == ["old_func"]
        assert result.errors == []

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
        assert "Function" in result.errors[0]
        assert "multiple statements" in result.errors[0]

    def test_no_return_statement(self):
        source = """
@replace_me()
def old_func(x, y):
    x + y
        """
        result = check_replacements(source)
        assert not result.success
        assert result.checked_functions == ["old_func"]
        assert "Function" in result.errors[0]
        assert "return statement" in result.errors[0]

    def test_empty_return(self):
        source = """
@replace_me()
def old_func(x, y):
    return
        """
        result = check_replacements(source)
        assert not result.success
        assert result.checked_functions == ["old_func"]
        assert "Function" in result.errors[0]
        assert "empty return" in result.errors[0]

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
        assert "Failed to parse source" in result.errors[0]

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

    def test_property_replacement(self):
        """Test checking @replace_me decorated properties."""
        source = """
class MyClass:
    @property
    @replace_me()
    def old_property(self):
        return self.new_property
        """
        result = check_replacements(source)
        assert result.success
        assert result.checked_functions == ["old_property"]
        assert result.errors == []

    def test_property_with_complex_body(self):
        """Test property with multiple statements should fail."""
        source = """
class MyClass:
    @property
    @replace_me()
    def old_property(self):
        x = self.compute()
        return self.new_property
        """
        result = check_replacements(source)
        assert not result.success
        assert result.checked_functions == ["old_property"]
        assert len(result.errors) == 1
