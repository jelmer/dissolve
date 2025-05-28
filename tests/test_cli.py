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
import sys
import tempfile
from io import StringIO

from dissolve.__main__ import main


def test_migrate_check_no_changes_needed():
    """Test --check with files that don't need migration."""
    source = """
def regular_function(x):
    return x + 1

result = regular_function(5)
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["migrate", "--check", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        assert "up to date" in output
    finally:
        os.unlink(temp_path)


def test_migrate_check_changes_needed():
    """Test --check with files that need migration."""
    source = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return x + 1

result = old_func(5)
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["migrate", "--check", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 1
        assert "needs migration" in output
    finally:
        os.unlink(temp_path)


def test_migrate_check_multiple_files():
    """Test --check with multiple files."""
    source_needs_migration = """
from dissolve import replace_me

@replace_me()
def old_func(x):
    return x + 1

result = old_func(5)
"""

    source_no_migration = """
def regular_func(x):
    return x + 1

result = regular_func(5)
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f1:
        f1.write(source_needs_migration)
        temp_path1 = f1.name

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f2:
        f2.write(source_no_migration)
        temp_path2 = f2.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["migrate", "--check", temp_path1, temp_path2])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert (
            exit_code == 1
        )  # Should return 1 because at least one file needs migration
        assert f"{temp_path1}: needs migration" in output
        assert f"{temp_path2}: up to date" in output
    finally:
        os.unlink(temp_path1)
        os.unlink(temp_path2)


def test_migrate_check_write_conflict():
    """Test that --check and --write cannot be used together."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write("print('test')")
        temp_path = f.name

    try:
        # Capture stderr
        old_stderr = sys.stderr
        sys.stderr = StringIO()

        try:
            # argparse.error() raises SystemExit
            exit_code = main(["migrate", "--check", "--write", temp_path])
        except SystemExit as e:
            exit_code = e.code
            error_output = sys.stderr.getvalue()
        finally:
            sys.stderr = old_stderr

        # Should fail with exit code 2
        assert exit_code == 2
        assert "--check and --write cannot be used together" in error_output
    finally:
        os.unlink(temp_path)


def test_migrate_write_no_changes():
    """Test --write with files that don't need changes."""
    source = """
def regular_function(x):
    return x + 1

result = regular_function(5)
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["migrate", "--write", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        assert "Unchanged:" in output

        # File should remain the same
        with open(temp_path) as f:
            assert f.read() == source
    finally:
        os.unlink(temp_path)


def test_remove_check_no_decorators():
    """Test remove --check with files that have no decorators to remove."""
    source = """def regular_function(x):
    return x + 1

result = regular_function(5)
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["remove", "--check", "--all", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        assert "no removable decorators" in output
    finally:
        os.unlink(temp_path)


def test_remove_check_has_decorators():
    """Test remove --check with files that have decorators to remove."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0")
def old_func(x):
    return x + 1

result = old_func(5)
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["remove", "--check", "--all", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 1
        assert "has removable decorators" in output
    finally:
        os.unlink(temp_path)


def test_remove_check_before_version():
    """Test remove --check with version filtering."""
    source = """
from dissolve import replace_me

@replace_me(since="0.5.0")
def very_old_func(x):
    return x + 1

@replace_me(since="2.0.0")
def newer_func(x):
    return x * 2
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["remove", "--check", "--before", "1.0.0", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        # Should detect removable decorators (0.5.0 < 1.0.0)
        assert exit_code == 1
        assert "has removable decorators" in output
    finally:
        os.unlink(temp_path)


def test_remove_check_write_conflict():
    """Test that remove --check and --write cannot be used together."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write("print('test')")
        temp_path = f.name

    try:
        # Capture stderr
        old_stderr = sys.stderr
        sys.stderr = StringIO()

        try:
            # argparse.error() raises SystemExit
            exit_code = main(["remove", "--check", "--write", temp_path])
        except SystemExit as e:
            exit_code = e.code
            error_output = sys.stderr.getvalue()
        finally:
            sys.stderr = old_stderr

        # Should fail with exit code 2
        assert exit_code == 2
        assert "--check and --write cannot be used together" in error_output
    finally:
        os.unlink(temp_path)
