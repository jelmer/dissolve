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

"""Migration functionality for replacing deprecated function calls.

This module provides the core logic for analyzing Python source code,
identifying calls to functions decorated with @replace_me, and replacing
those calls with their suggested alternatives.

The migration process involves:
1. Parsing source code to find @replace_me decorated functions
2. Extracting replacement expressions from function bodies
3. Locating calls to deprecated functions
4. Substituting actual arguments into replacement expressions
5. Generating updated source code

Example:
    Given a source file with::

        @replace_me()
        def old_api(x, y):
            return new_api(x, y, mode="legacy")

        result = old_api(5, 10)

    The migration will transform it to::

        @replace_me()
        def old_api(x, y):
            return new_api(x, y, mode="legacy")

        result = new_api(5, 10, mode="legacy")
"""

import ast
import logging
from typing import Callable, Literal, Optional

from .ast_helpers import (
    contains_local_imports,
    contains_recursive_call,
    expr_to_replacement_string,
    extract_module_names,
    extract_names_from_ast,
    filter_out_docstrings,
    get_single_return_value,
    get_variables_used,
    is_replace_me_decorator,
    substitute_variable_in_expr,
    uses_variable,
)
from .ast_utils import substitute_parameters
from .context_analyzer import ContextAnalyzer, analyze_replacement_context
from .import_utils import ImportManager, ImportRequirement
from .types import FunctionDefNode, ReplacementExtractionError, ReplacementFailureReason


def can_chain_assignments(body: list[ast.stmt]) -> bool:
    """Check if assignments can be chained into a single expression."""
    if len(body) < 3:  # Need at least 2 assignments + return
        return False

    # Check if all but last are assignments, last is return
    if not isinstance(body[-1], ast.Return) or not body[-1].value:
        return False

    for stmt in body[:-1]:
        if not isinstance(stmt, ast.Assign):
            return False
        if len(stmt.targets) != 1 or not isinstance(stmt.targets[0], ast.Name):
            return False

    # Check if assignments form a simple chain (each uses only the previous variable)
    assignments = body[:-1]
    return_stmt = body[-1]

    # Simple case: each assignment uses only the previous variable (method chaining pattern)
    for i in range(1, len(assignments)):
        prev_assign = assignments[i - 1]
        current_assign = assignments[i]
        assert isinstance(prev_assign, ast.Assign)  # Already checked above
        assert isinstance(current_assign, ast.Assign)  # Already checked above
        assert isinstance(prev_assign.targets[0], ast.Name)  # Already checked above
        prev_var = prev_assign.targets[0].id
        current_expr = current_assign.value

        # Check if current expression uses only the previous variable
        used_vars = get_variables_used(current_expr)
        if len(used_vars) != 1 or prev_var not in used_vars:
            return False

    # Check if return uses only the last variable
    last_assign = assignments[-1]
    assert isinstance(last_assign, ast.Assign)  # Should be guaranteed by caller
    assert isinstance(
        last_assign.targets[0], ast.Name
    )  # Should be guaranteed by caller
    last_var = last_assign.targets[0].id

    if not isinstance(return_stmt, ast.Return) or return_stmt.value is None:
        return False
    return_vars = get_variables_used(return_stmt.value)

    # Allow return to use multiple variables, but last assignment variable should be one of them
    if last_var not in return_vars:
        return False

    return True


