# Copyright (C) 2024 Jelmer Vernooij <jelmer@samba.org>
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

"""Shared AST helper utilities for the dissolve package."""

import ast
import re
import uuid
from collections.abc import Callable
from typing import Optional, Union


def is_replace_me_decorator(decorator: ast.AST) -> bool:
    """Check if a decorator is @replace_me.

    Args:
        decorator: The decorator AST node to check.

    Returns:
        True if the decorator is @replace_me, False otherwise.
    """
    if isinstance(decorator, ast.Name) and decorator.id == "replace_me":
        return True
    if isinstance(decorator, ast.Call):
        if isinstance(decorator.func, ast.Name) and decorator.func.id == "replace_me":
            return True
        if (
            isinstance(decorator.func, ast.Attribute)
            and decorator.func.attr == "replace_me"
        ):
            return True
    return False


def get_single_return_value(
    func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef],
) -> Union[ast.AST, None]:
    """Extract return value from single-statement function.

    Args:
        func_def: The function definition to analyze.

    Returns:
        The return value AST node if function has single return statement, None otherwise.
    """
    if not func_def.body or len(func_def.body) != 1:
        return None

    stmt = func_def.body[0]
    if isinstance(stmt, ast.Return):
        return stmt.value
    return None


def contains_ast_pattern(
    node: ast.AST, pattern_checker: Callable[[ast.AST], bool]
) -> bool:
    """Check if an AST node contains a specific pattern.

    Args:
        node: The AST node to search.
        pattern_checker: A function that takes an AST node and returns True if it matches.

    Returns:
        True if the pattern is found, False otherwise.
    """

    class PatternVisitor(ast.NodeVisitor):
        def __init__(self) -> None:
            self.found = False

        def visit(self, node) -> None:
            if pattern_checker(node):
                self.found = True
                return
            self.generic_visit(node)

    visitor = PatternVisitor()
    visitor.visit(node)
    return visitor.found


def extract_names_from_ast(
    node: ast.AST,
    *,
    include_builtins: bool = False,
    context_filter: Union[Callable[[ast.expr_context], bool], None] = None,
) -> set[str]:
    """Extract variable names from an AST node.

    Args:
        node: The AST node to analyze.
        include_builtins: Whether to include builtin names.
        context_filter: Optional function to filter name contexts (e.g., ast.Load).

    Returns:
        Set of variable names found in the AST.
    """

    class NameCollector(ast.NodeVisitor):
        def __init__(self) -> None:
            self.names: set[str] = set()
            if not include_builtins:
                import builtins as builtins_module

                self.builtins = set(dir(builtins_module))
                self.builtins.update({"True", "False", "None"})
            else:
                self.builtins = set()

        def visit_Name(self, node: ast.Name) -> None:
            if context_filter and not context_filter(node.ctx):
                return
            if node.id not in self.builtins:
                self.names.add(node.id)
            self.generic_visit(node)

    collector = NameCollector()
    collector.visit(node)
    return collector.names


def filter_out_docstrings(body: list[ast.stmt]) -> list[ast.stmt]:
    """Filter out docstring statements from function body.

    A docstring is the first statement in a function body that is a string literal
    expression statement.

    Args:
        body: The function body statements.

    Returns:
        List of statements with docstrings removed.
    """
    if not body:
        return body

    filtered = []
    skip_first_string = True

    for stmt in body:
        # Check if this is a potential docstring (first string literal expression)
        if (
            skip_first_string
            and isinstance(stmt, ast.Expr)
            and isinstance(stmt.value, ast.Constant)
            and isinstance(stmt.value.value, str)
        ):
            # This is a docstring, skip it
            skip_first_string = False
            continue
        else:
            # Not a docstring, include it
            skip_first_string = False
            filtered.append(stmt)

    return filtered


def contains_recursive_call(node: ast.AST, func_name: str) -> bool:
    """Check if an AST node contains a recursive call to the given function.

    Args:
        node: The AST node to search.
        func_name: The function name to look for.

    Returns:
        True if a recursive call is found, False otherwise.
    """

    def is_recursive_call(n):
        return (
            isinstance(n, ast.Call)
            and isinstance(n.func, ast.Name)
            and n.func.id == func_name
        )

    return contains_ast_pattern(node, is_recursive_call)


