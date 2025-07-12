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

"""Type definitions for dissolve."""

from enum import Enum
from typing import Optional


class ReplacementFailureReason(Enum):
    """Reasons why a function cannot be automatically replaced."""

    ARGS_KWARGS = "Function uses **kwargs"
    ASYNC_FUNCTION = "Async functions cannot be inlined"
    RECURSIVE_CALL = "Function contains recursive calls"
    LOCAL_IMPORTS = "Function contains local imports"
    COMPLEX_BODY = "Function body is too complex to inline"


class ReplacementExtractionError(Exception):
    """Exception raised when a function cannot be processed for replacement.

    This exception provides detailed information about why a function
    decorated with @replace_me cannot be automatically replaced.

    Attributes:
        function_name: Name of the function that cannot be processed.
        failure_reason: Enum indicating the specific type of failure.
        details: Optional additional details about the failure.
        line_number: Optional line number where the function is defined.
    """

    def __init__(
        self,
        function_name: str,
        failure_reason: ReplacementFailureReason,
        details: Optional[str] = None,
        line_number: Optional[int] = None,
    ) -> None:
        self.function_name = function_name
        self.failure_reason = failure_reason
        self.details = details
        self.line_number = line_number

        message = (
            f"Function '{function_name}' cannot be processed: {failure_reason.value}"
        )
        if details:
            message += f" ({details})"
        if line_number:
            message += f" (line {line_number})"

        super().__init__(message)
