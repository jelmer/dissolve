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

import ast

from dissolve.ast_utils import substitute_parameters


def test_substitute_parameters_with_ast_nodes():
    """Test direct AST substitution."""
    # Create an expression AST
    expr_ast = ast.parse("x + y", mode="eval").body

    # Create parameter map with AST nodes
    param_map = {"x": ast.Constant(value=5), "y": ast.Constant(value=10)}

    result_ast = substitute_parameters(expr_ast, param_map)
    result = ast.unparse(result_ast)
    assert result == "5 + 10"