def chain_assignments_to_expression(func_def: FunctionDefNode) -> Optional[str]:
    """Chain assignments into a single expression."""
    if not func_def.body:
        return None

    assignments = func_def.body[:-1]
    return_stmt = func_def.body[-1]

    # Start with the first assignment's value
    first_assign = assignments[0]
    assert isinstance(first_assign, ast.Assign)  # Should be guaranteed by caller
    result_expr = first_assign.value

    # Chain subsequent assignments by substituting variables
    for assignment in assignments[1:]:
        assert isinstance(assignment, ast.Assign)  # Should be guaranteed by caller
        prev_assign = assignments[assignments.index(assignment) - 1]
        assert isinstance(prev_assign, ast.Assign)  # Should be guaranteed by caller
        assert isinstance(
            prev_assign.targets[0], ast.Name
        )  # Should be guaranteed by caller
        var_name = prev_assign.targets[0].id
        current_expr = assignment.value

        # Substitute the previous variable with the accumulated expression
        substituted_expr = substitute_variable_in_expr(
            current_expr, var_name, result_expr
        )
        if not substituted_expr:
            return None
        # Safe to cast since substitute_variable_in_expr preserves expression type
        result_expr = substituted_expr  # type: ignore[assignment]

    # Finally, substitute in the return expression
    last_assign_final = assignments[-1]
    assert isinstance(last_assign_final, ast.Assign)  # Should be guaranteed by caller
    assert isinstance(
        last_assign_final.targets[0], ast.Name
    )  # Should be guaranteed by caller
    last_var = last_assign_final.targets[0].id

    if not isinstance(return_stmt, ast.Return) or return_stmt.value is None:
        return None
    final_expr = substitute_variable_in_expr(return_stmt.value, last_var, result_expr)

    if final_expr:
        return expr_to_replacement_string(final_expr, func_def)

    return None


def get_function_name(node: ast.Call) -> Optional[str]:
    """Extract the function name from a Call node."""
    if isinstance(node.func, ast.Name):
        return node.func.id
    return None


def build_param_map_fallback(
    call: ast.Call, replacement: "ReplaceInfo"
) -> dict[str, ast.expr]:
    """Fallback parameter mapping without function definition."""
    import re

    param_names = re.findall(r"\{(\w+)\}", replacement.replacement_expr)
    param_map: dict[str, ast.expr] = {}

    # Simple positional mapping
    for i, (param_name, arg) in enumerate(zip(param_names, call.args)):
        param_map[param_name] = arg

    # Map keyword arguments
    for keyword in call.keywords:
        if keyword.arg and keyword.arg in param_names:
            param_map[keyword.arg] = keyword.value

    return param_map


def add_default_values(
    param_map: dict[str, ast.expr], args: ast.arguments, replacement_expr: str
) -> None:
    """Add default values for missing parameters."""
    import re

    param_names = re.findall(r"\{(\w+)\}", replacement_expr)

    # Calculate default value positions
    num_defaults = len(args.defaults)
    default_start = len(args.args) - num_defaults

    for param_name in param_names:
        if param_name not in param_map:
            # Find parameter position and check for default
            for i, arg in enumerate(args.args):
                if arg.arg == param_name and i >= default_start:
                    default_idx = i - default_start
                    param_map[param_name] = args.defaults[default_idx]
                    break


class ReplaceInfo:
    """Information about a function that should be replaced.

    Attributes:
        old_name: The name of the deprecated function.
        replacement_expr: The replacement expression template with parameter
            placeholders in the format {param_name}.
        func_def: The original function definition AST node.
        source_module: The module where this function is defined (for imports).
    """

    def __init__(
        self,
        old_name: str,
        replacement_expr: str,
        func_def: Optional[FunctionDefNode] = None,
        source_module: Optional[str] = None,
    ) -> None:
        self.old_name = old_name
        self.replacement_expr = replacement_expr
        self.func_def = func_def
        self.source_module = source_module


class ImportInfo:
    """Information about imported names.

    Attributes:
        module: The module being imported from.
        names: List of (name, alias) tuples for imported names.
    """

    def __init__(self, module: str, names: list[tuple[str, Optional[str]]]) -> None:
        self.module = module
        self.names = names  # List of (name, alias) tuples


