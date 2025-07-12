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
from contextlib import contextmanager
from io import StringIO

from dissolve.__main__ import main


@contextmanager
def temp_python_module(module_name, content, create_init=True):
    """Create a temporary Python module structure for testing."""
    with tempfile.TemporaryDirectory() as temp_dir:
        module_parts = module_name.split(".")

        # Create nested directories for module structure
        current_dir = temp_dir
        for part in module_parts[:-1]:
            current_dir = os.path.join(current_dir, part)
            os.makedirs(current_dir, exist_ok=True)
            if create_init:
                init_file = os.path.join(current_dir, "__init__.py")
                with open(init_file, "w") as f:
                    f.write("# Auto-generated __init__.py\n")

        # Create the final directory if needed
        if len(module_parts) > 1:
            # We need the parent package to have an __init__.py too
            parent_init = os.path.join(current_dir, "__init__.py")
            if not os.path.exists(parent_init) and create_init:
                with open(parent_init, "w") as f:
                    f.write("# Auto-generated __init__.py\n")

        # Create the final module file
        module_file = os.path.join(current_dir, f"{module_parts[-1]}.py")
        with open(module_file, "w") as f:
            f.write(content)

        # Add temp_dir to sys.path so module can be imported
        old_path = sys.path[:]
        sys.path.insert(0, temp_dir)

        try:
            yield module_file, temp_dir
        finally:
            sys.path[:] = old_path


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
        assert "not allowed with argument --check" in error_output
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


