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

import libcst as cst
import pytest

from dissolve import replace_me
from dissolve.collector import ConstructType, DeprecatedFunctionCollector
from dissolve.migrate import migrate_file


def test_wrapper_class_collector():
    """Test that the collector detects wrapper-based deprecated classes."""
    source_code = """
from dissolve import replace_me

class UserManager:
    def __init__(self, database_url, cache_size=100):
        self.db = database_url
        self.cache = cache_size

@replace_me(since="2.0.0")
class UserService:
    def __init__(self, database_url, cache_size=50):
        self._manager = UserManager(database_url, cache_size * 2)
    
    def get_user(self, user_id):
        return self._manager.get_user(user_id)
"""

    # Parse with CST
    tree = cst.parse_module(source_code)

    # Collect deprecated functions/classes
    collector = DeprecatedFunctionCollector()
    tree.visit(collector)

    # Should detect the UserService class
    assert "UserService" in collector.replacements
    replacement = collector.replacements["UserService"]
    assert replacement.construct_type == ConstructType.CLASS
    assert (
        "UserManager({database_url}, {cache_size} * 2)" == replacement.replacement_expr
    )


def test_wrapper_class_migration():
    """Test that wrapper-based class deprecation works with the migration tool."""
    source_code = """
from dissolve import replace_me

class UserManager:
    def __init__(self, database_url, cache_size=100):
        self.db = database_url
        self.cache = cache_size

@replace_me(since="2.0.0")
class UserService:
    def __init__(self, database_url, cache_size=50):
        self._manager = UserManager(database_url, cache_size * 2)
    
    def get_user(self, user_id):
        return self._manager.get_user(user_id)

# Test instantiations
service = UserService("postgres://localhost")
admin_service = UserService("mysql://admin", cache_size=100)
services = [UserService(url) for url in ["db1", "db2"]]
"""

    result = migrate_file("dummy.py", content=source_code)

    assert result is not None, "Migration should return modified content"

    # Should replace class instantiations with the wrapper target
    # For the first call with no explicit cache_size, it should use the default placeholder
    assert 'service = UserManager("postgres://localhost", {cache_size} * 2)' in result
    # For the explicit cache_size, it should substitute the value
    assert 'admin_service = UserManager("mysql://admin", 100 * 2)' in result
    # For the comprehension with no explicit cache_size, it should use the default placeholder
    assert (
        'services = [UserManager(url, {cache_size} * 2) for url in ["db1", "db2"]]'
        in result
    )

    # Should not replace the class definition itself
    assert '@replace_me(since="2.0.0")' in result
    assert "class UserService:" in result


def test_wrapper_class_basic_deprecation():
    """Test basic wrapper class deprecation with runtime warnings."""

    class UserManager:
        def __init__(self, database_url, cache_size=100):
            self.db = database_url
            self.cache = cache_size

        def get_user(self, user_id):
            return f"User {user_id} from {self.db}"

    @replace_me(since="2.0.0")
    class UserService:
        def __init__(self, database_url, cache_size=50):
            self._manager = UserManager(database_url, cache_size * 2)

        def get_user(self, user_id):
            return self._manager.get_user(user_id)

    with pytest.deprecated_call() as warning_info:
        service = UserService("postgres://localhost")

    # Should return the wrapper class instance (not the wrapped instance)
    assert isinstance(service, UserService)
    assert service.get_user(123) == "User 123 from postgres://localhost"

    # Check warning message contains replacement suggestion
    warning_msg = str(warning_info.list[0].message)
    assert "UserService" in warning_msg
    assert "since 2.0.0" in warning_msg
    # Note: The runtime warning won't show the replacement since the decorator
    # doesn't analyze the class structure at runtime - that's for the migration tool


def test_wrapper_class_with_kwargs():
    """Test wrapper class deprecation with keyword arguments."""

    class Database:
        def __init__(self, url, timeout=30):
            self.url = url
            self.timeout = timeout

    @replace_me(since="1.5.0")
    class LegacyDB:
        def __init__(self, url, timeout=10):
            self._db = Database(url, timeout + 20)

    with pytest.deprecated_call():
        db = LegacyDB("postgres://localhost", timeout=15)

    assert isinstance(db, LegacyDB)
    assert db._db.url == "postgres://localhost"
    assert db._db.timeout == 35  # 15 + 20