class DeprecatedFunctionCollector(ast.NodeVisitor):
    """Collects information about functions decorated with @replace_me.

    This AST visitor traverses Python source code to find:
    - Functions decorated with @replace_me
    - Import statements for resolving external deprecated functions

    Attributes:
        replacements: Mapping from function names to their replacement info.
        imports: List of import information for module resolution.
        extraction_errors: List of functions that could not be processed and their errors.
    """

    def __init__(self) -> None:
        self.replacements: dict[str, ReplaceInfo] = {}
        self.imports: list[ImportInfo] = []
        self.extraction_errors: list[ReplacementExtractionError] = []

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        """Process function definitions to find @replace_me decorators."""
        for decorator in node.decorator_list:
            if is_replace_me_decorator(decorator):
                # For the new format, extract replacement from function body
                try:
                    replacement_expr = self._extract_replacement_from_body(node)
                    self.replacements[node.name] = ReplaceInfo(
                        node.name, replacement_expr, node
                    )
                except ReplacementExtractionError as e:
                    # Store the extraction error for later reporting
                    self.extraction_errors.append(e)
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        """Process async function definitions to find @replace_me decorators."""
        for decorator in node.decorator_list:
            if is_replace_me_decorator(decorator):
                # Try to extract replacement (this will raise ReplacementExtractionError for async functions)
                try:
                    replacement_expr = self._extract_replacement_from_body(node)
                    self.replacements[node.name] = ReplaceInfo(
                        node.name, replacement_expr, node
                    )
                except ReplacementExtractionError as e:
                    # Store the extraction error for later reporting
                    self.extraction_errors.append(e)
        self.generic_visit(node)

    def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
        """Collect import information for module resolution."""
        if node.module:
            names = [(alias.name, alias.asname) for alias in node.names]
            self.imports.append(ImportInfo(node.module, names))
        self.generic_visit(node)

    def _extract_replacement_from_body(self, func_def: FunctionDefNode) -> str:
        """Extract replacement expression from function body.

        Args:
            func_def: The function definition AST node.

        Returns:
            The replacement expression with parameter placeholders.

        Raises:
            ReplacementExtractionError: If no valid replacement can be extracted.
        """
        # Early validation checks
        self._validate_function_for_inlining(func_def)

        # Check for local imports in any statement (this should be checked early)
        for stmt in func_def.body:
            if contains_local_imports(stmt):
                raise ReplacementExtractionError(
                    func_def.name,
                    ReplacementFailureReason.LOCAL_IMPORTS,
                    "Function contains import statements which cannot be inlined",
                    func_def.lineno,
                )

        # Try single-statement function first
        return_value = get_single_return_value(func_def)
        if return_value:
            return self._process_single_return(func_def, return_value)

        # Try multi-statement function
        multi_stmt_result = self._try_simplify_multi_statement(func_def)
        if multi_stmt_result:
            return multi_stmt_result

        # Handle empty function bodies by returning None
        # First, filter out docstrings to evaluate the "real" body
        filtered_body = filter_out_docstrings(func_def.body)
        stmt_count = len(filtered_body)

        if stmt_count == 0:
            # Completely empty function body (or only docstring)
            return "None"
        elif stmt_count == 1 and isinstance(filtered_body[0], ast.Pass):
            # Function only contains 'pass' statement (plus optional docstring)
            return "None"
        else:
            # Function is too complex
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.COMPLEX_BODY,
                f"Function has {stmt_count} statements",
                func_def.lineno,
            )

    def _process_single_return(
        self, func_def: FunctionDefNode, return_value: ast.AST
    ) -> str:
        """Process a single return statement for inlining.

        Raises:
            ReplacementExtractionError: If the return statement cannot be processed.
        """
        # Check for recursive calls
        if contains_recursive_call(return_value, func_def.name):
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.RECURSIVE_CALL,
                "Function calls itself recursively which cannot be inlined",
                func_def.lineno,
            )

        # Check for local imports
        if contains_local_imports(return_value):
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.LOCAL_IMPORTS,
                "Function contains import statements which cannot be inlined",
                func_def.lineno,
            )

        return expr_to_replacement_string(return_value, func_def)

    @classmethod
    def _validate_function_for_inlining(cls, func_def: FunctionDefNode) -> None:
        """Check if function can be inlined at all.

        Raises:
            ReplacementExtractionError: If function cannot be inlined.
        """
        # Check if function has **kwargs (still too complex to handle)
        if func_def.args.kwarg:
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.ARGS_KWARGS,
                "Functions with **kwargs are too complex to inline automatically",
                func_def.lineno,
            )

        # Check if it's an async function
        if isinstance(func_def, ast.AsyncFunctionDef):
            raise ReplacementExtractionError(
                func_def.name,
                ReplacementFailureReason.ASYNC_FUNCTION,
                "Async function calls require await expressions which cannot be inlined",
                func_def.lineno,
            )

    def _get_non_parameter_names(
        self, func_def: FunctionDefNode, replacement_expr: str
    ) -> set[str]:
        """Get names in replacement expression that are not parameters."""
        # Get parameter names
        param_names = {arg.arg for arg in func_def.args.args}
        if func_def.args.vararg:
            param_names.add(func_def.args.vararg.arg)

        # Replace placeholders with dummy parameter names for parsing
        temp_expr = replacement_expr
        for param in param_names:
            temp_expr = temp_expr.replace(f"{{{param}}}", param)

        # Parse replacement expression
        try:
            tree = ast.parse(temp_expr, mode="eval")
        except SyntaxError:
            return set()

        # Get all names used as variables (not modules)
        all_names = extract_names_from_ast(
            tree, context_filter=lambda ctx: isinstance(ctx, ast.Load)
        )

        # Get module names separately
        module_names = extract_module_names(tree)

        # Return names that are not parameters and not modules
        return (all_names - param_names) - module_names

    def _try_simplify_multi_statement(self, func_def: FunctionDefNode) -> Optional[str]:
        """Try to simplify multi-statement functions into single expressions.

        Handles patterns like:
        - assignment + return (e.g., x = a * 2; return x + 1)
        - multiple assignments + return (chaining or substitution)
        - import + return (by hoisting imports)
        """
        if not func_def.body:
            return None

        # Check for recursive calls first
        for stmt in func_def.body:
            if contains_recursive_call(stmt, func_def.name):
                return None

        # Check for async function
        if isinstance(func_def, ast.AsyncFunctionDef):
            return None

        # Pattern 1: Single assignment + return
        if len(func_def.body) == 2:
            first_stmt = func_def.body[0]
            second_stmt = func_def.body[1]

            if (
                isinstance(first_stmt, ast.Assign)
                and isinstance(second_stmt, ast.Return)
                and second_stmt.value
                and len(first_stmt.targets) == 1
                and isinstance(first_stmt.targets[0], ast.Name)
            ):
                var_name = first_stmt.targets[0].id
                var_value = first_stmt.value
                return_expr = second_stmt.value

                # Check if the assigned variable is used in the return
                if uses_variable(return_expr, var_name):
                    # Substitute the variable with its value
                    simplified = substitute_variable_in_expr(
                        return_expr, var_name, var_value
                    )
                    if simplified:
                        return expr_to_replacement_string(simplified, func_def)

        # Pattern 2: Import + return (hoist the import)
        if len(func_def.body) == 2:
            first_stmt = func_def.body[0]
            second_stmt = func_def.body[1]

            if (
                (
                    isinstance(first_stmt, ast.Import)
                    or isinstance(first_stmt, ast.ImportFrom)
                )
                and isinstance(second_stmt, ast.Return)
                and second_stmt.value
            ):
                # For now, we'll handle this by warning that imports need to be hoisted manually
                # In a full implementation, we would modify the module's imports
                return None

        # Pattern 3: Multiple simple assignments that can be chained
        if can_chain_assignments(func_def.body):
            return chain_assignments_to_expression(func_def)

        return None


