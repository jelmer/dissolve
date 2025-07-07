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
import difflib
import re
from typing import Callable, Literal, Union

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

    def __init__(self, replacements: dict[str, ReplaceInfo]) -> None:
        self.replacements = replacements

    def visit_Call(self, node: ast.Call) -> ast.AST:
        """Visit Call nodes and replace deprecated function calls."""
        self.generic_visit(node)

        func_name = self._get_function_name(node)
        if func_name and func_name in self.replacements:
            replacement = self.replacements[func_name]
            return self._create_replacement_node(node, replacement)
        return node

    def visit_Attribute(self, node: ast.Attribute) -> ast.AST:
        """Visit Attribute nodes and replace deprecated property accesses."""
        self.generic_visit(node)

        # Check if this is a property access that should be replaced
        if node.attr in self.replacements:
            replacement = self.replacements[node.attr]
            # Only replace if this is marked as a property (not a method)
            if replacement.is_property:
                return self._create_property_replacement_node(node, replacement)
        return node

    def _get_function_name(self, node: ast.Call) -> Union[str, None]:
        """Extract the function name from a Call node."""
        if isinstance(node.func, ast.Name):
            return node.func.id
        elif isinstance(node.func, ast.Attribute):
            # For method calls, return just the method name
            # e.g., for pack.index.object_index(), return "object_index"
            return node.func.attr
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

        # Prepare the expression by replacing placeholders with valid identifiers
        temp_expr = replacement.replacement_expr
        
        # Extract all parameter placeholders
        param_pattern = re.compile(r"\{(\w+)\}")
        params = param_pattern.findall(replacement.replacement_expr)
        
        # Handle special parameters based on call type
        if isinstance(original_call.func, ast.Attribute):
            # Handle self/cls for method calls
            special_param = None
            if replacement.is_classmethod and "cls" in params:
                special_param = ("cls", "__cls_placeholder__")
            elif not replacement.is_staticmethod and "self" in params:
                special_param = ("self", "__self_placeholder__")
            
            if special_param:
                old_name, new_name = special_param
                temp_expr = temp_expr.replace(f"{{{old_name}}}", new_name)
                param_map[new_name] = original_call.func.value
                params.remove(old_name)
        
        # Replace remaining parameter placeholders
        for param in params:
            temp_expr = temp_expr.replace(f"{{{param}}}", param)

        try:
            # Parse and substitute parameters
            replacement_ast = ast.parse(temp_expr, mode="eval").body
            result = substitute_parameters(replacement_ast, param_map)
            ast.copy_location(result, original_call)
            return result
        except SyntaxError:
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
        # Extract parameter names from replacement expression
        param_names = re.findall(r"\{(\w+)\}", replacement.replacement_expr)
        
        # Filter out special parameters for method calls
        if isinstance(call.func, ast.Attribute):
            special_params = []
            if replacement.is_classmethod:
                special_params.append("cls")
            elif not replacement.is_staticmethod:
                special_params.append("self")
            param_names = [p for p in param_names if p not in special_params]

        # Build parameter map from positional and keyword arguments
        param_map = {}
        
        # Map positional arguments
        for param_name, arg in zip(param_names, call.args):
            param_map[param_name] = arg

        # Map keyword arguments (overwrites positional if same name)
        for keyword in call.keywords:
            if keyword.arg and keyword.arg in param_names:
                param_map[keyword.arg] = keyword.value

        return param_map

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
        # Replace {self} placeholder with a temporary identifier
        temp_expr = replacement.replacement_expr.replace("{self}", "self")

        try:
            # Parse and substitute self with the actual object
            replacement_ast = ast.parse(temp_expr, mode="eval").body
            result = substitute_parameters(replacement_ast, {"self": original_attr.value})
            ast.copy_location(result, original_attr)
            return result
        except SyntaxError:
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
        source: Union[str, None] = None,
    ) -> None:
        super().__init__(replacements)
        self.replace_all = False
        self.quit = False
        self.source = source
        self.source_lines = source.splitlines() if source else None
        self._current_node: Union[ast.Call, ast.Attribute, None] = None
        self._user_prompt_func = prompt_func
        # Always use our wrapper that has access to context
        self.prompt_func = self._context_aware_prompt
        self._processing_call = False

    def _context_aware_prompt(
        self, old_call: str, new_call: str
    ) -> Literal["y", "n", "a", "q"]:
        """Wrapper that adds context to prompts when available."""
        if self._user_prompt_func:
            # User provided custom prompt, just use it
            return self._user_prompt_func(old_call, new_call)
        else:
            # Use our default prompt which shows context
            return self._default_prompt(old_call, new_call)

    def _get_context_lines(
        self, node: Union[ast.Call, ast.Attribute], context_size: int = 3
    ) -> tuple[list[str], int]:
        """Get source lines around the node with context.

        Returns:
            A tuple of (lines, index_of_node_line)
        """
        if not self.source_lines or not hasattr(node, "lineno"):
            return [], -1

        # Line numbers in AST are 1-based, convert to 0-based for list indexing
        node_line_idx = node.lineno - 1

        # Calculate context range
        start_idx = max(0, node_line_idx - context_size)
        end_idx = min(len(self.source_lines), node_line_idx + context_size + 1)

        context_lines = self.source_lines[start_idx:end_idx]
        node_line_offset = node_line_idx - start_idx

        return context_lines, node_line_offset

    def _default_prompt(
        self, old_call: str, new_call: str
    ) -> Literal["y", "n", "a", "q"]:
        """Default interactive prompt for replacement confirmation."""
        print("\nFound deprecated call:")

        # If we have node context, show a context diff
        if hasattr(self, "_current_node") and self._current_node and self.source_lines:
            context_lines, node_line_offset = self._get_context_lines(
                self._current_node
            )

            if context_lines and node_line_offset >= 0:
                # Create modified version with the replacement
                modified_lines = context_lines.copy()
                if node_line_offset < len(modified_lines):
                    # Replace the specific call in the line
                    original_line = modified_lines[node_line_offset]
                    # This is a simplified replacement - in reality we'd need to handle
                    # the exact position within the line
                    modified_line = original_line.replace(old_call, new_call)
                    modified_lines[node_line_offset] = modified_line

                # Create unified diff with context
                diff = list(
                    difflib.unified_diff(
                        context_lines,
                        modified_lines,
                        fromfile="current",
                        tofile="proposed",
                        lineterm="",
                        n=len(context_lines),  # Show all context lines
                    )
                )

                # Print the diff
                for line in diff[2:]:  # Skip file headers
                    if line.startswith("-"):
                        print(f"- {line[1:]}")
                    elif line.startswith("+"):
                        print(f"+ {line[1:]}")
                    elif line.startswith("@@"):
                        # Skip the @@ line markers for cleaner output
                        continue
                    else:
                        print(f"  {line}")
            else:
                # Fallback to simple diff
                print(f"- {old_call}")
                print(f"+ {new_call}")
        else:
            # Fallback to simple diff without context
            print(f"- {old_call}")
            print(f"+ {new_call}")

        while True:
            response = input("\n[Y]es / [N]o / [A]ll / [Q]uit: ").lower().strip()
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

        # Set flag to prevent processing the function attribute in visit_Attribute
        self._processing_call = True
        self.generic_visit(node)
        self._processing_call = False

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

            # Store current node for context in prompt
            self._current_node = node

            # Prompt user
            response = self.prompt_func(old_call_str, new_call_str)

            # Clear current node
            self._current_node = None

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

        # Skip if we're processing a call (method calls are handled by visit_Call)
        if self._processing_call:
            return self.generic_visit(node)

        self.generic_visit(node)

        # Check if this is a property access that should be replaced
        if node.attr in self.replacements:
            replacement = self.replacements[node.attr]

            # Only replace if this is marked as a property (not a method)
            if replacement.is_property:
                # Get string representations of old and new attribute access
                old_attr_str = ast.unparse(node)
                replacement_node = self._create_property_replacement_node(
                    node, replacement
                )
                new_attr_str = ast.unparse(replacement_node)

                # Check if we should replace
                if self.replace_all:
                    return replacement_node

                # Store current node for context in prompt
                self._current_node = node

                # Prompt user
                response = self.prompt_func(old_attr_str, new_attr_str)

                # Clear current node
                self._current_node = None

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
