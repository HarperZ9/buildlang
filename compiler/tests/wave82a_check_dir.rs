// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 82A end-to-end integration tests for `buildc check-dir`:
//! - Parallel multi-file checking with deterministic sorted JSON output across
//!   `--jobs 1` and `--jobs 4`.
//! - Planted failing `.bld` files (parse & type errors) caught fail-closed.
//! - Expected-failure list (`--fail-list`, `CHECK-FAILS.txt` format) matching,
//!   stale xfail detection (file passes, stage changed, or file missing), and
//!   human summary output.
//! - Invalid CLI arguments (`--jobs 0`, missing directory) rejected fail-closed.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn buildc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_buildc"))
}

fn stdlib_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("stdlib")
}

fn make_temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_wave82a_{}_{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_valid_file(dir: &std::path::Path, rel: &str, val: i64) {
    let full = dir.join(rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(
        &full,
        format!(
            "pub fn compute(x: i64) -> i64 {{\n    x + {}\n}}\nfn main() -> i32 {{ 0 }}\n",
            val
        ),
    )
    .unwrap();
}

#[test]
fn wave82a_check_dir_deterministic_json_across_jobs_1_and_4() {
    let dir = make_temp_dir("determinism");
    let sub_root = dir.join("tree");
    std::fs::create_dir_all(&sub_root).unwrap();

    // Populate a nested directory with passing and planted failing `.bld` files.
    for i in 0..8 {
        let rel = format!("pkg_{}/mod_{}.bld", i % 3, i);
        write_valid_file(&sub_root, &rel, i + 1);
    }
    let bad_type = sub_root.join("z_bad/type_err.bld");
    std::fs::create_dir_all(bad_type.parent().unwrap()).unwrap();
    std::fs::write(
        &bad_type,
        "fn main() -> i32 {\n    let x: i32 = \"not_an_int\";\n    x\n}\n",
    )
    .unwrap();

    let bad_parse = sub_root.join("a_bad/parse_err.bld");
    std::fs::create_dir_all(bad_parse.parent().unwrap()).unwrap();
    std::fs::write(&bad_parse, "fn broken( -> i32 { let = ; }\n").unwrap();

    // Fail-list matching both planted failures -> exit 0!
    let fail_list = dir.join("CHECK-FAILS.txt");
    std::fs::write(
        &fail_list,
        "# Expected failures\na_bad/parse_err.bld parse\nz_bad/type_err.bld types\n",
    )
    .unwrap();

    let run_jobs = |jobs: &str| -> Value {
        let out = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                "check-dir",
                sub_root.to_str().unwrap(),
                "--jobs",
                jobs,
                "--json",
                "--fail-list",
                fail_list.to_str().unwrap(),
            ])
            .output()
            .expect("buildc check-dir failed to execute");
        assert!(
            out.status.success(),
            "check-dir --jobs {jobs} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("check-dir --json must emit valid JSON")
    };

    let j1 = run_jobs("1");
    let j4 = run_jobs("4");

    assert_eq!(j1["schema"], "buildlang.check-dir/v1");
    assert_eq!(j1["total"], 10);
    assert_eq!(j1["passed"], 8);
    assert_eq!(j1["expected_fail"], 2);
    assert_eq!(j1["unexpected_fail"], 0);
    assert_eq!(j1["stale_expected"], 0);
    assert!(j1["elapsed_ms"].is_u64());

    // Exact determinism across --jobs 1 vs --jobs 4!
    assert_eq!(j1["total"], j4["total"]);
    assert_eq!(j1["passed"], j4["passed"]);
    assert_eq!(j1["expected_fail"], j4["expected_fail"]);
    assert_eq!(j1["unexpected_fail"], j4["unexpected_fail"]);
    assert_eq!(j1["stale_expected"], j4["stale_expected"]);
    assert_eq!(j1["files"], j4["files"]);
    assert_eq!(j1["stale_entries"], j4["stale_entries"]);

    // Verify files are strictly lexicographically sorted by path.
    let paths: Vec<&str> = j4["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "files output must be strictly sorted");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave82a_check_dir_catches_planted_failure_and_stale_xfail_fail_closed() {
    let dir = make_temp_dir("fail_closed");
    let tree = dir.join("tree");
    std::fs::create_dir_all(&tree).unwrap();

    write_valid_file(&tree, "good_a.bld", 10);
    write_valid_file(&tree, "now_fixed.bld", 20);
    std::fs::write(
        tree.join("planted_fail.bld"),
        "fn main() -> i32 {\n    let bad: i64 = \"boom\";\n    0\n}\n",
    )
    .unwrap();

    // 1. Without --fail-list: planted_fail.bld must cause non-zero exit with unexpected_fail == 1.
    let no_list = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["check-dir", tree.to_str().unwrap(), "--jobs", "2", "--json"])
        .output()
        .unwrap();
    assert!(
        !no_list.status.success(),
        "unlisted failing .bld must fail check-dir"
    );
    let no_list_json: Value = serde_json::from_slice(&no_list.stdout).unwrap();
    assert_eq!(no_list_json["passed"], 2);
    assert_eq!(no_list_json["unexpected_fail"], 1);
    assert_eq!(no_list_json["expected_fail"], 0);
    assert_eq!(no_list_json["stale_expected"], 0);

    // 2. Stale fail-list: lists `now_fixed.bld types` (now passes!) and `ghost.bld types` (missing!).
    let stale_list = dir.join("STALE-FAILS.txt");
    std::fs::write(
        &stale_list,
        "planted_fail.bld types\nnow_fixed.bld types\nghost.bld types\n",
    )
    .unwrap();
    let stale_run = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            "check-dir",
            tree.to_str().unwrap(),
            "--jobs",
            "2",
            "--json",
            "--fail-list",
            stale_list.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !stale_run.status.success(),
        "stale xfail entries must fail check-dir fail-closed"
    );
    let stale_json: Value = serde_json::from_slice(&stale_run.stdout).unwrap();
    assert_eq!(stale_json["passed"], 2);
    assert_eq!(stale_json["expected_fail"], 1);
    assert_eq!(stale_json["unexpected_fail"], 0);
    assert_eq!(stale_json["stale_expected"], 2);
    assert_eq!(stale_json["stale_entries"].as_array().unwrap().len(), 2);

    // 3. Human output mode on clean fail-list exits 0 and prints summary counts.
    let exact_list = dir.join("EXACT-FAILS.txt");
    std::fs::write(&exact_list, "planted_fail.bld types\n").unwrap();
    let human = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            "check-dir",
            tree.to_str().unwrap(),
            "--jobs",
            "2",
            "--fail-list",
            exact_list.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        human.status.success(),
        "exact fail-list human mode must exit 0: stderr={}",
        String::from_utf8_lossy(&human.stderr)
    );
    let human_out = String::from_utf8_lossy(&human.stdout);
    assert!(
        human_out.contains("2 passed, 1 expected_fail, 0 unexpected_fail, 0 stale_expected"),
        "unexpected human output: {human_out}"
    );

    // 4. Invalid --jobs 0 and missing dir rejected with exit code 2.
    let bad_jobs = Command::new(buildc_bin())
        .args(["check-dir", tree.to_str().unwrap(), "--jobs", "0"])
        .output()
        .unwrap();
    assert_eq!(bad_jobs.status.code(), Some(2));

    let missing_dir = Command::new(buildc_bin())
        .args(["check-dir", dir.join("does_not_exist").to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(missing_dir.status.code(), Some(2));

    let _ = std::fs::remove_dir_all(&dir);
}
