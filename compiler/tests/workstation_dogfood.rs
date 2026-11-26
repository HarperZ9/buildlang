//! Dogfood verification for `universe/workstation/kcore_fast.bld` (12 verified
//! FORMULAS.tsv equations) and `universe/workstation/workstation_mcp.bld`
//! (8 compact model-native MCP JSON-RPC 2.0 Agent SDK v2 tools).
//!
//! Runs `buildc check`, `buildc lint`, and `buildc run --release` on both
//! modules, verifies multi-thread `par_for` bit-exact determinism at 1 and 2
//! threads against the Python f64 reference hash (`abb396501c5308dd`), runs
//! the MCP `--self-test` suite (including planted controls), and drives a live
//! piped JSON-RPC 2.0 stdio session against `workstation_mcp.bld`.

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn buildc() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", repo_root().join("stdlib"));
    cmd
}

fn assert_check_and_lint(rel: &str) {
    let file = repo_root().join(rel);
    for sub in ["check", "lint"] {
        let out = buildc()
            .arg(sub)
            .arg(&file)
            .output()
            .expect("buildc executes");
        assert!(
            out.status.success(),
            "buildc {sub} {rel} failed:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn run_release(rel: &str, threads: Option<u32>, extra: &[&str]) -> String {
    let file = repo_root().join(rel);
    let mut cmd = buildc();
    if let Some(t) = threads {
        cmd.env("BUILD_PAR_THREADS", t.to_string());
    }
    cmd.args(["run", "--release"]).arg(&file);
    if !extra.is_empty() {
        cmd.arg("--").args(extra);
    }
    let out = cmd.output().expect("buildc run executes");
    assert!(
        out.status.success(),
        "buildc run --release {rel} failed:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

fn extract_par_hash(stdout: &str) -> String {
    let line = stdout
        .lines()
        .find(|l| l.starts_with("[PASS] par_for_bit_exact "))
        .unwrap_or_else(|| panic!("missing par_for_bit_exact line:\n{stdout}"));
    line.split_whitespace()
        .find_map(|tok| tok.strip_prefix("hash="))
        .unwrap_or_else(|| panic!("missing hash= token in {line}"))
        .to_string()
}

#[test]
fn kcore_fast_checks_lints_and_is_bit_exact_across_thread_counts() {
    let rel = "universe/workstation/kcore_fast.bld";
    assert_check_and_lint(rel);

    let out_1 = run_release(rel, Some(1), &[]);
    let out_2 = run_release(rel, Some(2), &[]);

    for (label, out) in [("1-thread", &out_1), ("2-thread", &out_2)] {
        assert!(
            out.contains("[PASS] kcore_fast summary equations=12 failures=0"),
            "{label} failed:\n{out}"
        );
        assert!(!out.contains("[FAIL]"), "{label} reported failure:\n{out}");
    }

    let h1 = extract_par_hash(&out_1);
    let h2 = extract_par_hash(&out_2);
    assert_eq!(h1, h2, "par_for hash drifted between 1 and 2 threads");
    assert_eq!(
        h1, "abb396501c5308dd",
        "unexpected FNV-1a parity hash for 12-equation kcore_fast sweep"
    );
}

#[test]
fn workstation_mcp_checks_lints_passes_self_test_and_serves_stdio_jsonrpc() {
    let rel = "universe/workstation/workstation_mcp.bld";
    assert_check_and_lint(rel);

    let self_test = run_release(rel, None, &["--self-test"]);
    assert!(
        self_test.contains("[PASS] workstation_mcp summary failures=0"),
        "self-test failed:\n{self_test}"
    );
    assert!(
        !self_test.contains("[FAIL]"),
        "self-test reported failure:\n{self_test}"
    );

    let file = repo_root().join(rel);
    let mut child = buildc()
        .args(["run", "--release"])
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn workstation_mcp");

    let readme = repo_root()
        .join("universe/workstation/README.md")
        .to_string_lossy()
        .replace('\\', "/");
    let orphan_target = format!("{}:/dev/public/orphan-silo/app.py", "C");
    let frames = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#
            .to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string(),
        format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"anti_silo_preflight","arguments":{{"intent":"add HLSL shader kernel","target_path":"{orphan_target}"}}}}}}"#
        ),
        format!(
            r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"audit_public_hygiene","arguments":{{"path":"{readme}"}}}}}}"#
        ),
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"bitnet_ternary_dot","arguments":{"weights":[1,-1,0,1],"activations":[2.0,0.5,9.0,1.5]}}}"#
            .to_string(),
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"unknown_sdk_tool","arguments":{}}}"#
            .to_string(),
    ];
    {
        let stdin = child.stdin.as_mut().expect("piped stdin");
        for f in &frames {
            writeln!(stdin, "{f}").expect("write jsonrpc frame");
        }
    }
    let out = child.wait_with_output().expect("wait workstation_mcp");
    assert!(
        out.status.success(),
        "stdio MCP failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 6, "expected 6 JSON-RPC responses:\n{stdout}");

    let init: serde_json::Value = serde_json::from_str(lines[0]).expect("init json");
    assert_eq!(
        init["result"]["serverInfo"]["name"],
        "buildlang-workstation-mcp"
    );
    assert_eq!(init["result"]["serverInfo"]["version"], "2.0.0");

    let list: serde_json::Value = serde_json::from_str(lines[1]).expect("list json");
    assert_eq!(list["result"]["tools"].as_array().map(Vec::len), Some(8));

    let silo: serde_json::Value = serde_json::from_str(lines[2]).expect("silo json");
    assert_eq!(silo["result"]["isError"], false);
    assert_eq!(
        silo["result"]["structuredContent"]["verdict"],
        "REFUSED_SILO"
    );
    assert_eq!(
        silo["result"]["structuredContent"]["flagship_id"],
        "superframe"
    );
    assert_eq!(silo["result"]["structuredContent"]["silo_refused"], true);
    assert_eq!(silo["result"]["structuredContent"]["exit_code"], 3);

    let hyg: serde_json::Value = serde_json::from_str(lines[3]).expect("hygiene json");
    assert_eq!(hyg["result"]["isError"], false);
    assert_eq!(hyg["result"]["structuredContent"]["verdict"], "PASS");
    assert_eq!(hyg["result"]["structuredContent"]["clean"], true);
    assert_eq!(hyg["result"]["structuredContent"]["em_dashes"], 0);
    assert_eq!(hyg["result"]["structuredContent"]["bundler_secrets"], 0);
    assert_eq!(hyg["result"]["structuredContent"]["local_paths"], 0);
    assert_eq!(hyg["result"]["structuredContent"]["retired_brand"], 0);

    let bitnet: serde_json::Value = serde_json::from_str(lines[4]).expect("bitnet json");
    assert_eq!(bitnet["result"]["structuredContent"]["ok"], true);
    assert_eq!(
        bitnet["result"]["structuredContent"]["result"]["dot"].as_f64(),
        Some(3.0)
    );
    assert_eq!(
        bitnet["result"]["structuredContent"]["result"]["fp_multiplies"],
        0
    );

    let unk: serde_json::Value = serde_json::from_str(lines[5]).expect("unknown tool json");
    assert_eq!(unk["error"]["code"], -32601);
    assert_eq!(
        unk["error"]["data"]["valid_tools"].as_array().map(Vec::len),
        Some(8)
    );
    assert!(Path::new(&readme).is_file());
}
