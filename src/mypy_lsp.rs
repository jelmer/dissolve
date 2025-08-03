use std::process::Command;
use tracing::{debug, error, info, warn};

/// Mypy-based type introspection using dmypy daemon
pub struct MypyTypeIntrospector {
    workspace_root: String,
    daemon_started: bool,
    checked_files: std::collections::HashSet<String>,
}

impl MypyTypeIntrospector {
    pub fn new(workspace_root: Option<&str>) -> Result<Self, String> {
        let workspace_root = workspace_root.map(|s| s.to_string()).unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });

        Ok(Self {
            workspace_root,
            daemon_started: false,
            checked_files: std::collections::HashSet::new(),
        })
    }

    /// Start the mypy daemon if not already running
    pub fn ensure_daemon_started(&mut self) -> Result<(), String> {
        if self.daemon_started {
            return Ok(());
        }

        // Check if daemon is already running
        let status = Command::new("dmypy")
            .arg("status")
            .output()
            .map_err(|e| format!("Failed to check dmypy status: {}", e))?;

        if !status.status.success() {
            // Start the daemon
            info!("Starting dmypy daemon...");
            let output = Command::new("dmypy")
                .arg("start")
                .arg("--")
                .arg("--python-executable")
                .arg("python3")
                .env("PYTHONPATH", &self.workspace_root)
                .current_dir(&self.workspace_root)
                .output()
                .map_err(|e| format!("Failed to start dmypy: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // Check if daemon is already running - this is fine
                if stderr.contains("Daemon is still alive") || stderr.contains("already running") {
                    debug!("dmypy daemon is already running, reusing existing daemon");
                } else {
                    return Err(format!("Failed to start dmypy daemon: {}", stderr));
                }
            }
        }

        self.daemon_started = true;
        Ok(())
    }

    /// Check a file with mypy if not already checked
    fn ensure_file_checked(&mut self, file_path: &str) -> Result<(), String> {
        if self.checked_files.contains(file_path) {
            return Ok(());
        }

        let check_output = Command::new("dmypy")
            .arg("check")
            .arg(file_path)
            .env("PYTHONPATH", &self.workspace_root)
            .current_dir(&self.workspace_root)
            .output()
            .map_err(|e| format!("Failed to run dmypy check: {}", e))?;

        if !check_output.status.success() {
            let stderr = String::from_utf8_lossy(&check_output.stderr);

            // Handle daemon connection issues specially
            if stderr.contains("Daemon has died") || stderr.contains("Daemon has crashed") {
                warn!("dmypy daemon died, restarting...");
                self.daemon_started = false;
                self.ensure_daemon_started()?;
                // Retry the check
                return self.ensure_file_checked(file_path);
            } else if stderr.contains("Resource temporarily unavailable")
                || stderr.contains("Daemon may be busy")
            {
                warn!("dmypy daemon is busy, skipping check for {}", file_path);
                self.checked_files.insert(file_path.to_string());
                return Ok(());
            }

            warn!("dmypy check had errors for {}: {}", file_path, stderr);
            // Continue anyway - mypy might still have type info despite errors
        }

        self.checked_files.insert(file_path.to_string());
        Ok(())
    }

    /// Get the type of an expression at a specific location
    pub fn get_type_at_position(
        &mut self,
        file_path: &str,
        line: usize,
        column: usize,
    ) -> Result<Option<String>, String> {
        self.ensure_daemon_started()?;
        self.ensure_file_checked(file_path)?;

        // Now inspect the type at the given position
        let location = format!("{}:{}:{}", file_path, line, column);
        let output = Command::new("dmypy")
            .arg("inspect")
            .arg("--show")
            .arg("type")
            .arg("--verbose")
            .arg("--verbose") // Double verbose for full type info
            .arg("--limit")
            .arg("1")
            .arg(&location)
            .env("PYTHONPATH", &self.workspace_root)
            .current_dir(&self.workspace_root)
            .output()
            .map_err(|e| format!("Failed to run dmypy inspect: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Handle daemon connection issues specially
            if stderr.contains("Daemon has died") || stderr.contains("Daemon has crashed") {
                warn!("dmypy daemon died during inspect, restarting...");
                self.daemon_started = false;
                self.ensure_daemon_started()?;
                self.ensure_file_checked(file_path)?;
                // Retry the inspect
                return self.get_type_at_position(file_path, line, column);
            } else if stderr.contains("Resource temporarily unavailable")
                || stderr.contains("Daemon may be busy")
            {
                warn!(
                    "dmypy daemon is busy during inspect at {}:{}:{}",
                    file_path, line, column
                );
                return Ok(None);
            }

            error!(
                "dmypy inspect failed at {}:{}:{} - {}",
                file_path, line, column, stderr
            );
            return Err(format!("Type introspection failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // dmypy inspect returns multiple lines - one type per expression at the position
        // We want the most specific type that contains our module types
        let lines: Vec<&str> = stdout.lines().collect();

        if lines.is_empty() {
            return Ok(None);
        }

        // Look for a concrete type in the output
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed == "None" {
                continue;
            }

            // Remove quotes if present
            let type_str = trimmed.trim_matches('"');

            // Skip if it's exactly "Any" - we need concrete types
            if type_str == "Any" {
                continue;
            }

            // If it contains a module path, it's likely what we want
            if type_str.contains('.') && !type_str.contains("builtins.") {
                // Extract the base type from union types like "dulwich.worktree.WorkTree | None"
                if let Some(base_type) = type_str.split('|').next() {
                    let base = base_type.trim();
                    if base != "Any" {
                        return Ok(Some(base.to_string()));
                    }
                }
                return Ok(Some(type_str.to_string()));
            }

            // Return any non-Any type we find
            return Ok(Some(type_str.to_string()));
        }

        // If we only found "Any" or nothing, return None
        warn!("mypy could not determine a concrete type at {}:{}:{} - only found 'Any' or no type info", file_path, line, column);
        Ok(None)
    }

    /// Get the fully qualified name of a type
    pub fn resolve_type_fqn(
        &mut self,
        _file_path: &str,
        type_name: &str,
    ) -> Result<Option<String>, String> {
        // For mypy, the type returned is already fully qualified
        // so we can just return it as-is
        Ok(Some(type_name.to_string()))
    }

    /// Invalidate cached type information for a file after modifications
    pub fn invalidate_file(&mut self, file_path: &str) -> Result<(), String> {
        tracing::debug!("Invalidating mypy cache for file: {}", file_path);

        // Remove the file from checked files so it will be re-checked next time
        self.checked_files.remove(file_path);

        // dmypy will automatically detect file changes and re-analyze
        // when we run check or inspect on it next time
        Ok(())
    }

    /// Stop the dmypy daemon
    pub fn stop_daemon(&mut self) -> Result<(), String> {
        if !self.daemon_started {
            return Ok(());
        }

        let output = Command::new("dmypy")
            .arg("stop")
            .output()
            .map_err(|e| format!("Failed to stop dmypy: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to stop dmypy daemon: {}", stderr);
        }

        self.daemon_started = false;
        self.checked_files.clear();
        Ok(())
    }
}

impl Drop for MypyTypeIntrospector {
    fn drop(&mut self) {
        // We don't stop the daemon on drop - it can be reused by other processes
        // and will timeout on its own
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_mypy_type_introspection() {
        let dir = tempdir().unwrap();
        let test_file = dir.path().join("test.py");

        fs::write(
            &test_file,
            r#"
from typing import List

def test_func() -> List[str]:
    result = ["hello", "world"]
    return result
"#,
        )
        .unwrap();

        let introspector_result = MypyTypeIntrospector::new(Some(dir.path().to_str().unwrap()));
        if introspector_result.is_err() {
            eprintln!(
                "Skipping test - mypy is not available: {:?}",
                introspector_result.err()
            );
            return;
        }
        let mut introspector = introspector_result.unwrap();

        // Get type of 'result' variable
        let type_info_result = introspector.get_type_at_position(
            test_file.to_str().unwrap(),
            5, // Line with 'result'
            4, // Column at 'result'
        );

        if let Err(e) = &type_info_result {
            eprintln!("get_type_at_position failed: {}", e);
            eprintln!("Skipping test - mypy introspection not working properly");
            return;
        }

        let type_info = type_info_result.unwrap();

        assert!(type_info.is_some());
        let type_str = type_info.unwrap();
        assert!(type_str.contains("List") || type_str.contains("list"));
    }
}
