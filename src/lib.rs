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

pub mod ast_transformer;
pub mod checker;
pub mod core;
pub mod dependency_collector;
pub mod migrate_ruff;
pub mod mypy_lsp;
pub mod pyright_lsp;
pub mod remover;
pub mod ruff_parser;
pub mod ruff_parser_improved;
pub mod ruff_remover;
pub mod scanner;
pub mod type_introspection_context;
pub mod types;

pub use checker::CheckResult;
pub use core::*;
pub use dependency_collector::collect_deprecated_from_dependencies;
pub use migrate_ruff::check_file;
pub use remover::remove_from_file;
pub use ruff_remover::remove_deprecated_functions;
pub use scanner::*;
pub use types::{TypeIntrospectionMethod, UserResponse};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_import_tracking;

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
mod test_setup;