class FunctionCallReplacer(ast.NodeTransformer):
    """Replaces function calls with their replacement expressions.

    This AST transformer visits function calls and replaces calls to
    deprecated functions with their suggested replacements, substituting
    actual argument values.

    Attributes:
        replacements: Mapping from function names to their replacement info.
    """

    def __init__(self, replacements: dict[str, ReplaceInfo]) -> None:
        self.replacements = replacements
        self.new_functions_used: set[str] = set()

    def visit_Call(self, node: ast.Call) -> ast.AST:
        """Visit Call nodes and replace deprecated function calls."""
        self.generic_visit(node)

        func_name = get_function_name(node)
        if func_name and func_name in self.replacements:
            replacement = self.replacements[func_name]
            return self._create_replacement_node(node, replacement)
        return node

    def _create_replacement_node(
        self, original_call: ast.Call, replacement: ReplaceInfo
    ) -> ast.AST:
        """Create an AST node for the replacement expression.

        Args:
            original_call: The original function call to replace.
            replacement: Information about the replacement expression.

        Returns:
            AST node representing the replacement expression with arguments
            substituted.
        """
        # Build a mapping of parameter names to their AST values
        param_map = self._build_param_map(original_call, replacement)

        # Parse the replacement expression with placeholders
        # First, we need to convert {param} placeholders to valid Python identifiers
        temp_expr = replacement.replacement_expr
        for param in param_map.keys():
            temp_expr = temp_expr.replace(f"{{{param}}}", param)

        try:
            # Parse the expression
            replacement_ast = ast.parse(temp_expr, mode="eval").body

            # Substitute parameters using AST transformation
            result = substitute_parameters(replacement_ast, param_map)

            # Track new functions used in the replacement
            self._track_new_functions(result)

            # Copy location information from original call
            ast.copy_location(result, original_call)
            return result
        except SyntaxError:
            # If parsing fails, return the original call
            return original_call

    def _build_param_map(
        self, call: ast.Call, replacement: ReplaceInfo
    ) -> dict[str, ast.expr]:
        """Build a mapping of parameter names to their AST values.

        Args:
            call: The function call with arguments.
            replacement: Information about the replacement expression.

        Returns:
            Dictionary mapping parameter names to their AST representations.
        """
        if replacement.func_def:
            return self._build_param_map_with_definition(call, replacement)
        else:
            return build_param_map_fallback(call, replacement)

    def _build_param_map_with_definition(
        self, call: ast.Call, replacement: ReplaceInfo
    ) -> dict[str, ast.expr]:
        """Build parameter map using function definition for accurate mapping."""
        assert replacement.func_def is not None  # Should be guaranteed by caller
        param_map: dict[str, ast.expr] = {}
        args = replacement.func_def.args

        # Map positional arguments
        for i, arg in enumerate(call.args):
            if i < len(args.args):
                param_name = args.args[i].arg
                param_map[param_name] = arg

        # Handle *args if present
        if args.vararg:
            vararg_name = args.vararg.arg
            remaining_args = call.args[len(args.args) :]
            param_map[vararg_name] = ast.Tuple(elts=remaining_args, ctx=ast.Load())

        # Map keyword arguments
        for keyword in call.keywords:
            if keyword.arg:
                param_map[keyword.arg] = keyword.value

        # Fill in missing parameters with defaults
        add_default_values(param_map, args, replacement.replacement_expr)

        return param_map

    def _track_new_functions(self, node: ast.AST) -> None:
        """Track function names used in replacement expressions."""

        class FunctionNameCollector(ast.NodeVisitor):
            def __init__(self, tracker: set[str]):
                self.tracker = tracker

            def visit_Call(self, node: ast.Call) -> None:
                if isinstance(node.func, ast.Name):
                    self.tracker.add(node.func.id)
                elif isinstance(node.func, ast.Attribute):
                    # For module.function calls, track the full name
                    if isinstance(node.func.value, ast.Name):
                        self.tracker.add(f"{node.func.value.id}.{node.func.attr}")
                self.generic_visit(node)

        collector = FunctionNameCollector(self.new_functions_used)
        collector.visit(node)


