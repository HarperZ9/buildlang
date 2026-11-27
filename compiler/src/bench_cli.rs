// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Native deterministic micro-benchmark subcommand (`buildc bench`).
//!
//! Compiles a `.bld` file in release mode (with optional V5/V6 `--superopt`),
//! runs `W` warmups and `N` timed executions, verifies byte-identical stdout
//! SHA-256 across every run (failing closed if non-deterministic output is
//! detected), and emits human or `buildlang.bench/v1` JSON metrics.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use buildlang::codegen::StdioMode;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Canonical schema identifier for `buildc bench --json` output.
pub(crate) const BENCH_SCHEMA: &str = "buildlang.bench/v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct BenchReport {
    pub schema: &'static str,
    pub compiler_version: &'static str,
    pub file: String,
    pub superopt: bool,
    pub warmup: usize,
    pub runs: usize,
    pub min_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub mean_ms: f64,
    pub binary_bytes: u64,
    pub binary_sha256: String,
    pub stdout_sha256: String,
    pub deterministic: bool,
    pub samples_ms: Vec<f64>,
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn sha256_prefixed(bytes: &[u8]) -> String {
    let hex: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("sha256:{hex}")
}

fn round_us(ms: f64) -> f64 {
    (ms * 1000.0).round() / 1000.0
}

/// Compute `(min, p50, p95, max, mean)` from a non-empty slice of millisecond samples.
pub(crate) fn compute_stats(samples_ms: &[f64]) -> (f64, f64, f64, f64, f64) {
    assert!(!samples_ms.is_empty(), "samples_ms must be non-empty");
    let mut sorted = samples_ms.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let n = sorted.len();
    let min_ms = sorted[0];
    let max_ms = sorted[n - 1];
    let mean_ms = sorted.iter().sum::<f64>() / (n as f64);
    let p50_ms = percentile_sorted(&sorted, 50.0);
    let p95_ms = percentile_sorted(&sorted, 95.0);
    (
        round_us(min_ms),
        round_us(p50_ms),
        round_us(p95_ms),
        round_us(max_ms),
        round_us(mean_ms),
    )
}

fn percentile_sorted(sorted: &[f64], pct: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let rank = (pct / 100.0) * ((n - 1) as f64);
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = rank - (lo as f64);
        sorted[lo] + frac * (sorted[hi] - sorted[lo])
    }
}

