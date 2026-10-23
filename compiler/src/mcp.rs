// ===============================================================================
// BUILDLANG COMPILER - MCP SERVER
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! `buildc mcp`: the compiler as Model Context Protocol tools over stdio.
//!
//! Transport: one JSON-RPC 2.0 message per line on stdin and stdout, as the
//! MCP stdio transport specifies. Nothing but protocol messages is written to
//! stdout. Tools:
//!
//! - `check`: check a file (`path`) or a source buffer (`source`) and return
//!   the `buildlang.diagnostics/v1` report.
//! - `explain`: the title of a diagnostic code, or the whole code table.
//!
//! The server reads and checks programs; it never runs them, so a client gets
//! no execution authority from it.

use std::io::{BufRead, Write};
use std::path::Path;

use buildlang::diagnostics::{check::check_buffer, codes, Report};
use buildlang::lexer::SourceFile;
use serde_json::{json, Value};

/// Protocol revisions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2026-07-28", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Serve until stdin closes. `check_path` runs the full file check (imports,
/// includes and modules resolved) and returns its report.
pub fn serve(check_path: &dyn Fn(&Path) -> Report) -> Result<(), i32> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle_line(&line, check_path) {
            let mut out = stdout.lock();
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                return Err(1);
            }
        }
    }
    Ok(())
}

/// Handle one incoming line. Returns the response text, or `None` for a
/// notification.
pub fn handle_line(line: &str, check_path: &dyn Fn(&Path) -> Report) -> Option<String> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
    };
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str);
    let (id, method) = match (id, method) {
        (Some(id), Some(method)) => (id, method),
        // A notification (`notifications/initialized` and the like): no reply.
        (None, Some(_)) => return None,
        // A response from the client: nothing to answer.
        (Some(_), None) => return None,
        (None, None) => return Some(error(Value::Null, -32600, "invalid request")),
    };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => call_tool(&params, check_path),
        other => Err((-32601, format!("method not found: {other}"))),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string(),
        Err((code, message)) => error(id, code, &message),
    })
}

fn error(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|v| PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "buildc", "version": buildlang::VERSION },
        "instructions": "Check BuildLang programs. Call `check` after every edit and read each diagnostic's code, help and suggestions. A suggestion with widens_authority adds a capability; decide whether the code should have it before applying it."
    })
}

fn tool_list() -> Value {
    json!([
        {
            "name": "check",
            "title": "Check a BuildLang program",
            "description": "Type-check a BuildLang program and confirm the C backend can compile it. Pass `path` for a file on disk (imports and modules resolved) or `source` for a single buffer. Returns the buildlang.diagnostics/v1 report: ok, error_count and diagnostics with stable BLxxxx codes, spans, help and suggestions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to a .bld file" },
                    "source": { "type": "string", "description": "BuildLang source text" }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "explain",
            "title": "Explain a diagnostic code",
            "description": "Return the meaning of a BuildLang diagnostic code such as BL0403, or the whole code table when `code` is omitted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "A code such as BL0301" }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        }
    ])
}

fn text_result(text: String, structured: Option<Value>, is_error: bool) -> Value {
    let mut out = json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error
    });
    if let Some(s) = structured {
        out["structuredContent"] = s;
    }
    out
}

fn call_tool(params: &Value, check_path: &dyn Fn(&Path) -> Report) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "tools/call needs a tool name".to_string()))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    match name {
        "check" => {
            let report = match (
                args.get("path").and_then(Value::as_str),
                args.get("source").and_then(Value::as_str),
            ) {
                (Some(path), None) => check_path(Path::new(path)),
                (None, Some(source)) => {
                    let file = SourceFile::new("<source>", source.to_string());
                    Report::new("<source>", check_buffer(&file))
                }
                _ => {
                    return Ok(text_result(
                        "pass exactly one of `path` or `source`".to_string(),
                        None,
                        true,
                    ))
                }
            };
            let value = serde_json::to_value(&report).expect("report serializes");
            Ok(text_result(report.to_json(), Some(value), false))
        }
        "explain" => match args.get("code").and_then(Value::as_str) {
            None => {
                let table: Vec<String> = codes::CODES
                    .iter()
                    .map(|i| format!("{}  {}", i.code, i.title))
                    .collect();
                Ok(text_result(table.join("\n"), None, false))
            }
            Some(code) => match codes::lookup(code) {
                Some(info) => Ok(text_result(
                    format!("{}: {}", info.code, info.title),
                    Some(json!({"code": info.code, "title": info.title})),
                    false,
                )),
                None => Ok(text_result(
                    format!("unknown diagnostic code '{code}'"),
                    None,
                    true,
                )),
            },
        },
        other => Err((-32602, format!("unknown tool: {other}"))),
    }
}