class InteractiveFunctionCallReplacer(FunctionCallReplacer):
    """Interactive version of FunctionCallReplacer that prompts for user confirmation.

    This class extends FunctionCallReplacer to ask for user confirmation
    before each replacement. It supports options to replace all or quit.

    Attributes:
        replacements: Mapping from function names to their replacement info.
        replace_all: Whether to automatically replace all occurrences.
        prompt_func: Function to prompt user for confirmation.
    """

    def __init__(
        self,
        replacements: dict[str, ReplaceInfo],
        prompt_func: Optional[Callable[[str, str], Literal["y", "n", "a", "q"]]] = None,
    ) -> None:
        super().__init__(replacements)
        self.replace_all = False
        self.quit = False
        self.prompt_func = prompt_func or self._default_prompt

    def _default_prompt(
        self, old_call: str, new_call: str
    ) -> Literal["y", "n", "a", "q"]:
        """Default interactive prompt for replacement confirmation."""
        print(f"\nFound deprecated call: {old_call}")
        print(f"Replace with: {new_call}?")

        while True:
            response = input("[Y]es / [N]o / [A]ll / [Q]uit: ").lower().strip()
            if response in ["y", "yes"]:
                return "y"
            elif response in ["n", "no"]:
                return "n"
            elif response in ["a", "all"]:
                return "a"
            elif response in ["q", "quit"]:
                return "q"
            else:
                print("Invalid input. Please enter Y, N, A, or Q.")

    def visit_Call(self, node: ast.Call) -> ast.AST:
        """Visit Call nodes and interactively replace deprecated function calls."""
        if self.quit:
            return node

        self.generic_visit(node)

        func_name = get_function_name(node)
        if func_name and func_name in self.replacements:
            replacement = self.replacements[func_name]

            # Get string representations of old and new calls
            old_call_str = ast.unparse(node)
            replacement_node = self._create_replacement_node(node, replacement)
            new_call_str = ast.unparse(replacement_node)

            # Check if we should replace
            if self.replace_all:
                return replacement_node

            # Prompt user
            response = self.prompt_func(old_call_str, new_call_str)

            if response == "y":
                return replacement_node
            elif response == "a":
                self.replace_all = True
                return replacement_node
            elif response == "q":
                self.quit = True
                return node
            else:  # response == "n"
                return node

        return node


