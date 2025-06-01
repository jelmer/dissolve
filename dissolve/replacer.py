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

"""Function call replacement functionality.

This module provides classes for replacing deprecated function calls
with their suggested alternatives in Python AST nodes.
"""

import ast
from typing import Callable, Literal, Optional, Union

from .ast_utils import substitute_parameters
from .collector import ReplaceInfo


class FunctionCallReplacer(ast.NodeTransformer):
    """Replaces function calls with their replacement expressions.

    This AST transformer visits function calls and replaces calls to
    deprecated functions with their suggested replacements, substituting
    actual argument values.

    Attributes:
        replacements: Mapping from function names to their replacement info.
    """

    def __init__(
        self,
        replacements: dict[str, ReplaceInfo],
        all_deprecated: Optional[dict[str, ReplaceInfo]] = None,
        verbose: bool = False,
    ) -> None:
        self.replacements = replacements
        self.new_functions_used: set[str] = set()
        self.all_deprecated = all_deprecated or {}
        self.non_migratable_calls: list[
            tuple[str, int, str]
        ] = []  # (func_name, line_number, reason)
        self.successful_inlinings: list[
            tuple[str, int]
        ] = []  # (func_name, line_number)
        self.verbose = verbose

    def visit_Call(self, node: ast.Call) -> ast.AST:
        """Visit Call nodes and replace deprecated function calls."""
        self.generic_visit(node)

        func_name = self._get_function_name(node)
        if func_name:
            if func_name in self.replacements:
                replacement = self.replacements[func_name]
                # Track successful inlining
                line_num = getattr(node, "lineno", 0)
                self.successful_inlinings.append((func_name, line_num))
                return self._create_replacement_node(node, replacement)
            elif func_name in self.all_deprecated:
                # This is a deprecated function that couldn't be migrated
                line_num = getattr(node, "lineno", 0)
                from .extractor import extract_replacement_from_body
                from .types import ReplacementExtractionError

                replacement = self.all_deprecated[func_name]
                if replacement.func_def:
                    try:
                        extract_replacement_from_body(replacement.func_def)
                        reason = "unknown"
                    except ReplacementExtractionError as e:
                        reason = e.failure_reason.value
                else:
                    reason = "unknown function definition"

                self.non_migratable_calls.append((func_name, line_num, reason))
        return node

    def visit_Attribute(self, node: ast.Attribute) -> ast.AST:
        """Visit Attribute nodes and replace deprecated property accesses."""
        self.generic_visit(node)

        # Check if this is a property access that should be replaced
        if node.attr in self.replacements:
            replacement = self.replacements[node.attr]
            return self._create_property_replacement_node(node, replacement)
        return node

    def _get_function_name(self, node: ast.Call) -> Union[str, None]:
        """Extract the function name from a Call node."""
        if isinstance(node.func, ast.Name):
            return node.func.id
        return None

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

            # Track new functions used in the replacement
            self._track_new_functions(replacement_ast)

            # Substitute parameters using AST transformation
            result = substitute_parameters(replacement_ast, param_map)

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
        param_map = {}

        if not replacement.func_def:
            # Fallback to simple regex extraction if function definition is not available
            import re

            param_names = re.findall(r"\{(\w+)\}", replacement.replacement_expr)

            # Map positional arguments
            for i, (param_name, arg) in enumerate(zip(param_names, call.args)):
                param_map[param_name] = arg

            # Map keyword arguments
            for keyword in call.keywords:
                if keyword.arg and keyword.arg in param_names:
                    param_map[keyword.arg] = keyword.value

            return param_map

        # Use function definition to properly map parameters
        func_def = replacement.func_def
        args = func_def.args

        # Map regular positional arguments
        regular_args = args.args
        call_args = list(call.args)

        # Handle regular parameters first
        for i, param in enumerate(regular_args):
            if i < len(call_args):
                param_map[param.arg] = call_args[i]
            else:
                # Check for default values
                defaults_offset = (
                    len(regular_args) - len(args.defaults)
                    if args.defaults
                    else len(regular_args)
                )
                if i >= defaults_offset and args.defaults:
                    default_index = i - defaults_offset
                    param_map[param.arg] = args.defaults[default_index]

        # Handle *args if present
        if args.vararg:
            vararg_name = args.vararg.arg
            # Collect remaining arguments into a tuple
            remaining_args = call_args[len(regular_args) :]

            # Create a tuple node with remaining arguments
            if remaining_args:
                tuple_node = ast.Tuple(elts=remaining_args, ctx=ast.Load())
            else:
                tuple_node = ast.Tuple(elts=[], ctx=ast.Load())

            param_map[vararg_name] = tuple_node

        # Handle keyword arguments
        for keyword in call.keywords:
            if keyword.arg:
                # Check if this is a regular parameter
                for param in regular_args:
                    if param.arg == keyword.arg:
                        param_map[keyword.arg] = keyword.value
                        break

        return param_map

    def _track_new_functions(self, node: ast.AST) -> None:
        """Track function names that are used in replacement expressions."""
        for child in ast.walk(node):
            if isinstance(child, ast.Call) and isinstance(child.func, ast.Name):
                self.new_functions_used.add(child.func.id)
            elif isinstance(child, ast.Attribute):
                # Track module.function patterns
                module_parts: list[str] = []
                current: ast.expr = child
                while isinstance(current, ast.Attribute):
                    module_parts.insert(0, current.attr)
                    current = current.value
                if isinstance(current, ast.Name):
                    module_parts.insert(0, current.id)
                    self.new_functions_used.add(".".join(module_parts))

    def _create_property_replacement_node(
        self, original_attr: ast.Attribute, replacement: ReplaceInfo
    ) -> ast.AST:
        """Create an AST node for the property replacement expression.

        Args:
            original_attr: The original attribute access to replace.
            replacement: Information about the replacement expression.

        Returns:
            AST node representing the replacement expression with the object
            reference substituted.
        """
        # For properties, the replacement expression contains {self} placeholders
        # We need to substitute {self} with the actual object being accessed
        temp_expr = replacement.replacement_expr

        # Convert {self} placeholders to valid Python identifiers for parsing
        temp_expr = temp_expr.replace("{self}", "self")

        try:
            # Parse the replacement expression
            replacement_ast = ast.parse(temp_expr, mode="eval").body

            # Track new functions used in the replacement
            self._track_new_functions(replacement_ast)

            # Substitute 'self' with the actual object being accessed
            param_map = {"self": original_attr.value}
            result = substitute_parameters(replacement_ast, param_map)

            # Copy location information from original attribute access
            ast.copy_location(result, original_attr)
            return result
        except SyntaxError:
            # If parsing fails, return the original attribute access
            return original_attr


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
        prompt_func: Union[
            Callable[[str, str], Literal["y", "n", "a", "q"]], None
        ] = None,
        all_deprecated: Optional[dict[str, ReplaceInfo]] = None,
        verbose: bool = False,
    ) -> None:
        super().__init__(replacements, all_deprecated, verbose)
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

        func_name = self._get_function_name(node)
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

    def visit_Attribute(self, node: ast.Attribute) -> ast.AST:
        """Visit Attribute nodes and interactively replace deprecated property accesses."""
        if self.quit:
            return node

        self.generic_visit(node)

        # Check if this is a property access that should be replaced
        if node.attr in self.replacements:
            replacement = self.replacements[node.attr]

            # Get string representations of old and new attribute access
            old_attr_str = ast.unparse(node)
            replacement_node = self._create_property_replacement_node(node, replacement)
            new_attr_str = ast.unparse(replacement_node)

            # Check if we should replace
            if self.replace_all:
                return replacement_node

            # Prompt user
            response = self.prompt_func(old_attr_str, new_attr_str)

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
