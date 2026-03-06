//! LSP server implementation using JSON-RPC over stdio.
//!
//! Implements the core Language Server Protocol lifecycle:
//! - `initialize` / `initialized` / `shutdown`
//! - `textDocument/didOpen` / `textDocument/didChange` / `textDocument/didSave`
//! - `textDocument/completion`
//! - `textDocument/diagnostic`
//! - Custom `cmod/buildStatus` notifications

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use cmod_core::error::CmodError;

use crate::completion::CompletionProvider;
use crate::diagnostics::DiagnosticsEngine;

/// LSP server state.
pub struct LspServer {
    /// Project root directory.
    root: Option<PathBuf>,
    /// Open documents (URI → content).
    documents: Arc<Mutex<BTreeMap<String, String>>>,
    /// Completion provider.
    completion: CompletionProvider,
    /// Diagnostics engine.
    diagnostics: DiagnosticsEngine,
    /// Server capabilities.
    capabilities: ServerCapabilities,
    /// Whether the server has been initialized.
    initialized: bool,
    /// Whether shutdown has been requested.
    shutdown_requested: bool,
}

/// Server capabilities advertised to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Text document sync kind (1 = full, 2 = incremental).
    #[serde(rename = "textDocumentSync")]
    pub text_document_sync: u8,
    /// Whether completion is supported.
    #[serde(rename = "completionProvider")]
    pub completion_provider: Option<CompletionOptions>,
    /// Whether diagnostics are supported.
    #[serde(rename = "diagnosticProvider")]
    pub diagnostic_provider: Option<DiagnosticOptions>,
    /// Whether hover is supported.
    #[serde(rename = "hoverProvider")]
    pub hover_provider: bool,
    /// Whether go-to-definition is supported.
    #[serde(rename = "definitionProvider")]
    pub definition_provider: bool,
}

/// Completion provider options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionOptions {
    /// Trigger characters for completion.
    #[serde(rename = "triggerCharacters")]
    pub trigger_characters: Vec<String>,
    /// Whether the server can resolve completion items.
    #[serde(rename = "resolveProvider")]
    pub resolve_provider: bool,
}

/// Diagnostic provider options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticOptions {
    /// Unique identifier for diagnostics.
    pub identifier: String,
    /// Whether diagnostics relate to inter-file state.
    #[serde(rename = "interFileDependencies")]
    pub inter_file_dependencies: bool,
    /// Whether the server supports workspace diagnostics.
    #[serde(rename = "workspaceDiagnostics")]
    pub workspace_diagnostics: bool,
}

