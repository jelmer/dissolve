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


def main(argv=None):
    import argparse
    from .migrate import migrate_file_with_imports

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
    else:
        parser.print_help()
        return 1

    return 0


if __name__ == "__main__":
    import sys

    sys.exit(main(sys.argv[1:]))
