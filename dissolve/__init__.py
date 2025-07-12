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

"""Dissolve - A Python library for replacing deprecated API calls.

Dissolve helps library maintainers guide users from deprecated APIs to new ones
by providing clear, actionable replacement suggestions. It consists of two main
components:

1. The `@replace_me` decorator: Marks functions as deprecated and suggests
   replacements with actual argument values substituted.

2. Migration tools: Command-line utilities to automatically update codebases
   by replacing deprecated function calls with their suggested replacements.

Basic Usage:
    Mark a deprecated function::

        from dissolve import replace_me

        @replace_me(since="2.0.0")
        def old_function(x, y):
            return new_function(x, y, mode="legacy")

    Migrate a codebase::

        $ dissolve migrate myproject/*.py --write

    Remove decorators after migration::

        $ dissolve remove myproject/*.py --before 3.0.0 --write

See the documentation for more detailed examples and advanced usage.
"""

__version__: tuple[int, int, int] = (0, 2, 1)


__all__ = ["replace_me"]

from .decorators import replace_me
