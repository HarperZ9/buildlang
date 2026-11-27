// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 88B integration tests for `buildc bench`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "buildlang_wave88b_{}_{}_{}",
            std::process::id(),
            id,
            label
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, content).expect("write fixture");
        p
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_buildc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .args(args)
        .output()
        .expect("spawn buildc")
}

fn is_sha256_prefixed(s: &str) -> bool {
    match s.strip_prefix("sha256:") {
        Some(hex) => hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
        None => false,
    }
}

fn deterministic_kernel_src() -> &'static str {
    r#"fn compute(n: i64) -> i64 {
    let mut acc: i64 = 0;
    let mut i: i64 = 1;
    while i <= n {
        let term = (i * 3) + (i * 5) - (i * 0);
        acc = acc + term;
        i = i + 1;
    }
    acc
}

fn main() ~ Console {
    let total = compute(10000);
    println!("total={}", total);
}
"#
}

#[test]
fn bench_deterministic_json_and_superopt_parity() {
    let td = TempDirGuard::new("det_json");
    let src = td.write("det_kernel.bld", deterministic_kernel_src());
    let src_str = src.to_str().expect("utf8 path");

    let base_out = run_buildc(&["bench", src_str, "--runs", "5", "--warmup", "2", "--json"]);
    assert!(
        base_out.status.success(),
        "baseline bench failed: stderr={}",
        String::from_utf8_lossy(&base_out.stderr)
    );
    let base_val: serde_json::Value =
        serde_json::from_slice(&base_out.stdout).expect("parse baseline bench JSON");

    assert_eq!(base_val["schema"], "buildlang.bench/v1");
    assert_eq!(base_val["superopt"], false);
    assert_eq!(base_val["warmup"], 2);
    assert_eq!(base_val["runs"], 5);
    assert_eq!(base_val["deterministic"], true);

    let samples = base_val["samples_ms"].as_array().expect("samples_ms array");
    assert_eq!(samples.len(), 5);

    let min_ms = base_val["min_ms"].as_f64().expect("min_ms");
    let p50_ms = base_val["p50_ms"].as_f64().expect("p50_ms");
    let p95_ms = base_val["p95_ms"].as_f64().expect("p95_ms");
    let max_ms = base_val["max_ms"].as_f64().expect("max_ms");
    let mean_ms = base_val["mean_ms"].as_f64().expect("mean_ms");
    assert!(min_ms >= 0.0);
    assert!(min_ms <= p50_ms && p50_ms <= p95_ms && p95_ms <= max_ms);
    assert!(min_ms <= mean_ms && mean_ms <= max_ms);

    let binary_bytes = base_val["binary_bytes"].as_u64().expect("binary_bytes");
    assert!(binary_bytes > 0);
    let base_bin_sha = base_val["binary_sha256"].as_str().expect("binary_sha256");
    let base_out_sha = base_val["stdout_sha256"].as_str().expect("stdout_sha256");
    assert!(is_sha256_prefixed(base_bin_sha));
    assert!(is_sha256_prefixed(base_out_sha));

    // Superopt lane must emit byte-identical stdout_sha256.
    let sopt_out = run_buildc(&[
        "bench",
        src_str,
        "--runs",
        "5",
        "--warmup",
        "2",
        "--superopt",
        "--json",
    ]);
    assert!(
        sopt_out.status.success(),
        "superopt bench failed: stderr={}",
        String::from_utf8_lossy(&sopt_out.stderr)
    );
    let sopt_val: serde_json::Value =
        serde_json::from_slice(&sopt_out.stdout).expect("parse superopt bench JSON");
    assert_eq!(sopt_val["schema"], "buildlang.bench/v1");
    assert_eq!(sopt_val["superopt"], true);
    assert_eq!(sopt_val["deterministic"], true);
    assert_eq!(sopt_val["stdout_sha256"], base_val["stdout_sha256"]);
    assert!(is_sha256_prefixed(
        sopt_val["binary_sha256"]
            .as_str()
            .expect("sopt binary_sha256")
    ));
}

#[test]
fn bench_human_output_reports_quantiles_and_determinism() {
    let td = TempDirGuard::new("human");
    let src = td.write("human_kernel.bld", deterministic_kernel_src());
    let src_str = src.to_str().expect("utf8 path");

    let out = run_buildc(&["bench", src_str, "--runs", "3", "--warmup", "1"]);
    assert!(
        out.status.success(),
        "human bench failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("bench ("), "stdout was: {stdout}");
    assert!(stdout.contains("min="), "stdout was: {stdout}");
    assert!(stdout.contains("p50="), "stdout was: {stdout}");
    assert!(stdout.contains("p95="), "stdout was: {stdout}");
    assert!(
        stdout.contains("deterministic=true"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("sha256:"), "stdout was: {stdout}");
}

#[test]
fn bench_planted_nondeterministic_output_fails_closed() {
    let td = TempDirGuard::new("nondet");
    let src = td.write(
        "nondet_clock.bld",
        r#"fn main() ~ Console + Clock {
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < 50000 {
        acc = acc + i;
        i = i + 1;
    }
    println!("{} {}", clock_ns(), acc);
}
"#,
    );
    let src_str = src.to_str().expect("utf8 path");

    let out = run_buildc(&["bench", src_str, "--runs", "3", "--warmup", "1", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "non-deterministic fixture must fail closed with exit 1; stdout={}, stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let val: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("nondet bench still emits JSON report");
    assert_eq!(val["schema"], "buildlang.bench/v1");
    assert_eq!(val["deterministic"], false);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("non-deterministic stdout detected"),
        "expected determinism error on stderr, got: {stderr}"
    );
}

#[test]
fn bench_planted_compile_error_fails_closed() {
    let td = TempDirGuard::new("compile_err");
    let src = td.write(
        "broken.bld",
        r#"fn main() ~ Console {
    let x: i64 = "not_an_int";
    println!("{}", x);
}
"#,
    );
    let src_str = src.to_str().expect("utf8 path");

    let out = run_buildc(&["bench", src_str, "--runs", "2", "--warmup", "0", "--json"]);
    assert!(
        !out.status.success(),
        "broken fixture must fail compilation"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("buildlang.bench/v1"),
        "broken fixture must not emit a bench report"
    );
}

#[test]
fn bench_invalid_cli_args_exit_2() {
    let td = TempDirGuard::new("bad_args");
    let src = td.write("ok.bld", deterministic_kernel_src());
    let src_str = src.to_str().expect("utf8 path");

    let zero_runs = run_buildc(&["bench", src_str, "--runs", "0"]);
    assert_eq!(
        zero_runs.status.code(),
        Some(2),
        "--runs 0 must exit 2; stderr={}",
        String::from_utf8_lossy(&zero_runs.stderr)
    );

    let missing = td.0.join("does_not_exist.bld");
    let missing_path: &Path = &missing;
    let missing_out = run_buildc(&["bench", missing_path.to_str().expect("utf8"), "--runs", "1"]);
    assert_eq!(
        missing_out.status.code(),
        Some(2),
        "missing file must exit 2; stderr={}",
        String::from_utf8_lossy(&missing_out.stderr)
    );
}
