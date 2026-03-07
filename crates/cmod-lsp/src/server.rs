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
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use cmod_build::graph::ModuleGraph;
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
    /// Cached module graph.
    module_graph: Option<ModuleGraph>,
    /// When the cached graph was last built.
    graph_timestamp: Option<Instant>,
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
    /// Whether document symbol (outline) is supported.
    #[serde(rename = "documentSymbolProvider")]
    pub document_symbol_provider: bool,
    /// Whether find references is supported.
    #[serde(rename = "referencesProvider")]
    pub references_provider: bool,
    /// Whether code actions are supported.
    #[serde(rename = "codeActionProvider")]
    pub code_action_provider: bool,
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
                document_symbol_provider: true,
                references_provider: true,
                code_action_provider: true,
            },
            initialized: false,
            shutdown_requested: false,
            module_graph: None,
            graph_timestamp: None,
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
    #[allow(clippy::too_many_lines)]
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
            "textDocument/didChange" => self.handle_did_change(msg.params.as_ref()),
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
            "textDocument/documentSymbol" => {
                let symbols = self.handle_document_symbol(msg.params.as_ref());
                let result = serde_json::to_value(symbols).ok()?;
                Some(vec![make_response(id, Some(result), None)])
            }
            "textDocument/references" => {
                let refs = self.handle_references(msg.params.as_ref());
                let result = serde_json::to_value(refs).ok()?;
                Some(vec![make_response(id, Some(result), None)])
            }
            "textDocument/codeAction" => {
                let actions = self.handle_code_action(msg.params.as_ref());
                let result = serde_json::to_value(actions).ok()?;
                Some(vec![make_response(id, Some(result), None)])
            }
            "cmod/dependencies" => {
                let result = self.handle_dependencies(msg.params.as_ref());
                Some(vec![make_response(id, result, None)])
            }
            "cmod/criticalPath" => {
                let result = self.handle_critical_path();
                Some(vec![make_response(id, result, None)])
            }
            "cmod/cacheStatus" => {
                let result = self.handle_cache_status();
                Some(vec![make_response(id, result, None)])
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

    fn handle_did_change(&self, params: Option<&Value>) -> Option<Vec<JsonRpcMessage>> {
        let params = params?;
        let uri = params
            .get("textDocument")
            .and_then(|d| d.get("uri"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // For full sync, take the last content change
        let text = params
            .get("contentChanges")
            .and_then(|v| v.as_array())
            .and_then(|changes| changes.last())
            .and_then(|last| last.get("text"))
            .and_then(|v| v.as_str());

        if let Some(text) = text {
            if let Ok(mut docs) = self.documents.lock() {
                docs.insert(uri.to_string(), text.to_string());
            }

            // Run lightweight source-level diagnostics on the changed content
            let path = uri_to_path(uri);
            let diagnostics = self.diagnostics.diagnose_source(text, &path);

            let notification = JsonRpcMessage {
                jsonrpc: "2.0".to_string(),
                id: None,
                method: Some("textDocument/publishDiagnostics".to_string()),
                params: Some(serde_json::json!({
                    "uri": uri,
                    "diagnostics": diagnostics,
                })),
                result: None,
                error: None,
            };

            return Some(vec![notification]);
        }

        None
    }

    fn handle_did_save(&mut self, params: Option<&Value>) -> Option<Vec<JsonRpcMessage>> {
        let uri = params?.get("textDocument")?.get("uri")?.as_str()?;

        let path = uri_to_path(uri);

        // If cmod.toml was saved, refresh completion provider and invalidate graph cache
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name == "cmod.toml" {
            if let Some(ref root) = self.root {
                self.completion.set_project_root(root.clone());
            }
            self.module_graph = None;
            self.graph_timestamp = None;
        }

        let mut all_diagnostics = self.diagnostics.diagnose_file(&path);

        // Also check for build log diagnostics from the last build
        if let Some(ref root) = self.root {
            let build_log = root.join("build").join("build.log");
            if build_log.exists() {
                if let Ok(log_content) = std::fs::read_to_string(&build_log) {
                    let clang_diags = crate::diagnostics::parse_clang_diagnostics(&log_content);
                    let by_file = crate::diagnostics::clang_diagnostics_to_lsp(&clang_diags);

                    let file_str = path.to_string_lossy();
                    for (diag_file, diags) in &by_file {
                        if file_str.ends_with(diag_file) {
                            all_diagnostics.extend(diags.iter().cloned());
                        }
                    }
                }
            }
        }

        let mut messages = Vec::new();

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
        messages.push(notification);

        // Propagate diagnostics through module graph
        if !all_diagnostics.is_empty() {
            if let Some(ref root) = self.root.clone() {
                let propagated =
                    crate::diagnostics::propagate_diagnostics(&path, root, &self.module_graph);
                for (dep_path, dep_diags) in propagated {
                    let dep_uri = format!("file://{}", dep_path.display());
                    let prop_notification = JsonRpcMessage {
                        jsonrpc: "2.0".to_string(),
                        id: None,
                        method: Some("textDocument/publishDiagnostics".to_string()),
                        params: Some(serde_json::json!({
                            "uri": dep_uri,
                            "diagnostics": dep_diags,
                        })),
                        result: None,
                        error: None,
                    };
                    messages.push(prop_notification);
                }
            }
        }

        // Emit cmod/buildStatus notification
        if let Some(ref root) = self.root {
            if let Some(status) = build_status_notification(root) {
                messages.push(status);
            }
        }

        Some(messages)
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

    /// Handle `textDocument/documentSymbol` — return outline symbols.
    fn handle_document_symbol(&self, params: Option<&Value>) -> Vec<Value> {
        let uri = match params
            .and_then(|p| p.get("textDocument"))
            .and_then(|d| d.get("uri"))
            .and_then(|v| v.as_str())
        {
            Some(u) => u,
            None => return Vec::new(),
        };

        let content = self
            .documents
            .lock()
            .ok()
            .and_then(|docs| docs.get(uri).cloned())
            .unwrap_or_default();

        extract_document_symbols(&content)
    }

    /// Handle `textDocument/references` — find all importers of the module at cursor.
    fn handle_references(&self, params: Option<&Value>) -> Vec<Value> {
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

        let word = match extract_word_at(&content, line, character) {
            Some(w) => w,
            None => return Vec::new(),
        };

        let module_name = word.trim_end_matches(';');

        // Search open documents for imports of this module
        let mut refs = Vec::new();
        if let Ok(docs) = self.documents.lock() {
            for (doc_uri, doc_content) in docs.iter() {
                for (line_num, line_text) in doc_content.lines().enumerate() {
                    let trimmed = line_text.trim();
                    if trimmed.starts_with("import") && trimmed.contains(module_name) {
                        let import_name = trimmed
                            .strip_prefix("import")
                            .unwrap_or("")
                            .trim()
                            .trim_end_matches(';')
                            .trim();
                        if import_name == module_name {
                            refs.push(serde_json::json!({
                                "uri": doc_uri,
                                "range": {
                                    "start": { "line": line_num, "character": 0 },
                                    "end": { "line": line_num, "character": line_text.len() },
                                }
                            }));
                        }
                    }
                }
            }
        }

        // Also search source files on disk
        if let Some(ref root) = self.root {
            let importers = crate::completion::find_importers(module_name, root);
            for (file_path, file_line) in importers {
                let file_uri = format!("file://{}", file_path.display());
                // Skip if already found in open documents
                if refs.iter().any(|r| r["uri"] == file_uri) {
                    continue;
                }
                refs.push(serde_json::json!({
                    "uri": file_uri,
                    "range": {
                        "start": { "line": file_line, "character": 0 },
                        "end": { "line": file_line, "character": 1 },
                    }
                }));
            }
        }

        refs
    }

    /// Handle `textDocument/codeAction` — return quick-fix code actions.
    fn handle_code_action(&self, params: Option<&Value>) -> Vec<Value> {
        let uri = match params
            .and_then(|p| p.get("textDocument"))
            .and_then(|d| d.get("uri"))
            .and_then(|v| v.as_str())
        {
            Some(u) => u,
            None => return Vec::new(),
        };

        let diagnostics = params
            .and_then(|p| p.get("context"))
            .and_then(|c| c.get("diagnostics"))
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();

        let mut actions = Vec::new();

        for diag in &diagnostics {
            let code = diag.get("code").and_then(|c| c.as_str()).unwrap_or("");
            let message = diag.get("message").and_then(|m| m.as_str()).unwrap_or("");

            match code {
                "cmod-unknown-import" => {
                    // Extract module name from diagnostic message
                    if let Some(module_name) = message
                        .strip_prefix("module '")
                        .and_then(|s| s.split('\'').next())
                    {
                        actions.push(serde_json::json!({
                            "title": format!("Add '{}' to dependencies", module_name),
                            "kind": "quickfix",
                            "diagnostics": [diag],
                            "command": {
                                "title": format!("cmod add {}", module_name),
                                "command": "cmod.add",
                                "arguments": [module_name],
                            }
                        }));
                    }
                }
                "cmod-syntax" => {
                    if message.contains("semicolon") {
                        let range = diag.get("range").cloned().unwrap_or(serde_json::json!({
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 0 },
                        }));
                        let end_line = range
                            .get("end")
                            .and_then(|e| e.get("line"))
                            .and_then(|l| l.as_u64())
                            .unwrap_or(0);
                        // Get the line content to find where to insert semicolon
                        let insert_char = self
                            .documents
                            .lock()
                            .ok()
                            .and_then(|docs| docs.get(uri).cloned())
                            .and_then(|content| {
                                content
                                    .lines()
                                    .nth(end_line as usize)
                                    .map(|l| l.trim_end().len())
                            })
                            .unwrap_or(0);

                        actions.push(serde_json::json!({
                            "title": "Add missing semicolon",
                            "kind": "quickfix",
                            "diagnostics": [diag],
                            "edit": {
                                "changes": {
                                    uri: [{
                                        "range": {
                                            "start": { "line": end_line, "character": insert_char },
                                            "end": { "line": end_line, "character": insert_char },
                                        },
                                        "newText": ";",
                                    }]
                                }
                            }
                        }));
                    }
                }
                _ => {}
            }
        }

        actions
    }

    /// Handle `cmod/dependencies` — return deps/dependents for a module.
    fn handle_dependencies(&mut self, params: Option<&Value>) -> Option<Value> {
        let module_name = params?.get("module")?.as_str()?;
        let graph = self.ensure_graph()?;

        let mut dependencies = Vec::new();
        let mut dependents = Vec::new();

        if let Some(node) = graph.nodes.values().find(|n| n.name == module_name) {
            dependencies = node.imports.clone();
        }

        // Find dependents: nodes that import this module
        for node in graph.nodes.values() {
            if node.imports.contains(&module_name.to_string()) {
                dependents.push(node.name.clone());
            }
        }

        Some(serde_json::json!({
            "dependencies": dependencies,
            "dependents": dependents,
        }))
    }

    /// Handle `cmod/criticalPath` — return the critical path.
    fn handle_critical_path(&mut self) -> Option<Value> {
        let root = self.root.clone()?;
        let graph = self.ensure_graph()?;

        // Load timings from build state if available
        let timings_path = root.join("build").join("timings.json");
        let timings: BTreeMap<String, u64> = std::fs::read_to_string(&timings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let path = graph.critical_path(&timings);
        Some(serde_json::json!({ "criticalPath": path }))
    }

    /// Handle `cmod/cacheStatus` — return cache info.
    fn handle_cache_status(&self) -> Option<Value> {
        let root = self.root.as_ref()?;
        let cache_dir = root.join("build").join("cache");

        let (entries, total_size) = if cache_dir.exists() {
            let mut count = 0u64;
            let mut size = 0u64;
            if let Ok(rd) = std::fs::read_dir(&cache_dir) {
                for entry in rd.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        count += 1;
                        size += meta.len();
                    }
                }
            }
            (count, size)
        } else {
            (0, 0)
        };

        Some(serde_json::json!({
            "entries": entries,
            "totalSizeBytes": total_size,
        }))
    }

    /// Get (or rebuild) the cached module graph. Returns None if no project root.
    fn ensure_graph(&mut self) -> Option<&ModuleGraph> {
        let stale = match self.graph_timestamp {
            Some(ts) => ts.elapsed().as_secs() > 30,
            None => true,
        };

        if stale || self.module_graph.is_none() {
            if let Some(ref root) = self.root.clone() {
                if let Some(graph) = build_module_graph_from_root(root) {
                    self.module_graph = Some(graph);
                    self.graph_timestamp = Some(Instant::now());
                }
            }
        }

        self.module_graph.as_ref()
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

/// Extract document symbols (outline) from C++ module source content.
fn extract_document_symbols(content: &str) -> Vec<Value> {
    let mut symbols = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let line_u32 = line_num as u32;

        // export module <name>;
        if let Some(rest) = trimmed.strip_prefix("export module ") {
            let name = rest.trim_end_matches(';').trim();
            if !name.is_empty() {
                symbols.push(make_symbol(name, 2, line_u32)); // 2 = Module
            }
            continue;
        }

        // import <name>;
        if let Some(rest) = trimmed.strip_prefix("import ") {
            let name = rest.trim_end_matches(';').trim();
            if !name.is_empty() && !name.starts_with('<') && !name.starts_with('"') {
                symbols.push(make_symbol(name, 4, line_u32)); // 4 = Package
            }
            continue;
        }

        // export class|struct <name>
        if let Some(rest) = trimmed
            .strip_prefix("export class ")
            .or_else(|| trimmed.strip_prefix("export struct "))
        {
            let name = rest
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                symbols.push(make_symbol(name, 5, line_u32)); // 5 = Class
            }
            continue;
        }

        // export namespace <name>
        if let Some(rest) = trimmed.strip_prefix("export namespace ") {
            let name = rest
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                symbols.push(make_symbol(name, 3, line_u32)); // 3 = Namespace
            }
            continue;
        }

        // export enum <name>
        if let Some(rest) = trimmed.strip_prefix("export enum ") {
            let name = rest
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                symbols.push(make_symbol(name, 10, line_u32)); // 10 = Enum
            }
            continue;
        }

        // export [qualifiers] <return_type> <name>(  — function
        if trimmed.starts_with("export ") && trimmed.contains('(') {
            // Skip if already matched above
            if !trimmed.contains("module ")
                && !trimmed.contains("class ")
                && !trimmed.contains("struct ")
                && !trimmed.contains("namespace ")
                && !trimmed.contains("enum ")
                && !trimmed.contains("import ")
            {
                // Find function name: the token before '('
                if let Some(paren_idx) = trimmed.find('(') {
                    let before_paren = &trimmed[..paren_idx].trim_end();
                    let func_name = before_paren
                        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or("");
                    if !func_name.is_empty() {
                        symbols.push(make_symbol(func_name, 12, line_u32)); // 12 = Function
                    }
                }
            }
        }
    }

    symbols
}

