// Copyright (C) 2026 Jelmer Vernooij <jelmer@samba.org>
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

//! Type inference through a type server speaking the Type Server Protocol.
//!
//! See <https://github.com/microsoft/pylance-release/blob/main/docs/tsp/type-server-protocol.md>.
//! TSP is JSON-RPC layered on LSP: documents are synced with the usual
//! `textDocument/*` notifications, and `typeServer/getComputedType` returns a
//! structured type for a source range rather than hover text.
//!
//! A class type only carries its bare name and the location of its declaration,
//! so the qualified name is rebuilt from the declaring file: the module from the
//! server's search paths, and any enclosing classes from parsing that file.

use anyhow::{anyhow, bail, Context, Result};
use ruff_python_ast::{self as ast, PySourceType, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

/// Server started when no other command is given.
pub const DEFAULT_COMMAND: &[&str] = &["pyright-typeserver", "--stdio"];

/// Protocol version this client was written against.
///
/// While TSP is 0.x, minor versions may break compatibility.
const PROTOCOL_VERSION: &str = "0.4";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

const TYPE_KIND_CLASS: u32 = 3;
const DECLARATION_KIND_REGULAR: u32 = 0;
const TYPE_FLAG_INSTANTIABLE: u32 = 1;
const TYPE_FLAG_INSTANCE: u32 = 2;

#[derive(Debug, Deserialize)]
struct TspType {
    kind: u32,
    flags: u32,
    declaration: Option<TspDeclaration>,
}

#[derive(Debug, Deserialize)]
struct TspDeclaration {
    kind: u32,
    node: Option<TspNode>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TspNode {
    uri: String,
    range: LspRange,
}

#[derive(Debug, Deserialize)]
struct LspRange {
    start: LspPosition,
}

#[derive(Debug, Deserialize)]
struct LspPosition {
    line: u32,
    character: u32,
}

/// Client for a TSP type server running as a child process.
pub struct TspClient {
    process: Child,
    stdin: ChildStdin,
    messages: Receiver<Result<Value>>,
    next_id: u64,
    /// Contents of the documents opened on the server, with their LSP versions.
    documents: HashMap<PathBuf, (String, i32)>,
    /// Enclosing classes of class declarations, by declaring file and position.
    enclosing: HashMap<(PathBuf, u32, u32), Vec<String>>,
    snapshot: Option<u64>,
    search_paths: Option<Vec<PathBuf>>,
    is_shutdown: bool,
}

impl TspClient {
    /// Start the default type server.
    pub fn new(workspace_root: Option<&str>) -> Result<Self> {
        Self::with_command(DEFAULT_COMMAND, workspace_root)
    }

    /// Start the type server run by `command` and initialize it for `workspace_root`.
    pub fn with_command(command: &[&str], workspace_root: Option<&str>) -> Result<Self> {
        let (program, args) = command
            .split_first()
            .ok_or_else(|| anyhow!("empty type server command"))?;
        let mut process = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to start type server {program}"))?;

        let stdin = process.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = process.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;

        // Reading on a separate thread lets requests time out instead of hanging
        // on a stalled server.
        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let message = read_message(&mut reader);
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        });

        let mut client = Self {
            process,
            stdin,
            messages,
            next_id: 0,
            documents: HashMap::new(),
            enclosing: HashMap::new(),
            snapshot: None,
            search_paths: None,
            is_shutdown: false,
        };
        client.initialize(workspace_root)?;
        Ok(client)
    }

    fn initialize(&mut self, workspace_root: Option<&str>) -> Result<()> {
        let root = match workspace_root {
            Some(root) => PathBuf::from(root),
            None => std::env::current_dir()?,
        };
        let root = std::path::absolute(&root)?;
        let root_uri = path_to_uri(&root);
        self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {},
                "workspaceFolders": [{"uri": root_uri, "name": "workspace"}],
            }),
        )?;
        self.notify("initialized", json!({}))?;

        let version = self.request("typeServer/getSupportedProtocolVersion", Value::Null)?;
        let version = version
            .as_str()
            .ok_or_else(|| anyhow!("type server returned protocol version {version}"))?;
        if !version_compatible(version) {
            bail!("type server speaks TSP {version}, but only {PROTOCOL_VERSION}.x is supported");
        }
        Ok(())
    }

    /// Make `content` the source the server sees for `file_path`.
    pub fn set_file_content(&mut self, file_path: &Path, content: &str) -> Result<()> {
        let path = std::path::absolute(file_path)?;
        let uri = path_to_uri(&path);
        self.enclosing
            .retain(|(declared_in, _, _), _| *declared_in != path);
        match self.documents.get_mut(&path) {
            Some((text, version)) => {
                *version += 1;
                *text = content.to_string();
                let version = *version;
                self.notify(
                    "textDocument/didChange",
                    json!({
                        "textDocument": {"uri": uri, "version": version},
                        "contentChanges": [{"text": content}],
                    }),
                )?;
            }
            None => {
                self.documents.insert(path, (content.to_string(), 1));
                self.notify(
                    "textDocument/didOpen",
                    json!({
                        "textDocument": {
                            "uri": uri,
                            "languageId": "python",
                            "version": 1,
                            "text": content,
                        }
                    }),
                )?;
            }
        }
        self.snapshot = None;
        Ok(())
    }

    /// Look up the type of the expression at `range`, returning its qualified name.
    ///
    /// Classes declared in `file_path` itself are named under `module_name`, as
    /// the caller knows what module it is migrating even if the server's search
    /// paths place the file elsewhere.
    pub fn query_type(
        &mut self,
        file_path: &Path,
        module_name: &str,
        range: TextRange,
    ) -> Result<Option<String>> {
        let path = std::path::absolute(file_path)?;
        if !self.documents.contains_key(&path) {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            self.set_file_content(&path, &content)?;
        }
        let text = &self.documents[&path].0;
        let node = json!({
            "uri": path_to_uri(&path),
            "range": {
                "start": offset_to_position(text, range.start()),
                "end": offset_to_position(text, range.end()),
            },
        });

        let snapshot = self.snapshot()?;
        let result = self.request(
            "typeServer/getComputedType",
            json!({"arg": node, "snapshot": snapshot}),
        )?;
        if result.is_null() {
            return Ok(None);
        }
        let ty: TspType =
            serde_json::from_value(result).context("type server returned a malformed type")?;
        self.qualified_name(&ty, &path, module_name)
    }

    /// The qualified name of the class `ty` is an instance of, or the class itself.
    ///
    /// Anything else, including unions, has no single class and so no name.
    fn qualified_name(
        &mut self,
        ty: &TspType,
        queried_path: &Path,
        queried_module: &str,
    ) -> Result<Option<String>> {
        if ty.kind != TYPE_KIND_CLASS
            || ty.flags & (TYPE_FLAG_INSTANCE | TYPE_FLAG_INSTANTIABLE) == 0
        {
            return Ok(None);
        }
        let Some(declaration) = &ty.declaration else {
            return Ok(None);
        };
        if declaration.kind != DECLARATION_KIND_REGULAR {
            return Ok(None);
        }
        let (Some(node), Some(name)) = (&declaration.node, &declaration.name) else {
            return Ok(None);
        };

        let path = uri_to_path(&node.uri)?;
        let module = if path == queried_path {
            Some(queried_module.to_string())
        } else {
            self.module_name(&path)?
        };
        let mut parts: Vec<String> = module.into_iter().collect();
        parts.extend(self.enclosing_classes(&path, &node.range.start, name)?);
        parts.push(name.clone());
        Ok(Some(parts.join(".")))
    }

    /// The module `path` is imported as, if it lies under one of the search paths.
    fn module_name(&mut self, path: &Path) -> Result<Option<String>> {
        let search_paths = self.search_paths(path)?;
        // The most specific search path wins, so a stub package nested in a
        // broader root still resolves relative to itself.
        let Some(relative) = search_paths
            .iter()
            .filter_map(|root| path.strip_prefix(root).ok())
            .min_by_key(|relative| relative.components().count())
        else {
            return Ok(None);
        };

        let mut parts: Vec<String> = relative
            .with_extension("")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if parts.last().is_some_and(|last| last == "__init__") {
            parts.pop();
        }
        Ok((!parts.is_empty()).then(|| parts.join(".")))
    }

    fn search_paths(&mut self, from: &Path) -> Result<&[PathBuf]> {
        if self.search_paths.is_none() {
            let snapshot = self.snapshot()?;
            let result = self.request(
                "typeServer/getPythonSearchPaths",
                json!({"fromUri": path_to_uri(from), "snapshot": snapshot}),
            )?;
            let uris: Vec<String> = serde_json::from_value(result)
                .context("type server returned malformed search paths")?;
            let paths = uris
                .iter()
                .map(|uri| uri_to_path(uri))
                .collect::<Result<_>>()?;
            self.search_paths = Some(paths);
        }
        Ok(self.search_paths.as_deref().unwrap_or_default())
    }

    /// Names of the classes enclosing the class `name` declared at `start` in `path`.
    fn enclosing_classes(
        &mut self,
        path: &Path,
        start: &LspPosition,
        name: &str,
    ) -> Result<Vec<String>> {
        let key = (path.to_path_buf(), start.line, start.character);
        if let Some(chain) = self.enclosing.get(&key) {
            return Ok(chain.clone());
        }
        let source = match self.documents.get(path) {
            Some((text, _)) => text.clone(),
            None => std::fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?,
        };
        let offset = position_to_offset(&source, start).ok_or_else(|| {
            anyhow!(
                "class declaration position {}:{} is outside {}",
                start.line,
                start.character,
                path.display()
            )
        })?;
        // Source under migration may not parse cleanly; the class header still will.
        let parsed = ruff_python_parser::parse_unchecked_source(&source, PySourceType::from(path));

        let mut chain = Vec::new();
        if !class_chain(&parsed.syntax().body, offset, name, &mut chain) {
            bail!(
                "class {name} not found at {}:{} in {}",
                start.line,
                start.character,
                path.display()
            );
        }
        self.enclosing.insert(key, chain.clone());
        Ok(chain)
    }

    fn snapshot(&mut self) -> Result<u64> {
        if let Some(snapshot) = self.snapshot {
            return Ok(snapshot);
        }
        let result = self.request("typeServer/getSnapshot", Value::Null)?;
        let snapshot = result
            .as_u64()
            .ok_or_else(|| anyhow!("type server returned snapshot {result}"))?;
        self.snapshot = Some(snapshot);
        Ok(snapshot)
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;

        loop {
            let message = match self.messages.recv_timeout(REQUEST_TIMEOUT) {
                Ok(message) => message?,
                Err(RecvTimeoutError::Timeout) => {
                    bail!("type server did not answer {method} within {REQUEST_TIMEOUT:?}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("type server exited while handling {method}")
                }
            };

            match (message.get("id"), message.get("method")) {
                (Some(message_id), None) if message_id.as_u64() == Some(id) => {
                    if let Some(error) = message.get("error") {
                        bail!("type server failed {method}: {error}");
                    }
                    return Ok(message.get("result").cloned().unwrap_or(Value::Null));
                }
                // The server asks things like workspace/configuration; we have
                // nothing to configure, so answer with no settings.
                (Some(request_id), Some(_)) => {
                    let request_id = request_id.clone();
                    self.send(&json!({"jsonrpc": "2.0", "id": request_id, "result": null}))?;
                }
                (_, Some(server_method)) => {
                    if server_method == "typeServer/snapshotChanged" {
                        self.snapshot = None;
                    }
                }
                (Some(other), None) => {
                    bail!("type server answered unknown request {other} while handling {method}")
                }
                (None, None) => bail!("type server sent a malformed message: {message}"),
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn send(&mut self, message: &Value) -> Result<()> {
        let body = serde_json::to_vec(message)?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len())?;
        self.stdin.write_all(&body)?;
        self.stdin.flush()?;
        Ok(())
    }

    pub fn is_shutdown(&self) -> bool {
        self.is_shutdown
    }

    /// Ask the server to shut down and wait for it to exit.
    pub fn shutdown(&mut self) -> Result<()> {
        if self.is_shutdown {
            return Ok(());
        }
        self.is_shutdown = true;
        self.request("shutdown", Value::Null)?;
        self.notify("exit", Value::Null)?;
        self.process.wait()?;
        Ok(())
    }
}

impl Drop for TspClient {
    fn drop(&mut self) {
        if self.shutdown().is_err() {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}

fn version_compatible(version: &str) -> bool {
    version
        .strip_prefix(PROTOCOL_VERSION)
        .is_some_and(|rest| rest.starts_with('.'))
}

fn read_message(reader: &mut impl BufRead) -> Result<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            bail!("type server closed its output");
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = Some(value.trim().parse::<usize>()?);
            }
        }
    }
    let length = length.ok_or_else(|| anyhow!("type server message has no Content-Length"))?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

/// Find the class `name` whose header contains `offset`, collecting the names of
/// the classes around it into `chain`.
fn class_chain(body: &[Stmt], offset: TextSize, name: &str, chain: &mut Vec<String>) -> bool {
    for stmt in body {
        if !stmt.range().contains_inclusive(offset) {
            continue;
        }
        match stmt {
            Stmt::ClassDef(class) => {
                if class.name.as_str() == name && !class_body_contains(class, offset) {
                    return true;
                }
                chain.push(class.name.to_string());
                if class_chain(&class.body, offset, name, chain) {
                    return true;
                }
                chain.pop();
            }
            // A class local to a function is named after its enclosing classes only.
            Stmt::FunctionDef(function) => {
                if class_chain(&function.body, offset, name, chain) {
                    return true;
                }
            }
            _ => {
                if let Some(nested) = nested_bodies(stmt) {
                    if nested
                        .into_iter()
                        .any(|body| class_chain(body, offset, name, chain))
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn class_body_contains(class: &ast::StmtClassDef, offset: TextSize) -> bool {
    class
        .body
        .first()
        .is_some_and(|first| offset >= first.start())
}

/// Statement bodies of compound statements that can hold a class definition.
fn nested_bodies(stmt: &Stmt) -> Option<Vec<&[Stmt]>> {
    let bodies: Vec<&[Stmt]> = match stmt {
        Stmt::If(s) => std::iter::once(s.body.as_slice())
            .chain(s.elif_else_clauses.iter().map(|c| c.body.as_slice()))
            .collect(),
        Stmt::Try(s) => std::iter::once(s.body.as_slice())
            .chain(s.handlers.iter().map(|h| {
                let ast::ExceptHandler::ExceptHandler(h) = h;
                h.body.as_slice()
            }))
            .chain([s.orelse.as_slice(), s.finalbody.as_slice()])
            .collect(),
        Stmt::With(s) => vec![s.body.as_slice()],
        Stmt::For(s) => vec![s.body.as_slice(), s.orelse.as_slice()],
        Stmt::While(s) => vec![s.body.as_slice(), s.orelse.as_slice()],
        _ => return None,
    };
    Some(bodies)
}

/// Convert a byte offset into an LSP position, which counts UTF-16 code units.
fn offset_to_position(text: &str, offset: TextSize) -> Value {
    let offset = usize::from(offset).min(text.len());
    let before = &text[..offset];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before.matches('\n').count();
    let character: usize = before[line_start..].chars().map(char::len_utf16).sum();
    json!({"line": line, "character": character})
}

fn position_to_offset(text: &str, position: &LspPosition) -> Option<TextSize> {
    let mut line_start = 0;
    for _ in 0..position.line {
        line_start += text[line_start..].find('\n')? + 1;
    }
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |i| line_start + i);
    let mut units = 0;
    for (index, ch) in text[line_start..line_end].char_indices() {
        if units >= position.character as usize {
            return TextSize::try_from(line_start + index).ok();
        }
        units += ch.len_utf16();
    }
    TextSize::try_from(line_end).ok()
}

fn path_to_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(byte as char)
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

fn uri_to_path(uri: &str) -> Result<PathBuf> {
    let encoded = uri
        .strip_prefix("file://")
        .ok_or_else(|| anyhow!("type server returned non-file URI {uri}"))?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut rest = encoded.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'%' {
            let hex = tail
                .get(..2)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| anyhow!("invalid percent-encoding in URI {uri}"))?;
            bytes.push(hex);
            rest = &tail[2..];
        } else {
            bytes.push(byte);
            rest = tail;
        }
    }
    let path = String::from_utf8(bytes).map_err(|_| anyhow!("URI {uri} is not UTF-8"))?;
    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_round_trip() {
        let path = Path::new("/tmp/with space/caf\u{e9}.py");
        assert_eq!("file:///tmp/with%20space/caf%C3%A9.py", path_to_uri(path));
        assert_eq!(path, uri_to_path(&path_to_uri(path)).unwrap());
    }

    #[test]
    fn test_positions_count_utf16() {
        let text = "a = 1\n\u{1f600}x = 2\n";
        let offset = TextSize::try_from(text.find('x').unwrap()).unwrap();
        assert_eq!(
            json!({"line": 1, "character": 2}),
            offset_to_position(text, offset)
        );
        assert_eq!(
            Some(offset),
            position_to_offset(
                text,
                &LspPosition {
                    line: 1,
                    character: 2
                }
            )
        );
    }

    #[test]
    fn test_version_compatible() {
        assert!(version_compatible("0.4.1"));
        assert!(!version_compatible("0.5.0"));
        assert!(!version_compatible("0.40.0"));
    }
}