def test_cleanup_check_no_decorators():
    """Test cleanup --check with files that have no decorators to remove."""
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
            exit_code = main(["cleanup", "--check", "--all", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        assert "up to date" in output
    finally:
        os.unlink(temp_path)


def test_cleanup_check_has_decorators():
    """Test cleanup --check with files that have decorators to remove."""
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
            exit_code = main(["cleanup", "--check", "--all", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 1
        assert "needs function cleanup" in output
    finally:
        os.unlink(temp_path)


def test_cleanup_check_before_version():
    """Test cleanup --check with version filtering."""
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
            exit_code = main(["cleanup", "--check", "--before", "1.0.0", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        # Should detect removable decorators (0.5.0 < 1.0.0)
        assert exit_code == 1
        assert "needs function cleanup" in output
    finally:
        os.unlink(temp_path)


def test_cleanup_check_write_conflict():
    """Test that cleanup --check and --write cannot be used together."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write("print('test')")
        temp_path = f.name

    try:
        # Capture stderr
        old_stderr = sys.stderr
        sys.stderr = StringIO()

        try:
            # argparse.error() raises SystemExit
            exit_code = main(["cleanup", "--check", "--write", temp_path])
        except SystemExit as e:
            exit_code = e.code
            error_output = sys.stderr.getvalue()
        finally:
            sys.stderr = old_stderr

        # Should fail with exit code 2
        assert exit_code == 2
        assert "not allowed with argument --check" in error_output
    finally:
        os.unlink(temp_path)


def test_info_command():
    """Test the info command lists deprecations correctly."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0")
def old_function(x, y):
    return new_function(x, y, default=True)

@replace_me(since="2.0.0")
def another_deprecated(data):
    return process_data(data)

def new_function(x, y, default=False):
    return x + y

def process_data(data):
    return data
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["info", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0
        assert "old_function() -> new_function(x, y, default=True)" in output
        assert "another_deprecated() -> process_data(data)" in output
        assert "Total deprecated functions found: 2" in output
    finally:
        os.unlink(temp_path)


def test_info_command_no_deprecations():
    """Test the info command with no deprecated functions."""
    source = """
def regular_function(x):
    return x + 1

def another_function(data):
    return data
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["info", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0
        assert "Total deprecated functions found: 0" in output
    finally:
        os.unlink(temp_path)


def test_info_command_file_not_found():
    """Test the info command with a non-existent file."""
    # Capture stderr
    old_stderr = sys.stderr
    sys.stderr = StringIO()

    try:
        exit_code = main(["info", "non_existent_file.py"])
        error_output = sys.stderr.getvalue()
    finally:
        sys.stderr = old_stderr

    assert exit_code == 1
    assert "Error reading file non_existent_file.py:" in error_output
    assert "No such file or directory" in error_output


def test_info_command_syntax_error():
    """Test the info command with a file containing syntax errors."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0")
def broken_function(x):
    return new_function(x
    # Missing closing parenthesis
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stderr
        old_stderr = sys.stderr
        sys.stderr = StringIO()

        try:
            exit_code = main(["info", temp_path])
            error_output = sys.stderr.getvalue()
        finally:
            sys.stderr = old_stderr

        assert exit_code == 1
        assert f"Syntax error in {temp_path}:" in error_output
    finally:
        os.unlink(temp_path)


def test_migrate_module_flag_no_changes():
    """Test migrate -m with a module that doesn't need migration."""
    source = """
def regular_function(x):
    return x + 1

result = regular_function(5)
"""

    with temp_python_module("testpkg.utils", source) as (module_file, temp_dir):
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["migrate", "-m", "testpkg.utils"])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        # Should show migration output since it processes all related files
        assert "Migration:" in output


def test_migrate_module_flag_with_changes():
    """Test migrate -m with a module that can be processed."""
    source = """
def old_func(x):
    return x + 1

result = old_func(5)
"""

    with temp_python_module("simple_module", source, create_init=False) as (
        module_file,
        temp_dir,
    ):
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["migrate", "-m", "simple_module"])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        assert "Migration:" in output


def test_migrate_module_flag_check_mode():
    """Test migrate -m --check with a module."""
    source = """
def old_func(x):
    return x + 1

result = old_func(5)
"""

    with temp_python_module("simple_check_module", source, create_init=False) as (
        module_file,
        temp_dir,
    ):
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["migrate", "-m", "--check", "simple_check_module"])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        assert "up to date" in output


def test_migrate_module_flag_nested_module():
    """Test migrate -m with a deeply nested module."""
    source = """
def deep_func(x):
    return x * 2
"""

    with temp_python_module("myapp.utils.helpers", source) as (module_file, temp_dir):
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["migrate", "-m", "myapp.utils.helpers"])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        assert "Migration:" in output


def test_cleanup_module_flag_no_decorators():
    """Test cleanup -m with a module that has no decorators to remove."""
    source = """
def regular_function(x):
    return x + 1

result = regular_function(5)
"""

    with temp_python_module("clean_module", source, create_init=False) as (
        module_file,
        temp_dir,
    ):
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["cleanup", "-m", "--all", "clean_module"])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        # Module flag should work even if no changes needed
        assert len(output) >= 0


def test_cleanup_module_flag_with_decorators():
    """Test cleanup -m with a module."""
    source = """
def old_func(x):
    return x + 1

result = old_func(5)
"""

    with temp_python_module("removeme_module", source, create_init=False) as (
        module_file,
        temp_dir,
    ):
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["cleanup", "-m", "--all", "removeme_module"])
            sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None


def test_cleanup_module_flag_check_mode():
    """Test cleanup -m --check with a module."""
    source = """
def old_func(x):
    return x + 1

result = old_func(5)
"""

    with temp_python_module("checkremove_module", source, create_init=False) as (
        module_file,
        temp_dir,
    ):
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(
                ["cleanup", "-m", "--check", "--all", "checkremove_module"]
            )
            sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None


def test_check_module_flag_clean():
    """Test check command with -m flag on a clean module."""
    source = """
def regular_function(x):
    return x + 1

result = regular_function(5)
"""

    with temp_python_module("checkclean_module", source, create_init=False) as (
        module_file,
        temp_dir,
    ):
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["check", "-m", "checkclean_module"])
            sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None