def migrate_source(
    source: str,
    module_resolver: Optional[Callable[[str, Optional[str]], Optional[str]]] = None,
    interactive: bool = False,
    prompt_func: Optional[Callable[[str, str], Literal["y", "n", "a", "q"]]] = None,
) -> str:
    """Migrate Python source code by inlining replace_me decorated functions.

    This function analyzes the source code for calls to functions decorated
    with @replace_me and replaces those calls with their suggested replacements.
    It can also resolve imports to find deprecated functions in other modules.

    Args:
        source: Python source code to migrate.
        module_resolver: Optional callable that takes (module_name, file_dir)
            and returns the module's source code as a string, or None if the
            module cannot be resolved.
        interactive: Whether to prompt for confirmation before each replacement.
        prompt_func: Optional custom prompt function for interactive mode.

    Returns:
        The migrated source code with deprecated function calls replaced.

    Example:
        Basic migration::

            source = '''
            @replace_me()
            def old_func(x):
                return new_func(x * 2)

            result = old_func(5)
            '''

            migrated = migrate_source(source)
            # result = new_func(5 * 2)

        Interactive migration::

            migrated = migrate_source(source, interactive=True)
            # Will prompt: Found deprecated call: old_func(5)
            # Replace with: new_func(5 * 2)?
            # [Y]es / [N]o / [A]ll / [Q]uit:
    """
    # Parse the source code
    tree = ast.parse(source)

    # First pass: analyze context (imports, local definitions)
    context = ContextAnalyzer()
    context.visit(tree)

    # Second pass: collect imports and local deprecations
    collector = DeprecatedFunctionCollector()
    collector.visit(tree)

    # If module_resolver is provided, analyze imported modules
    if module_resolver:
        for import_info in collector.imports:
            try:
                module_source = module_resolver(import_info.module, None)
                if module_source:
                    module_tree = ast.parse(module_source)

                    # Collect deprecated functions from the module
                    module_collector = DeprecatedFunctionCollector()
                    module_collector.visit(module_tree)

                    # Add imported deprecated functions to our replacements
                    for name, alias in import_info.names:
                        if name in module_collector.replacements:
                            replacement_info = module_collector.replacements[name]
                            # Create a new ReplaceInfo with the source module information
                            key = alias if alias else name
                            collector.replacements[key] = ReplaceInfo(
                                old_name=key,
                                replacement_expr=replacement_info.replacement_expr,
                                func_def=replacement_info.func_def,
                                source_module=import_info.module,
                            )
            except BaseException as e:
                logging.warning(
                    'Failed to resolve module "%s", ignoring: %s', import_info.module, e
                )

    if not collector.replacements:
        return source

    # Third pass: replace function calls
    if interactive:
        replacer: FunctionCallReplacer = InteractiveFunctionCallReplacer(
            collector.replacements, prompt_func
        )
    else:
        replacer = FunctionCallReplacer(collector.replacements)
    new_tree = replacer.visit(tree)

    # Fourth pass: intelligent import management
    if replacer.new_functions_used or collector.replacements:
        import_manager = ImportManager(new_tree)

        # Analyze replacement expressions for import requirements
        for old_func, replacement_info in collector.replacements.items():
            requirements = analyze_replacement_context(
                replacement_info.replacement_expr, context
            )

            for req in requirements:
                if req.is_local_reference:
                    # This references something defined locally, no import needed
                    continue

                if req.suggested_module:
                    # We have a suggestion for where this should be imported from
                    actual_req = ImportRequirement(
                        module=req.suggested_module, name=req.name, alias=req.alias
                    )
                    import_manager.add_import(actual_req)
                elif req.module:
                    # We know the exact module
                    import_manager.add_import(req)

            # If this replacement comes from an imported module, check for local variable references
            if replacement_info.source_module and replacement_info.func_def:
                non_param_names = collector._get_non_parameter_names(
                    replacement_info.func_def, replacement_info.replacement_expr
                )

                # Add imports for non-parameter names from the source module
                for name in non_param_names:
                    # Skip if already imported or defined locally
                    if not context.is_local_reference(
                        name
                    ) and not context.get_import_source(name):
                        import_req = ImportRequirement(
                            module=replacement_info.source_module, name=name, alias=None
                        )
                        import_manager.add_import(import_req)

    # Convert back to source code
    return ast.unparse(new_tree)


