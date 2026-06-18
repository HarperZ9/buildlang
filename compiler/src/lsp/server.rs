// ===============================================================================
// QUANTALANG LSP SERVER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Main LSP server implementation.

use super::completion::CompletionProvider;
use super::diagnostics::DiagnosticsProvider;
use super::document::DocumentStore;
use super::hover::HoverProvider;
use super::jsonrpc::JsonRpcMessage;
use super::message::*;
use super::raw_params;
use super::response_json;
use super::semantic_tokens::{SemanticTokens, SemanticTokensProvider};
use super::symbols::SymbolProvider;
use super::transport::*;
use super::types::*;
use super::workspace_index::{WorkspaceSymbolIndex, WorkspaceSymbolIndexStats};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// =============================================================================
// SERVER STATE
// =============================================================================

/// Server state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    /// Not initialized.
    Uninitialized,
    /// Initializing.
    Initializing,
    /// Running.
    Running,
    /// Shutting down.
    ShuttingDown,
    /// Shut down.
    Shutdown,
}

// =============================================================================
// LSP SERVER
// =============================================================================

/// The QuantaLang Language Server.
pub struct LanguageServer {
    /// Server state.
    state: ServerState,
    /// Shutdown flag.
    shutdown: AtomicBool,
    /// Document store.
    documents: Arc<DocumentStore>,
    /// Client capabilities.
    client_capabilities: Option<ClientCapabilities>,
    /// Root URI.
    root_uri: Option<DocumentUri>,
    /// Completion provider.
    completion: CompletionProvider,
    /// Hover provider.
    hover: HoverProvider,
    /// Diagnostics provider.
    diagnostics: DiagnosticsProvider,
    /// Symbol provider.
    symbols: SymbolProvider,
    /// Root-backed workspace symbol index.
    workspace_index: WorkspaceSymbolIndex,
    /// Semantic tokens provider.
    semantic_tokens: SemanticTokensProvider,
}

impl LanguageServer {
    /// Create a new language server.
    pub fn new() -> Self {
        let documents = Arc::new(DocumentStore::new());
        Self {
            state: ServerState::Uninitialized,
            shutdown: AtomicBool::new(false),
            documents: documents.clone(),
            client_capabilities: None,
            root_uri: None,
            completion: CompletionProvider::new(documents.clone()),
            hover: HoverProvider::new(documents.clone()),
            diagnostics: DiagnosticsProvider::new(documents.clone()),
            symbols: SymbolProvider::new(documents.clone()),
            workspace_index: WorkspaceSymbolIndex::new(),
            semantic_tokens: SemanticTokensProvider::new(),
        }
    }

    /// Get server state.
    pub fn state(&self) -> ServerState {
        self.state
    }

    /// Check if server should shutdown.
    pub fn should_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Get document store.
    pub fn documents(&self) -> &Arc<DocumentStore> {
        &self.documents
    }

    // =========================================================================
    // LIFECYCLE
    // =========================================================================

