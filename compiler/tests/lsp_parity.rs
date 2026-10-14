//! M2 exit criterion: the language server reports the same codes and help
//! lines as `buildc check --error-format=json`.
//!
//! Drives `buildc lsp` over stdio (initialize, didOpen, read
//! publishDiagnostics, shutdown, exit) and compares its compiler diagnostics
//! with the JSON report for the same file.

use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

use serde_json::{json, Value};

fn buildc() -> Command {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", root.join("stdlib"));
    cmd
}

fn frame(v: &Value) -> Vec<u8> {
    let body = v.to_string();
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
}

fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let mut buf = vec![0; len?];
    reader.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

/// Publish diagnostics for `source` through the language server.
fn lsp_diagnostics(source: &str) -> Vec<Value> {
    let mut child = buildc()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start buildc lsp");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let uri = "file:///parity.bld";
    let msgs = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{}}}),
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"buildlang","version":1,"text":source}}}),
    ];
    for m in &msgs {
        stdin.write_all(&frame(m)).unwrap();
    }
    stdin.flush().unwrap();
    let mut found = None;
    while let Some(msg) = read_message(&mut out) {
        if msg["method"] == "textDocument/publishDiagnostics" {
            found = Some(
                msg["params"]["diagnostics"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
            break;
        }
    }
    stdin
        .write_all(&frame(&json!({"jsonrpc":"2.0","id":2,"method":"shutdown"})))
        .ok();
    stdin
        .write_all(&frame(&json!({"jsonrpc":"2.0","method":"exit"})))
        .ok();
    drop(stdin);
    let _ = child.wait();
    found.expect("the server published diagnostics")
}

fn cli_report(name: &str, source: &str) -> Value {
    let dir = std::env::temp_dir().join(format!("buildlang_lsp_parity_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{name}.bld"));
    std::fs::write(&file, source).unwrap();
    let out = buildc()
        .arg("check")
        .arg(&file)
        .arg("--error-format=json")
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).expect("JSON report")
}

#[test]
fn lsp_and_cli_report_the_same_codes_and_help() {
    let cases = [
        ("clean", "fn main() ~ Console {\n    println!(\"hi\");\n}\n"),
        ("type", "fn main() ~ Console {\n    let x: i32 = \"a\";\n    println!(\"{}\", x);\n}\n"),
        ("effect", "fn main() ~ Console {\n    let s = read_file(\"a.txt\");\n    println!(\"{}\", s);\n}\n"),
        ("macro_arg", "fn f(n: i32) -> i32 { n }\nfn main() ~ Console {\n    println!(\"{}\", f(\"x\"));\n}\n"),
        ("backend", "fn main() ~ Console {\n    let n = \"12\".parse::<i64>().unwrap();\n    println!(\"{}\", n);\n}\n"),
        ("lexer", "fn main() ~ Console {\n    let s = \"open;\n}\n"),
    ];
    for (name, source) in cases {
        let report = cli_report(name, source);
        let cli: Vec<&Value> = report["diagnostics"].as_array().unwrap().iter().collect();
        let lsp: Vec<Value> = lsp_diagnostics(source)
            .into_iter()
            .filter(|d| d["code"].as_str().is_some_and(|c| c.starts_with("BL")))
            .collect();
        let cli_codes: Vec<&str> = cli.iter().map(|d| d["code"].as_str().unwrap()).collect();
        let lsp_codes: Vec<&str> = lsp.iter().map(|d| d["code"].as_str().unwrap()).collect();
        assert_eq!(lsp_codes, cli_codes, "{name}: codes differ");
        for (c, l) in cli.iter().zip(&lsp) {
            let message = l["message"].as_str().unwrap();
            assert!(
                message.starts_with(c["message"].as_str().unwrap()),
                "{name}: {message}"
            );
            for help in c["help"].as_array().unwrap() {
                assert!(
                    message.contains(help.as_str().unwrap()),
                    "{name}: help missing in LSP: {help}"
                );
            }
            if let Some(span) = c["span"].as_object() {
                assert_eq!(
                    l["range"]["start"]["line"],
                    json!(span["line"].as_u64().unwrap() - 1),
                    "{name}"
                );
            }
        }
        if name == "clean" {
            assert!(cli_codes.is_empty());
        } else {
            assert!(
                !cli_codes.is_empty(),
                "{name}: the case must produce a diagnostic"
            );
        }
    }
}
