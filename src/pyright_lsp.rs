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

//! Pyright LSP integration for type inference
//!
//! This module provides type querying capabilities using pyright language server.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// LSP request message
#[derive(Debug, Serialize)]
struct LspRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    params: Value,
}

/// LSP notification message
#[derive(Debug, Serialize)]
struct LspNotification {
    jsonrpc: &'static str,
    method: String,
    params: Value,
}

/// LSP response message
#[derive(Debug, Deserialize)]
struct LspResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<u64>,
    result: Option<Value>,
    error: Option<LspError>,
}

/// LSP error
#[derive(Debug, Deserialize)]
struct LspError {
    #[allow(dead_code)]
    code: i32,
    message: String,
    #[allow(dead_code)]
    data: Option<Value>,
}

/// Position in a text document
#[derive(Debug, Serialize)]
struct Position {
    line: u32,
    character: u32,
}

/// Text document identifier
#[derive(Debug, Serialize)]
struct TextDocumentIdentifier {
    uri: String,
}

/// Text document item
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct TextDocumentItem {
    uri: String,
    #[serde(rename = "languageId")]
    language_id: String,
    version: i32,
    text: String,
}

/// Hover params
#[derive(Debug, Serialize)]
struct HoverParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocumentIdentifier,
    position: Position,
}

/// Type definition params (same structure as hover params)
#[derive(Debug, Serialize)]
struct TypeDefinitionParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocumentIdentifier,
    position: Position,
}

/// Pyright LSP client
pub struct PyrightLspClient {
    process: Arc<Mutex<Child>>,
    request_id: AtomicU64,
    reader: Arc<Mutex<BufReader<std::process::ChildStdout>>>,
    is_shutdown: Arc<Mutex<bool>>,
}

impl PyrightLspClient {
    /// Create and start a new pyright LSP client
    pub fn new(workspace_root: Option<&str>) -> Result<Self> {
        tracing::debug!("Starting PyrightLspClient::new()");
        // Try to find pyright executable
        let pyright_cmd = if Command::new("pyright-langserver")
            .arg("--version")
            .output()
            .is_ok()
        {
            "pyright-langserver"
        } else if Command::new("pyright").arg("--version").output().is_ok() {
            // Some installations use 'pyright' directly
            "pyright"
        } else {
            return Err(anyhow!(
                "pyright not found. Please install pyright: pip install pyright"
            ));
        };

        // Start pyright in LSP mode
        tracing::debug!("Starting pyright process with command: {}", pyright_cmd);
        let mut process = Command::new(pyright_cmd)
            .args(["--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| anyhow!("Failed to start pyright: {}", e))?;

        let stdout = process.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;
        let reader = BufReader::new(stdout);

        let mut client = Self {
            process: Arc::new(Mutex::new(process)),
            request_id: AtomicU64::new(0),
            reader: Arc::new(Mutex::new(reader)),
            is_shutdown: Arc::new(Mutex::new(false)),
        };

        // Initialize the LSP connection
        client.initialize(workspace_root)?;

        Ok(client)
    }

    /// Initialize the LSP connection
    fn initialize(&mut self, workspace_root: Option<&str>) -> Result<()> {
        // Use provided workspace root or fall back to current directory
        let workspace_root = if let Some(root) = workspace_root {
            std::path::Path::new(root).to_path_buf()
        } else {
            std::env::current_dir()?
        };
        let workspace_uri = format!("file://{}", workspace_root.display());

        tracing::debug!(
            "Initializing pyright with workspace: {}",
            workspace_root.display()
        );

        let init_params = json!({
            "processId": std::process::id(),
            "clientInfo": {
                "name": "dissolve",
                "version": "0.1.0"
            },
            "locale": "en",
            "rootPath": workspace_root.to_str(),
            "rootUri": workspace_uri,
            "capabilities": {
                "textDocument": {
                    "hover": {
                        "contentFormat": ["plaintext", "markdown"]
                    },
                    "typeDefinition": {
                        "dynamicRegistration": false
                    }
                }
            },
            "trace": "off",
            "workspaceFolders": [{
                "uri": workspace_uri,
                "name": "test_workspace"
            }],
            "initializationOptions": {
                "autoSearchPaths": true,
                "useLibraryCodeForTypes": true,
                "typeCheckingMode": "basic",
                "python": {
                    "analysis": {
                        "extraPaths": []
                    }
                }
            }
        });

        // Use timeout for initialization
        let _response =
            self.send_request_with_timeout("initialize", init_params, Duration::from_secs(10))?;

        // Send initialized notification
        self.send_notification("initialized", json!({}))?;

        Ok(())
    }

    /// Send a request to the language server
    fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        // Use timeout for all requests, not just initialization
        self.send_request_with_timeout(method, params, Duration::from_secs(5))
    }

    /// Send a request to the language server with timeout
    fn send_request_with_timeout(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = LspRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        self.send_message(&request)?;

        // Read response with timeout
        self.read_response_with_timeout(id, timeout)
    }

    /// Send a notification to the language server
    fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let notification = LspNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };

