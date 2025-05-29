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
from collections.abc import Callable


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


def main(argv: list[str] | None = None) -> int:
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
    migrate_parser.add_argument("files", nargs="+", help="Python files to migrate")
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
    remove_parser.add_argument("files", nargs="+", help="Python files to process")
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

        return _process_files_common(
            args.files, migrate_processor, args.check, args.write, "migration"
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

        return _process_files_common(
            args.files,
            remove_processor,
            args.check,
            args.write,
            "decorator removal",
            use_ast_comparison=True,
        )
    else:
        parser.print_help()
        return 1

    return 0


if __name__ == "__main__":
    import sys

    sys.exit(main(sys.argv[1:]))
