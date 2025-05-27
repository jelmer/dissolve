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

from typing import Optional, Union, Tuple


def replace_me(since: Optional[Union[Tuple[int, ...], str]] = None):
    """Decorate to indicate an object should be replaced with its body expression.

    Args:
      since: Version of containing package since when to replace
    """
    import warnings
    import ast
    import inspect
    import textwrap

    def function_decorator(callable):
        def decorated_function(*args, **kwargs):
            # Get the source code of the function
            source = inspect.getsource(callable)
            # Parse to extract the function body
            tree = ast.parse(textwrap.dedent(source))
            func_def = tree.body[0]

            # Get the function body (assuming single expression/return statement)
            if func_def.body and len(func_def.body) == 1:
                stmt = func_def.body[0]
                if isinstance(stmt, ast.Return) and stmt.value:
                    # Get the expression being returned
                    replacement_expr = ast.unparse(stmt.value)

                    # Build argument mapping
                    arg_map = {}
                    func_args = func_def.args

                    # Map positional arguments
                    for i, arg in enumerate(func_args.args):
                        if i < len(args):
                            arg_map[arg.arg] = repr(args[i])

                    # Map keyword arguments
                    for key, value in kwargs.items():
                        arg_map[key] = repr(value)

                    # Replace parameter names with actual values
                    evaluated = replacement_expr
                    for param, value in arg_map.items():
                        # Simple replacement - could be enhanced
                        evaluated = evaluated.replace(param, str(value))

                    if since:
                        w = DeprecationWarning(
                            "%r has been deprecated since %s; use '%s' instead"
                            % (callable, since, evaluated)
                        )
                    else:
                        w = DeprecationWarning(
                            "%r has been deprecated; use '%s' instead"
                            % (callable, evaluated)
                        )
                    warnings.warn(w, stacklevel=2)

            return callable(*args, **kwargs)

        return decorated_function

    return function_decorator