    /// Handle initialize request.
    pub fn initialize(&mut self, params: InitializeParams) -> InitializeResult {
        self.state = ServerState::Initializing;
        self.client_capabilities = Some(params.capabilities);
        self.root_uri = params.root_uri.clone();
        self.workspace_index
            .rebuild_from_uri(params.root_uri.as_deref(), &self.symbols);

        InitializeResult {
            capabilities: ServerCapabilities::full(),
            server_info: Some(ServerInfo {
                name: "quantalang-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        }
    }

    /// Handle initialized notification.
    pub fn initialized(&mut self) {
        self.state = ServerState::Running;
    }

    /// Handle shutdown request.
    pub fn shutdown(&mut self) {
        self.state = ServerState::ShuttingDown;
        self.shutdown.store(true, Ordering::Release);
    }

    /// Handle exit notification.
    pub fn exit(&mut self) {
        self.state = ServerState::Shutdown;
    }

    // =========================================================================
    // TEXT DOCUMENT
    // =========================================================================

    /// Handle didOpen notification.
    pub fn did_open(
        &mut self,
        params: DidOpenTextDocumentParams,
    ) -> Option<PublishDiagnosticsParams> {
        let doc = self.documents.open(params.text_document);
        Some(self.diagnostics.compute(&doc))
    }

    /// Handle didChange notification.
    pub fn did_change(
        &mut self,
        params: DidChangeTextDocumentParams,
    ) -> Option<PublishDiagnosticsParams> {
        let doc = self.documents.update(
            &params.text_document.uri,
            params.text_document.version,
            &params.content_changes,
        )?;
        Some(self.diagnostics.compute(&doc))
    }

    /// Handle didSave notification.
    pub fn did_save(
        &mut self,
        params: DidSaveTextDocumentParams,
    ) -> Option<PublishDiagnosticsParams> {
        let doc = self.documents.get(&params.text_document.uri)?;
        Some(self.diagnostics.compute(&doc))
    }

    /// Handle didClose notification.
    pub fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        self.documents.close(&params.text_document.uri);
    }

    // =========================================================================
    // LANGUAGE FEATURES
    // =========================================================================

    /// Handle completion request.
    pub fn completion(&self, params: CompletionParams) -> Option<CompletionList> {
        let doc = self
            .documents
            .get(&params.text_document_position.text_document.uri)?;
        Some(
            self.completion
                .provide(&doc, params.text_document_position.position),
        )
    }

    /// Handle hover request.
    pub fn hover(&self, params: TextDocumentPositionParams) -> Option<Hover> {
        let doc = self.documents.get(&params.text_document.uri)?;
        self.hover.provide(&doc, params.position)
    }

    /// Handle definition request.
    pub fn definition(&self, params: TextDocumentPositionParams) -> Vec<Location> {
        let Some(doc) = self.documents.get(&params.text_document.uri) else {
            return Vec::new();
        };

        // Get the word at the cursor position
        let Some((word, _word_range)) = doc.word_at(params.position) else {
            return Vec::new();
        };

        // Search document symbols for a matching definition
        let doc_symbols = self.symbols.document_symbols(&doc);
        let mut locations = Vec::new();

        self.find_definition_in_symbols(&word, &doc_symbols, &doc.uri, &mut locations);

        // If no symbol found in current document, search workspace
        if locations.is_empty() {
            for uri in self.documents.uris() {
                if uri == doc.uri {
                    continue;
                }
                if let Some(other_doc) = self.documents.get(&uri) {
                    let other_symbols = self.symbols.document_symbols(&other_doc);
                    self.find_definition_in_symbols(&word, &other_symbols, &uri, &mut locations);
                }
            }
        }

        locations
    }

    /// Find definition matching name in symbol tree.
    fn find_definition_in_symbols(
        &self,
        name: &str,
        symbols: &[DocumentSymbol],
        uri: &str,
        locations: &mut Vec<Location>,
    ) {
        for symbol in symbols {
            if symbol.name == name {
                locations.push(Location::new(uri.to_string(), symbol.selection_range));
            }
            // Recurse into children
            self.find_definition_in_symbols(name, &symbol.children, uri, locations);
        }
    }

    /// Handle references request.
    pub fn references(&self, params: TextDocumentPositionParams) -> Vec<Location> {
        let Some(doc) = self.documents.get(&params.text_document.uri) else {
            return Vec::new();
        };

        // Get the word at the cursor position
        let Some((word, _word_range)) = doc.word_at(params.position) else {
            return Vec::new();
        };

        let mut locations = Vec::new();

        // Find all occurrences in current document
        self.find_references_in_document(&word, &doc, &mut locations);

        // Search all workspace documents
        for uri in self.documents.uris() {
            if uri == doc.uri {
                continue; // Already searched
            }
            if let Some(other_doc) = self.documents.get(&uri) {
                self.find_references_in_document(&word, &other_doc, &mut locations);
            }
        }

        locations
    }

    /// Find all references to a word in a document.
    fn find_references_in_document(
        &self,
        word: &str,
        doc: &super::document::Document,
        locations: &mut Vec<Location>,
    ) {
        let content = &doc.content;
        let mut offset = 0;

        while let Some(pos) = content[offset..].find(word) {
            let abs_pos = offset + pos;

            // Check word boundaries to avoid matching substrings
            let before_ok = abs_pos == 0
                || !content.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                    && content.as_bytes()[abs_pos - 1] != b'_';
            let after_pos = abs_pos + word.len();
            let after_ok = after_pos >= content.len()
                || !content.as_bytes()[after_pos].is_ascii_alphanumeric()
                    && content.as_bytes()[after_pos] != b'_';

            if before_ok && after_ok {
                let start = doc.position_at(abs_pos);
                let end = doc.position_at(after_pos);
                locations.push(Location::new(doc.uri.clone(), Range::new(start, end)));
            }

            offset = abs_pos + 1; // Move past this occurrence
        }
    }

    /// Handle documentSymbol request.
    pub fn document_symbol(&self, uri: &DocumentUri) -> Vec<DocumentSymbol> {
        let Some(doc) = self.documents.get(uri) else {
            return Vec::new();
        };
        self.symbols.document_symbols(&doc)
    }

    /// Handle workspace symbol request for currently opened documents.
    pub fn workspace_symbol(&self, query: &str) -> Vec<SymbolInformation> {
        let query_lower = query.to_lowercase();
        let mut symbols = Vec::new();
        let mut opened_uris = self.documents.uris();
        opened_uris.sort();

        for uri in &opened_uris {
            if let Some(doc) = self.documents.get(uri) {
                let doc_symbols = self.symbols.document_symbols(&doc);
                symbols.extend(self.symbols.matching_symbol_information(
                    &doc_symbols,
                    uri,
                    &query_lower,
                ));
            }
        }

        for (uri, indexed_symbols) in self.workspace_index.symbols() {
            if opened_uris.binary_search(uri).is_ok() {
                continue;
            }
            symbols.extend(self.symbols.matching_symbol_information(
                indexed_symbols,
                uri,
                &query_lower,
            ));
        }

        symbols
    }

    /// Rebuild workspace symbols from a deterministic root mapping.
    pub(crate) fn rebuild_workspace_symbol_index_for_root(
        &mut self,
        root_uri: &str,
        root_path: &Path,
    ) -> WorkspaceSymbolIndexStats {
        self.workspace_index
            .rebuild_from_path(root_uri, root_path, &self.symbols)
    }

    /// Handle semantic tokens request.
    pub fn semantic_tokens(&self, uri: &DocumentUri) -> Option<SemanticTokens> {
        let doc = self.documents.get(uri)?;
        Some(self.semantic_tokens.full(&doc))
    }

    /// Handle code action request.
    pub fn code_action(&self, params: CodeActionParams) -> Vec<CodeAction> {
        let Some(doc) = self.documents.get(&params.text_document.uri) else {
            return Vec::new();
        };

        let mut actions = Vec::new();

        // Generate quick fixes for diagnostics
        for diagnostic in &params.context.diagnostics {
            if let Some(fix) = self.generate_quick_fix(&doc, diagnostic) {
                actions.push(fix);
            }
        }

        actions
    }

    /// Handle formatting request.
    pub fn format(&self, params: DocumentFormattingParams) -> Vec<TextEdit> {
        let Some(doc) = self.documents.get(&params.text_document.uri) else {
            return Vec::new();
        };

        // TODO: Use actual formatter
        // For now, just trim trailing whitespace
        let mut edits = Vec::new();
        for (line_num, line) in doc.content.lines().enumerate() {
            let trimmed = line.trim_end();
            if trimmed.len() < line.len() {
                let start = Position::new(line_num as u32, trimmed.len() as u32);
                let end = Position::new(line_num as u32, line.len() as u32);
                edits.push(TextEdit::delete(Range::new(start, end)));
            }
        }

        edits
    }

    /// Handle rename request.
    pub fn rename(&self, params: RenameParams) -> Option<WorkspaceEdit> {
        let doc = self
            .documents
            .get(&params.text_document_position.text_document.uri)?;
        let (word, _range) = doc.word_at(params.text_document_position.position)?;

        // Find all occurrences in the document
        let mut edit = WorkspaceEdit::new();
        let content = &doc.content;
        let mut offset = 0;

        while let Some(pos) = content[offset..].find(&word) {
            let abs_pos = offset + pos;
            // Check word boundaries
            let before_ok =
                abs_pos == 0 || !content.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
            let after_pos = abs_pos + word.len();
            let after_ok = after_pos >= content.len()
                || !content.as_bytes()[after_pos].is_ascii_alphanumeric();

            if before_ok && after_ok {
                let start = doc.position_at(abs_pos);
                let end = doc.position_at(after_pos);
                edit.add_edit(
                    doc.uri.clone(),
                    TextEdit::replace(Range::new(start, end), params.new_name.clone()),
                );
            }

            offset = abs_pos + word.len();
        }

        Some(edit)
    }

    /// Handle folding range request.
    pub fn folding_range(&self, uri: &DocumentUri) -> Vec<FoldingRange> {
        let Some(doc) = self.documents.get(uri) else {
            return Vec::new();
        };

        let mut ranges = Vec::new();
        let mut brace_stack: Vec<u32> = Vec::new();
        let mut comment_start: Option<u32> = None;

        let lines: Vec<&str> = doc.content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num as u32;
            let trimmed = line.trim_start();

            // Track brace nesting for code folding
            for c in line.chars() {
                if c == '{' {
                    brace_stack.push(line_num);
                } else if c == '}' {
                    if let Some(start_line) = brace_stack.pop() {
                        if line_num > start_line {
                            ranges.push(FoldingRange {
                                start_line,
                                start_character: None,
                                end_line: line_num,
                                end_character: None,
                                kind: None,
                            });
                        }
                    }
                }
            }

            // Track consecutive comment blocks
            let is_comment = trimmed.starts_with("//");
            if is_comment {
                if comment_start.is_none() {
                    comment_start = Some(line_num);
                }
            } else {
                // End of comment block - check if we had consecutive comments
                if let Some(start) = comment_start {
                    if line_num > start + 1 {
                        // At least 2 consecutive comment lines
                        ranges.push(FoldingRange {
                            start_line: start,
                            start_character: None,
                            end_line: line_num - 1,
                            end_character: None,
                            kind: Some(FoldingRangeKind::Comment),
                        });
                    }
                    comment_start = None;
                }
            }
        }

        // Handle comment block at end of file
        if let Some(start) = comment_start {
            let end = lines.len() as u32 - 1;
            if end > start {
                ranges.push(FoldingRange {
                    start_line: start,
                    start_character: None,
                    end_line: end,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Comment),
                });
            }
        }

        ranges
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    /// Generate a quick fix for a diagnostic.
    fn generate_quick_fix(
        &self,
        doc: &super::document::Document,
        diagnostic: &Diagnostic,
    ) -> Option<CodeAction> {
        let message = &diagnostic.message;

        // Missing semicolon fix
        if message.contains("expected ';'") {
            let mut action = CodeAction::quick_fix("Add missing semicolon");
            let pos = diagnostic.range.end;
            let mut edit = WorkspaceEdit::new();
            edit.add_edit(doc.uri.clone(), TextEdit::insert(pos, ";".to_string()));
            action.edit = Some(edit);
            action.is_preferred = true;
            return Some(action);
        }

        // Unused variable fix
        if message.contains("unused variable") {
            let mut action = CodeAction::quick_fix("Prefix with underscore");
            let start = diagnostic.range.start;
            let mut edit = WorkspaceEdit::new();
            edit.add_edit(doc.uri.clone(), TextEdit::insert(start, "_".to_string()));
            action.edit = Some(edit);
            return Some(action);
        }

        // Import suggestion
        if message.contains("not found in this scope") {
            if let Some((word, _)) = doc.word_at(diagnostic.range.start) {
                // Suggest common imports
                let import_suggestions = suggest_import(&word);
                if !import_suggestions.is_empty() {
                    let suggestion = &import_suggestions[0];
                    let mut action = CodeAction::quick_fix(format!("Import {}", suggestion));
                    let mut edit = WorkspaceEdit::new();
                    edit.add_edit(
                        doc.uri.clone(),
                        TextEdit::insert(Position::new(0, 0), format!("use {};\n", suggestion)),
                    );
                    action.edit = Some(edit);
                    return Some(action);
                }
            }
        }

        None
    }
}

impl Default for LanguageServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Suggest imports for a name.
fn suggest_import(name: &str) -> Vec<String> {
    // Common stdlib imports
    let suggestions: Vec<(&str, &str)> = vec![
        ("HashMap", "std::collections::HashMap"),
        ("HashSet", "std::collections::HashSet"),
        ("Vec", "std::vec::Vec"),
        ("String", "std::string::String"),
        ("Arc", "std::sync::Arc"),
        ("Mutex", "std::sync::Mutex"),
        ("Rc", "std::rc::Rc"),
        ("RefCell", "std::cell::RefCell"),
        ("Path", "std::path::Path"),
        ("PathBuf", "std::path::PathBuf"),
        ("File", "std::fs::File"),
    ];

    suggestions
        .into_iter()
        .filter(|(n, _)| *n == name)
        .map(|(_, path)| path.to_string())
        .collect()
}

// =============================================================================
// SERVER RUNNER
// =============================================================================

/// Run the language server with stdio transport.
pub fn run_server() -> Result<(), TransportError> {
    let transport = StdioTransport::new();
    let mut server = LanguageServer::new();

    loop {
        let raw_msg = transport.recv()?;

        // Parse and handle message
        let response = handle_raw_message(&mut server, &raw_msg.content);

        if let Some(response_content) = response {
            transport.send(RawMessage::new(response_content))?;
        }

        // Check for exit
        if server.should_shutdown() {
            break;
        }
    }

    Ok(())
}

/// Build a JSON response with result.
fn build_response(id: String, result: String) -> String {
    JsonObjectBuilder::new()
        .field_str("jsonrpc", "2.0")
        .field("id", id)
        .field("result", result)
        .build()
}

/// Build a JSON notification (no id).
fn build_notification(method: &str, params: String) -> String {
    JsonObjectBuilder::new()
        .field_str("jsonrpc", "2.0")
        .field_str("method", method)
        .field("params", params)
        .build()
}

/// Build diagnostics notification JSON.
fn build_diagnostics_notification(params: &PublishDiagnosticsParams) -> String {
    let mut diag_array = JsonArrayBuilder::new();
    for d in &params.diagnostics {
        let severity = match d.severity {
            Some(DiagnosticSeverity::Error) => 1,
            Some(DiagnosticSeverity::Warning) => 2,
            Some(DiagnosticSeverity::Information) => 3,
            Some(DiagnosticSeverity::Hint) => 4,
            None => 1,
        };
        diag_array = diag_array.item(
            JsonObjectBuilder::new()
                .field("range", build_range_json(&d.range))
                .field_number("severity", severity)
                .field_str("message", &d.message)
                .field_str("source", d.source.as_deref().unwrap_or("quantalang"))
                .build(),
        );
    }
    let params_json = JsonObjectBuilder::new()
        .field_str("uri", &params.uri)
        .field("diagnostics", diag_array.build())
        .build();
    build_notification("textDocument/publishDiagnostics", params_json)
}

/// Build a JSON-RPC error response.
fn build_error_response(id: String, code: i32, message: &str) -> String {
    JsonObjectBuilder::new()
        .field_str("jsonrpc", "2.0")
        .field("id", id)
        .field(
            "error",
            JsonObjectBuilder::new()
                .field_number("code", code)
                .field_str("message", message)
                .build(),
        )
        .build()
}

fn build_invalid_params_response(id: String, error: &raw_params::RawParamError) -> String {
    build_error_response(id, -32602, &format!("Invalid params: {}", error.detail()))
}

/// Build range JSON.
fn build_range_json(range: &Range) -> String {
    JsonObjectBuilder::new()
        .field(
            "start",
            JsonObjectBuilder::new()
                .field_number("line", range.start.line)
                .field_number("character", range.start.character)
                .build(),
        )
        .field(
            "end",
            JsonObjectBuilder::new()
                .field_number("line", range.end.line)
                .field_number("character", range.end.character)
                .build(),
        )
        .build()
}

/// Build location JSON.
fn build_location_json(loc: &Location) -> String {
    JsonObjectBuilder::new()
        .field_str("uri", &loc.uri)
        .field("range", build_range_json(&loc.range))
        .build()
}

/// Handle a raw JSON message and return a response (and optionally a notification to send after).
fn handle_raw_message(server: &mut LanguageServer, content: &str) -> Option<String> {
    let message = match JsonRpcMessage::parse(content) {
        Ok(message) => message,
        Err(_) => {
            let id = extract_id(content)?;
            return Some(build_error_response(id, -32700, "Parse error"));
        }
    };
    let id = message.id_json().map(str::to_string);
    let method = message.method()?;

    // =========================================================================
    // LIFECYCLE
    // =========================================================================

    if method == "initialize" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let params = match raw_params::decode_initialize(&message) {
            Ok(params) => params,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let result = server.initialize(params);
        return Some(build_response(
            response_id,
            build_initialize_result(&result),
        ));
    }

    if method == "initialized" {
        server.initialized();
        return None;
    }

    if method == "shutdown" {
        server.shutdown();
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            JsonBuilder::null(),
        ));
    }

    if method == "exit" {
        server.exit();
        return None;
    }

    // =========================================================================
    // TEXT DOCUMENT SYNC
    // =========================================================================

    if method == "textDocument/didOpen" {
        let params = match raw_params::decode_did_open(&message) {
            Ok(params) => params,
            Err(error) => return id.map(|id| build_invalid_params_response(id, &error)),
        };
        if let Some(diag) = server.did_open(params) {
            return Some(build_diagnostics_notification(&diag));
        }
        return None;
    }

    if method == "textDocument/didChange" {
        let params = match raw_params::decode_did_change(&message) {
            Ok(params) => params,
            Err(error) => return id.map(|id| build_invalid_params_response(id, &error)),
        };
        if let Some(diag) = server.did_change(params) {
            return Some(build_diagnostics_notification(&diag));
        }
        return None;
    }

    if method == "textDocument/didSave" {
        let params = match raw_params::decode_did_save(&message) {
            Ok(params) => params,
            Err(error) => return id.map(|id| build_invalid_params_response(id, &error)),
        };
        if let Some(diag) = server.did_save(params) {
            return Some(build_diagnostics_notification(&diag));
        }
        return None;
    }

    if method == "textDocument/didClose" {
        let params = match raw_params::decode_did_close(&message) {
            Ok(params) => params,
            Err(error) => return id.map(|id| build_invalid_params_response(id, &error)),
        };
        server.did_close(params);
        return None;
    }

    // =========================================================================
    // LANGUAGE FEATURES (requests - require id)
    // =========================================================================

    if method == "textDocument/completion" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let params = match raw_params::decode_completion(&message) {
            Ok(params) => params,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let result = server.completion(params);
        let result_json = match result {
            Some(list) => {
                let mut items = JsonArrayBuilder::new();
                for item in &list.items {
                    let mut obj = JsonObjectBuilder::new()
                        .field_str("label", &item.label)
                        .field_number("kind", item.kind.map(|k| k as i32).unwrap_or(1));
                    if let Some(ref detail) = item.detail {
                        obj = obj.field_str("detail", detail);
                    }
                    if let Some(ref doc) = item.documentation {
                        obj = obj.field_str("documentation", &doc.value);
                    }
                    if let Some(ref insert) = item.insert_text {
                        obj = obj.field_str("insertText", insert);
                    }
                    items = items.item(obj.build());
                }
                JsonObjectBuilder::new()
                    .field_bool("isIncomplete", list.is_incomplete)
                    .field("items", items.build())
                    .build()
            }
            None => JsonBuilder::null(),
        };
        return Some(build_response(response_id, result_json));
    }

    if method == "textDocument/hover" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let params = match raw_params::decode_text_document_position(&message) {
            Ok(params) => params,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let result = server.hover(params);
        let result_json = match result {
            Some(hover) => {
                let kind_str = match hover.contents.kind {
                    MarkupKind::PlainText => "plaintext",
                    MarkupKind::Markdown => "markdown",
                };
                let mut obj = JsonObjectBuilder::new().field(
                    "contents",
                    JsonObjectBuilder::new()
                        .field_str("kind", kind_str)
                        .field_str("value", &hover.contents.value)
                        .build(),
                );
                if let Some(ref range) = hover.range {
                    obj = obj.field("range", build_range_json(range));
                }
                obj.build()
            }
            None => JsonBuilder::null(),
        };
        return Some(build_response(response_id, result_json));
    }

    if method == "textDocument/definition" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let params = match raw_params::decode_text_document_position(&message) {
            Ok(params) => params,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let locations = server.definition(params);
        let mut arr = JsonArrayBuilder::new();
        for loc in &locations {
            arr = arr.item(build_location_json(loc));
        }
        return Some(build_response(response_id, arr.build()));
    }

    if method == "textDocument/references" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let params = match raw_params::decode_text_document_position(&message) {
            Ok(params) => params,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let locations = server.references(params);
        let mut arr = JsonArrayBuilder::new();
        for loc in &locations {
            arr = arr.item(build_location_json(loc));
        }
        return Some(build_response(response_id, arr.build()));
    }

    if method == "textDocument/documentSymbol" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let uri = match raw_params::decode_document_uri(&message) {
            Ok(uri) => uri,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let symbols = server.document_symbol(&uri);
        let result = build_symbols_json(&symbols);
        return Some(build_response(response_id, result));
    }

    if method == "workspace/symbol" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let query = match raw_params::decode_workspace_symbol_query(&message) {
            Ok(query) => query,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let symbols = server.workspace_symbol(&query);
        return Some(build_response(
            response_id,
            response_json::build_symbol_information_json(&symbols),
        ));
    }

    if method == "textDocument/semanticTokens/full" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let uri = match raw_params::decode_document_uri(&message) {
            Ok(uri) => uri,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let result_json = server
            .semantic_tokens(&uri)
            .as_ref()
            .map(response_json::build_semantic_tokens_json)
            .unwrap_or_else(JsonBuilder::null);
        return Some(build_response(response_id, result_json));
    }

    if method == "textDocument/codeAction" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let params = match raw_params::decode_code_action(&message) {
            Ok(params) => params,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let actions = server.code_action(params);
        return Some(build_response(
            response_id,
            response_json::build_code_actions_json(&actions),
        ));
    }

    if method == "textDocument/formatting" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let params = match raw_params::decode_formatting(&message) {
            Ok(params) => params,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let edits = server.format(params);
        let mut arr = JsonArrayBuilder::new();
        for edit in &edits {
            arr = arr.item(
                JsonObjectBuilder::new()
                    .field("range", build_range_json(&edit.range))
                    .field_str("newText", &edit.new_text)
                    .build(),
            );
        }
        return Some(build_response(response_id, arr.build()));
    }

    if method == "textDocument/rename" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let params = match raw_params::decode_rename(&message) {
            Ok(params) => params,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let result_json = server
            .rename(params)
            .as_ref()
            .map(response_json::build_workspace_edit_json)
            .unwrap_or_else(JsonBuilder::null);
        return Some(build_response(response_id, result_json));
    }

    if method == "textDocument/foldingRange" {
        let response_id = id.unwrap_or_else(|| "1".to_string());
        let uri = match raw_params::decode_document_uri(&message) {
            Ok(uri) => uri,
            Err(error) => return Some(build_invalid_params_response(response_id, &error)),
        };
        let ranges = server.folding_range(&uri);
        let mut arr = JsonArrayBuilder::new();
        for r in &ranges {
            let mut obj = JsonObjectBuilder::new()
                .field_number("startLine", r.start_line)
                .field_number("endLine", r.end_line);
            if let Some(kind) = &r.kind {
                let kind_str = match kind {
                    FoldingRangeKind::Comment => "comment",
                    FoldingRangeKind::Imports => "imports",
                    FoldingRangeKind::Region => "region",
                };
                obj = obj.field_str("kind", kind_str);
            }
            arr = arr.item(obj.build());
        }
        return Some(build_response(response_id, arr.build()));
    }

    // =========================================================================
    // UNKNOWN METHOD
    // =========================================================================

    if let Some(id) = id {
        return Some(build_error_response(id, -32601, "Method not found"));
    }

    None
}

