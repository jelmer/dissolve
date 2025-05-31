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
- `remove`: Remove @replace_me decorators from source files, optionally
  filtering by version.

Run `dissolve --help` for more information on available commands and options.
"""

import ast
import glob
import importlib.util
import os
from collections.abc import Callable
from typing import Union


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
            $ python -m dissolve remove myfile.py --all --write
    """
    import argparse

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
    migrate_parser.add_argument(
        "-w",
        "--write",
        action="store_true",
        help="Write changes back to files (default: print to stdout)",
    )
    migrate_parser.add_argument(
        "--check",
        action="store_true",
        help="Check if files need migration without modifying them (exit 1 if changes needed)",
    )
    migrate_parser.add_argument(
        "--interactive",
        action="store_true",
        help="Interactively confirm each replacement before applying",
    )

    # Remove command
    remove_parser = subparsers.add_parser(
        "remove", help="Remove @replace_me decorators from Python files"
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

    remove_parser.add_argument(
        "paths", nargs="+", help="Python files or directories to process"
    )
    remove_parser.add_argument(
        "-m",
        "--module",
        action="store_true",
        help="Treat paths as Python module paths (e.g. package.module)",
    )
    remove_parser.add_argument(
        "-w",
        "--write",
        action="store_true",
        help="Write changes back to files (default: print to stdout)",
    )
    remove_parser.add_argument(
        "--before",
        metavar="VERSION",
        help="Remove decorators with version older than this",
    )
    remove_parser.add_argument(
        "--all",
        action="store_true",
        help="Remove all @replace_me decorators regardless of version",
    )
    remove_parser.add_argument(
        "--check",
        action="store_true",
        help="Check if files have removable decorators without modifying them (exit 1 if changes needed)",
    )

    args = parser.parse_args(argv)

    if args.command == "migrate":
        if args.check and args.write:
            parser.error("--check and --write cannot be used together")
        if args.interactive and args.check:
            parser.error("--interactive and --check cannot be used together")

        def migrate_processor(filepath: str) -> tuple[str, str]:
            with open(filepath) as f:
                original = f.read()
            result = migrate_file_with_imports(
                filepath, write=False, interactive=args.interactive
            )
            return original, result

        files = _expand_paths(args.paths, as_module=args.module)
        return _process_files_common(
            files, migrate_processor, args.check, args.write, "migration"
        )
    elif args.command == "remove":
        if args.check and args.write:
            parser.error("--check and --write cannot be used together")

        def remove_processor(filepath: str) -> tuple[str, str]:
            with open(filepath) as f:
                original = f.read()
            result = remove_from_file(
                filepath,
                before_version=args.before,
                remove_all=args.all,
                write=False,
            )
            return original, result

        files = _expand_paths(args.paths, as_module=args.module)
        return _process_files_common(
            files,
            remove_processor,
            args.check,
            args.write,
            "decorator removal",
            use_ast_comparison=True,
        )
    elif args.command == "check":
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
    else:
        parser.print_help()
        return 1

    return 0


if __name__ == "__main__":
    import sys

    sys.exit(main(sys.argv[1:]))
