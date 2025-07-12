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

"""Command-line interface for the dissolve tool.

This module provides the entry point for the dissolve CLI, which offers
commands for:

- `migrate`: Automatically replace deprecated function calls with their
  suggested replacements in Python source files.
- `cleanup`: Remove deprecated functions decorated with @replace_me from source files
  (primarily for library maintainers after deprecation period), optionally filtering by version.

Run `dissolve --help` for more information on available commands and options.
"""

import ast
import glob
import importlib.metadata
import importlib.util
import os
import sys
from collections.abc import Callable
from pathlib import Path
from typing import Optional, Union


def _check_libcst_available() -> bool:
    """Check if libcst is available and print error if not.

    Returns:
        True if libcst is available, False otherwise.
    """
    import importlib.util

    if importlib.util.find_spec("libcst") is not None:
        return True
    else:
        print("Error: libcst is required for this command.", file=sys.stderr)
        print("Install it with: pip install libcst", file=sys.stderr)
        return False


def _detect_package_version(start_path: str = ".") -> Optional[str]:
    """Detect the current package version using importlib.metadata.

    This function tries to find Python packages in the directory structure
    and get their version from the installed package metadata.

    Args:
        start_path: Starting directory to search for package information.

    Returns:
        The detected version string, or None if not found.
    """
    start_dir = Path(start_path).resolve()

    # Walk up the directory tree looking for Python packages
    current_dir = start_dir
    for _ in range(10):  # Limit search depth to avoid infinite loops
        # Look for Python packages (directories with __init__.py)
        try:
            python_packages = [
                d
                for d in current_dir.iterdir()
                if d.is_dir() and (d / "__init__.py").exists()
            ]

            for package_dir in python_packages:
                package_name = package_dir.name
                try:
                    return importlib.metadata.version(package_name)
                except importlib.metadata.PackageNotFoundError:
                    continue  # Try next package

        except OSError:
            # Directory access issue, move up
            pass

        # Move up one directory
        parent = current_dir.parent
        if parent == current_dir:  # Reached filesystem root
            break
        current_dir = parent

    return None


def _resolve_python_object_path(path: str) -> list[str]:
    """Resolve a Python object path to file paths.

    Args:
        path: Python object path like "module.submodule.function" or "package.module"

    Returns:
        List of file paths that could contain the specified object.
    """
    parts = path.split(".")
    file_paths = []

    # Try different combinations of the path parts
    for i in range(1, len(parts) + 1):
        module_path = ".".join(parts[:i])

        # Try to find the module
        try:
            spec = importlib.util.find_spec(module_path)
            if spec and spec.origin:
                file_paths.append(spec.origin)
        except (ImportError, ModuleNotFoundError, ValueError):
            continue

    return file_paths


def _discover_python_files(path: str, as_module: bool = False) -> list[str]:
    """Discover Python files in a directory or resolve a path argument.

    Args:
        path: Either a file path, directory path, or Python object path
        as_module: If True, treat path as a Python module path

    Returns:
        List of Python file paths to process.
    """
    # If explicitly treating as module path, resolve it
    if as_module:
        return _resolve_python_object_path(path)

    # If it's already a Python file, return it
    if os.path.isfile(path) and path.endswith(".py"):
        return [path]

    # If it's a directory, scan recursively for Python files
    if os.path.isdir(path):
        python_files = []
        for root, dirs, files in os.walk(path):
            # Skip hidden directories and __pycache__
            dirs[:] = [d for d in dirs if not d.startswith(".") and d != "__pycache__"]

            for file in files:
                if file.endswith(".py"):
                    python_files.append(os.path.join(root, file))
        return sorted(python_files)

    # Try glob pattern matching for file paths
    if "*" in path or "?" in path:
        return sorted(glob.glob(path))

    # Fall back to treating it as a file path (may not exist)
    return [path]


def _expand_paths(paths: list[str], as_module: bool = False) -> list[str]:
    """Expand a list of paths to include directories and Python object paths.

    Args:
        paths: List of file paths, directory paths, or Python object paths
        as_module: If True, treat paths as Python module paths

    Returns:
        Expanded list of Python file paths.
    """
    expanded = []
    for path in paths:
        expanded.extend(_discover_python_files(path, as_module=as_module))

    # Remove duplicates while preserving order
    seen = set()
    result = []
    for file_path in expanded:
        if file_path not in seen:
            seen.add(file_path)
            result.append(file_path)

    return result


