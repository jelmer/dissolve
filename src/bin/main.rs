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

//! Command-line interface for the dissolve tool.
//!
//! This binary provides the entry point for the dissolve CLI, which offers
//! commands for:
//!
//! - `migrate`: Automatically replace deprecated function calls with their
//!   suggested replacements in Python source files.
//! - `cleanup`: Remove deprecated functions decorated with @replace_me from source files
//!   (primarily for library maintainers after deprecation period), optionally filtering by version.
//! - `check`: Verify that @replace_me decorated functions can be successfully replaced
//! - `info`: List all @replace_me decorated functions and their replacements

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::{Path, PathBuf};

use dissolve_python::migrate_ruff;
use dissolve_python::type_introspection_context::TypeIntrospectionContext;
use dissolve_python::TypeIntrospectionMethod;
use dissolve_python::{
    check_file, collect_deprecated_from_dependencies, remove_from_file,
    RuffDeprecatedFunctionCollector,
};

#[derive(Parser)]
#[command(name = "dissolve")]
#[command(about = "Dissolve - Replace deprecated API usage")]
#[command(version)]
struct Cli {
    /// Enable debug logging
    #[arg(long)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Migrate Python files by inlining deprecated function calls
    Migrate {
        /// Python files or directories to migrate
        paths: Vec<String>,

        /// Treat paths as Python module paths (e.g. package.module)
        #[arg(short, long)]
        module: bool,

        /// Write changes back to files (default: print to stdout)
        #[arg(short, long, group = "mode")]
        write: bool,

        /// Check if files need migration without modifying them (exit 1 if changes needed)
        #[arg(long, group = "mode")]
        check: bool,

        /// Interactively confirm each replacement before applying
        #[arg(long, group = "mode")]
        interactive: bool,

        /// Type introspection method to use
        #[arg(long, value_enum, default_value = "pyright-mypy")]
        type_introspection: TypeIntrospectionMethodArg,
    },

    /// Remove deprecated functions decorated with @replace_me from Python files (for library maintainers)
    Cleanup {
        /// Python files or directories to process
        paths: Vec<String>,

        /// Treat paths as Python module paths (e.g. package.module)
        #[arg(short, long)]
        module: bool,

        /// Write changes back to files (default: print to stdout)
        #[arg(short, long, group = "cleanup_mode")]
        write: bool,

        /// Remove functions with decorators with version older than this
        #[arg(long)]
        before: Option<String>,

        /// Remove all functions with @replace_me decorators regardless of version
        #[arg(long)]
        all: bool,

        /// Check if files have deprecated functions that can be removed without modifying them (exit 1 if changes needed)
        #[arg(long, group = "cleanup_mode")]
        check: bool,

        /// Current package version for remove_in comparison (auto-detected if not provided)
        #[arg(long)]
        current_version: Option<String>,
    },

    /// Verify that @replace_me decorated functions can be successfully replaced
    Check {
        /// Python files or directories to check
        paths: Vec<String>,

        /// Treat paths as Python module paths (e.g. package.module)
        #[arg(short, long)]
        module: bool,
    },

    /// List all @replace_me decorated functions and their replacements
    Info {
        /// Python files or directories to analyze
        paths: Vec<String>,

        /// Treat paths as Python module paths (e.g. package.module)
        #[arg(short, long)]
        module: bool,
    },
}

#[derive(ValueEnum, Clone)]
enum TypeIntrospectionMethodArg {
    #[value(name = "pyright-lsp")]
    PyrightLsp,
    #[value(name = "mypy-daemon")]
    MypyDaemon,
    #[value(name = "pyright-mypy")]
    PyrightWithMypyFallback,
}

