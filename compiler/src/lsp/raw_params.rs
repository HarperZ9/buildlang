use serde_json::Value;

use super::jsonrpc::JsonRpcMessage;
use super::message::*;
use super::types::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawParamError {
    detail: String,
}

impl RawParamError {
    fn required(path: &'static str) -> Self {
        Self {
            detail: format!("{path} is required"),
        }
    }

    fn non_negative_integer(path: &'static str) -> Self {
        Self {
            detail: format!("{path} must be a non-negative integer"),
        }
    }

    fn integer(path: &'static str) -> Self {
        Self {
            detail: format!("{path} must be an integer"),
        }
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

pub fn decode_initialize(message: &JsonRpcMessage) -> Result<InitializeParams, RawParamError> {
    Ok(InitializeParams {
        process_id: None,
        root_path: None,
        root_uri: message.string_at(&["params", "rootUri"]),
        capabilities: ClientCapabilities::default(),
        initialization_options: None,
        trace: None,
        workspace_folders: None,
    })
}

pub fn decode_did_open(
    message: &JsonRpcMessage,
) -> Result<DidOpenTextDocumentParams, RawParamError> {
    Ok(DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: required_string(
                message,
                &["params", "textDocument", "uri"],
                "params.textDocument.uri",
            )?,
            language_id: required_string(
                message,
                &["params", "textDocument", "languageId"],
                "params.textDocument.languageId",
            )?,
            version: required_i32(
                message,
                &["params", "textDocument", "version"],
                "params.textDocument.version",
            )?,
            text: required_string(
                message,
                &["params", "textDocument", "text"],
                "params.textDocument.text",
            )?,
        },
    })
}

pub fn decode_did_change(
    message: &JsonRpcMessage,
) -> Result<DidChangeTextDocumentParams, RawParamError> {
    Ok(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: required_document_uri(message)?,
            version: required_i32(
                message,
                &["params", "textDocument", "version"],
                "params.textDocument.version",
            )?,
        },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: None,
            text: required_first_content_change_text(message)?,
        }],
    })
}

pub fn decode_did_save(
    message: &JsonRpcMessage,
) -> Result<DidSaveTextDocumentParams, RawParamError> {
    Ok(DidSaveTextDocumentParams {
        text_document: TextDocumentIdentifier {
            uri: required_document_uri(message)?,
        },
        text: None,
    })
}

pub fn decode_did_close(
    message: &JsonRpcMessage,
) -> Result<DidCloseTextDocumentParams, RawParamError> {
    Ok(DidCloseTextDocumentParams {
        text_document: TextDocumentIdentifier {
            uri: required_document_uri(message)?,
        },
    })
}

pub fn decode_completion(message: &JsonRpcMessage) -> Result<CompletionParams, RawParamError> {
    Ok(CompletionParams {
        text_document_position: decode_text_document_position(message)?,
        context: None,
    })
}

pub fn decode_text_document_position(
    message: &JsonRpcMessage,
) -> Result<TextDocumentPositionParams, RawParamError> {
    Ok(TextDocumentPositionParams {
        text_document: TextDocumentIdentifier {
            uri: required_document_uri(message)?,
        },
        position: required_position(message.value_at(&["params", "position"]), "params.position")?,
    })
}

pub fn decode_document_uri(message: &JsonRpcMessage) -> Result<DocumentUri, RawParamError> {
    required_document_uri(message)
}

pub fn decode_workspace_symbol_query(message: &JsonRpcMessage) -> Result<String, RawParamError> {
    required_string(message, &["params", "query"], "params.query")
}

pub fn decode_code_action(message: &JsonRpcMessage) -> Result<CodeActionParams, RawParamError> {
    require_value(message, &["params", "context"], "params.context")?;
    Ok(CodeActionParams {
        text_document: TextDocumentIdentifier {
            uri: required_document_uri(message)?,
        },
        range: required_range(message.value_at(&["params", "range"]), "params.range")?,
        context: CodeActionContext {
            diagnostics: message.diagnostics(),
            only: message.string_vec_at(&["params", "context", "only"]),
            trigger_kind: message.code_action_trigger_kind(),
        },
    })
}

pub fn decode_formatting(
    message: &JsonRpcMessage,
) -> Result<DocumentFormattingParams, RawParamError> {
    Ok(DocumentFormattingParams {
        text_document: TextDocumentIdentifier {
            uri: required_document_uri(message)?,
        },
        options: FormattingOptions::default(),
    })
}

pub fn decode_rename(message: &JsonRpcMessage) -> Result<RenameParams, RawParamError> {
    Ok(RenameParams {
        text_document_position: decode_text_document_position(message)?,
        new_name: required_string(message, &["params", "newName"], "params.newName")?,
    })
}

fn required_document_uri(message: &JsonRpcMessage) -> Result<DocumentUri, RawParamError> {
    required_string(
        message,
        &["params", "textDocument", "uri"],
        "params.textDocument.uri",
    )
}

fn required_first_content_change_text(message: &JsonRpcMessage) -> Result<String, RawParamError> {
    message
        .value_at(&["params", "contentChanges"])
        .and_then(Value::as_array)
        .and_then(|changes| changes.first())
        .and_then(|change| change.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| RawParamError::required("params.contentChanges[0].text"))
}

fn require_value<'a>(
    message: &'a JsonRpcMessage,
    path: &[&str],
    label: &'static str,
) -> Result<&'a Value, RawParamError> {
    message
        .value_at(path)
        .ok_or_else(|| RawParamError::required(label))
}

fn required_string(
    message: &JsonRpcMessage,
    path: &[&str],
    label: &'static str,
) -> Result<String, RawParamError> {
    require_value(message, path, label)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| RawParamError::required(label))
}

fn required_i32(
    message: &JsonRpcMessage,
    path: &[&str],
    label: &'static str,
) -> Result<i32, RawParamError> {
    let value = require_value(message, path, label)?
        .as_i64()
        .ok_or_else(|| RawParamError::integer(label))?;
    i32::try_from(value).map_err(|_| RawParamError::integer(label))
}

fn required_position(
    value: Option<&Value>,
    label: &'static str,
) -> Result<Position, RawParamError> {
    let value = value.ok_or_else(|| RawParamError::required(label))?;
    Ok(Position::new(
        required_u32_value(value.get("line"), position_label(label, "line"))?,
        required_u32_value(value.get("character"), position_label(label, "character"))?,
    ))
}

fn required_range(value: Option<&Value>, label: &'static str) -> Result<Range, RawParamError> {
    let value = value.ok_or_else(|| RawParamError::required(label))?;
    Ok(Range::new(
        required_position(value.get("start"), "params.range.start")?,
        required_position(value.get("end"), "params.range.end")?,
    ))
}

fn required_u32_value(value: Option<&Value>, label: &'static str) -> Result<u32, RawParamError> {
    let value = value
        .and_then(Value::as_i64)
        .ok_or_else(|| RawParamError::required(label))?;
    u32::try_from(value).map_err(|_| RawParamError::non_negative_integer(label))
}

fn position_label(base: &'static str, field: &'static str) -> &'static str {
    match (base, field) {
        ("params.position", "line") => "params.position.line",
        ("params.position", "character") => "params.position.character",
        ("params.range.start", "line") => "params.range.start.line",
        ("params.range.start", "character") => "params.range.start.character",
        ("params.range.end", "line") => "params.range.end.line",
        ("params.range.end", "character") => "params.range.end.character",
        _ => base,
    }
}