def _process_files_common(
    files: list[str],
    process_func: Callable[[str], tuple[str, str]],
    check: bool,
    write: bool,
    operation_name: str,
    *,
    use_ast_comparison: bool = False,
) -> int:
    """Common logic for processing files with check/write modes.

    Args:
        files: List of file paths to process
        process_func: Function to process each file, returns (original, result)
        check: Whether to run in check mode
        write: Whether to write changes back
        operation_name: Name of operation for error messages
        use_ast_comparison: If True, compare AST structure instead of text for check mode

    Returns:
        Exit code: 0 for success, 1 for errors or changes needed in check mode
    """
    import sys

    needs_changes = False
    for filepath in files:
        try:
            original, result = process_func(filepath)

            # Determine if changes are needed
            if use_ast_comparison and check:
                # Compare AST structure for semantic changes (ignores formatting)
                try:
                    original_tree = ast.parse(original)
                    result_tree = ast.parse(result)
                    has_changes = ast.dump(original_tree) != ast.dump(result_tree)
                except SyntaxError:
                    # If parsing fails, fall back to text comparison
                    has_changes = result != original
            else:
                has_changes = result != original

            if check:
                # Check mode: just report if changes are needed
                if has_changes:
                    print(f"{filepath}: needs {operation_name}")
                    needs_changes = True
                else:
                    print(f"{filepath}: up to date")
            elif write:
                # Write mode: update file if changed
                if has_changes:
                    with open(filepath, "w") as f:
                        f.write(result)
                    print(f"Modified: {filepath}")
                else:
                    print(f"Unchanged: {filepath}")
            else:
                # Default: print to stdout
                print(f"# {operation_name.title()}: {filepath}")
                print(result)
                print()
        except Exception as e:
            print(f"Error processing {filepath}: {e}", file=sys.stderr)
            return 1

    # In check mode, exit with code 1 if any files need changes
    return 1 if check and needs_changes else 0