        self.send_message(&notification)
    }

    /// Send a message to the language server
    fn send_message<T: Serialize>(&mut self, message: &T) -> Result<()> {
        let content = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        let mut process = self.process.lock().unwrap();
        let stdin = process.stdin.as_mut().ok_or_else(|| anyhow!("No stdin"))?;
        stdin.write_all(header.as_bytes())?;
        stdin.write_all(content.as_bytes())?;
        stdin.flush()?;

        Ok(())
    }

    /// Read a response from the language server
    #[allow(dead_code)]
    fn read_response(&self, expected_id: u64) -> Result<Value> {
        let mut reader = self.reader.lock().unwrap();

        loop {
            // Read headers
            let mut headers = Vec::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line)?;
                if line == "\r\n" || line == "\n" {
                    break;
                }
                headers.push(line);
            }

            // Parse Content-Length header
            let content_length = headers
                .iter()
                .find(|h| h.starts_with("Content-Length:"))
                .and_then(|h| h.split(':').nth(1))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .ok_or_else(|| anyhow!("Missing or invalid Content-Length header"))?;

            // Read content
            let mut content = vec![0u8; content_length];
            reader.read_exact(&mut content)?;

            // Parse JSON
            let response: LspResponse = serde_json::from_slice(&content)?;

            // Skip notifications
            if response.id.is_none() {
                continue;
            }

            // Check if this is our response
            if response.id == Some(expected_id) {
                if let Some(error) = response.error {
                    return Err(anyhow!("LSP error: {}", error.message));
                }
                return response
                    .result
                    .ok_or_else(|| anyhow!("No result in response"));
            }
        }
    }

    /// Read a response from the language server with timeout
    fn read_response_with_timeout(&self, expected_id: u64, timeout: Duration) -> Result<Value> {
        use std::time::Instant;
        let start = Instant::now();

        let mut reader = self.reader.lock().unwrap();

        // Poll for response with timeout
        while start.elapsed() < timeout {
            // Try to read with a small timeout to avoid blocking indefinitely
            std::thread::sleep(Duration::from_millis(10));

            // Check if the process is still alive
            {
                let mut process = self.process.lock().unwrap();
                match process.try_wait() {
                    Ok(Some(_)) => return Err(anyhow!("Pyright process has exited")),
                    Ok(None) => {} // Still running
                    Err(e) => return Err(anyhow!("Failed to check process status: {}", e)),
                }
            }

            // Try to read response
            loop {
                // Read headers
                let mut headers = Vec::new();
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => return Err(anyhow!("Connection closed")),
                        Ok(_) => {
                            if line == "\r\n" || line == "\n" {
                                break;
                            }
                            headers.push(line);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // No data available yet, continue outer loop
                            break;
                        }
                        Err(e) => return Err(anyhow!("Failed to read line: {}", e)),
                    }
                }

                if headers.is_empty() {
                    break; // No data available, continue with timeout loop
                }

                // Parse Content-Length header
                let content_length = headers
                    .iter()
                    .find(|h| h.starts_with("Content-Length:"))
                    .and_then(|h| h.split(':').nth(1))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .ok_or_else(|| anyhow!("Missing or invalid Content-Length header"))?;

                // Read content
                let mut content = vec![0u8; content_length];
                reader.read_exact(&mut content)?;

                // Parse JSON
                let response: LspResponse = serde_json::from_slice(&content)?;

                // Skip notifications
                if response.id.is_none() {
                    continue;
                }

                // Check if this is our response
                if response.id == Some(expected_id) {
                    if let Some(error) = response.error {
                        return Err(anyhow!("LSP error: {}", error.message));
                    }
                    return response
                        .result
                        .ok_or_else(|| anyhow!("No result in response"));
                }
            }
        }

        Err(anyhow!(
            "Timeout waiting for LSP response ({}s)",
            timeout.as_secs()
        ))
    }

    /// Open a file in the language server
    pub fn open_file(&mut self, file_path: &str, content: &str) -> Result<()> {
        // Convert to absolute path if relative
        let abs_path = if std::path::Path::new(file_path).is_relative() {
            std::env::current_dir()?.join(file_path)
        } else {
            std::path::PathBuf::from(file_path)
        };
        let uri = format!("file://{}", abs_path.display());
        let params = json!({
            "textDocument": {
                "uri": uri,
                "languageId": "python",
                "version": 1,
                "text": content
            }
        });

        self.send_notification("textDocument/didOpen", params)?;

        // Give pyright time to analyze the file
        std::thread::sleep(Duration::from_millis(100));

        Ok(())
    }

    /// Update file content in the language server
    pub fn update_file(&mut self, file_path: &str, content: &str, version: i32) -> Result<()> {
        tracing::debug!(
            "Updating file in pyright LSP: {} (version {})",
            file_path,
            version
        );

        // Convert to absolute path if relative
        let abs_path = if std::path::Path::new(file_path).is_relative() {
            std::env::current_dir()?.join(file_path)
        } else {
            std::path::PathBuf::from(file_path)
        };
        let uri = format!("file://{}", abs_path.display());
        let params = json!({
            "textDocument": {
                "uri": uri,
                "version": version
            },
            "contentChanges": [{
                "text": content
            }]
        });

        self.send_notification("textDocument/didChange", params)?;

        // Give pyright time to analyze the changes
        std::thread::sleep(Duration::from_millis(100));

        Ok(())
    }

    /// Get hover information (type) at a specific position
    pub fn get_hover(&mut self, file_path: &str, line: u32, column: u32) -> Result<Option<String>> {
        // Convert to absolute path if relative
        let abs_path = if std::path::Path::new(file_path).is_relative() {
            std::env::current_dir()?.join(file_path)
        } else {
            std::path::PathBuf::from(file_path)
        };
        let uri = format!("file://{}", abs_path.display());
        let params = HoverParams {
            text_document: TextDocumentIdentifier { uri },
            position: Position {
                line: line - 1, // Convert to 0-based
                character: column,
            },
        };

        let response = self.send_request("textDocument/hover", serde_json::to_value(params)?)?;

        // Extract type information from hover response
        if let Some(hover) = response.as_object() {
            if let Some(contents) = hover.get("contents") {
                let type_info = match contents {
                    Value::String(s) => s.clone(),
                    Value::Object(obj) => {
                        if let Some(Value::String(s)) = obj.get("value") {
                            s.clone()
                        } else {
                            return Ok(None);
                        }
                    }
                    _ => return Ok(None),
                };

                // Parse pyright's hover format
                // Examples:
                //   "(variable) repo: Repo"
                //   "(module) porcelain\n..."
                tracing::debug!("Pyright hover response: {}", type_info);

                // Check for module format first
                if type_info.starts_with("(module) ") {
                    // Extract module name - it's between "(module) " and the first newline or end of string
                    let module_start = "(module) ".len();
                    let module_end = type_info[module_start..]
                        .find('\n')
                        .map(|pos| module_start + pos)
                        .unwrap_or(type_info.len());
                    let module_name = type_info[module_start..module_end].trim();
                    tracing::debug!("Extracted module type: {}", module_name);
                    return Ok(Some(module_name.to_string()));
                }

                // Check for class format
                if type_info.starts_with("(class) ") {
                    // Extract class name - it's between "(class) " and the first newline or end of string
                    let class_start = "(class) ".len();
                    let class_end = type_info[class_start..]
                        .find('\n')
                        .map(|pos| class_start + pos)
                        .unwrap_or(type_info.len());
                    let class_name = type_info[class_start..class_end].trim();
                    tracing::debug!("Extracted class type: {}", class_name);
                    return Ok(Some(class_name.to_string()));
                }

                // Otherwise look for colon format for variables
                if let Some(colon_pos) = type_info.find(':') {
                    let type_part = type_info[colon_pos + 1..].trim();
                    tracing::debug!("Extracted type: {}", type_part);

                    // Check if pyright returned "Unknown" - treat as no type info
                    if type_part == "Unknown" {
                        tracing::warn!(
                            "Pyright returned 'Unknown' type at {}:{}:{}",
                            file_path,
                            line,
                            column
                        );
                        return Ok(None);
                    }

                    return Ok(Some(type_part.to_string()));
                }
            }
        }

        Ok(None)
    }

    /// Get type definition location
    pub fn get_type_definition(
        &mut self,
        file_path: &str,
        line: u32,
        column: u32,
    ) -> Result<Option<String>> {
        // Convert to absolute path if relative
        let abs_path = if std::path::Path::new(file_path).is_relative() {
            std::env::current_dir()?.join(file_path)
        } else {
            std::path::PathBuf::from(file_path)
        };
        let uri = format!("file://{}", abs_path.display());
        let params = TypeDefinitionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position {
                line: line - 1, // Convert to 0-based
                character: column,
            },
        };

        let response =
            self.send_request("textDocument/typeDefinition", serde_json::to_value(params)?)?;

        // Parse the response to get the location
        if let Some(locations) = response.as_array() {
            if let Some(first_location) = locations.first() {
                if let Some(target_uri) = first_location.get("uri").and_then(|u| u.as_str()) {
                    // The URI contains the file path which might have the module information
                    if let Some(target_range) = first_location.get("range") {
                        // We have the location of the type definition
                        // Now we need to read that location to get the type name
                        tracing::debug!(
                            "Type definition location: {} at {:?}",
                            target_uri,
                            target_range
                        );

                        // For now, just extract the filename which might give us module info
                        if let Some(path) = target_uri.strip_prefix("file://") {
                            if let Some(module_name) = path
                                .strip_suffix(".py")
                                .and_then(|p| p.split('/').next_back())
                            {
                                // This is a simple heuristic - the file name is often the module name
                                return Ok(Some(module_name.to_string()));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Query type at a specific location
    pub fn query_type(
        &mut self,
        file_path: &str,
        _content: &str,
        line: u32,
        column: u32,
    ) -> Result<Option<String>> {
        // Note: we assume the file is already open to avoid redundant open calls

        // First try hover for immediate type info
        let hover_result = self.get_hover(file_path, line, column);

        // Debug output
        match &hover_result {
            Ok(Some(type_str)) => {
                tracing::debug!("Pyright hover returned type: {}", type_str);

                // If we get a simple type name, try to get more info from type definition
                if !type_str.contains('.') {
                    if let Ok(Some(type_def_info)) =
                        self.get_type_definition(file_path, line, column)
                    {
                        tracing::debug!("Type definition info: {}", type_def_info);
                        // For now, still return the hover result
                        // In the future we could combine this info
                    }
                }

                return Ok(Some(type_str.clone()));
            }
            Ok(None) => {
                tracing::debug!("Pyright returned no type information");
            }
            Err(e) => {
                tracing::debug!("Pyright error: {}", e);
            }
        }

        hover_result
    }

    /// Shutdown the language server
    pub fn shutdown(&mut self) -> Result<()> {
        {
            let mut is_shutdown = self.is_shutdown.lock().unwrap();
            if *is_shutdown {
                return Ok(());
            }
            *is_shutdown = true;
        }

        // For shutdown, we expect a null result, so we need special handling
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = LspRequest {
            jsonrpc: "2.0",
            id,
            method: "shutdown".to_string(),
            params: json!({}),
        };
        self.send_message(&request)?;

        // Read shutdown response - expect null result
        self.read_shutdown_response(id)?;

        self.send_notification("exit", json!({}))?;
        Ok(())
    }

    /// Read shutdown response that expects null result
    fn read_shutdown_response(&self, expected_id: u64) -> Result<()> {
        let mut reader = self.reader.lock().unwrap();

        loop {
            // Read headers
            let mut headers = Vec::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line)?;
                if line == "\r\n" || line == "\n" {
                    break;
                }
                headers.push(line);
            }

            // Parse Content-Length header
            let content_length = headers
                .iter()
                .find(|h| h.starts_with("Content-Length:"))
                .and_then(|h| h.split(':').nth(1))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .ok_or_else(|| anyhow!("Missing or invalid Content-Length header"))?;

            // Read content
            let mut content = vec![0u8; content_length];
            reader.read_exact(&mut content)?;

            // Parse JSON
            let response: LspResponse = serde_json::from_slice(&content)?;

            // Skip notifications
            if response.id.is_none() {
                continue;
            }

            // Check if this is our response
            if response.id == Some(expected_id) {
                if let Some(error) = response.error {
                    return Err(anyhow!("LSP error: {}", error.message));
                }
                // For shutdown, result is null - this is expected and valid
                return Ok(());
            }
        }
    }
}

impl Drop for PyrightLspClient {
    fn drop(&mut self) {
        // Try to shutdown gracefully
        let _ = self.shutdown();

        // Kill the process if it's still running
        if let Ok(mut process) = self.process.lock() {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

/// Get type for a variable at a specific location using pyright
pub fn get_type_with_pyright(
    file_path: &str,
    content: &str,
    line: u32,
    column: u32,
) -> Result<Option<String>> {
    let mut client = PyrightLspClient::new(None)?;
    client.query_type(file_path, content, line, column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    #[ignore] // Ignore by default as it requires pyright to be installed
    fn test_pyright_type_inference() {
        let code = r#"
class Repo:
    @staticmethod
    def init(path):
        return Repo()

def test():
    repo = Repo.init(".")
"#;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(&temp_file, code).unwrap();

        let result = get_type_with_pyright(
            temp_file.path().to_str().unwrap(),
            code,
            8, // Line with 'repo' variable
            4, // Column of 'repo'
        );

        match result {
            Ok(Some(type_str)) => {
                assert!(
                    type_str.contains("Repo"),
                    "Expected Repo type, got: {}",
                    type_str
                );
            }
            Ok(None) => panic!("No type information returned"),
            Err(e) => panic!("Error: {}", e),
        }
    }
}