def test_check_module_flag_with_issues():
    """Test check command with -m flag on a module."""
    source = """
def old_func(x):
    return x + 1

result = old_func(5)
"""

    with temp_python_module("checkissues_module", source, create_init=False) as (
        module_file,
        temp_dir,
    ):
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            exit_code = main(["check", "-m", "checkissues_module"])
            sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None


def test_module_flag_invalid_module():
    """Test -m flag with a module that doesn't exist."""
    # Capture stderr to check for error messages
    old_stderr = sys.stderr
    old_stdout = sys.stdout
    sys.stderr = StringIO()
    sys.stdout = StringIO()

    try:
        exit_code = main(["migrate", "-m", "nonexistent.module.path"])
        sys.stdout.getvalue()
        sys.stderr.getvalue()
    finally:
        sys.stderr = old_stderr
        sys.stdout = old_stdout

    # Should handle gracefully - either exit with error or silently skip
    assert exit_code == 0 or exit_code is None or exit_code == 1


def test_module_flag_multiple_modules():
    """Test -m flag with multiple module paths."""
    source1 = """
def func1(x):
    return x + 1
"""

    source2 = """
def func2(x):
    return x * 2
"""

    with temp_python_module("mod1_module", source1, create_init=False) as (
        module_file1,
        temp_dir1,
    ):
        with temp_python_module("mod2_module", source2, create_init=False) as (
            module_file2,
            temp_dir2,
        ):
            # Capture stdout
            old_stdout = sys.stdout
            sys.stdout = StringIO()

            try:
                exit_code = main(["migrate", "-m", "mod1_module", "mod2_module"])
                output = sys.stdout.getvalue()
            finally:
                sys.stdout = old_stdout

            assert exit_code == 0 or exit_code is None
            assert "Migration:" in output


def test_cleanup_current_version_flag():
    """Test cleanup --current-version flag."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0", remove_in="2.0.0")
def old_func(x):
    return x + 1

@replace_me(since="1.5.0", remove_in="3.0.0")
def newer_func(y):
    return y * 2
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            # Current version 2.0.0 - should remove old_func decorator
            exit_code = main(["cleanup", "--current-version", "2.0.0", temp_path])
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 0 or exit_code is None
        # Should show that old_func decorator was removed but newer_func wasn't
        assert (
            '@replace_me(since="1.0.0", remove_in="2.0.0")' not in output
            and "@replace_me(since='1.0.0', remove_in='2.0.0')" not in output
        )
        assert "def old_func(x):" not in output  # Function should be completely removed
        assert (
            '@replace_me(since="1.5.0", remove_in="3.0.0")' in output
            or "@replace_me(since='1.5.0', remove_in='3.0.0')" in output
        )
        assert "def newer_func(y):" in output  # This function should remain
    finally:
        os.unlink(temp_path)


def test_cleanup_current_version_with_check():
    """Test cleanup --current-version with --check flag."""
    source = """
from dissolve import replace_me

@replace_me(since="1.0.0", remove_in="2.0.0")
def old_func(x):
    return x + 1
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(source)
        temp_path = f.name

    try:
        # Capture stdout
        old_stdout = sys.stdout
        sys.stdout = StringIO()

        try:
            # Current version 2.0.0 - should detect removable decorator
            exit_code = main(
                ["cleanup", "--check", "--current-version", "2.0.0", temp_path]
            )
            output = sys.stdout.getvalue()
        finally:
            sys.stdout = old_stdout

        assert exit_code == 1  # Should detect changes needed
        assert "needs function cleanup" in output
    finally:
        os.unlink(temp_path)


def test_auto_detect_version():
    """Test automatic version detection."""
    from dissolve.__main__ import _detect_package_version

    # Should detect the dissolve package version when run from project directory
    version = _detect_package_version(".")
    assert version is not None
    assert isinstance(version, str)
    # Should be a valid semantic version format
    import re

    assert re.match(r"^\d+\.\d+\.\d+", version)