def main(argv: Union[list[str], None] = None) -> int:
    """Main entry point for the dissolve command-line interface.

    Args:
        argv: Command-line arguments. If None, uses sys.argv[1:].

    Returns:
        Exit code: 0 for success, 1 for errors.

    Example:
        Run from command line::

            $ python -m dissolve migrate myfile.py
            $ python -m dissolve cleanup myfile.py --all --write
    """
    import argparse
    import sys

    from .check import check_file
    from .migrate import migrate_file_with_imports
    from .remove import remove_from_file

    parser = argparse.ArgumentParser(
        description="Dissolve - Replace deprecated API usage"
    )
    subparsers = parser.add_subparsers(dest="command", help="Commands")

    # Migrate command
    migrate_parser = subparsers.add_parser(
        "migrate", help="Migrate Python files by inlining deprecated function calls"
    )
    migrate_parser.add_argument(
        "paths", nargs="+", help="Python files or directories to migrate"
    )
    migrate_parser.add_argument(
        "-m",
        "--module",
        action="store_true",
        help="Treat paths as Python module paths (e.g. package.module)",
    )
    # Create mutually exclusive group for all conflicting options
    mode_group = migrate_parser.add_mutually_exclusive_group()
    mode_group.add_argument(
        "-w",
        "--write",
        action="store_true",
        help="Write changes back to files (default: print to stdout)",
    )
    mode_group.add_argument(
        "--check",
        action="store_true",
        help="Check if files need migration without modifying them (exit 1 if changes needed)",
    )
    mode_group.add_argument(
        "--interactive",
        action="store_true",
        help="Interactively confirm each replacement before applying",
    )

    # Cleanup command
    cleanup_parser = subparsers.add_parser(
        "cleanup",
        help="Remove deprecated functions decorated with @replace_me from Python files (for library maintainers)",
    )

    # Check command
    check_parser = subparsers.add_parser(
        "check",
        help="Verify that @replace_me decorated functions can be successfully replaced",
    )
    check_parser.add_argument(
        "paths", nargs="+", help="Python files or directories to check"
    )
    check_parser.add_argument(
        "-m",
        "--module",
        action="store_true",
        help="Treat paths as Python module paths (e.g. package.module)",
    )

    # Info command
    info_parser = subparsers.add_parser(
        "info", help="List all @replace_me decorated functions and their replacements"
    )
    info_parser.add_argument(
        "paths", nargs="+", help="Python files or directories to analyze"
    )
    info_parser.add_argument(
        "-m",
        "--module",
        action="store_true",
        help="Treat paths as Python module paths (e.g. package.module)",
    )

    cleanup_parser.add_argument(
        "paths", nargs="+", help="Python files or directories to process"
    )
    cleanup_parser.add_argument(
        "-m",
        "--module",
        action="store_true",
        help="Treat paths as Python module paths (e.g. package.module)",
    )
    # Create mutually exclusive group for conflicting options
    cleanup_mode_group = cleanup_parser.add_mutually_exclusive_group()
    cleanup_mode_group.add_argument(
        "-w",
        "--write",
        action="store_true",
        help="Write changes back to files (default: print to stdout)",
    )
    cleanup_parser.add_argument(
        "--before",
        metavar="VERSION",
        help="Remove functions with decorators with version older than this",
    )
    cleanup_parser.add_argument(
        "--all",
        action="store_true",
        help="Remove all functions with @replace_me decorators regardless of version",
    )
    cleanup_mode_group.add_argument(
        "--check",
        action="store_true",
        help="Check if files have deprecated functions that can be removed without modifying them (exit 1 if changes needed)",
    )
    cleanup_parser.add_argument(
        "--current-version",
        metavar="VERSION",
        help="Current package version for remove_in comparison (auto-detected if not provided)",
    )

    args = parser.parse_args(argv)

    if args.command == "migrate":
        if not _check_libcst_available():
            return 1

        def migrate_processor(filepath: str) -> tuple[str, str]:
            with open(filepath) as f:
                original = f.read()
            result = migrate_file_with_imports(
                filepath, write=False, interactive=args.interactive
            )
            # If no changes, return the original
            return original, result if result is not None else original

        files = _expand_paths(args.paths, as_module=args.module)
        return _process_files_common(
            files, migrate_processor, args.check, args.write, "migration"
        )
    elif args.command == "cleanup":
        if not _check_libcst_available():
            return 1

        # Get current version: explicit arg > auto-detected > None
        current_version = getattr(args, "current_version", None)
        version_source = "specified"

        if current_version is None:
            # Try to auto-detect version from the first file's project directory
            if args.paths:
                first_file = (
                    _expand_paths(args.paths[:1], as_module=args.module)[0]
                    if _expand_paths(args.paths[:1], as_module=args.module)
                    else None
                )
                if first_file:
                    file_dir = os.path.dirname(os.path.abspath(first_file))
                    current_version = _detect_package_version(file_dir)
                    version_source = "auto-detected"

        # Print version information
        if current_version:
            print(f"Using {version_source} package version: {current_version}")
        else:
            print(
                "No package version detected. Decorators with 'remove_in' will not be removed."
            )
            print("Hint: Use --current-version to specify the current package version.")

        def remove_processor(filepath: str) -> tuple[str, str]:
            with open(filepath) as f:
                original = f.read()

            result = remove_from_file(
                filepath,
                before_version=args.before,
                remove_all=args.all,
                write=False,
                current_version=current_version,
            )
            # When write=False, remove_from_file returns str
            assert isinstance(result, str)
            return original, result

        files = _expand_paths(args.paths, as_module=args.module)
        return _process_files_common(
            files,
            remove_processor,
            args.check,
            args.write,
            "function cleanup",
            use_ast_comparison=True,
        )
    elif args.command == "check":
        if not _check_libcst_available():
            return 1
        errors_found = False
        files = _expand_paths(args.paths, as_module=args.module)
        for filepath in files:
            result = check_file(filepath)
            if result.success:
                if result.checked_functions:
                    print(
                        f"{filepath}: {len(result.checked_functions)} @replace_me function(s) can be replaced"
                    )
            else:
                errors_found = True
                print(f"{filepath}: ERRORS found")
                for error in result.errors:
                    print(f"  {error}")
        return 1 if errors_found else 0
    elif args.command == "info":
        if not _check_libcst_available():
            return 1

        import libcst as cst

        from .collector import DeprecatedFunctionCollector

        files = _expand_paths(args.paths, as_module=args.module)
        total_functions = 0

        for filepath in files:
            try:
                with open(filepath) as f:
                    source = f.read()

                module = cst.parse_module(source)
                collector = DeprecatedFunctionCollector()
                wrapper = cst.MetadataWrapper(module)
                wrapper.visit(collector)

                if collector.replacements:
                    print(f"\n{filepath}:")
                    for func_name, replacement in collector.replacements.items():
                        # Clean up the replacement expression for display
                        clean_expr = replacement.replacement_expr
                        # Replace placeholder patterns more carefully
                        import re

                        clean_expr = re.sub(r"\{(\w+)\}", r"\1", clean_expr)
                        print(f"  {func_name}() -> {clean_expr}")
                        total_functions += 1

            except OSError as e:
                print(f"Error reading file {filepath}: {e}", file=sys.stderr)
                return 1
            except cst.ParserSyntaxError as e:
                print(f"Syntax error in {filepath}: {e}", file=sys.stderr)
                return 1
            except UnicodeDecodeError as e:
                print(f"Encoding error in {filepath}: {e}", file=sys.stderr)
                return 1

        print(f"\nTotal deprecated functions found: {total_functions}")
        return 0
    else:
        parser.print_help()
        return 1

    return 0


if __name__ == "__main__":
    import sys

    sys.exit(main(sys.argv[1:]))
