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

from typing import Optional, List


def main(argv: Optional[List[str]] = None) -> int:
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

    args = parser.parse_args(argv)

    if args.command == "migrate":
        for filepath in args.files:
            try:
                result = migrate_file_with_imports(filepath, write=args.write)
                if not args.write:
                    print(f"# Migrated: {filepath}")
                    print(result)
                    print()
                else:
                    print(f"Modified: {filepath}")
            except Exception as e:
                import sys

                print(f"Error processing {filepath}: {e}", file=sys.stderr)
                return 1
    elif args.command == "remove":
        for filepath in args.files:
            try:
                result = remove_from_file(
                    filepath,
                    before_version=args.before,
                    remove_all=args.all,
                    write=args.write,
                )
                if not args.write:
                    print(f"# Removed decorators from: {filepath}")
                    print(result)
                    print()
                else:
                    print(f"Modified: {filepath}")
            except Exception as e:
                import sys

                print(f"Error processing {filepath}: {e}", file=sys.stderr)
                return 1
    else:
        parser.print_help()
        return 1

    return 0


if __name__ == "__main__":
    import sys

    sys.exit(main(sys.argv[1:]))