/// A JSON-RPC message (request, response, or notification).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl LspServer {
    /// Create a new LSP server.
    pub fn new() -> Self {
        LspServer {
            root: None,
            documents: Arc::new(Mutex::new(BTreeMap::new())),
            completion: CompletionProvider::new(),
            diagnostics: DiagnosticsEngine::new(),
            capabilities: ServerCapabilities {
                text_document_sync: 1, // Full sync
                completion_provider: Some(CompletionOptions {
                    trigger_characters: vec![".".into(), ":".into(), "<".into(), "\"".into()],
                    resolve_provider: false,
                }),
                diagnostic_provider: Some(DiagnosticOptions {
                    identifier: "cmod".into(),
                    inter_file_dependencies: true,
                    workspace_diagnostics: true,
                }),
                hover_provider: true,
                definition_provider: true,
            },
            initialized: false,
            shutdown_requested: false,
        }
    }

    /// Run the server main loop, reading from stdin and writing to stdout.
    pub fn run(&mut self) -> Result<(), CmodError> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut reader = stdin.lock();
        let mut writer = stdout.lock();

        loop {
            match read_message(&mut reader) {
                Ok(Some(msg)) => {
                    if let Some(responses) = self.handle_message(msg) {
                        for response in responses {
                            write_message(&mut writer, &response)?;
                        }
                    }
                    if self.shutdown_requested {
                        break;
                    }
                }
                Ok(None) => break, // EOF
                Err(e) => {
                    eprintln!("LSP read error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a single JSON-RPC message.
    pub fn handle_message(&mut self, msg: JsonRpcMessage) -> Option<Vec<JsonRpcMessage>> {
        let method = msg.method.as_deref()?;
        let id = msg.id.clone();

        match method {
            "initialize" => {
                self.handle_initialize(msg.params.as_ref());
                let result = serde_json::json!({
                    "capabilities": self.capabilities,
                    "serverInfo": {
                        "name": "cmod-lsp",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                });
                Some(vec![make_response(id, Some(result), None)])
            }
            "initialized" => {
                self.initialized = true;
                None
            }
            "shutdown" => {
                self.shutdown_requested = true;
                Some(vec![make_response(id, Some(Value::Null), None)])
            }
            "exit" => {
                self.shutdown_requested = true;
                None
            }
            "textDocument/didOpen" => {
                self.handle_did_open(msg.params.as_ref());
                None
            }
            "textDocument/didChange" => {
                self.handle_did_change(msg.params.as_ref());
                None
            }
            "textDocument/didSave" => self.handle_did_save(msg.params.as_ref()),
            "textDocument/completion" => {
                let items = self.handle_completion(msg.params.as_ref());
                let result = serde_json::to_value(items).ok()?;
                Some(vec![make_response(id, Some(result), None)])
            }
            "textDocument/hover" => {
                let hover = self.handle_hover(msg.params.as_ref());
                Some(vec![make_response(id, hover, None)])
            }
            "textDocument/definition" => {
                let location = self.handle_definition(msg.params.as_ref());
                Some(vec![make_response(id, location, None)])
            }
            _ => {
                // Method not found
                if id.is_some() {
                    Some(vec![make_response(
                        id,
                        None,
                        Some(JsonRpcError {
                            code: -32601,
                            message: format!("method not found: {}", method),
                            data: None,
                        }),
                    )])
                } else {
                    None
                }
            }
        }
    }

    fn handle_initialize(&mut self, params: Option<&Value>) {
        if let Some(params) = params {
            if let Some(root_uri) = params.get("rootUri").and_then(|v| v.as_str()) {
                // Convert file:// URI to path
                let path = uri_to_path(root_uri);
                self.root = Some(path.clone());
                self.completion.set_project_root(path.clone());
                self.diagnostics.set_project_root(path);
            }
        }
    }

    fn handle_did_open(&self, params: Option<&Value>) {
        if let Some(params) = params {
            if let Some(doc) = params.get("textDocument") {
                let uri = doc.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                let text = doc.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if let Ok(mut docs) = self.documents.lock() {
                    docs.insert(uri.to_string(), text.to_string());
                }
            }
        }
    }

    fn handle_did_change(&self, params: Option<&Value>) {
        if let Some(params) = params {
            let uri = params
                .get("textDocument")
                .and_then(|d| d.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // For full sync, take the last content change
            if let Some(changes) = params.get("contentChanges").and_then(|v| v.as_array()) {
                if let Some(last) = changes.last() {
                    if let Some(text) = last.get("text").and_then(|v| v.as_str()) {
                        if let Ok(mut docs) = self.documents.lock() {
                            docs.insert(uri.to_string(), text.to_string());
                        }
                    }
                }
            }
        }
    }

    fn handle_did_save(&self, params: Option<&Value>) -> Option<Vec<JsonRpcMessage>> {
        let uri = params?.get("textDocument")?.get("uri")?.as_str()?;

        let path = uri_to_path(uri);
        let mut all_diagnostics = self.diagnostics.diagnose_file(&path);

        // Also check for build log diagnostics from the last build
        if let Some(ref root) = self.root {
            let build_log = root.join("build").join("build.log");
            if build_log.exists() {
                if let Ok(log_content) = std::fs::read_to_string(&build_log) {
                    let clang_diags = crate::diagnostics::parse_clang_diagnostics(&log_content);
                    let by_file = crate::diagnostics::clang_diagnostics_to_lsp(&clang_diags);

                    // Get the file path relative to root for matching
                    let file_str = path.to_string_lossy();
                    for (diag_file, diags) in &by_file {
                        if file_str.ends_with(diag_file) {
                            all_diagnostics.extend(diags.iter().cloned());
                        }
                    }
                }
            }
        }

        let notification = JsonRpcMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some("textDocument/publishDiagnostics".to_string()),
            params: Some(serde_json::json!({
                "uri": uri,
                "diagnostics": all_diagnostics,
            })),
            result: None,
            error: None,
        };

        Some(vec![notification])
    }

    fn handle_completion(&self, params: Option<&Value>) -> Vec<Value> {
        let uri = params
            .and_then(|p| p.get("textDocument"))
            .and_then(|d| d.get("uri"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let content = self
            .documents
            .lock()
            .ok()
            .and_then(|docs| docs.get(uri).cloned())
            .unwrap_or_default();

        let line = params
            .and_then(|p| p.get("position"))
            .and_then(|pos| pos.get("line"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let character = params
            .and_then(|p| p.get("position"))
            .and_then(|pos| pos.get("character"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        self.completion.complete(&content, line, character)
    }

    fn handle_hover(&self, params: Option<&Value>) -> Option<Value> {
        let uri = params?.get("textDocument")?.get("uri")?.as_str()?;

        let content = self
            .documents
            .lock()
            .ok()
            .and_then(|docs| docs.get(uri).cloned())?;

        let line = params?.get("position")?.get("line")?.as_u64()? as usize;

        let character = params?.get("position")?.get("character")?.as_u64()? as usize;

        let word = extract_word_at(&content, line, character)?;

        // Check if it's a module import
        let lines: Vec<&str> = content.lines().collect();
        if line < lines.len() {
            let current_line = lines[line];
            if current_line.contains("import") {
                let module_name = word.trim_end_matches(';');

                // Try to find rich metadata from known modules
                if let Some(info) = self.completion.find_module_info(module_name) {
                    let mut hover_parts = vec![format!("**Module:** `{}`", info.name)];

                    if let Some(ref ver) = info.version {
                        hover_parts.push(format!("**Version:** {}", ver));
                    }

                    if let Some(ref desc) = info.description {
                        hover_parts.push(desc.clone());
                    }

                    if let Some(ref repo) = info.repository {
                        hover_parts.push(format!("**Source:** {}", repo));
                    }

                    if info.is_local {
                        hover_parts.push("*Local module*".to_string());
                    }

                    if !info.partitions.is_empty() {
                        hover_parts.push(format!("**Partitions:** {}", info.partitions.join(", ")));
                    }

                    return Some(serde_json::json!({
                        "contents": {
                            "kind": "markdown",
                            "value": hover_parts.join("\n\n"),
                        }
                    }));
                }

                return Some(serde_json::json!({
                    "contents": {
                        "kind": "markdown",
                        "value": format!("**Module:** `{}`\n\nImported C++20 module", word),
                    }
                }));
            }
        }

        None
    }

    fn handle_definition(&self, params: Option<&Value>) -> Option<Value> {
        let uri = params?.get("textDocument")?.get("uri")?.as_str()?;
        let content = self
            .documents
            .lock()
            .ok()
            .and_then(|docs| docs.get(uri).cloned())?;

        let line = params?.get("position")?.get("line")?.as_u64()? as usize;
        let character = params?.get("position")?.get("character")?.as_u64()? as usize;

        let word = extract_word_at(&content, line, character)?;

        // Check if this is an import line
        let lines: Vec<&str> = content.lines().collect();
        if line >= lines.len() {
            return None;
        }
        let current_line = lines[line];
        if !current_line.contains("import") {
            return None;
        }

        // Look up module in known modules
        let module_name = word.trim_end_matches(';');
        if let Some(root_path) = self.completion.find_module_root(module_name) {
            let target_uri = format!("file://{}", root_path.display());
            return Some(serde_json::json!({
                "uri": target_uri,
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 0 },
                }
            }));
        }

        None
    }
}

impl Default for LspServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a single LSP message from the input stream.
pub fn read_message(reader: &mut impl BufRead) -> Result<Option<JsonRpcMessage>, CmodError> {
    // Read headers
    let mut content_length: Option<usize> = None;
    let mut header_line = String::new();

    loop {
        header_line.clear();
        let bytes_read = reader.read_line(&mut header_line)?;
        if bytes_read == 0 {
            return Ok(None); // EOF
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break; // End of headers
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str.parse().ok();
        }
    }

    let length = content_length
        .ok_or_else(|| CmodError::Other("missing Content-Length header".to_string()))?;

    // Read body
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    let msg: JsonRpcMessage = serde_json::from_slice(&body)
        .map_err(|e| CmodError::Other(format!("invalid JSON-RPC message: {}", e)))?;

    Ok(Some(msg))
}

/// Write an LSP message to the output stream.
pub fn write_message(writer: &mut impl Write, msg: &JsonRpcMessage) -> Result<(), CmodError> {
    let body = serde_json::to_string(msg)
        .map_err(|e| CmodError::Other(format!("failed to serialize response: {}", e)))?;

    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    writer.flush()?;

    Ok(())
}

fn make_response(
    id: Option<Value>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
) -> JsonRpcMessage {
    JsonRpcMessage {
        jsonrpc: "2.0".to_string(),
        id,
        method: None,
        params: None,
        result,
        error,
    }
}

fn uri_to_path(uri: &str) -> PathBuf {
    let path_str = uri.strip_prefix("file://").unwrap_or(uri);
    PathBuf::from(path_str)
}

fn extract_word_at(content: &str, line: usize, character: usize) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let current_line = lines.get(line)?;

    let chars: Vec<char> = current_line.chars().collect();
    if character > chars.len() {
        return None;
    }

    let mut start = character;
    while start > 0
        && (chars[start - 1].is_alphanumeric()
            || chars[start - 1] == '_'
            || chars[start - 1] == '.')
    {
        start -= 1;
    }

    let mut end = character;
    while end < chars.len()
        && (chars[end].is_alphanumeric() || chars[end] == '_' || chars[end] == '.')
    {
        end += 1;
    }

    if start == end {
        return None;
    }

    Some(chars[start..end].iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let server = LspServer::new();
        assert!(!server.initialized);
        assert!(!server.shutdown_requested);
    }

    #[test]
    fn test_handle_initialize() {
        let mut server = LspServer::new();
        let msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: Some("initialize".into()),
            params: Some(serde_json::json!({
                "rootUri": "file:///tmp/test",
                "capabilities": {},
            })),
            result: None,
            error: None,
        };

        let responses = server.handle_message(msg).unwrap();
        assert_eq!(responses.len(), 1);
        assert!(responses[0].result.is_some());
    }

    #[test]
    fn test_handle_shutdown() {
        let mut server = LspServer::new();
        let msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: Some("shutdown".into()),
            params: None,
            result: None,
            error: None,
        };

        server.handle_message(msg);
        assert!(server.shutdown_requested);
    }

    #[test]
    fn test_handle_unknown_method() {
        let mut server = LspServer::new();
        let msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: Some("unknownMethod".into()),
            params: None,
            result: None,
            error: None,
        };

        let responses = server.handle_message(msg).unwrap();
        assert!(responses[0].error.is_some());
    }

    #[test]
    fn test_uri_to_path() {
        assert_eq!(uri_to_path("file:///tmp/test"), PathBuf::from("/tmp/test"));
        assert_eq!(uri_to_path("/tmp/test"), PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_extract_word_at() {
        let content = "import my.module;\nint x = 42;";
        assert_eq!(
            extract_word_at(content, 0, 7),
            Some("my.module".to_string())
        );
        assert_eq!(extract_word_at(content, 1, 4), Some("x".to_string()));
    }

    #[test]
    fn test_json_rpc_serde() {
        let msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: Some("test".into()),
            params: None,
            result: None,
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: JsonRpcMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.method.as_deref(), Some("test"));
    }

    #[test]
    fn test_did_open_stores_document() {
        let server = LspServer::new();
        let params = serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/test.cpp",
                "text": "import std;\nint main() {}"
            }
        });
        server.handle_did_open(Some(&params));

        let docs = server.documents.lock().unwrap();
        assert!(docs.contains_key("file:///tmp/test.cpp"));
    }
}
