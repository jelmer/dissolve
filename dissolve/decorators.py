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

import functools
from typing import Any, Callable, Optional, TypeVar, Union, cast

# Type variable for preserving function signatures
F = TypeVar("F", bound=Callable[..., Any])


def replace_me(
    since: Optional[Union[tuple[int, ...], str]] = None,
    remove_in: Optional[Union[tuple[int, ...], str]] = None,
) -> Callable[[F], F]:
    """Mark a function as deprecated and suggest its replacement.

    This decorator analyzes the decorated function's return statement to
    extract a replacement expression. When the deprecated function is called,
    it emits a DeprecationWarning showing the suggested replacement with
    actual argument values substituted.

    Args:
        since: Version when the function was deprecated. Can be a string
            (e.g., "2.0.0") or a tuple of integers (e.g., (2, 0, 0)).
            If provided, the warning will mention this version.
        remove_in: Version when the decorator should be removed. Can be a string
            (e.g., "2.0.0") or a tuple of integers (e.g., (2, 0, 0)).
            If provided, 'dissolve remove' will only remove this decorator
            when the current package version is at or after this version.

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

        With removal version::

            @replace_me(since="1.0.0", remove_in="2.0.0")
            def old_function(x):
                return new_function(x)

    Note:
        - The decorator expects the function body to contain a single return
          statement with the replacement expression.
        - Parameter names in the replacement expression are automatically
          substituted with actual argument values when generating the warning.
        - The original function is still executed after emitting the warning.
    """
    import ast
    import inspect
    import textwrap
    import warnings

    from .ast_utils import create_ast_from_value, substitute_parameters

    def function_decorator(callable: F) -> F:
        # Generate deprecation notice
        deprecation_notice = ".. deprecated::"
        if since:
            deprecation_notice += f" {since}"
        deprecation_notice += "\n   This function is deprecated."
        if remove_in:
            deprecation_notice += f" It will be removed in version {remove_in}."

        # Update the docstring with deprecation information
        if callable.__doc__:
            callable.__doc__ += "\n\n" + deprecation_notice
        else:
            callable.__doc__ = deprecation_notice

        def emit_warning(callable, args, kwargs):
            # Get the source code of the function
            source = inspect.getsource(callable)
            # Parse to extract the function body
            tree = ast.parse(textwrap.dedent(source))
            func_def = tree.body[0]

            if isinstance(func_def, (ast.FunctionDef, ast.AsyncFunctionDef)):
                stmts = [
                    expr
                    for expr in func_def.body
                    if not isinstance(expr, ast.Expr)
                    or not isinstance(expr.value, ast.Constant)
                ]
                # Get the function body (assuming single expression/return statement; ignoring docstrings)
                if (
                    len(stmts) == 1
                    and isinstance(stmts[0], ast.Return)
                    and stmts[0].value
                ):
                    stmt = cast(ast.Return, stmts[0])
                    assert isinstance(stmt.value, ast.expr)
                    # Get the expression being returned
                    replacement_expr: str = ast.unparse(stmt.value)

                    # Build argument mapping
                    arg_map: dict[str, Any] = {}
                    func_args = (
                        func_def.args
                        if isinstance(func_def, (ast.FunctionDef, ast.AsyncFunctionDef))
                        else None
                    )

                    # Map positional arguments
                    if func_args:
                        for i, arg in enumerate(func_args.args):
                            if i < len(args):
                                arg_map[arg.arg] = args[i]

                    # Map keyword arguments
                    for key, value in kwargs.items():
                        arg_map[key] = value

                    # Replace parameter names with actual values using AST
                    try:
                        # Parse the replacement expression
                        expr_ast = ast.parse(replacement_expr, mode="eval").body

                        # Convert values to AST nodes
                        ast_param_map = {
                            name: create_ast_from_value(value)
                            if not isinstance(value, ast.AST)
                            else value
                            for name, value in arg_map.items()
                        }

                        # Substitute parameters
                        result_ast = substitute_parameters(expr_ast, ast_param_map)

                        # Convert back to string
                        evaluated = ast.unparse(result_ast)
                    except Exception:
                        # Fallback to original if AST manipulation fails
                        evaluated = replacement_expr

                    if since:
                        w = DeprecationWarning(
                            f"{callable!r} has been deprecated since {since}; use '{evaluated}' instead. Run 'dissolve migrate' to update your code automatically."
                        )
                    else:
                        w = DeprecationWarning(
                            f"{callable!r} has been deprecated; use '{evaluated}' instead. Run 'dissolve migrate' to update your code automatically."
                        )
                else:
                    if since:
                        w = DeprecationWarning(
                            f"{callable.__name__} has been deprecated since {since}. Run 'dissolve migrate' to update your code automatically."
                        )
                    else:
                        w = DeprecationWarning(
                            f"{callable.__name__} has been deprecated. Run 'dissolve migrate' to update your code automatically."
                        )
            else:
                if since:
                    w = DeprecationWarning(
                        f"{callable.__name__} has been deprecated since {since}. Run 'dissolve migrate' to update your code automatically."
                    )
                else:
                    w = DeprecationWarning(
                        f"{callable.__name__} has been deprecated. Run 'dissolve migrate' to update your code automatically."
                    )
            warnings.warn(w, stacklevel=3)

        # Check if the callable is a class
        if inspect.isclass(callable):
            # For wrapper classes, we'll add a deprecation warning to __init__
            original_init = callable.__init__

            def deprecated_init(self, *args, **kwargs):
                emit_warning(callable, args, kwargs)
                return original_init(self, *args, **kwargs)

            callable.__init__ = deprecated_init
            return callable  # type: ignore[return-value]

        # Check if the callable is an async function
        elif inspect.iscoroutinefunction(callable):

            @functools.wraps(callable)
            async def async_decorated_function(*args: Any, **kwargs: Any) -> Any:
                emit_warning(callable, args, kwargs)
                return await callable(*args, **kwargs)

            return async_decorated_function  # type: ignore[return-value]
        else:

            @functools.wraps(callable)
            def decorated_function(*args: Any, **kwargs: Any) -> Any:
                emit_warning(callable, args, kwargs)
                return callable(*args, **kwargs)

            return decorated_function  # type: ignore[return-value]

    return function_decorator
