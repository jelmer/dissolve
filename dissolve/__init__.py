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

from typing import Optional


__version__ = (0, 0, 2)


def replace_me(replacement_expr, since=Optional[str]):
    """Decorate to indicate an object should be replaced with an expression.

    Args:
      replacement_expr: Expression to replace with
      since: Version of containing package since when to replace
    """
    import warnings

    def function_decorator(callable):

        def decorated_function(*args, **kwargs):
            evaluated = replacement_expr.format(*args, **kwargs)
            if since:
                w = DeprecationWarning(
                    "%r has been deprecated since %s; use %r instead" % (
                        callable, since, evaluated))
            else:
                w = DeprecationWarning(
                    "%r has been deprecated; use %r instead" % (
                        callable, evaluated))
            warnings.warn(w, stacklevel=2)
            return callable(*args, **kwargs)
        return decorated_function

    return function_decorator
