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
    import ast

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

        needs_migration = False
        for filepath in args.files:
            try:
                with open(filepath) as f:
                    original = f.read()
                result = migrate_file_with_imports(filepath, write=False)

                if args.check:
                    # Check mode: just report if changes are needed
                    if result != original:
                        print(f"{filepath}: needs migration")
                        needs_migration = True
                    else:
                        print(f"{filepath}: up to date")
                elif args.write:
                    # Write mode: update file if changed
                    if result != original:
                        with open(filepath, "w") as f:
                            f.write(result)
                        print(f"Modified: {filepath}")
                    else:
                        print(f"Unchanged: {filepath}")
                else:
                    # Default: print to stdout
                    print(f"# Migrated: {filepath}")
                    print(result)
                    print()
            except Exception as e:
                import sys

                print(f"Error processing {filepath}: {e}", file=sys.stderr)
                return 1

        # In check mode, exit with code 1 if any files need migration
        if args.check and needs_migration:
            return 1
    elif args.command == "remove":
        if args.check and args.write:
            parser.error("--check and --write cannot be used together")

        needs_removal = False
        for filepath in args.files:
            try:
                with open(filepath) as f:
                    original = f.read()
                result = remove_from_file(
                    filepath,
                    before_version=args.before,
                    remove_all=args.all,
                    write=False,
                )

                if args.check:
                    # Check mode: just report if changes are needed
                    # Note: AST transformations may normalize whitespace, so we need to
                    # check if there are actual semantic changes beyond formatting
                    original_tree = ast.parse(original)
                    result_tree = ast.parse(result)

                    # Simple check: compare AST dumps (this ignores formatting)
                    if ast.dump(original_tree) != ast.dump(result_tree):
                        print(f"{filepath}: has removable decorators")
                        needs_removal = True
                    else:
                        print(f"{filepath}: no removable decorators")
                elif args.write:
                    # Write mode: update file if changed
                    if result != original:
                        with open(filepath, "w") as f:
                            f.write(result)
                        print(f"Modified: {filepath}")
                    else:
                        print(f"Unchanged: {filepath}")
                else:
                    # Default: print to stdout
                    print(f"# Removed decorators from: {filepath}")
                    print(result)
                    print()
            except Exception as e:
                import sys

                print(f"Error processing {filepath}: {e}", file=sys.stderr)
                return 1

        # In check mode, exit with code 1 if any files need removal
        if args.check and needs_removal:
            return 1
    else:
        parser.print_help()
        return 1

    return 0


if __name__ == "__main__":
    import sys

    sys.exit(main(sys.argv[1:]))
