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

"""Tests for problematic module variable references in @replace_me."""

from dissolve.migrate import migrate_source


def test_module_constants():
    """Test replacement with module-level constants."""
    source = """
import math
import os
from dissolve import replace_me

@replace_me()
def get_pi():
    return math.pi

@replace_me()
def get_separator():
    return os.sep

result1 = get_pi()
result2 = get_separator()
"""

    result = migrate_source(source)

    # Should replace with module constants
    assert "result1 = math.pi" in result
    assert "result2 = os.sep" in result
    # Imports should be preserved
    assert "import math" in result
    assert "import os" in result


def test_nested_module_attributes():
    """Test replacement with nested module attributes."""
    source = """
import os
import urllib.parse
from dissolve import replace_me

@replace_me()
def expand_path(path):
    return os.path.expanduser(path)

@replace_me()
def quote_url(url):
    return urllib.parse.quote(url)

result1 = expand_path("~/file.txt")
result2 = quote_url("hello world")
"""

    result = migrate_source(source)

    # Should handle nested module attributes
    assert (
        'os.path.expanduser("~/file.txt")' in result
        or "os.path.expanduser('~/file.txt')" in result
    )
    assert (
        'urllib.parse.quote("hello world")' in result
        or "urllib.parse.quote('hello world')" in result
    )


def test_class_references():
    """Test replacement with class references from modules."""
    source = """
import datetime
from collections import defaultdict
from dissolve import replace_me

@replace_me()
def create_date(year, month, day):
    return datetime.date(year, month, day)

@replace_me()
def create_dict():
    return defaultdict(list)

result1 = create_date(2024, 1, 1)
result2 = create_dict()
"""

    result = migrate_source(source)

    # Should handle class constructor calls
    assert "datetime.date(2024, 1, 1)" in result
    assert "defaultdict(list)" in result


def test_exception_classes():
    """Test replacement with exception classes."""
    source = """
import json
from dissolve import replace_me

@replace_me()
def parse_json_safely(text):
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return None

result = parse_json_safely('{"invalid": json}')
"""

    result = migrate_source(source)

    # Complex function bodies (with try/except) cannot be migrated
    # The function call should NOT be replaced
    assert "parse_json_safely(" in result
    # The function definition should still be there
    assert "def parse_json_safely(text):" in result


def test_module_level_enums_and_flags():
    """Test replacement with module-level enums and flags."""
    source = """
import re
import logging
from dissolve import replace_me

@replace_me()
def search_case_insensitive(pattern, text):
    return re.search(pattern, text, re.IGNORECASE)

@replace_me()
def get_debug_level():
    return logging.DEBUG

result1 = search_case_insensitive("hello", "HELLO WORLD")
result2 = get_debug_level()
"""

    result = migrate_source(source)

    # Should handle module constants/flags
    # Check for both quote styles
    assert (
        're.search("hello", "HELLO WORLD", re.IGNORECASE)' in result
        or "re.search('hello', 'HELLO WORLD', re.IGNORECASE)" in result
    )
    assert "result2 = logging.DEBUG" in result


def test_module_dictionaries_and_lists():
    """Test replacement with module-level dictionaries/lists."""
    source = """
import sys
import os
from dissolve import replace_me

@replace_me()
def get_env_var(key):
    return os.environ.get(key)

@replace_me()
def get_python_path():
    return sys.path[0]

result1 = get_env_var("HOME")
result2 = get_python_path()
"""

    result = migrate_source(source)

    # Should handle module attribute access
    assert 'os.environ.get("HOME")' in result or "os.environ.get('HOME')" in result
    assert "sys.path[0]" in result


def test_star_imports():
    """Test replacement with star imports."""
    source = """
from math import *
from dissolve import replace_me

@replace_me()
def calculate_circle_area(radius):
    return pi * radius ** 2

result = calculate_circle_area(5)
"""

    result = migrate_source(source)

    # Should replace with the expression
    # Note: pi might not be properly tracked with star imports
    assert "pi * 5 ** 2" in result
    assert "from math import *" in result


def test_module_aliases_shadowing():
    """Test replacement when module aliases might shadow other names."""
    source = """
import datetime as dt
from datetime import datetime
from dissolve import replace_me

@replace_me()
def get_current_date():
    return dt.date.today()

@replace_me()
def get_current_datetime():
    return datetime.now()

result1 = get_current_date()
result2 = get_current_datetime()
"""

    result = migrate_source(source)

    # Should handle both aliased module and direct import
    assert "dt.date.today()" in result
    assert "datetime.now()" in result


def test_getattr_dynamic_access():
    """Test replacement with dynamic attribute access."""
    source = """
import os
from dissolve import replace_me

@replace_me()
def get_attribute(attr_name):
    return getattr(os, attr_name)

result = get_attribute("sep")
"""

    result = migrate_source(source)

    # Should replace with getattr call
    assert 'getattr(os, "sep")' in result or "getattr(os, 'sep')" in result


def test_callable_module_attributes():
    """Test replacement with callable module attributes."""
    source = """
import operator
from itertools import chain
from dissolve import replace_me

@replace_me()
def add_numbers(a, b):
    return operator.add(a, b)

@replace_me()
def flatten(lists):
    return list(chain(*lists))

result1 = add_numbers(5, 3)
result2 = flatten([[1, 2], [3, 4]])
"""

    result = migrate_source(source)

    # Should handle operator functions and itertools
    assert "operator.add(5, 3)" in result
    assert "list(chain(*[[1, 2], [3, 4]]))" in result


def test_module_with_same_name_as_local():
    """Test when a module has the same name as a local variable."""
    source = """
import json
from dissolve import replace_me

json_data = {"key": "value"}  # Local variable named 'json'

@replace_me()
def parse_json(text):
    return json.loads(text)  # Should refer to module, not local var

result = parse_json('{"test": true}')
"""

    result = migrate_source(source)

    # Should correctly identify json as the module
    assert "json.loads('{\"test\": true}')" in result
    # Check for both quote styles in dict literal
    assert (
        'json_data = {"key": "value"}' in result
        or "json_data = {'key': 'value'}" in result
    )


def test_complex_module_paths():
    """Test replacement with complex module paths."""
    source = """
import xml.etree.ElementTree as ET
from email.mime.text import MIMEText
from dissolve import replace_me

@replace_me()
def parse_xml(xml_string):
    return ET.fromstring(xml_string)

@replace_me()
def create_email(content):
    return MIMEText(content)

result1 = parse_xml("<root>test</root>")
result2 = create_email("Hello, World!")
"""

    result = migrate_source(source)

    # Should handle complex module paths
    assert (
        'ET.fromstring("<root>test</root>")' in result
        or "ET.fromstring('<root>test</root>')" in result
    )
    assert (
        'MIMEText("Hello, World!")' in result or "MIMEText('Hello, World!')" in result
    )