fn make_symbol(name: &str, kind: u8, line: u32) -> Value {
    serde_json::json!({
        "name": name,
        "kind": kind,
        "range": {
            "start": { "line": line, "character": 0 },
            "end": { "line": line, "character": 0 },
        },
        "selectionRange": {
            "start": { "line": line, "character": 0 },
            "end": { "line": line, "character": 0 },
        },
    })
}

/// Build a `ModuleGraph` from the project root by scanning sources.
fn build_module_graph_from_root(root: &std::path::Path) -> Option<ModuleGraph> {
    let manifest_path = root.join("cmod.toml");
    let content = std::fs::read_to_string(&manifest_path).ok()?;
    let manifest = cmod_core::manifest::Manifest::from_str(&content).ok()?;

    let pkg_name = &manifest.package.name;

    let src_dirs: Vec<PathBuf> = manifest
        .build
        .as_ref()
        .map(|b| {
            if b.sources.is_empty() {
                vec![root.join("src")]
            } else {
                b.sources.iter().map(|s| root.join(s)).collect()
            }
        })
        .unwrap_or_else(|| vec![root.join("src")]);
    let exclude = manifest
        .build
        .as_ref()
        .map(|b| b.exclude.clone())
        .unwrap_or_default();

    let sources = cmod_build::runner::discover_sources_multi(&src_dirs, &exclude).ok()?;

    let mut graph = ModuleGraph::new();
    for source in &sources {
        let kind = cmod_build::runner::classify_source(source)
            .unwrap_or(cmod_core::types::ModuleUnitKind::LegacyUnit);
        let module_name = cmod_build::runner::extract_module_name(source)
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                source
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });
        let partition_of = cmod_build::runner::extract_partition_owner(source)
            .ok()
            .flatten();

        let node = cmod_build::graph::ModuleNode {
            id: source.display().to_string(),
            name: module_name,
            kind,
            source: source.clone(),
            package: pkg_name.clone(),
            imports: Vec::new(), // simplified — we don't parse imports in LSP graph
            partition_of,
        };
        graph.add_node(node);
    }

    Some(graph)
}

