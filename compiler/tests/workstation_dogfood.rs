//! Dogfood verification for `universe/workstation/kcore_fast.bld` and
//! `universe/workstation/workstation_mcp.bld`.
//!
//! Runs `buildc check`, `buildc lint`, and `buildc run --release` on both
//! modules, verifies multi-thread `par_for` bit-exact determinism at 1 and 2
//! threads, runs the MCP `--self-test` suite (including planted controls), and
//! drives a live piped JSON-RPC 2.0 stdio session against `workstation_mcp.bld`.

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
            out.contains("[PASS] kcore_fast summary failures=0"),
            "{label} failed:\n{out}"
        );
        assert!(!out.contains("[FAIL]"), "{label} reported failure:\n{out}");
    }

    let h1 = extract_par_hash(&out_1);
    let h2 = extract_par_hash(&out_2);
    assert_eq!(h1, h2, "par_for hash drifted between 1 and 2 threads");
    assert_eq!(
        h1, "fc904aa6dfed3a04",
        "unexpected FNV-1a parity hash for kcore_fast sweep"
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
    let frames = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#.to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string(),
        format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"check_em_dash_and_secret","arguments":{{"path":"{readme}"}}}}}}"#
        ),
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
    assert_eq!(lines.len(), 3, "expected 3 JSON-RPC responses:\n{stdout}");

    let init: serde_json::Value = serde_json::from_str(lines[0]).expect("init json");
    assert_eq!(
        init["result"]["serverInfo"]["name"],
        "buildlang-workstation-mcp"
    );

    let list: serde_json::Value = serde_json::from_str(lines[1]).expect("list json");
    assert_eq!(list["result"]["tools"].as_array().map(Vec::len), Some(2));

    let scan: serde_json::Value = serde_json::from_str(lines[2]).expect("scan json");
    assert_eq!(scan["result"]["isError"], false);
    assert_eq!(scan["result"]["structuredContent"]["clean"], true);
    assert_eq!(scan["result"]["structuredContent"]["em_dashes"], 0);
    assert_eq!(scan["result"]["structuredContent"]["secret_hits"], 0);
    assert!(Path::new(&readme).is_file());
}
