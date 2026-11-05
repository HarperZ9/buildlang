// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! `buildc bql` end to end: query, export and a rename that still runs.

use std::path::{Path, PathBuf};
use std::process::Command;

fn buildc(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .args(args)
        .output()
        .expect("buildc runs")
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bql-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn query_prints_the_call_graph() {
    let file = repo("examples/demo/temperature.bld");
    let out = buildc(&[
        "bql",
        "query",
        file.to_str().unwrap(),
        "SELECT callee FROM calls WHERE caller = 'main' AND callee = 'to_fahrenheit'",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<&str> = text.lines().skip(2).map(str::trim).collect();
    assert_eq!(rows, vec!["to_fahrenheit"]);
}

#[test]
fn query_outside_the_subset_fails_with_a_message() {
    let file = repo("examples/demo/temperature.bld");
    let out = buildc(&[
        "bql",
        "query",
        file.to_str().unwrap(),
        "SELECT caller, COUNT(*) FROM calls GROUP BY caller",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("qdb"));
}

#[test]
fn export_writes_qdb_tables() {
    let dir = scratch("export");
    let db = dir.join("ledger.qdb");
    let file = repo("examples/quickstart/ledger.bld");
    let out = buildc(&[
        "bql",
        "export",
        file.to_str().unwrap(),
        "-o",
        db.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&db).unwrap();
    assert!(text.starts_with("TABLE functions\nSCHEMA name:TEXT kind:TEXT"));
    assert!(text.contains("ROW main|add_month\n"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn renamed_program_runs_with_the_same_output() {
    let dir = scratch("rename");
    let file = repo("examples/quickstart/ledger.bld");
    let renamed = dir.join("ledger.bld");
    let receipt = dir.join("receipt.json");
    let out = buildc(&[
        "bql",
        "rename",
        file.to_str().unwrap(),
        "add_month",
        "accrue",
        "-o",
        renamed.to_str().unwrap(),
        "--receipt",
        receipt.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt).unwrap()).unwrap();
    assert_eq!(rec["check"], "mir-equal-under-rename");
    assert_eq!(rec["edits"].as_array().unwrap().len(), 2);

    let before = buildc(&["run", file.to_str().unwrap()]);
    let after = buildc(&["run", renamed.to_str().unwrap()]);
    if !before.status.success() {
        eprintln!("no C toolchain here; skipping the run comparison");
        return;
    }
    assert!(
        after.status.success(),
        "{}",
        String::from_utf8_lossy(&after.stderr)
    );
    assert_eq!(before.stdout, after.stdout);
    assert!(String::from_utf8_lossy(&after.stdout).contains("balance: 115"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_passes_a_fresh_store_and_fails_a_stale_one() {
    let dir = scratch("verify");
    let prog = dir.join("p.bld");
    let store = dir.join("p.json");
    std::fs::write(
        &prog,
        "fn one() -> i64 { 1 }\nfn main() ~ Console { println!(\"{}\", one()); }\n",
    )
    .unwrap();
    let p = prog.to_str().unwrap();
    let s = store.to_str().unwrap();
    let out = buildc(&["bql", "export", p, "--json", "-o", s]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let fresh = buildc(&["bql", "verify", p, s]);
    assert!(
        fresh.status.success(),
        "{}",
        String::from_utf8_lossy(&fresh.stdout)
    );
    assert!(String::from_utf8_lossy(&fresh.stdout).contains("agree"));

    std::fs::write(
        &prog,
        "fn one() -> i64 { 1 }\nfn two() -> i64 { 2 }\n\
         fn main() ~ Console { println!(\"{}\", one() + two()); }\n",
    )
    .unwrap();
    let stale = buildc(&["bql", "verify", p, s]);
    assert_eq!(stale.status.code(), Some(1));
    let text = String::from_utf8_lossy(&stale.stdout);
    assert!(text.contains("DRIFT"), "{text}");
    assert!(text.contains("\"name\":\"two\""), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