/// Build the `cmod/buildStatus` notification from build state on disk.
fn build_status_notification(root: &std::path::Path) -> Option<JsonRpcMessage> {
    let build_dir = root.join("build");
    if !build_dir.exists() {
        return None;
    }

    // Try to load build state JSON
    let state_path = build_dir.join("build_state.json");
    let build_state: Option<BTreeMap<String, Value>> = std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());

    let mut modules = Vec::new();
    let (mut total, mut up_to_date, mut needs_rebuild, mut never_built) = (0u32, 0u32, 0u32, 0u32);

    // Scan source files to enumerate modules
    let manifest_path = root.join("cmod.toml");
    if let Ok(content) = std::fs::read_to_string(&manifest_path) {
        if let Ok(manifest) = cmod_core::manifest::Manifest::from_str(&content) {
            let src_dirs: Vec<PathBuf> = manifest
                .build
                .as_ref()
                .map(|b| {
                    if b.sources.is_empty() {
                        vec![root.join("src")]
                    } else {
                        b.sources.iter().map(|s| root.join(s)).collect()
                    }
                })
                .unwrap_or_else(|| vec![root.join("src")]);
            let exclude = manifest
                .build
                .as_ref()
                .map(|b| b.exclude.clone())
                .unwrap_or_default();

            if let Ok(sources) = cmod_build::runner::discover_sources_multi(&src_dirs, &exclude) {
                for source in &sources {
                    if let Ok(Some(name)) = cmod_build::runner::extract_module_name(source) {
                        total += 1;
                        let status = if let Some(ref state) = build_state {
                            if state.contains_key(&name) {
                                up_to_date += 1;
                                "up-to-date"
                            } else {
                                needs_rebuild += 1;
                                "needs-rebuild"
                            }
                        } else {
                            never_built += 1;
                            "never-built"
                        };

                        modules.push(serde_json::json!({
                            "name": name,
                            "status": status,
                        }));
                    }
                }
            }
        }
    }

    Some(JsonRpcMessage {
        jsonrpc: "2.0".to_string(),
        id: None,
        method: Some("cmod/buildStatus".to_string()),
        params: Some(serde_json::json!({
            "modules": modules,
            "summary": {
                "total": total,
                "upToDate": up_to_date,
                "needsRebuild": needs_rebuild,
                "neverBuilt": never_built,
            }
        })),
        result: None,
        error: None,
    })
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
    fn test_did_change_publishes_diagnostics() {
        let mut server = LspServer::new();
        // First open a file
        let open_params = serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/test.cppm",
                "text": "export module test;\n"
            }
        });
        server.handle_did_open(Some(&open_params));

        // Now change to have a duplicate module declaration
        let change_msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: None,
            method: Some("textDocument/didChange".into()),
            params: Some(serde_json::json!({
                "textDocument": { "uri": "file:///tmp/test.cppm" },
                "contentChanges": [{
                    "text": "export module test;\nexport module other;\n"
                }]
            })),
            result: None,
            error: None,
        };

        let responses = server.handle_message(change_msg);
        assert!(responses.is_some());
        let msgs = responses.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0].method.as_deref(),
            Some("textDocument/publishDiagnostics")
        );
        // Should contain a duplicate-module diagnostic
        let diags = msgs[0].params.as_ref().unwrap()["diagnostics"]
            .as_array()
            .unwrap();
        assert!(diags.iter().any(|d| d["code"] == "cmod-duplicate-module"));
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

    // --- 5B-1: Document Symbol tests ---

    #[test]
    fn test_document_symbol_empty_file() {
        let symbols = extract_document_symbols("");
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_document_symbol_module_and_imports() {
        let content = "export module mylib;\nimport std;\nimport fmt;\n";
        let symbols = extract_document_symbols(content);
        assert_eq!(symbols.len(), 3);
        // Module declaration
        assert_eq!(symbols[0]["name"], "mylib");
        assert_eq!(symbols[0]["kind"], 2); // Module
                                           // Imports
        assert_eq!(symbols[1]["name"], "std");
        assert_eq!(symbols[1]["kind"], 4); // Package
        assert_eq!(symbols[2]["name"], "fmt");
        assert_eq!(symbols[2]["kind"], 4);
    }

    #[test]
    fn test_document_symbol_exports() {
        let content = concat!(
            "export module test;\n",
            "export class Widget {\n};\n",
            "export namespace utils {\n};\n",
            "export enum Color {\n};\n",
            "export int compute(int x) {\n}\n",
        );
        let symbols = extract_document_symbols(content);
        let names: Vec<&str> = symbols
            .iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"test"));
        assert!(names.contains(&"Widget"));
        assert!(names.contains(&"utils"));
        assert!(names.contains(&"Color"));
        assert!(names.contains(&"compute"));
    }

    // --- 5B-2: References tests ---

    #[test]
    fn test_references_no_refs() {
        let mut server = LspServer::new();
        let open_params = serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/a.cppm",
                "text": "export module mymod;\n"
            }
        });
        server.handle_did_open(Some(&open_params));

        let msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(2.into())),
            method: Some("textDocument/references".into()),
            params: Some(serde_json::json!({
                "textDocument": { "uri": "file:///tmp/a.cppm" },
                "position": { "line": 0, "character": 14 },
                "context": { "includeDeclaration": true }
            })),
            result: None,
            error: None,
        };

        let responses = server.handle_message(msg).unwrap();
        let refs = responses[0].result.as_ref().unwrap().as_array().unwrap();
        // No other files import "mymod"
        assert!(refs.is_empty());
    }

    #[test]
    fn test_references_in_open_docs() {
        let mut server = LspServer::new();
        // Open module file
        server.handle_did_open(Some(&serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/lib.cppm",
                "text": "export module mylib;\n"
            }
        })));
        // Open a file that imports it
        server.handle_did_open(Some(&serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/main.cppm",
                "text": "import mylib;\nint main() {}\n"
            }
        })));

        let msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(3.into())),
            method: Some("textDocument/references".into()),
            params: Some(serde_json::json!({
                "textDocument": { "uri": "file:///tmp/lib.cppm" },
                "position": { "line": 0, "character": 14 },
                "context": { "includeDeclaration": true }
            })),
            result: None,
            error: None,
        };

        let responses = server.handle_message(msg).unwrap();
        let refs = responses[0].result.as_ref().unwrap().as_array().unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["uri"], "file:///tmp/main.cppm");
    }

    #[test]
    fn test_references_on_module_declaration() {
        let mut server = LspServer::new();
        server.handle_did_open(Some(&serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/mod.cppm",
                "text": "export module mymod;\nexport int foo() { return 1; }\n"
            }
        })));
        server.handle_did_open(Some(&serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/user.cppm",
                "text": "import mymod;\nint x = foo();\n"
            }
        })));

        let msg = JsonRpcMessage {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(4.into())),
            method: Some("textDocument/references".into()),
            params: Some(serde_json::json!({
                "textDocument": { "uri": "file:///tmp/mod.cppm" },
                "position": { "line": 0, "character": 14 },
                "context": { "includeDeclaration": true }
            })),
            result: None,
            error: None,
        };

        let responses = server.handle_message(msg).unwrap();
        let refs = responses[0].result.as_ref().unwrap().as_array().unwrap();
        assert_eq!(refs.len(), 1);
    }

    // --- 5C-1: Build status tests ---

    #[test]
    fn test_build_status_no_build_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        // No build/ dir → None
        let result = build_status_notification(tmp.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_build_status_with_build_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("build")).unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("cmod.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("src").join("lib.cppm"),
            "export module test;\n",
        )
        .unwrap();

        let result = build_status_notification(tmp.path());
        assert!(result.is_some());
        let msg = result.unwrap();
        assert_eq!(msg.method.as_deref(), Some("cmod/buildStatus"));
        let params = msg.params.unwrap();
        let summary = &params["summary"];
        assert_eq!(summary["total"], 1);
        assert_eq!(summary["neverBuilt"], 1);
    }

    // --- 5C-2: Graph query tests ---

    #[test]
    fn test_handle_dependencies_empty() {
        let mut server = LspServer::new();
        // No root set, so graph won't build
        let result = server.handle_dependencies(Some(&serde_json::json!({
            "module": "nonexistent"
        })));
        assert!(result.is_none());
    }

    #[test]
    fn test_handle_critical_path_no_root() {
        let mut server = LspServer::new();
        let result = server.handle_critical_path();
        assert!(result.is_none());
    }

    #[test]
    fn test_handle_cache_status_no_root() {
        let server = LspServer::new();
        let result = server.handle_cache_status();
        assert!(result.is_none());
    }

    // --- 5C-4: Graph caching tests ---

    #[test]
    fn test_graph_caching_returns_same() {
        let mut server = LspServer::new();
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("cmod.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("src").join("lib.cppm"),
            "export module test;\n",
        )
        .unwrap();

        server.root = Some(tmp.path().to_path_buf());

        // First call builds the graph
        let graph1 = server.ensure_graph();
        assert!(graph1.is_some());

        // Immediate re-request should use cached graph (timestamp < 30s)
        assert!(server.graph_timestamp.is_some());
        let graph2 = server.ensure_graph();
        assert!(graph2.is_some());
    }

    // --- 5D-1: Code Action tests ---

    #[test]
    fn test_code_action_no_diagnostics() {
        let server = LspServer::new();
        let params = serde_json::json!({
            "textDocument": { "uri": "file:///tmp/test.cppm" },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 0 },
            },
            "context": { "diagnostics": [] }
        });

        let actions = server.handle_code_action(Some(&params));
        assert!(actions.is_empty());
    }

    #[test]
    fn test_code_action_unknown_import() {
        let server = LspServer::new();
        let params = serde_json::json!({
            "textDocument": { "uri": "file:///tmp/test.cppm" },
            "range": {
                "start": { "line": 1, "character": 0 },
                "end": { "line": 1, "character": 1 },
            },
            "context": {
                "diagnostics": [{
                    "range": {
                        "start": { "line": 1, "character": 0 },
                        "end": { "line": 1, "character": 1 },
                    },
                    "severity": 2,
                    "code": "cmod-unknown-import",
                    "message": "module 'github.fmtlib.fmt' not found in dependencies; add with `cmod add`",
                }]
            }
        });

        let actions = server.handle_code_action(Some(&params));
        assert_eq!(actions.len(), 1);
        assert!(actions[0]["title"]
            .as_str()
            .unwrap()
            .contains("github.fmtlib.fmt"));
    }

    #[test]
    fn test_code_action_missing_semicolon() {
        let server = LspServer::new();
        server.handle_did_open(Some(&serde_json::json!({
            "textDocument": {
                "uri": "file:///tmp/fix.cppm",
                "text": "export module test\n"
            }
        })));

        let params = serde_json::json!({
            "textDocument": { "uri": "file:///tmp/fix.cppm" },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 },
            },
            "context": {
                "diagnostics": [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 1 },
                    },
                    "severity": 1,
                    "code": "cmod-syntax",
                    "message": "module declaration should end with semicolon",
                }]
            }
        });

        let actions = server.handle_code_action(Some(&params));
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["title"], "Add missing semicolon");
    }

    // --- 5D-2: Completion refresh on manifest save ---

    #[test]
    fn test_manifest_save_triggers_rescan() {
        let mut server = LspServer::new();
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("cmod.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        server.root = Some(tmp.path().to_path_buf());
        server.completion.set_project_root(tmp.path().to_path_buf());

        // Set a cached graph
        server.module_graph = Some(ModuleGraph::new());
        server.graph_timestamp = Some(Instant::now());

        // Simulate saving cmod.toml
        let save_params = serde_json::json!({
            "textDocument": {
                "uri": format!("file://{}/cmod.toml", tmp.path().display()),
            }
        });
        server.handle_did_save(Some(&save_params));

        // Graph cache should be invalidated
        assert!(server.module_graph.is_none());
        assert!(server.graph_timestamp.is_none());
    }
}
