// Copyright (C) 2024 Jelmer Vernooij <jelmer@samba.org>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Test utilities for managing tests

use crate::type_introspection_context::TypeIntrospectionContext;
use crate::types::TypeIntrospectionMethod;

/// Instructions for running tests to avoid timeouts:
///
/// Due to the resource-intensive nature of Pyright LSP instances,
/// running all tests in parallel may cause timeouts. To avoid this:
///
/// 1. Run tests with limited parallelism:
///    `cargo test -- --test-threads=4`
///
/// 2. Run specific test suites separately:
///    `cargo test --lib`  # Run unit tests
///    `cargo test --test test_cli`  # Run CLI tests
///    `cargo test --tests`  # Run all integration tests
///
/// 3. For CI environments, consider using:
///    `cargo test -- --test-threads=2`
///
/// 4. Alternative approaches for resource management:
///    - Use test groups that share setup/teardown
///    - Consider mocking type introspection for unit tests
///    - Use the fallback type introspection method for simpler tests
pub const TEST_PARALLELISM_NOTE: &str = "
To avoid test timeouts, run with limited parallelism:
  cargo test -- --test-threads=4
";

/// Create a type introspection context for tests
/// This creates a new context each time - consider using the shared pool instead
pub fn create_test_type_context() -> Result<TypeIntrospectionContext, String> {
    TypeIntrospectionContext::new(TypeIntrospectionMethod::PyrightLsp).map_err(|e| e.to_string())
}