/// Entry point for `buildc bench <file.bld> [--runs N] [--warmup W] [--superopt] [--json]`.
pub(crate) fn cmd_bench(
    file: &Path,
    runs: usize,
    warmup: usize,
    superopt: bool,
    json: bool,
) -> Result<(), i32> {
    if runs == 0 {
        eprintln!("Error: --runs must be at least 1");
        return Err(2);
    }
    if !file.exists() {
        eprintln!("Error: file '{}' does not exist", file.display());
        return Err(2);
    }
    if !file.is_file() {
        eprintln!("Error: '{}' is not a regular file", file.display());
        return Err(2);
    }

    // Compile in release mode (with optional --superopt) into a temporary directory.
    let compiled =
        crate::compile_program_to_exe(file, StdioMode::Native, None, true, superopt, None)?;
    let _guard = TempDirGuard(compiled.temp_dir);

    let bin_bytes = std::fs::read(&compiled.exe_file).map_err(|e| {
        eprintln!(
            "Error reading compiled binary '{}': {}",
            compiled.exe_file.display(),
            e
        );
        1
    })?;
    let binary_bytes = bin_bytes.len() as u64;
    let binary_sha256 = sha256_prefixed(&bin_bytes);

    let mut expected_stdout_hash: Option<String> = None;
    let mut deterministic = true;
    let mut mismatch_detail: Option<String> = None;

    // Warmup runs (stdout still hashed for determinism checking).
    for w in 0..warmup {
        let output = Command::new(&compiled.exe_file).output().map_err(|e| {
            eprintln!("Error executing warmup run {}: {}", w + 1, e);
            1
        })?;
        if !output.status.success() {
            let code = output.status.code().unwrap_or(1);
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "Error: benchmark warmup run {} exited with status {}: {}",
                w + 1,
                code,
                stderr.trim()
            );
            return Err(if code == 0 { 1 } else { code });
        }
        let h = sha256_prefixed(&output.stdout);
        match &expected_stdout_hash {
            None => expected_stdout_hash = Some(h),
            Some(prev) if prev != &h => {
                deterministic = false;
                if mismatch_detail.is_none() {
                    mismatch_detail = Some(format!(
                        "warmup run {} stdout_sha256 {} differed from baseline {}",
                        w + 1,
                        h,
                        prev
                    ));
                }
            }
            Some(_) => {}
        }
    }

    // Timed runs.
    let mut samples_ms = Vec::with_capacity(runs);
    for r in 0..runs {
        let t0 = Instant::now();
        let output = Command::new(&compiled.exe_file).output().map_err(|e| {
            eprintln!("Error executing timed run {}: {}", r + 1, e);
            1
        })?;
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        if !output.status.success() {
            let code = output.status.code().unwrap_or(1);
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "Error: benchmark run {} exited with status {}: {}",
                r + 1,
                code,
                stderr.trim()
            );
            return Err(if code == 0 { 1 } else { code });
        }
        let h = sha256_prefixed(&output.stdout);
        match &expected_stdout_hash {
            None => expected_stdout_hash = Some(h),
            Some(prev) if prev != &h => {
                deterministic = false;
                if mismatch_detail.is_none() {
                    mismatch_detail = Some(format!(
                        "timed run {} stdout_sha256 {} differed from baseline {}",
                        r + 1,
                        h,
                        prev
                    ));
                }
            }
            Some(_) => {}
        }
        samples_ms.push(round_us(elapsed_ms));
    }

    let (min_ms, p50_ms, p95_ms, max_ms, mean_ms) = compute_stats(&samples_ms);
    let stdout_sha256 = expected_stdout_hash.unwrap_or_else(|| sha256_prefixed(&[]));
    let display_file = file.to_string_lossy().replace('\\', "/");

    let report = BenchReport {
        schema: BENCH_SCHEMA,
        compiler_version: buildlang::VERSION,
        file: display_file,
        superopt,
        warmup,
        runs,
        min_ms,
        p50_ms,
        p95_ms,
        max_ms,
        mean_ms,
        binary_bytes,
        binary_sha256,
        stdout_sha256,
        deterministic,
        samples_ms,
    };

    if json {
        let rendered = serde_json::to_string_pretty(&report).map_err(|e| {
            eprintln!("Error serializing bench JSON: {}", e);
            1
        })?;
        println!("{}", rendered);
    } else {
        println!(
            "buildc {} bench ({}, superopt={}, warmup={}, runs={}):",
            report.compiler_version, report.file, report.superopt, report.warmup, report.runs
        );
        println!(
            "  min={:.3} ms  p50={:.3} ms  p95={:.3} ms  max={:.3} ms  mean={:.3} ms",
            report.min_ms, report.p50_ms, report.p95_ms, report.max_ms, report.mean_ms
        );
        println!(
            "  binary={} bytes ({})  stdout={}  deterministic={}",
            report.binary_bytes, report.binary_sha256, report.stdout_sha256, report.deterministic
        );
    }

    if deterministic {
        Ok(())
    } else {
        eprintln!(
            "Error: non-deterministic stdout detected ({})",
            mismatch_detail.unwrap_or_else(|| "stdout SHA-256 mismatch".to_string())
        );
        Err(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_stats_exact_quantiles() {
        let samples = [10.0, 20.0, 30.0, 40.0, 50.0];
        let (min_ms, p50_ms, p95_ms, max_ms, mean_ms) = compute_stats(&samples);
        assert_eq!(min_ms, 10.0);
        assert_eq!(p50_ms, 30.0);
        assert_eq!(p95_ms, 48.0);
        assert_eq!(max_ms, 50.0);
        assert_eq!(mean_ms, 30.0);
    }

    #[test]
    fn sha256_prefixed_matches_known_abc_vector() {
        assert_eq!(
            sha256_prefixed(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