impl From<TypeIntrospectionMethodArg> for TypeIntrospectionMethod {
    fn from(arg: TypeIntrospectionMethodArg) -> Self {
        match arg {
            TypeIntrospectionMethodArg::PyrightLsp => TypeIntrospectionMethod::PyrightLsp,
            TypeIntrospectionMethodArg::MypyDaemon => TypeIntrospectionMethod::MypyDaemon,
            TypeIntrospectionMethodArg::PyrightWithMypyFallback => {
                TypeIntrospectionMethod::PyrightWithMypyFallback
            }
        }
    }
}

/// Discover Python files in a directory or resolve a path argument
fn discover_python_files(path: &str, _as_module: bool) -> Result<Vec<PathBuf>> {
    let path = Path::new(path);

    // If it's already a Python file, return it
    if path.is_file() && path.extension().is_some_and(|ext| ext == "py") {
        return Ok(vec![path.to_path_buf()]);
    }

    // If it's a directory, scan recursively for Python files
    if path.is_dir() {
        let mut python_files = Vec::new();
        visit_python_files(path, &mut python_files)?;
        python_files.sort();
        return Ok(python_files);
    }

    // Try glob pattern matching for file paths
    if path.to_string_lossy().contains('*') || path.to_string_lossy().contains('?') {
        let pattern = path.to_string_lossy();
        let glob_results = glob::glob(&pattern)?;
        let mut files = Vec::new();
        for entry in glob_results {
            let entry = entry?;
            if entry.extension().is_some_and(|ext| ext == "py") {
                files.push(entry);
            }
        }
        files.sort();
        return Ok(files);
    }

    // Fall back to treating it as a file path (may not exist)
    Ok(vec![path.to_path_buf()])
}

fn visit_python_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Skip hidden directories and __pycache__
                if let Some(name) = path.file_name() {
                    let name = name.to_string_lossy();
                    if !name.starts_with('.') && name != "__pycache__" {
                        visit_python_files(&path, files)?;
                    }
                }
            } else if path.extension().is_some_and(|ext| ext == "py") {
                files.push(path);
            }
        }
    }
    Ok(())
}

/// Expand a list of paths to include directories and Python object paths
fn expand_paths(paths: &[String], as_module: bool) -> Result<Vec<PathBuf>> {
    use indexmap::IndexSet;

    let mut expanded = IndexSet::new();
    for path in paths {
        expanded.extend(discover_python_files(path, as_module)?);
    }

    Ok(expanded.into_iter().collect())
}

