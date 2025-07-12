# Copyright (C) 2024 Jelmer Vernooij <jelmer@samba.org>
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

"""Tests for replace_me() call pattern on attributes."""

import libcst as cst

from dissolve.collector import ConstructType, DeprecatedFunctionCollector


class TestReplaceMeCallAttributes:
    """Test collection of attributes using replace_me(value) pattern."""

    def test_replace_me_call_pattern(self):
        """Test attribute using replace_me(value) pattern."""
        source = """
from dissolve import replace_me

OLD_CONSTANT = replace_me(42)
"""
        tree = cst.parse_module(source.strip())
        collector = DeprecatedFunctionCollector()
        wrapper = cst.MetadataWrapper(tree)
        wrapper.visit(collector)

        assert "OLD_CONSTANT" in collector.replacements
        info = collector.replacements["OLD_CONSTANT"]
        assert info.old_name == "OLD_CONSTANT"
        assert info.replacement_expr == "42"
        assert info.construct_type == ConstructType.MODULE_ATTRIBUTE

    def test_replace_me_call_with_string(self):
        """Test replace_me() with string value."""
        source = """
OLD_URL = replace_me("https://new.example.com")
"""
        tree = cst.parse_module(source.strip())
        collector = DeprecatedFunctionCollector()
        wrapper = cst.MetadataWrapper(tree)
        wrapper.visit(collector)

        assert "OLD_URL" in collector.replacements
        info = collector.replacements["OLD_URL"]
        assert info.replacement_expr == '"https://new.example.com"'

    def test_replace_me_call_in_class(self):
        """Test replace_me() pattern in class."""
        source = """
class Settings:
    OLD_TIMEOUT = replace_me(30)
    OLD_DEBUG = replace_me(True)
"""
        tree = cst.parse_module(source.strip())
        collector = DeprecatedFunctionCollector()
        wrapper = cst.MetadataWrapper(tree)
        wrapper.visit(collector)

        assert "Settings.OLD_TIMEOUT" in collector.replacements
        assert collector.replacements["Settings.OLD_TIMEOUT"].replacement_expr == "30"
        assert (
            collector.replacements["Settings.OLD_TIMEOUT"].construct_type
            == ConstructType.CLASS_ATTRIBUTE
        )

        assert "Settings.OLD_DEBUG" in collector.replacements
        assert collector.replacements["Settings.OLD_DEBUG"].replacement_expr == "True"

    def test_replace_me_with_complex_value(self):
        """Test replace_me() with complex expressions."""
        source = """
from dissolve import replace_me

OLD_CONFIG = replace_me({"timeout": 30, "retries": 3})
OLD_CALC = replace_me(2 * 3 + 1)
"""
        tree = cst.parse_module(source.strip())
        collector = DeprecatedFunctionCollector()
        wrapper = cst.MetadataWrapper(tree)
        wrapper.visit(collector)

        assert "OLD_CONFIG" in collector.replacements
        assert (
            collector.replacements["OLD_CONFIG"].replacement_expr
            == '{"timeout": 30, "retries": 3}'
        )

        assert "OLD_CALC" in collector.replacements
        assert collector.replacements["OLD_CALC"].replacement_expr == "2 * 3 + 1"

    def test_annotated_replace_me_call(self):
        """Test replace_me() with type annotation."""
        source = """
DEFAULT_TIMEOUT: int = replace_me(30)
"""
        tree = cst.parse_module(source.strip())
        collector = DeprecatedFunctionCollector()
        wrapper = cst.MetadataWrapper(tree)
        wrapper.visit(collector)

        assert "DEFAULT_TIMEOUT" in collector.replacements
        assert collector.replacements["DEFAULT_TIMEOUT"].replacement_expr == "30"

    def test_no_args_to_replace_me(self):
        """Test that replace_me() with no args is not collected as attribute."""
        source = """
# This should not be collected as an attribute
SOMETHING = replace_me()
"""
        tree = cst.parse_module(source.strip())
        collector = DeprecatedFunctionCollector()
        wrapper = cst.MetadataWrapper(tree)
        wrapper.visit(collector)

        assert "SOMETHING" not in collector.replacements

    def test_multiple_args_to_replace_me(self):
        """Test replace_me with multiple args (only first is used)."""
        source = """
# Only the first argument should be used
OLD_VAL = replace_me(42, since="1.0")
"""
        tree = cst.parse_module(source.strip())
        collector = DeprecatedFunctionCollector()
        wrapper = cst.MetadataWrapper(tree)
        wrapper.visit(collector)

        assert "OLD_VAL" in collector.replacements
        assert collector.replacements["OLD_VAL"].replacement_expr == "42"
