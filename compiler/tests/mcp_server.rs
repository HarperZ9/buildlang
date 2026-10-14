//! M2: `buildc mcp` follows the MCP stdio transport, lifecycle and tools rules.
//!
//! Checked here: one JSON-RPC message per line and nothing else on stdout;
//! version negotiation in `initialize`; no reply to notifications; `tools/list`
//! entries carry a name, a description and an object input schema;
//! `tools/call` returns content (and structured content for `check`); tool
//! failures come back as `isError` results; protocol failures come back as
//! JSON-RPC errors (-32700 parse, -32601 method, -32602 params).
//! Not checked: an official MCP conformance suite was not available here.

use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use serde_json::{json, Value};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Send `lines` to one `buildc mcp` session and return every stdout line.
fn session(lines: &[String]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .arg("mcp")
        .env("BUILDLANG_STDLIB", root().join("stdlib"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start buildc mcp");
    {
        let mut stdin = child.stdin.take().unwrap();
        for line in lines {
            writeln!(stdin, "{line}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "server exits cleanly at end of input");
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| {
            serde_json::from_str(l).unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {l}"))
        })
        .collect()
}

fn req(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn init(version: &str) -> String {
    req(
        1,
        "initialize",
        json!({"protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
    )
}

fn by_id(replies: &[Value], id: u64) -> &Value {
    replies
        .iter()
        .find(|r| r["id"] == json!(id))
        .unwrap_or_else(|| panic!("no reply with id {id}: {replies:?}"))
}

#[test]
fn lifecycle_and_version_negotiation() {
    let replies = session(&[
        init("2026-07-28"),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
        req(2, "ping", json!({})),
    ]);
    assert_eq!(
        replies.len(),
        2,
        "a notification gets no reply: {replies:?}"
    );
    let first = &by_id(&replies, 1)["result"];
    assert_eq!(first["protocolVersion"], "2026-07-28");
    assert!(first["capabilities"]["tools"].is_object());
    assert_eq!(first["serverInfo"]["name"], "buildc");
    assert_eq!(by_id(&replies, 2)["result"], json!({}));
    for r in &replies {
        assert_eq!(r["jsonrpc"], "2.0");
    }

    let replies = session(&[init("1999-01-01")]);
    assert_eq!(
        by_id(&replies, 1)["result"]["protocolVersion"],
        "2026-07-28"
    );
    let replies = session(&[init("2025-06-18")]);
    assert_eq!(
        by_id(&replies, 1)["result"]["protocolVersion"],
        "2025-06-18"
    );
}

#[test]
fn tools_list_and_calls() {
    let hello = root().join("examples/quickstart/hello.bld");
    let replies = session(&[
        init("2026-07-28"),
        req(2, "tools/list", json!({})),
        req(
            3,
            "tools/call",
            json!({"name": "check", "arguments": {"path": hello}}),
        ),
        req(
            4,
            "tools/call",
            json!({"name": "check", "arguments": {"source": "fn main() ~ Console {\n    let x: i32 = \"a\";\n    println!(\"{}\", x);\n}\n"}}),
        ),
        req(5, "tools/call", json!({"name": "check", "arguments": {}})),
        req(
            6,
            "tools/call",
            json!({"name": "explain", "arguments": {"code": "BL0403"}}),
        ),
        req(
            7,
            "tools/call",
            json!({"name": "explain", "arguments": {"code": "BL9999"}}),
        ),
    ]);
    let tools = by_id(&replies, 2)["result"]["tools"].as_array().unwrap();
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["check", "explain"]);
    for t in tools {
        assert!(!t["description"].as_str().unwrap().is_empty());
        assert_eq!(t["inputSchema"]["type"], "object");
    }

    let ok = &by_id(&replies, 3)["result"];
    assert_eq!(ok["isError"], false);
    assert_eq!(
        ok["structuredContent"]["schema"],
        "buildlang.diagnostics/v1"
    );
    assert_eq!(ok["structuredContent"]["ok"], true, "{ok}");
    assert_eq!(ok["content"][0]["type"], "text");

    let bad = &by_id(&replies, 4)["result"]["structuredContent"];
    assert_eq!(bad["ok"], false);
    assert_eq!(bad["diagnostics"][0]["code"], "BL0301");

    assert_eq!(by_id(&replies, 5)["result"]["isError"], true);
    assert!(by_id(&replies, 6)["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("BL0403"));
    assert_eq!(by_id(&replies, 7)["result"]["isError"], true);
}

#[test]
fn protocol_errors() {
    let replies = session(&[
        "{not json".to_string(),
        req(2, "no/such/method", json!({})),
        req(3, "tools/call", json!({"name": "run", "arguments": {}})),
        req(4, "tools/call", json!({})),
    ]);
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert!(replies[0]["id"].is_null());
    assert_eq!(by_id(&replies, 2)["error"]["code"], -32601);
    assert_eq!(by_id(&replies, 3)["error"]["code"], -32602);
    assert_eq!(by_id(&replies, 4)["error"]["code"], -32602);
}