/// Detect the module name from a file path
fn detect_module_name(file_path: &Path) -> String {
    let mut current_dir = file_path.parent().unwrap_or(Path::new("."));
    let mut module_parts = Vec::new();

    // Add file stem if it's not __init__
    if let Some(stem) = file_path.file_stem() {
        if stem != "__init__" {
            module_parts.push(stem.to_string_lossy());
        }
    }

    // Look for __init__.py files to determine package structure
    loop {
        let init_file = current_dir.join("__init__.py");
        if !init_file.exists() {
            break;
        }

        // This directory is a package
        if let Some(package_name) = current_dir.file_name() {
            module_parts.insert(0, package_name.to_string_lossy());
        }

        match current_dir.parent() {
            Some(parent) if parent != current_dir => current_dir = parent,
            _ => break,
        }
    }

    // Return the full module name if we found package structure
    if !module_parts.is_empty() {
        module_parts.join(".")
    } else {
        // Fallback to just the filename stem
        file_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Process files with common check/write logic
fn process_files_common<F>(
    files: &[PathBuf],
    mut process_func: F,
    check: bool,
    write: bool,
    operation_name: &str,
) -> Result<i32>
where
    F: FnMut(&PathBuf) -> Result<(String, String)>,
{
    let mut needs_changes = false;

    for filepath in files {
        let (original, result) = process_func(filepath)?;

        let has_changes = result != original;

        if check {
            // Check mode: just report if changes are needed
            if has_changes {
                println!("{}: needs {}", filepath.display(), operation_name);
                needs_changes = true;
            } else {
                println!("{}: up to date", filepath.display());
            }
        } else if write {
            // Write mode: update file if changed
            if has_changes {
                fs::write(filepath, &result)?;
                println!("Modified: {}", filepath.display());
            } else {
                println!("Unchanged: {}", filepath.display());
            }
        } else {
            // Default: print to stdout
            println!("# {}: {}", operation_name, filepath.display());
            println!("{}", result);
            println!();
        }
    }

    // In check mode, exit with code 1 if any files need changes
    Ok(if check && needs_changes { 1 } else { 0 })
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging
    if cli.debug || std::env::var("RUST_LOG").is_ok() {
        let filter = match tracing_subscriber::EnvFilter::try_from_default_env() {
            Ok(filter) => filter,
            Err(_) => {
                if cli.debug {
                    tracing_subscriber::EnvFilter::new("debug")
                } else {
                    tracing_subscriber::EnvFilter::new("warn")
                }
            }
        };
        tracing_subscriber::fmt().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .init();
    }

    match cli.command {
        Commands::Migrate {
            paths,
            module: _module,
            write,
            check,
            interactive,
            type_introspection,
        } => {
            let files = expand_paths(&paths, false)?; // TODO: Handle module mode
            let type_method: TypeIntrospectionMethod = type_introspection.into();

            // Create type introspection context once for all files
            let mut type_context = TypeIntrospectionContext::new(type_method)?;

            let mut needs_changes = false;
            for filepath in &files {
                let original = fs::read_to_string(filepath)?;
                let module_name = detect_module_name(filepath);

                let result = if interactive {
                    interactive_migrate_file_content(
                        &original,
                        &module_name,
                        filepath,
                        &mut type_context,
                    )?
                } else {
                    migrate_file_content(&original, &module_name, filepath, &mut type_context)?
                };

                let has_changes = result.as_ref().is_some_and(|r| r != &original);

                if check {
                    // Check mode: just report if changes are needed
                    if has_changes {
                        println!("{}: needs migration", filepath.display());
                        needs_changes = true;
                    } else {
                        println!("{}: up to date", filepath.display());
                    }
                } else if write {
                    // Write mode: write changes back to file
                    if let Some(new_content) = result {
                        if new_content != original {
                            fs::write(filepath, new_content)?;
                            println!("Modified: {}", filepath.display());
                        } else {
                            println!("Unchanged: {}", filepath.display());
                        }
                    } else {
                        println!("Unchanged: {}", filepath.display());
                    }
                } else {
                    // Default: print diff to stdout
                    if let Some(new_content) = result {
                        if new_content != original {
                            println!("# migration: {}", filepath.display());
                            print!("{}", new_content);
                        }
                    }
                }
            }

            // Shutdown cleanly
            type_context.shutdown()?;

            std::process::exit(if check && needs_changes { 1 } else { 0 });
        }

        Commands::Cleanup {
            paths,
            module: _,
            write,
            before,
            all,
            check,
            current_version,
        } => {
            let files = expand_paths(&paths, false)?; // TODO: Handle module mode

            let exit_code = process_files_common(
                &files,
                |filepath| {
                    let original = fs::read_to_string(filepath)?;
                    let (removed_count, result) = remove_from_file(
                        &filepath.to_string_lossy(),
                        before.as_deref(),
                        all,
                        false, // Don't write in the processor
                        current_version.as_deref(),
                    )?;

                    if removed_count > 0 {
                        println!(
                            "Would remove {} functions from {}",
                            removed_count,
                            filepath.display()
                        );
                    }

                    Ok((original, result))
                },
                check,
                write,
                "function cleanup",
            )?;

            std::process::exit(exit_code);
        }

        Commands::Check { paths, module: _ } => {
            let files = expand_paths(&paths, false)?; // TODO: Handle module mode
            let mut errors_found = false;

            for filepath in &files {
                let source = fs::read_to_string(filepath)?;
                let module_name = detect_module_name(filepath);
                let result = check_file(&source, &module_name, filepath)?;
                if result.success {
                    if !result.checked_functions.is_empty() {
                        println!(
                            "{}: {} @replace_me function(s) can be replaced",
                            filepath.display(),
                            result.checked_functions.len()
                        );
                    }
                } else {
                    errors_found = true;
                    println!("{}: ERRORS found", filepath.display());
                    for error in &result.errors {
                        println!("  {}", error);
                    }
                }
            }

            std::process::exit(if errors_found { 1 } else { 0 });
        }

        Commands::Info { paths, module: _ } => {
            let files = expand_paths(&paths, false)?; // TODO: Handle module mode

            // Collect all deprecated functions from specified files
            let mut all_deprecated: std::collections::HashMap<
                String,
                dissolve_python::ReplaceInfo,
            > = std::collections::HashMap::new();
            let mut total_files = 0;

            for filepath in &files {
                total_files += 1;
                let source = fs::read_to_string(filepath)?;
                let module_name = detect_module_name(filepath);

                // Collect deprecated functions from this file
                let collector = RuffDeprecatedFunctionCollector::new(module_name.clone(), None);
                let result = collector.collect_from_source(source.clone())?;

                if !result.replacements.is_empty() {
                    println!(
                        "\n{}: {} deprecated function(s)",
                        filepath.display(),
                        result.replacements.len()
                    );
                    for (name, info) in &result.replacements {
                        println!("  - {}", name);
                        println!("    Replacement: {}", info.replacement_expr);
                        if let Some(since) = &info.since {
                            println!("    Since: {}", since);
                        }
                        if let Some(remove_in) = &info.remove_in {
                            println!("    Remove in: {}", remove_in);
                        }
                        if let Some(message) = &info.message {
                            println!("    Message: {}", message);
                        }
                    }
                }

                // Also collect from dependencies if they are imported
                let dep_result = collect_deprecated_from_dependencies(&source, &module_name, 5)?;
                all_deprecated.extend(result.replacements);
                all_deprecated.extend(dep_result.replacements);
            }

            // Summary
            println!("\n=== Summary ===");
            println!("Total files analyzed: {}", total_files);
            println!("Total deprecated functions found: {}", all_deprecated.len());

            if !all_deprecated.is_empty() {
                println!("\n=== All deprecated functions ===");
                let mut functions: Vec<_> = all_deprecated.iter().collect();
                functions.sort_by_key(|(name, _)| name.as_str());

                for (name, info) in functions {
                    println!("\n{}", name);
                    println!("  Replacement: {}", info.replacement_expr);
                    if let Some(since) = &info.since {
                        println!("  Since: {}", since);
                    }
                    if let Some(remove_in) = &info.remove_in {
                        println!("  Remove in: {}", remove_in);
                    }
                    if let Some(message) = &info.message {
                        println!("  Message: {}", message);
                    }
                    if !info.parameters.is_empty() {
                        println!("  Parameters:");
                        for param in &info.parameters {
                            print!("    - {}", param.name);
                            if param.has_default {
                                print!(" (has default");
                                if let Some(default) = &param.default_value {
                                    print!(": {}", default);
                                }
                                print!(")");
                            }
                            if param.is_vararg {
                                print!(" (*args)");
                            }
                            if param.is_kwarg {
                                print!(" (**kwargs)");
                            }
                            if param.is_kwonly {
                                print!(" (keyword-only)");
                            }
                            println!();
                        }
                    }
                }
            }

            std::process::exit(0);
        }
    }
}

/// Migrate file content using the Rust backend
fn migrate_file_content(
    source: &str,
    module_name: &str,
    file_path: &Path,
    type_context: &mut TypeIntrospectionContext,
) -> Result<Option<String>> {
    tracing::debug!("Migrating {} ({} bytes)", module_name, source.len());

    // Collect deprecated functions from this file using Ruff
    let collector = RuffDeprecatedFunctionCollector::new(module_name.to_string(), None);
    let result = collector.collect_from_source(source.to_string())?;

    let mut all_replacements = result.replacements;

    // Collect deprecated functions from dependencies
    // TODO: Update dependency collector to use Ruff
    let dep_result = collect_deprecated_from_dependencies(source, module_name, 5)?;
    let dep_count = dep_result.replacements.len();
    all_replacements.extend(dep_result.replacements);

    if dep_count > 0 {
        tracing::debug!("Found {} deprecated functions in dependencies", dep_count);
    }

    // Report constructs that cannot be processed
    if !result.unreplaceable.is_empty() {
        for (name, unreplaceable_node) in &result.unreplaceable {
            let construct_type =
                format!("{:?}", unreplaceable_node.construct_type).replace('_', " ");
            tracing::warn!(
                "{} '{}' cannot be processed: {:?}{}",
                construct_type,
                name,
                unreplaceable_node.reason,
                if !unreplaceable_node.message.is_empty() {
                    format!(" ({})", unreplaceable_node.message)
                } else {
                    String::new()
                }
            );
        }
    }

    if all_replacements.is_empty() {
        // No deprecated functions found
        return Ok(None);
    }

    tracing::debug!("Total replacements available: {}", all_replacements.len());
    for key in all_replacements.keys() {
        tracing::debug!("  Available replacement: {}", key);
    }

    // Use Ruff-based migration
    let modified_source = migrate_ruff::migrate_file(
        source,
        module_name,
        file_path,
        type_context,
        all_replacements,
        dep_result.inheritance_map,
    )?;

    // Check if any changes were made
    if modified_source == source {
        return Ok(None);
    }

    // Return the modified code
    Ok(Some(modified_source))
}

/// Migrate file content interactively using the Rust backend
fn interactive_migrate_file_content(
    source: &str,
    module_name: &str,
    file_path: &Path,
    type_context: &mut TypeIntrospectionContext,
) -> Result<Option<String>> {
    tracing::debug!(
        "Interactively migrating {} ({} bytes)",
        module_name,
        source.len()
    );

    // Collect deprecated functions from this file using Ruff
    let collector = RuffDeprecatedFunctionCollector::new(module_name.to_string(), None);
    let result = collector.collect_from_source(source.to_string())?;
    let mut all_replacements = result.replacements;

    // Collect deprecated functions from dependencies
    // TODO: Update dependency collector to use Ruff
    let dep_result = collect_deprecated_from_dependencies(source, module_name, 5)?;
    let dep_count = dep_result.replacements.len();
    all_replacements.extend(dep_result.replacements);

    if dep_count > 0 {
        tracing::debug!("Found {} deprecated functions in dependencies", dep_count);
    }

    // Report constructs that cannot be processed
    if !result.unreplaceable.is_empty() {
        for (name, unreplaceable_node) in &result.unreplaceable {
            let construct_type =
                format!("{:?}", unreplaceable_node.construct_type).replace('_', " ");
            tracing::warn!(
                "{} '{}' cannot be processed: {:?}{}",
                construct_type,
                name,
                unreplaceable_node.reason,
                if !unreplaceable_node.message.is_empty() {
                    format!(" ({})", unreplaceable_node.message)
                } else {
                    String::new()
                }
            );
        }
    }

    if all_replacements.is_empty() {
        // No deprecated functions found
        return Ok(None);
    }

    tracing::debug!("Total replacements available: {}", all_replacements.len());

    // Use Ruff-based interactive migration
    let modified_source = migrate_ruff::migrate_file_interactive(
        source,
        module_name,
        file_path,
        type_context,
        all_replacements,
        dep_result.inheritance_map,
    )?;

    // Check if any changes were made
    if modified_source == source {
        return Ok(None);
    }

    // Return the modified code
    Ok(Some(modified_source))
}
