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

//! Test setup that enforces parallelism limits

// Module is already cfg(test) from lib.rs

use std::sync::Once;

static INIT: Once = Once::new();

/// Enforces test parallelism limits by setting RUST_TEST_THREADS if not already set
pub fn enforce_test_limits() {
    INIT.call_once(|| {
        // Check if RUST_TEST_THREADS is already set
        if std::env::var("RUST_TEST_THREADS").is_err() {
            // Set it to 4 threads maximum
            std::env::set_var("RUST_TEST_THREADS", "4");
            eprintln!("\n========================================");
            eprintln!("ℹ️  Auto-limiting test parallelism to 4 threads");
            eprintln!("ℹ️  This prevents timeouts from too many Pyright LSP instances");
            eprintln!("ℹ️  To override: RUST_TEST_THREADS=n cargo test");
            eprintln!("========================================\n");
        }
    });
}

#[ctor::ctor]
fn setup() {
    enforce_test_limits();
}
