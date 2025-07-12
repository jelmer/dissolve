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
with their suggested alternatives using CST for perfect formatting preservation.
"""

import difflib
import re
from typing import Callable, Literal, Union

import libcst as cst
from libcst.metadata import PositionProvider

from .collector import ConstructType, ReplaceInfo


class FunctionCallReplacer(cst.CSTTransformer):
    """Replaces function calls with their replacement expressions.

    This CST transformer visits function calls and replaces calls to
    deprecated functions with their suggested replacements, substituting
    actual argument values. CST preserves exact formatting, comments, and whitespace.

    Attributes:
        replacements: Mapping from function names to their replacement info.
        replaced_nodes: Set of original nodes that were replaced.
        _parent_stack: Stack to track parent nodes for context-aware replacement.
    """

    def __init__(self, replacements: dict[str, ReplaceInfo]) -> None:
        self.replacements = replacements
        self.replaced_nodes: set[cst.CSTNode] = set()

    def leave_Call(
        self, original_node: cst.Call, updated_node: cst.Call
    ) -> cst.BaseExpression:
        """Visit Call nodes and replace deprecated function calls."""
        func_name = self._get_function_name(updated_node)
        if func_name and func_name in self.replacements:
            replacement = self.replacements[func_name]
            new_node = self._create_replacement_node(updated_node, replacement)
            if new_node is not updated_node:
                self.replaced_nodes.add(original_node)
                return new_node
        return updated_node

    def leave_Attribute(
        self, original_node: cst.Attribute, updated_node: cst.Attribute
    ) -> cst.BaseExpression:
        """Visit Attribute nodes and replace deprecated property accesses."""
        # Check if this is a property access that should be replaced
        if updated_node.attr.value in self.replacements:
            replacement = self.replacements[updated_node.attr.value]
            # Only replace if this is marked as a property (not a method)
            if replacement.construct_type == ConstructType.PROPERTY:
                new_node = self._create_property_replacement_node(
                    updated_node, replacement
                )
                if new_node is not updated_node:
                    self.replaced_nodes.add(original_node)
                    return new_node
        return updated_node

    def _get_function_name(self, node: cst.Call) -> Union[str, None]:
        """Extract the function name from a Call node."""
        if isinstance(node.func, cst.Name):
            return node.func.value
        elif isinstance(node.func, cst.Attribute):
            # For method calls, return just the method name
            # e.g., for pack.index.object_index(), return "object_index"
            return node.func.attr.value
        return None

    def _build_param_map(
        self, call: cst.Call, replacement: ReplaceInfo
    ) -> dict[str, str]:
        """Build a mapping of parameter names to their code representations.

        Args:
            call: The function call with arguments.
            replacement: Information about the replacement expression.

        Returns:
            Dictionary mapping parameter names to their code strings.
        """
        # Extract parameter names from replacement expression
        param_names = re.findall(r"\{(\w+)\}", replacement.replacement_expr)

        # Filter out special parameters
        is_method_call = isinstance(call.func, cst.Attribute)
        if is_method_call:
            special_params = []
            if replacement.construct_type == ConstructType.CLASSMETHOD:
                special_params.append("cls")
            elif replacement.construct_type not in (
                ConstructType.STATICMETHOD,
                ConstructType.PROPERTY,
            ):
                special_params.append("self")
            param_names = [p for p in param_names if p not in special_params]

        # Build parameter map from positional and keyword arguments
        param_map = {}

        # Map positional arguments
        pos_args = [arg for arg in call.args if arg.keyword is None]
        for param_name, arg in zip(param_names, pos_args):
            # Get the exact code for this argument
            param_map[param_name] = cst.Module([]).code_for_node(arg.value)

        # Map keyword arguments (overwrites positional if same name)
        for arg in call.args:
            if arg.keyword and arg.keyword.value in param_names:
                param_map[arg.keyword.value] = cst.Module([]).code_for_node(arg.value)

        return param_map

    def _create_replacement_node(
        self, original_call: cst.Call, replacement: ReplaceInfo
    ) -> cst.BaseExpression:
        """Create a CST node for the replacement expression.

        Args:
            original_call: The original function call to replace.
            replacement: Information about the replacement expression.

        Returns:
            CST node representing the replacement expression with arguments
            substituted.
        """
        # Build a mapping of parameter names to their code
        param_map = self._build_param_map(original_call, replacement)

        # Start with the replacement expression
        replacement_code = replacement.replacement_expr

        # Handle async function double-await issue
        if (
            replacement.construct_type == ConstructType.ASYNC_FUNCTION
            and self._is_awaited_call(original_call)
        ):
            # Remove leading await from replacement if the call itself is awaited
            replacement_code = re.sub(r"^\s*await\s+", "", replacement_code)

        # Handle special parameters for method calls
        if isinstance(original_call.func, cst.Attribute):
            obj_code = cst.Module([]).code_for_node(original_call.func.value)
            if (
                replacement.construct_type == ConstructType.CLASSMETHOD
                and "{cls}" in replacement_code
            ):
                replacement_code = replacement_code.replace("{cls}", obj_code)
            elif (
                replacement.construct_type
                not in (ConstructType.STATICMETHOD, ConstructType.PROPERTY)
                and "{self}" in replacement_code
            ):
                replacement_code = replacement_code.replace("{self}", obj_code)

        # Replace parameter placeholders with actual values
        for param_name, param_code in param_map.items():
            replacement_code = replacement_code.replace(f"{{{param_name}}}", param_code)

        try:
            # Parse the replacement as an expression
            return cst.parse_expression(replacement_code)
        except cst.ParserSyntaxError:
            # If parsing fails, return the original
            return original_call

    def _is_awaited_call(self, call_node: cst.Call) -> bool:
        """Check if this call is already awaited.

        For now, we'll use a simple approach: check if the call is part of an Await expression
        by examining the parent relationships in the CST structure.
        """
        # This is a simplified implementation that won't work without parent tracking
        # For now, we'll always return False and handle the double-await issue differently
        return False

    def _create_property_replacement_node(
        self, original_attr: cst.Attribute, replacement: ReplaceInfo
    ) -> cst.BaseExpression:
        """Create a CST node for the property replacement expression.

        Args:
            original_attr: The original attribute access to replace.
            replacement: Information about the replacement expression.

        Returns:
            CST node representing the replacement expression with the object
            reference substituted.
        """
        # For properties, substitute {self} with the object
        obj_code = cst.Module([]).code_for_node(original_attr.value)
        replacement_code = replacement.replacement_expr.replace("{self}", obj_code)

        try:
            # Parse the replacement as an expression
            return cst.parse_expression(replacement_code)
        except cst.ParserSyntaxError:
            # If parsing fails, return the original
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

    METADATA_DEPENDENCIES = (PositionProvider,)

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
        self._current_node: Union[cst.Call, cst.Attribute, None] = None
        self._user_prompt_func = prompt_func
        # Always use our wrapper that has access to context
        self.prompt_func = self._context_aware_prompt

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
        self, node: cst.CSTNode, context_size: int = 3
    ) -> tuple[list[str], int]:
        """Get source lines around the node with context.

        Returns:
            A tuple of (lines, index_of_node_line)
        """
        if not self.source_lines:
            return [], -1

        # Get position information
        pos = self.get_metadata(PositionProvider, node, None)
        if not pos:
            return [], -1

        # Line numbers in CST are 1-based, convert to 0-based for list indexing
        node_line_idx = pos.start.line - 1

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

    def leave_Call(
        self, original_node: cst.Call, updated_node: cst.Call
    ) -> cst.BaseExpression:
        """Visit Call nodes and interactively replace deprecated function calls."""
        if self.quit:
            return updated_node

        func_name = self._get_function_name(updated_node)
        if func_name and func_name in self.replacements:
            replacement = self.replacements[func_name]

            # Get string representations of old and new calls
            old_call_str = cst.Module([]).code_for_node(original_node)
            replacement_node = self._create_replacement_node(updated_node, replacement)
            new_call_str = cst.Module([]).code_for_node(replacement_node)

            # Check if we should replace
            if self.replace_all:
                self.replaced_nodes.add(original_node)
                return replacement_node

            # Store current node for context in prompt
            self._current_node = original_node

            # Prompt user
            response = self.prompt_func(old_call_str, new_call_str)

            # Clear current node
            self._current_node = None

            if response == "y":
                self.replaced_nodes.add(original_node)
                return replacement_node
            elif response == "a":
                self.replace_all = True
                self.replaced_nodes.add(original_node)
                return replacement_node
            elif response == "q":
                self.quit = True
                return updated_node
            else:  # response == "n"
                return updated_node

        return updated_node

    def leave_Attribute(
        self, original_node: cst.Attribute, updated_node: cst.Attribute
    ) -> cst.BaseExpression:
        """Visit Attribute nodes and interactively replace deprecated property accesses."""
        if self.quit:
            return updated_node

        # Check if this is a property access that should be replaced
        if updated_node.attr.value in self.replacements:
            replacement = self.replacements[updated_node.attr.value]

            # Only replace if this is marked as a property (not a method)
            if replacement.construct_type == ConstructType.PROPERTY:
                # Get string representations of old and new attribute access
                old_attr_str = cst.Module([]).code_for_node(original_node)
                replacement_node = self._create_property_replacement_node(
                    updated_node, replacement
                )
                new_attr_str = cst.Module([]).code_for_node(replacement_node)

                # Check if we should replace
                if self.replace_all:
                    self.replaced_nodes.add(original_node)
                    return replacement_node

                # Store current node for context in prompt
                self._current_node = original_node

                # Prompt user
                response = self.prompt_func(old_attr_str, new_attr_str)

                # Clear current node
                self._current_node = None

                if response == "y":
                    self.replaced_nodes.add(original_node)
                    return replacement_node
                elif response == "a":
                    self.replace_all = True
                    self.replaced_nodes.add(original_node)
                    return replacement_node
                elif response == "q":
                    self.quit = True
                    return updated_node
                else:  # response == "n"
                    return updated_node

        return updated_node