/// Dispatch a raw LSP JSON-RPC payload through the same request path used by
/// the stdio server loop.
pub fn dispatch_raw_message(server: &mut LanguageServer, content: &str) -> Option<String> {
    handle_raw_message(server, content)
}

/// Build document symbols JSON recursively.
fn build_symbols_json(symbols: &[DocumentSymbol]) -> String {
    let mut arr = JsonArrayBuilder::new();
    for sym in symbols {
        let mut obj = JsonObjectBuilder::new()
            .field_str("name", &sym.name)
            .field_number("kind", sym.kind as i32)
            .field("range", build_range_json(&sym.range))
            .field("selectionRange", build_range_json(&sym.selection_range));
        if let Some(ref detail) = sym.detail {
            obj = obj.field_str("detail", detail);
        }
        if !sym.children.is_empty() {
            obj = obj.field("children", build_symbols_json(&sym.children));
        }
        arr = arr.item(obj.build());
    }
    arr.build()
}

/// Extract request ID from JSON (very simplified).
fn extract_id(content: &str) -> Option<String> {
    if let Some(pos) = content.find("\"id\":") {
        let rest = &content[pos + 5..];
        let rest = rest.trim_start();

        if rest.starts_with('"') {
            // String ID
            if let Some(end) = rest[1..].find('"') {
                return Some(format!("\"{}\"", &rest[1..1 + end]));
            }
        } else {
            // Number ID
            let end = rest
                .find(|c: char| !c.is_ascii_digit() && c != '-')
                .unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Build initialize result JSON.
fn build_initialize_result(result: &InitializeResult) -> String {
    let _caps = &result.capabilities;

    let mut builder =
        JsonObjectBuilder::new().field(
            "capabilities",
            JsonObjectBuilder::new()
                .field_number("textDocumentSync", 2) // Incremental
                .field(
                    "completionProvider",
                    JsonObjectBuilder::new()
                        .field(
                            "triggerCharacters",
                            JsonArrayBuilder::new()
                                .item(JsonBuilder::string("."))
                                .item(JsonBuilder::string(":"))
                                .build(),
                        )
                        .field_bool("resolveProvider", true)
                        .build(),
                )
                .field_bool("hoverProvider", true)
                .field_bool("definitionProvider", true)
                .field_bool("referencesProvider", true)
                .field_bool("documentSymbolProvider", true)
                .field_bool("workspaceSymbolProvider", true)
                .field_bool("documentFormattingProvider", true)
                .field_bool("renameProvider", true)
                .field_bool("foldingRangeProvider", true)
                .field(
                    "semanticTokensProvider",
                    response_json::build_semantic_tokens_options_json(
                        &SemanticTokensProvider::legend(),
                    ),
                )
                .build(),
        );

    if let Some(ref info) = result.server_info {
        builder = builder.field(
            "serverInfo",
            JsonObjectBuilder::new()
                .field_str("name", &info.name)
                .field_str_if_some("version", info.version.as_deref())
                .build(),
        );
    }

    builder.build()
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn temp_workspace_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "quantalang_lsp_workspace_{label}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp workspace root");
        root
    }

    fn path_file_uri(path: &Path) -> String {
        let mut path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(stripped) = path.strip_prefix("//?/") {
            path = stripped.to_string();
        }
        format!("file:///{}", path.trim_start_matches('/'))
    }

    #[test]
    fn test_server_lifecycle() {
        let mut server = LanguageServer::new();
        assert_eq!(server.state(), ServerState::Uninitialized);

        let params = InitializeParams {
            process_id: Some(1234),
            root_path: None,
            root_uri: Some("file:///workspace".to_string()),
            capabilities: ClientCapabilities::default(),
            initialization_options: None,
            trace: None,
            workspace_folders: None,
        };

        let result = server.initialize(params);
        assert!(result.server_info.is_some());
        assert_eq!(server.state(), ServerState::Initializing);

        server.initialized();
        assert_eq!(server.state(), ServerState::Running);

        server.shutdown();
        assert!(server.should_shutdown());
        assert_eq!(server.state(), ServerState::ShuttingDown);

        server.exit();
        assert_eq!(server.state(), ServerState::Shutdown);
    }

    #[test]
    fn test_extract_id() {
        assert_eq!(
            extract_id(r#"{"id":1,"method":"test"}"#),
            Some("1".to_string())
        );
        assert_eq!(
            extract_id(r#"{"id":"abc","method":"test"}"#),
            Some("\"abc\"".to_string())
        );
    }

    #[test]
    fn raw_dispatch_initialize_reports_core_capabilities() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///workspace"}}"#,
        )
        .expect("initialize should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse initialize response");

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["result"]["capabilities"]["hoverProvider"], true);
        assert_eq!(json["result"]["capabilities"]["definitionProvider"], true);
        assert_eq!(json["result"]["capabilities"]["referencesProvider"], true);
        assert_eq!(
            json["result"]["capabilities"]["documentSymbolProvider"],
            true
        );
    }

    #[test]
    fn raw_dispatch_initialize_reports_semantic_tokens_capability() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///workspace"}}"#,
        )
        .expect("initialize should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse initialize response");
        let provider = &json["result"]["capabilities"]["semanticTokensProvider"];

        assert_eq!(provider["range"], false);
        assert_eq!(provider["full"], true);
        let token_types = provider["legend"]["tokenTypes"]
            .as_array()
            .expect("tokenTypes array");
        assert!(token_types.iter().any(|token| token == "function"));
        assert!(token_types.iter().any(|token| token == "string"));
    }

    #[test]
    fn raw_dispatch_initialize_reports_workspace_symbol_capability() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":"file:///workspace"}}"#,
        )
        .expect("initialize should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse initialize response");

        assert_eq!(
            json["result"]["capabilities"]["workspaceSymbolProvider"],
            true
        );
    }

    #[test]
    fn raw_dispatch_did_open_returns_diagnostics_notification() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"fn main() {\n    let x = 1;\n}\n"}}}"#,
        )
        .expect("didOpen should publish diagnostics");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse diagnostics notification");

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "textDocument/publishDiagnostics");
        assert_eq!(json["params"]["uri"], "file:///workspace/main.quanta");
        assert!(json["params"]["diagnostics"].is_array());
    }

    #[test]
    fn raw_dispatch_did_open_returns_type_checker_diagnostic_source() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/type_error.quanta","languageId":"quanta","version":1,"text":"const BAD: i32 = \"oops\";\nfn main() {}\n"}}}"#,
        )
        .expect("didOpen should publish diagnostics");
        let json: serde_json::Value = serde_json::from_str(&response).expect("parse diagnostics");
        let diagnostics = json["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array");

        assert!(
            diagnostics.iter().any(|d| {
                d["source"] == "quantalang/type-checker"
                    && d["message"]
                        .as_str()
                        .is_some_and(|message| message.contains("type mismatch"))
            }),
            "expected type-checker diagnostic in {diagnostics:#?}"
        );
    }

    #[test]
    fn raw_dispatch_document_symbol_returns_opened_function() {
        let mut server = LanguageServer::new();
        dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"fn helper() -> i32 { 1 }\nfn main() { helper(); }\n"}}}"#,
        )
        .expect("didOpen should publish diagnostics");

        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"file:///workspace/main.quanta"}}}"#,
        )
        .expect("documentSymbol should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse documentSymbol response");
        let names = json["result"]
            .as_array()
            .expect("documentSymbol result array")
            .iter()
            .filter_map(|symbol| symbol["name"].as_str())
            .collect::<Vec<_>>();

        assert!(
            names.contains(&"helper"),
            "expected helper symbol in {names:?}"
        );
        assert!(names.contains(&"main"), "expected main symbol in {names:?}");
    }

    #[test]
    fn raw_dispatch_workspace_symbol_returns_opened_symbol() {
        let mut server = LanguageServer::new();
        dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"fn helper() -> i32 { 1 }\nfn main() { helper(); }\n"}}}"#,
        )
        .expect("didOpen should publish diagnostics");

        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":30,"method":"workspace/symbol","params":{"query":"help"}}"#,
        )
        .expect("workspace/symbol should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse workspace/symbol response");
        let symbols = json["result"]
            .as_array()
            .expect("workspace symbol result array");

        assert!(symbols.iter().any(|symbol| {
            symbol["name"] == "helper"
                && symbol["location"]["uri"] == "file:///workspace/main.quanta"
        }));
    }

    #[test]
    fn raw_dispatch_workspace_symbol_returns_unopened_root_file_symbol() {
        let root = temp_workspace_root("unopened");
        std::fs::write(
            root.join("library.quanta"),
            "fn library_helper() -> i32 { 7 }\n",
        )
        .expect("write library");
        let root_uri = path_file_uri(&root);
        let mut server = LanguageServer::new();
        dispatch_raw_message(
            &mut server,
            &format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{root_uri}"}}}}"#
            ),
        )
        .expect("initialize response");

        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":40,"method":"workspace/symbol","params":{"query":"library_helper"}}"#,
        )
        .expect("workspace response");
        let json: serde_json::Value = serde_json::from_str(&response).expect("parse response");

        assert!(json["result"]
            .as_array()
            .expect("result array")
            .iter()
            .any(|symbol| symbol["name"] == "library_helper"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn raw_dispatch_open_document_overrides_indexed_file_symbol() {
        let root = temp_workspace_root("override");
        let file = root.join("main.quanta");
        std::fs::write(&file, "fn disk_only() -> i32 { 1 }\n").expect("write disk file");
        let root_uri = path_file_uri(&root);
        let file_uri = path_file_uri(&file);
        let mut server = LanguageServer::new();
        dispatch_raw_message(
            &mut server,
            &format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{root_uri}"}}}}"#
            ),
        )
        .expect("initialize response");
        dispatch_raw_message(
            &mut server,
            &format!(
                r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{file_uri}","languageId":"quanta","version":1,"text":"fn editor_only() -> i32 {{ 2 }}\n"}}}}}}"#
            ),
        )
        .expect("didOpen response");

        let disk = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":41,"method":"workspace/symbol","params":{"query":"disk_only"}}"#,
        )
        .expect("disk response");
        let editor = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":42,"method":"workspace/symbol","params":{"query":"editor_only"}}"#,
        )
        .expect("editor response");

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&disk).unwrap()["result"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&editor).unwrap()["result"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn raw_dispatch_workspace_symbol_unmatched_query_returns_empty_array() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":31,"method":"workspace/symbol","params":{"query":"missing"}}"#,
        )
        .expect("workspace/symbol should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse workspace/symbol response");

        assert!(json.get("error").is_none());
        assert_eq!(json["result"].as_array().expect("result array").len(), 0);
    }

    #[test]
    fn raw_dispatch_semantic_tokens_full_returns_opened_document_tokens() {
        let mut server = LanguageServer::new();
        dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"// comment\nfn helper() -> i32 { 42 }\nfn main() { helper(\"x\"); }\n"}}}"#,
        )
        .expect("didOpen should publish diagnostics");

        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":12,"method":"textDocument/semanticTokens/full","params":{"textDocument":{"uri":"file:///workspace/main.quanta"}}}"#,
        )
        .expect("semanticTokens/full should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse semanticTokens response");
        let data = json["result"]["data"]
            .as_array()
            .expect("semantic token data array");

        assert_eq!(json["id"], 12);
        assert!(!data.is_empty());
        assert_eq!(data.len() % 5, 0);
    }

    #[test]
    fn raw_dispatch_semantic_tokens_full_unknown_document_returns_null() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":13,"method":"textDocument/semanticTokens/full","params":{"textDocument":{"uri":"file:///workspace/missing.quanta"}}}"#,
        )
        .expect("semanticTokens/full should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse semanticTokens response");

        assert_eq!(json["id"], 13);
        assert!(json.get("error").is_none());
        assert!(json.get("result").is_some());
        assert!(json["result"].is_null());
    }

    #[test]
    fn raw_dispatch_initialize_accepts_pretty_json_and_string_id() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{
              "jsonrpc": "2.0",
              "params": { "rootUri": "file:///workspace" },
              "method": "initialize",
              "id": "init-1"
            }"#,
        )
        .expect("initialize should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse initialize response");

        assert_eq!(json["id"], "init-1");
        assert_eq!(json["result"]["capabilities"]["hoverProvider"], true);
    }

    #[test]
    fn raw_dispatch_document_symbol_accepts_reordered_pretty_json() {
        let mut server = LanguageServer::new();
        dispatch_raw_message(
            &mut server,
            r#"{
              "params": {
                "textDocument": {
                  "text": "fn helper() -> i32 { 1 }\nfn main() { helper(); }\n",
                  "version": 1,
                  "languageId": "quanta",
                  "uri": "file:///workspace/main.quanta"
                }
              },
              "method": "textDocument/didOpen",
              "jsonrpc": "2.0"
            }"#,
        )
        .expect("didOpen should publish diagnostics");

        let response = dispatch_raw_message(
            &mut server,
            r#"{
              "method": "textDocument/documentSymbol",
              "params": { "textDocument": { "uri": "file:///workspace/main.quanta" } },
              "id": 2,
              "jsonrpc": "2.0"
            }"#,
        )
        .expect("documentSymbol should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse documentSymbol response");
        let names = json["result"]
            .as_array()
            .expect("documentSymbol result array")
            .iter()
            .filter_map(|symbol| symbol["name"].as_str())
            .collect::<Vec<_>>();

        assert!(
            names.contains(&"helper"),
            "expected helper symbol in {names:?}"
        );
        assert!(names.contains(&"main"), "expected main symbol in {names:?}");
    }

    #[test]
    fn raw_dispatch_malformed_json_request_returns_parse_error() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":9,"method":"initialize""#,
        )
        .expect("malformed request with id should return an error response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse error response");

        assert_eq!(json["id"], 9);
        assert_eq!(json["error"]["code"], -32700);
    }

    fn assert_invalid_params(response: Option<String>, expected_detail: &str) {
        let response = response.expect("invalid params should return an error response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse invalid params response");

        assert_eq!(json["error"]["code"], -32602);
        let message = json["error"]["message"].as_str().expect("message");
        assert!(
            message.contains(expected_detail),
            "expected '{expected_detail}' in '{message}'"
        );
    }

    #[test]
    fn raw_dispatch_invalid_params_did_open_missing_uri_returns_error() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":20,"method":"textDocument/didOpen","params":{"textDocument":{"languageId":"quanta","version":1,"text":"fn main() {}\n"}}}"#,
        );

        assert_invalid_params(response, "params.textDocument.uri is required");
    }

    #[test]
    fn raw_dispatch_invalid_params_hover_negative_position_returns_error() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":21,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///workspace/main.quanta"},"position":{"line":-1,"character":0}}}"#,
        );

        assert_invalid_params(
            response,
            "params.position.line must be a non-negative integer",
        );
    }

    #[test]
    fn raw_dispatch_invalid_params_rename_missing_new_name_returns_error() {
        let mut server = LanguageServer::new();
        dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"fn helper() -> i32 { 1 }\nfn main() { helper(); }\n"}}}"#,
        )
        .expect("didOpen should publish diagnostics");
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":22,"method":"textDocument/rename","params":{"textDocument":{"uri":"file:///workspace/main.quanta"},"position":{"line":1,"character":14}}}"#,
        );

        assert_invalid_params(response, "params.newName is required");
    }

    #[test]
    fn raw_dispatch_invalid_params_code_action_missing_context_returns_error() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":23,"method":"textDocument/codeAction","params":{"textDocument":{"uri":"file:///workspace/main.quanta"},"range":{"start":{"line":1,"character":13},"end":{"line":1,"character":13}}}}"#,
        );

        assert_invalid_params(response, "params.context is required");
    }

    #[test]
    fn raw_dispatch_invalid_params_semantic_tokens_missing_uri_returns_error() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":24,"method":"textDocument/semanticTokens/full","params":{"textDocument":{}}}"#,
        );

        assert_invalid_params(response, "params.textDocument.uri is required");
    }

    #[test]
    fn raw_dispatch_invalid_params_workspace_symbol_missing_query_returns_error() {
        let mut server = LanguageServer::new();
        let response = dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","id":32,"method":"workspace/symbol","params":{}}"#,
        );

        assert_invalid_params(response, "params.query is required");
    }

    #[test]
    fn raw_dispatch_code_action_returns_supplied_diagnostic_quick_fix() {
        let mut server = LanguageServer::new();
        dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"fn main() {\n    let x = 1\n}\n"}}}"#,
        )
        .expect("didOpen should publish diagnostics");

        let response = dispatch_raw_message(
            &mut server,
            r#"{
              "jsonrpc": "2.0",
              "id": 10,
              "method": "textDocument/codeAction",
              "params": {
                "textDocument": { "uri": "file:///workspace/main.quanta" },
                "range": {
                  "start": { "line": 1, "character": 13 },
                  "end": { "line": 1, "character": 13 }
                },
                "context": {
                  "diagnostics": [{
                    "range": {
                      "start": { "line": 1, "character": 13 },
                      "end": { "line": 1, "character": 13 }
                    },
                    "severity": 1,
                    "source": "quantalang",
                    "message": "expected ';'"
                  }]
                }
              }
            }"#,
        )
        .expect("codeAction should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse codeAction response");

        assert_eq!(json["id"], 10);
        let actions = json["result"].as_array().expect("code actions array");
        assert!(actions
            .iter()
            .any(|action| action["title"] == "Add missing semicolon"));
    }

    #[test]
    fn raw_dispatch_rename_returns_workspace_edits_for_symbol_occurrences() {
        let mut server = LanguageServer::new();
        dispatch_raw_message(
            &mut server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///workspace/main.quanta","languageId":"quanta","version":1,"text":"fn helper() -> i32 { 1 }\nfn main() { helper(); }\n"}}}"#,
        )
        .expect("didOpen should publish diagnostics");

        let response = dispatch_raw_message(
            &mut server,
            r#"{
              "jsonrpc": "2.0",
              "id": 11,
              "method": "textDocument/rename",
              "params": {
                "textDocument": { "uri": "file:///workspace/main.quanta" },
                "position": { "line": 1, "character": 14 },
                "newName": "renamed_helper"
              }
            }"#,
        )
        .expect("rename should return a response");
        let json: serde_json::Value =
            serde_json::from_str(&response).expect("parse rename response");

        assert_eq!(json["id"], 11);
        let edits = json["result"]["changes"]["file:///workspace/main.quanta"]
            .as_array()
            .expect("rename edits for document");
        assert!(
            edits.len() >= 2,
            "expected definition and call-site edits: {edits:#?}"
        );
        assert!(edits.iter().all(|edit| edit["newText"] == "renamed_helper"));
    }
}
