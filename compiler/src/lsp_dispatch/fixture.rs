use serde_json::Value;
use sha2::{Digest, Sha256};

use super::model::{LspDispatchDigest, LspDispatchFixture, LspDispatchObserved};

pub(super) struct RawFixture {
    pub(super) id: &'static str,
    pub(super) method: &'static str,
    pub(super) content: String,
}

pub(super) fn fixture_sequence() -> Vec<RawFixture> {
    let uri = "file:///workspace/main.quanta";
    let source = "// fixture comment\n// folded comment\nfn helper() -> i32 { 1 }   \nfn main() { helper(); }\n";
    let changed_source = "// fixture comment\n// folded comment\nfn helper() -> i32 { 2 }\nfn main() { helper(); }\n";
    let document = serde_json::json!({"uri": uri});
    let position = serde_json::json!({"line": 3, "character": 14});
    vec![
        fixture(
            "initialize",
            "initialize",
            serde_json::json!({"rootUri": "file:///workspace"}),
        ),
        fixture("initialized", "initialized", serde_json::json!({})),
        fixture(
            "did-open",
            "textDocument/didOpen",
            serde_json::json!({"textDocument": {"uri": uri, "languageId": "quanta", "version": 1, "text": source}}),
        ),
        text_document_fixture(
            2,
            "document-symbol",
            "textDocument/documentSymbol",
            &document,
        ),
        text_position_fixture(
            3,
            "completion",
            "textDocument/completion",
            &document,
            &position,
        ),
        text_position_fixture(4, "hover", "textDocument/hover", &document, &position),
        text_position_fixture(
            5,
            "definition",
            "textDocument/definition",
            &document,
            &position,
        ),
        text_position_fixture(
            6,
            "references",
            "textDocument/references",
            &document,
            &position,
        ),
        text_document_fixture(7, "formatting", "textDocument/formatting", &document),
        text_document_fixture(8, "folding-range", "textDocument/foldingRange", &document),
        fixture(
            "did-change",
            "textDocument/didChange",
            serde_json::json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": changed_source}]}),
        ),
        request_fixture(9, "shutdown", "shutdown", serde_json::json!({})),
        fixture("exit", "exit", serde_json::json!({})),
    ]
}

pub(super) fn build_fixture(
    raw: RawFixture,
    response: Option<&str>,
) -> Result<LspDispatchFixture, String> {
    let response_kind = response_kind(response);
    let value = response
        .map(|content| serde_json::from_str::<Value>(content))
        .transpose()
        .map_err(|err| {
            format!(
                "lsp dispatch fixture {} returned invalid JSON: {err}",
                raw.id
            )
        })?;
    Ok(LspDispatchFixture {
        id: raw.id.to_string(),
        method: raw.method.to_string(),
        response_kind: response_kind.to_string(),
        result_digest: digest_response(value.as_ref())?,
        observed: observe_response(raw.method, value.as_ref()),
    })
}

fn fixture(id: &'static str, method: &'static str, params: Value) -> RawFixture {
    RawFixture {
        id,
        method,
        content: serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params})
            .to_string(),
    }
}

fn request_fixture(
    id: u32,
    fixture_id: &'static str,
    method: &'static str,
    params: Value,
) -> RawFixture {
    RawFixture {
        id: fixture_id,
        method,
        content:
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
                .to_string(),
    }
}

fn text_document_fixture(
    id: u32,
    fixture_id: &'static str,
    method: &'static str,
    document: &Value,
) -> RawFixture {
    request_fixture(
        id,
        fixture_id,
        method,
        serde_json::json!({"textDocument": document}),
    )
}

fn text_position_fixture(
    id: u32,
    fixture_id: &'static str,
    method: &'static str,
    document: &Value,
    position: &Value,
) -> RawFixture {
    request_fixture(
        id,
        fixture_id,
        method,
        serde_json::json!({"textDocument": document, "position": position}),
    )
}

fn response_kind(response: Option<&str>) -> &'static str {
    match response.and_then(|content| serde_json::from_str::<Value>(content).ok()) {
        None => "none",
        Some(value) if value.get("method").is_some() && value.get("id").is_none() => "notification",
        Some(_) => "response",
    }
}

fn digest_response(response: Option<&Value>) -> Result<LspDispatchDigest, String> {
    let bytes = match response {
        Some(value) => serde_json::to_vec(value)
            .map_err(|err| format!("lsp dispatch failed to normalize response JSON: {err}"))?,
        None => b"none".to_vec(),
    };
    Ok(LspDispatchDigest {
        algorithm: "sha256".to_string(),
        hex: digest_hex(&bytes),
    })
}

fn observe_response(method: &str, response: Option<&Value>) -> LspDispatchObserved {
    let mut observed = LspDispatchObserved {
        has_result: response.is_some_and(|value| value.get("result").is_some()),
        diagnostics: 0,
        completion_items: 0,
        document_symbols: 0,
        locations: 0,
        text_edits: 0,
        folding_ranges: 0,
    };
    let Some(value) = response else {
        return observed;
    };
    if value.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics") {
        observed.diagnostics = value
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        return observed;
    }
    match method {
        "textDocument/completion" => {
            observed.completion_items = value
                .pointer("/result/items")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
        }
        "textDocument/documentSymbol" => observed.document_symbols = result_array_len(value),
        "textDocument/definition" | "textDocument/references" => {
            observed.locations = result_array_len(value)
        }
        "textDocument/formatting" => observed.text_edits = result_array_len(value),
        "textDocument/foldingRange" => observed.folding_ranges = result_array_len(value),
        _ => {}
    }
    observed
}

fn result_array_len(value: &Value) -> usize {
    value
        .get("result")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write digest hex");
    }
    hex
}
