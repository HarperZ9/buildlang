use serde_json::Value;
use sha2::{Digest, Sha256};

use super::model::{LspDispatchDigest, LspDispatchFixture};
use super::observe::observe_response;

pub(super) struct RawFixture {
    pub(super) id: &'static str,
    pub(super) method: &'static str,
    pub(super) content: String,
}

pub(super) fn fixture_sequence() -> Vec<RawFixture> {
    let uri = "file:///workspace/main.bld";
    let source = "// fixture comment\n// folded comment\nfn helper() -> i32 { 1 }   \nfn main() { helper(); }\n";
    let changed_source = "// fixture comment\n// folded comment\nfn helper() -> i32 { 2 }\nfn main() { helper(); }\n";
    let document = serde_json::json!({"uri": uri});
    let position = serde_json::json!({"line": 3, "character": 14});
    vec![
        raw_fixture(
            "initialize",
            "initialize",
            r#"{
              "jsonrpc": "2.0",
              "params": { "rootUri": "file:///workspace" },
              "method": "initialize"
            }"#,
        ),
        fixture("initialized", "initialized", serde_json::json!({})),
        fixture(
            "did-open",
            "textDocument/didOpen",
            serde_json::json!({"textDocument": {"uri": uri, "languageId": "build", "version": 1, "text": source}}),
        ),
        text_document_fixture(
            12,
            "semantic-tokens",
            "textDocument/semanticTokens/full",
            &document,
        ),
        request_fixture(
            13,
            "workspace-symbol",
            "workspace/symbol",
            serde_json::json!({"query": "help"}),
        ),
        raw_fixture(
            "document-symbol",
            "textDocument/documentSymbol",
            r#"{
              "method": "textDocument/documentSymbol",
              "params": { "textDocument": { "uri": "file:///workspace/main.bld" } },
              "id": 2,
              "jsonrpc": "2.0"
            }"#,
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
        raw_fixture(
            "code-action",
            "textDocument/codeAction",
            r#"{
              "jsonrpc": "2.0",
              "id": 9,
              "method": "textDocument/codeAction",
              "params": {
                "textDocument": { "uri": "file:///workspace/main.bld" },
                "range": {
                  "start": { "line": 2, "character": 24 },
                  "end": { "line": 2, "character": 24 }
                },
                "context": {
                  "diagnostics": [{
                    "range": {
                      "start": { "line": 2, "character": 24 },
                      "end": { "line": 2, "character": 24 }
                    },
                    "severity": 1,
                    "source": "buildlang",
                    "message": "expected ';'"
                  }]
                }
              }
            }"#,
        ),
        raw_fixture(
            "rename",
            "textDocument/rename",
            r#"{
              "jsonrpc": "2.0",
              "id": 10,
              "method": "textDocument/rename",
              "params": {
                "textDocument": { "uri": "file:///workspace/main.bld" },
                "position": { "line": 3, "character": 14 },
                "newName": "renamed_helper"
              }
            }"#,
        ),
        fixture(
            "did-change-type-error",
            "textDocument/didChange",
            serde_json::json!({"textDocument": {"uri": uri, "version": 3}, "contentChanges": [{"text": "const BAD: i32 = \"oops\";\nfn main() {}\n"}]}),
        ),
        request_fixture(11, "shutdown", "shutdown", serde_json::json!({})),
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

fn raw_fixture(id: &'static str, method: &'static str, content: &str) -> RawFixture {
    RawFixture {
        id,
        method,
        content: content.to_string(),
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
        Some(value) => {
            let normalized = normalize_for_digest(value);
            serde_json::to_vec(&normalized)
                .map_err(|err| format!("lsp dispatch failed to normalize response JSON: {err}"))?
        }
        None => b"none".to_vec(),
    };
    Ok(LspDispatchDigest {
        algorithm: "sha256".to_string(),
        hex: digest_hex(&bytes),
    })
}

/// Normalize a captured response before digesting so the receipt is stable
/// across benign metadata that is not a property of the dispatch behavior. The
/// `initialize` result carries the compiler version (`serverInfo.version` =
/// CARGO_PKG_VERSION); pinning it in the digest would break the receipt on every
/// version bump, so it is redacted here (the dispatch behavior is unchanged).
fn normalize_for_digest(value: &Value) -> Value {
    let mut normalized = value.clone();
    if let Some(version) = normalized.pointer_mut("/result/serverInfo/version") {
        *version = Value::String("<redacted>".to_string());
    }
    normalized
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