def contains_local_imports(node: ast.AST) -> bool:
    """Check if an AST node contains import statements.

    Args:
        node: The AST node to search.

    Returns:
        True if import statements are found, False otherwise.
    """

    def is_import(n):
        return isinstance(n, (ast.Import, ast.ImportFrom))

    return contains_ast_pattern(node, is_import)


def uses_variable(expr: ast.AST, var_name: str) -> bool:
    """Check if an expression uses a specific variable.

    Args:
        expr: The expression to check.
        var_name: The variable name to look for.

    Returns:
        True if the variable is used, False otherwise.
    """

    class VariableChecker(ast.NodeVisitor):
        def __init__(self, target_var: str):
            self.target_var = target_var
            self.found = False

        def visit_Name(self, node: ast.Name) -> None:
            if isinstance(node.ctx, ast.Load) and node.id == self.target_var:
                self.found = True
            self.generic_visit(node)

    checker = VariableChecker(var_name)
    checker.visit(expr)
    return checker.found


def substitute_variable_in_expr(
    expr: ast.AST, var_name: str, var_value: ast.AST
) -> Optional[ast.AST]:
    """Substitute a variable in an expression with its value.

    Args:
        expr: The expression to modify.
        var_name: The variable name to substitute.
        var_value: The AST node to substitute with.

    Returns:
        The modified expression, or None if substitution failed.
    """

    class VariableSubstitutor(ast.NodeTransformer):
        def __init__(self, target_var: str, replacement: ast.AST):
            self.target_var = target_var
            self.replacement = replacement

        def visit_Name(self, node: ast.Name) -> ast.AST:
            if isinstance(node.ctx, ast.Load) and node.id == self.target_var:
                # Return a copy of the replacement
                return ast.copy_location(self.replacement, node)
            return node

    substitutor = VariableSubstitutor(var_name, var_value)
    try:
        return substitutor.visit(expr)
    except Exception:
        return None


def get_variables_used(expr: ast.AST) -> set[str]:
    """Get all variables used in an expression (excluding builtins).

    Args:
        expr: The expression to analyze.

    Returns:
        Set of variable names used in the expression.
    """

    class VariableCollector(ast.NodeVisitor):
        def __init__(self):
            self.variables = set()
            import builtins as builtins_module

            self.builtins = set(dir(builtins_module))
            self.builtins.update({"True", "False", "None"})

        def visit_Name(self, node: ast.Name) -> None:
            if isinstance(node.ctx, ast.Load) and node.id not in self.builtins:
                self.variables.add(node.id)
            self.generic_visit(node)

    collector = VariableCollector()
    collector.visit(expr)
    return collector.variables


def extract_module_names(tree: ast.AST) -> set[str]:
    """Extract module names from module.attribute patterns.

    Args:
        tree: The AST to analyze.

    Returns:
        Set of module names found in attribute access patterns.
    """

    class ModuleNameCollector(ast.NodeVisitor):
        def __init__(self):
            self.module_names = set()

        def visit_Attribute(self, node: ast.Attribute) -> None:
            if isinstance(node.value, ast.Name):
                self.module_names.add(node.value.id)
            self.generic_visit(node)

    collector = ModuleNameCollector()
    collector.visit(tree)
    return collector.module_names


def expr_to_replacement_string(
    expr: ast.AST, func_def: Union[ast.FunctionDef, ast.AsyncFunctionDef]
) -> str:
    """Convert an AST expression to a replacement string with parameter placeholders.

    Args:
        expr: The expression to convert.
        func_def: The function definition containing the parameters.

    Returns:
        String representation with {param} placeholders.
    """
    # Get parameter names
    param_names = [arg.arg for arg in func_def.args.args]
    if func_def.args.vararg:
        param_names.append(func_def.args.vararg.arg)

    param_names.sort(key=len, reverse=True)

    # Convert expression to string
    temp_markers = {param: str(uuid.uuid4()) for param in param_names}

    # Get the expression as string
    replacement_expr = ast.unparse(expr)

    # Replace parameters with unique markers
    for param in param_names:
        pattern = r"\b" + re.escape(param) + r"\b"
        replacement_expr = re.sub(pattern, temp_markers[param], replacement_expr)

    # Replace markers with placeholders
    for param, marker in temp_markers.items():
        replacement_expr = replacement_expr.replace(marker, f"{{{param}}}")

    return replacement_expr