def migrate_file(filepath: str, write: bool = False) -> str:
    """Migrate a Python file by inlining replace_me decorated functions.

    This is a simple wrapper that reads a file, migrates its content,
    and optionally writes it back. It only processes deprecations defined
    within the same file.

    Args:
        filepath: Path to the Python file to migrate.
        write: Whether to write changes back to the file.

    Returns:
        The migrated source code.

    Raises:
        IOError: If the file cannot be read or written.
    """
    with open(filepath) as f:
        source = f.read()

    new_source = migrate_source(source)

    if write and new_source != source:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source


def migrate_file_with_imports(
    filepath: str,
    write: bool = False,
    interactive: bool = False,
    prompt_func: Optional[Callable[[str, str], Literal["y", "n", "a", "q"]]] = None,
) -> str:
    """Migrate a Python file, considering imported deprecated functions.

    This enhanced version analyzes imports and attempts to fetch replacement
    information from imported modules in the same directory structure.
    It can handle cases where deprecated functions are imported from other
    local modules.

    Args:
        filepath: Path to the Python file to migrate.
        write: Whether to write changes back to the file.
        interactive: Whether to prompt for confirmation before each replacement.
        prompt_func: Optional custom prompt function for interactive mode.

    Returns:
        The migrated source code.

    Raises:
        IOError: If the file cannot be read or written.

    Example:
        If module_a.py contains::

            from module_b import old_func
            result = old_func(10)

        And module_b.py contains::

            @replace_me()
            def old_func(x):
                return new_func(x, mode="legacy")

        The migration will update module_a.py to::

            from module_b import old_func
            result = new_func(10, mode="legacy")
    """
    import os

    with open(filepath) as f:
        source = f.read()

    file_dir = os.path.dirname(os.path.abspath(filepath))

    # Create a module resolver for local files
    def local_module_resolver(module_name: str, _: Optional[str]) -> Optional[str]:
        module_path = module_name.replace(".", "/")
        potential_paths = [
            os.path.join(file_dir, f"{module_path}.py"),
            os.path.join(file_dir, module_path, "__init__.py"),
        ]

        for path in potential_paths:
            if os.path.exists(path):
                try:
                    with open(path) as f:
                        return f.read()
                except BaseException as e:
                    logging.warning('Failed to read module "%s", ignoring: %s', path, e)
                    continue
        return None

    new_source = migrate_source(
        source,
        module_resolver=local_module_resolver,
        interactive=interactive,
        prompt_func=prompt_func,
    )

    if write and new_source != source:
        with open(filepath, "w") as f:
            f.write(new_source)

    return new_source
