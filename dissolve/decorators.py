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

"""Decorators for marking deprecated APIs and suggesting replacements.

This module provides the core functionality of dissolve - the `@replace_me` decorator
that helps library maintainers mark deprecated functions and guide users to new APIs.

The decorator analyzes the decorated function's return statement and suggests
a replacement expression with actual argument values substituted when the
deprecated function is called.

Example:
    Basic usage with a simple replacement::

        @replace_me(since="2.0.0")
        def old_api(x, y):
            return new_api(x, y, default=True)

    When called as `old_api(5, 10)`, it will emit a deprecation warning
    suggesting to use `new_api(5, 10, default=True)` instead.
"""

from typing import Optional, Union, Tuple, Callable, TypeVar, Any

# Type variable for preserving function signatures
F = TypeVar("F", bound=Callable[..., Any])


def replace_me(since: Optional[Union[Tuple[int, ...], str]] = None) -> Callable[[F], F]:
    """Mark a function as deprecated and suggest its replacement.

    This decorator analyzes the decorated function's return statement to
    extract a replacement expression. When the deprecated function is called,
    it emits a DeprecationWarning showing the suggested replacement with
    actual argument values substituted.

    Args:
        since: Version when the function was deprecated. Can be a string
            (e.g., "2.0.0") or a tuple of integers (e.g., (2, 0, 0)).
            If provided, the warning will mention this version.

    Returns:
        A decorator function that wraps the original function with deprecation
        warning functionality.

    Raises:
        DeprecationWarning: When the decorated function is called.

    Example:
        Simple replacement with version::

            @replace_me(since="1.5.0")
            def get_value(obj, key):
                return obj.get(key)

        When called as `get_value(my_dict, "name")`, emits:
        "get_value has been deprecated since 1.5.0; use 'my_dict.get('name')' instead"

        Replacement with default arguments::

            @replace_me()
            def process_data(data, verbose=False):
                return new_process(data, log_level="INFO" if verbose else "WARN")

        Complex replacement with transformations::

            @replace_me(since=(2, 0))
            def calculate(x, y, operation="add"):
                return math_ops[operation](x, y)

    Note:
        - The decorator expects the function body to contain a single return
          statement with the replacement expression.
        - Parameter names in the replacement expression are automatically
          substituted with actual argument values when generating the warning.
        - The original function is still executed after emitting the warning.
    """
    import warnings
    import ast
    import inspect
    import textwrap
    from .ast_utils import substitute_in_expression

    def function_decorator(callable: F) -> F:
        def decorated_function(*args: Any, **kwargs: Any) -> Any:
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
                    replacement_expr: str = ast.unparse(stmt.value)

                    # Build argument mapping
                    arg_map: dict[str, Any] = {}
                    func_args = func_def.args

                    # Map positional arguments
                    for i, arg in enumerate(func_args.args):
                        if i < len(args):
                            arg_map[arg.arg] = args[i]

                    # Map keyword arguments
                    for key, value in kwargs.items():
                        arg_map[key] = value

                    # Replace parameter names with actual values using AST
                    evaluated: str = substitute_in_expression(replacement_expr, arg_map)

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

        return decorated_function  # type: ignore[return-value]

    return function_decorator
