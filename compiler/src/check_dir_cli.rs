// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Parallel multi-file `buildc check-dir` subcommand (`--jobs`, `--json`, `--fail-list`).
//!
//! Discovers every `.bld` file under a root directory in deterministic sorted
//! order, checks them in parallel using stdlib worker threads with dynamic
//! work-stealing, compares results against an optional `CHECK-FAILS.txt`-style
//! expected-failure list, and emits either a human summary or deterministic JSON.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;

/// Canonical schema identifier for `buildc check-dir --json` output.
pub(crate) const CHECK_DIR_SCHEMA: &str = "buildlang.check-dir/v1";

/// Worker thread stack size (64 MiB) to handle deep recursive ASTs cleanly.
const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CheckDirFileResult {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StaleExpectedEntry {
    pub path: String,
    pub expected_stage: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CheckDirSummary {
    pub schema: &'static str,
    pub compiler_version: &'static str,
    pub root: String,
    pub jobs: usize,
    pub total: usize,
    pub passed: usize,
    pub expected_fail: usize,
    pub unexpected_fail: usize,
    pub stale_expected: usize,
    pub elapsed_ms: u64,
    pub files: Vec<CheckDirFileResult>,
    pub stale_entries: Vec<StaleExpectedEntry>,
}

#[derive(Debug, Clone)]
struct RawFileOutcome {
    stage: Option<&'static str>,
    diagnostic: Option<String>,
}

/// Normalize a relative path to forward-slash POSIX form without leading `./`.
pub(crate) fn normalize_rel_path(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    raw.strip_prefix("./").unwrap_or(&raw).to_string()
}

/// Recursively discover all `.bld` files under `root`, returning
/// `(normalized_rel_path, full_path)` sorted strictly by `normalized_rel_path`.
pub(crate) fn discover_bld_files(root: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    if !root.exists() {
        return Err(format!("directory '{}' does not exist", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("'{}' is not a directory", root.display()));
    }
    let mut stack = vec![root.to_path_buf()];
    let mut found = Vec::new();

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("cannot read directory '{}': {}", dir.display(), e))?;
        for entry in entries {
            let entry =
                entry.map_err(|e| format!("error reading entry in '{}': {}", dir.display(), e))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| format!("cannot inspect '{}': {}", path.display(), e))?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && path.extension().and_then(|e| e.to_str()) == Some("bld")
            {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                found.push((normalize_rel_path(rel), path));
            }
        }
    }

    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}

/// Parse an expected-failure list in `CHECK-FAILS.txt` format (`<rel_path> <stage>`).
/// Blank lines and `#` comment lines are ignored.
pub(crate) fn parse_fail_list(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for (idx, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(rel) = parts.next() else {
            continue;
        };
        let Some(stage) = parts.next() else {
            return Err(format!(
                "malformed fail-list line {}: expected '<path> <stage>', found '{}'",
                idx + 1,
                line
            ));
        };
        if parts.next().is_some() {
            return Err(format!(
                "malformed fail-list line {}: unexpected extra token in '{}'",
                idx + 1,
                line
            ));
        }
        let norm_rel = rel.replace('\\', "/");
        let norm_rel = norm_rel.strip_prefix("./").unwrap_or(&norm_rel).to_string();
        map.insert(norm_rel, stage.to_string());
    }
    Ok(map)
}

/// Canonicalize process-global inference variable IDs (`?T12345` -> `?T0`, `?T1`, ...)
/// per diagnostic so `--jobs 1` and `--jobs N` emit byte-identical diagnostics even
/// when multiple worker threads allocate type variables concurrently.
pub(crate) fn canonicalize_diagnostic_tyvars(msg: &str) -> String {
    let bytes = msg.as_bytes();
    let mut out = String::with_capacity(msg.len());
    let mut mapping: BTreeMap<&str, usize> = BTreeMap::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 2 < bytes.len()
            && bytes[i] == b'?'
            && bytes[i + 1] == b'T'
            && bytes[i + 2].is_ascii_digit()
        {
            let start = i + 2;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            let raw_id = &msg[start..end];
            let next_id = mapping.len();
            let norm_id = *mapping.entry(raw_id).or_insert(next_id);
            out.push_str("?T");
            out.push_str(&norm_id.to_string());
            i = end;
        } else {
            let ch = msg[i..].chars().next().expect("valid UTF-8 boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn check_one_file(path: &Path) -> RawFileOutcome {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut early = None;
        match crate::run_check_reporting(path, &mut early) {
            Ok(outcome) => {
                if !outcome.parse_errors.is_empty() {
                    let diag = outcome
                        .parse_errors
                        .first()
                        .map(|e| format!("{}:{}: {}", e.line, e.col, e.message));
                    RawFileOutcome {
                        stage: Some("parse"),
                        diagnostic: diag,
                    }
                } else if !outcome.type_errors.is_empty() {
                    let diag = outcome
                        .diagnostics
                        .first()
                        .map(|d| canonicalize_diagnostic_tyvars(&d.message))
                        .or_else(|| {
                            outcome
                                .type_errors
                                .first()
                                .map(|e| canonicalize_diagnostic_tyvars(&e.error.to_string()))
                        });
                    RawFileOutcome {
                        stage: Some("types"),
                        diagnostic: diag,
                    }
                } else {
                    RawFileOutcome {
                        stage: None,
                        diagnostic: None,
                    }
                }
            }
            Err(_) => {
                let diag = early.map(|d| canonicalize_diagnostic_tyvars(&d.message));
                RawFileOutcome {
                    stage: Some("lex"),
                    diagnostic: diag,
                }
            }
        }
    }));

    match result {
        Ok(outcome) => outcome,
        Err(_) => RawFileOutcome {
            stage: Some("panic"),
            diagnostic: Some("internal compiler panic while checking file".to_string()),
        },
    }
}

/// Entry point for `buildc check-dir <dir> [--jobs N] [--json] [--fail-list <file>]`.
pub(crate) fn cmd_check_dir(
    dir: &Path,
    jobs: Option<usize>,
    json: bool,
    fail_list_path: Option<&Path>,
) -> Result<(), i32> {
    let worker_count = match jobs {
        Some(0) => {
            eprintln!("Error: --jobs must be at least 1");
            return Err(2);
        }
        Some(n) => n,
        None => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
    };

    let expected_fails = if let Some(fl_path) = fail_list_path {
        let text = std::fs::read_to_string(fl_path).map_err(|e| {
            eprintln!("Error reading fail-list '{}': {}", fl_path.display(), e);
            2
        })?;
        parse_fail_list(&text).map_err(|e| {
            eprintln!("Error in fail-list '{}': {}", fl_path.display(), e);
            2
        })?
    } else {
        BTreeMap::new()
    };

    let discovered = discover_bld_files(dir).map_err(|e| {
        eprintln!("Error: {}", e);
        2
    })?;

    let started = Instant::now();
    let total = discovered.len();
    let effective_workers = worker_count.min(total.max(1));

    let shared_files = Arc::new(discovered);
    let next_idx = Arc::new(AtomicUsize::new(0));
    let mut raw_results: Vec<Option<RawFileOutcome>> = vec![None; total];

    if effective_workers == 1 || total <= 1 {
        for (idx, (_, full_path)) in shared_files.iter().enumerate() {
            raw_results[idx] = Some(check_one_file(full_path));
        }
    } else {
        let mut handles = Vec::with_capacity(effective_workers);
        for worker_id in 0..effective_workers {
            let files_ref = Arc::clone(&shared_files);
            let cursor = Arc::clone(&next_idx);
            let handle = std::thread::Builder::new()
                .name(format!("buildc-check-{}", worker_id))
                .stack_size(WORKER_STACK_BYTES)
                .spawn(move || {
                    let mut local_out: Vec<(usize, RawFileOutcome)> = Vec::new();
                    loop {
                        let idx = cursor.fetch_add(1, Ordering::Relaxed);
                        if idx >= files_ref.len() {
                            break;
                        }
                        let outcome = check_one_file(&files_ref[idx].1);
                        local_out.push((idx, outcome));
                    }
                    local_out
                })
                .map_err(|e| {
                    eprintln!("Error spawning worker thread: {}", e);
                    1
                })?;
            handles.push(handle);
        }

        for handle in handles {
            let chunk = handle.join().map_err(|_| {
                eprintln!("Error: check-dir worker thread panicked unexpectedly");
                1
            })?;
            for (idx, outcome) in chunk {
                raw_results[idx] = Some(outcome);
            }
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;

    let mut passed = 0usize;
    let mut expected_fail = 0usize;
    let mut unexpected_fail = 0usize;
    let mut stale_expected = 0usize;
    let mut file_entries = Vec::with_capacity(total);
    let mut stale_entries = Vec::new();
    let mut seen_rel_paths = BTreeMap::new();

    for (idx, (rel_path, _)) in shared_files.iter().enumerate() {
        seen_rel_paths.insert(rel_path.clone(), ());
        let outcome = raw_results[idx]
            .take()
            .expect("every discovered file must be checked");
        let listed_stage = expected_fails.get(rel_path);

        match (outcome.stage, listed_stage) {
            (None, None) => {
                passed += 1;
                file_entries.push(CheckDirFileResult {
                    path: rel_path.clone(),
                    status: "passed".to_string(),
                    stage: None,
                    expected_stage: None,
                    diagnostic: None,
                });
            }
            (None, Some(exp_stage)) => {
                passed += 1;
                stale_expected += 1;
                stale_entries.push(StaleExpectedEntry {
                    path: rel_path.clone(),
                    expected_stage: exp_stage.clone(),
                    reason: "file now passes".to_string(),
                });
                file_entries.push(CheckDirFileResult {
                    path: rel_path.clone(),
                    status: "stale_expected".to_string(),
                    stage: None,
                    expected_stage: Some(exp_stage.clone()),
                    diagnostic: None,
                });
            }
            (Some(actual_stage), Some(exp_stage)) if actual_stage == exp_stage.as_str() => {
                expected_fail += 1;
                file_entries.push(CheckDirFileResult {
                    path: rel_path.clone(),
                    status: "expected_fail".to_string(),
                    stage: Some(actual_stage.to_string()),
                    expected_stage: Some(exp_stage.clone()),
                    diagnostic: outcome.diagnostic,
                });
            }
            (Some(actual_stage), Some(exp_stage)) => {
                unexpected_fail += 1;
                stale_expected += 1;
                stale_entries.push(StaleExpectedEntry {
                    path: rel_path.clone(),
                    expected_stage: exp_stage.clone(),
                    reason: format!("stage changed from '{}' to '{}'", exp_stage, actual_stage),
                });
                file_entries.push(CheckDirFileResult {
                    path: rel_path.clone(),
                    status: "unexpected_fail".to_string(),
                    stage: Some(actual_stage.to_string()),
                    expected_stage: Some(exp_stage.clone()),
                    diagnostic: outcome.diagnostic,
                });
            }
            (Some(actual_stage), None) => {
                unexpected_fail += 1;
                file_entries.push(CheckDirFileResult {
                    path: rel_path.clone(),
                    status: "unexpected_fail".to_string(),
                    stage: Some(actual_stage.to_string()),
                    expected_stage: None,
                    diagnostic: outcome.diagnostic,
                });
            }
        }
    }

    // Any fail-list entry whose file does not exist under `dir` is also stale.
    for (listed_path, exp_stage) in &expected_fails {
        if !seen_rel_paths.contains_key(listed_path) {
            stale_expected += 1;
            stale_entries.push(StaleExpectedEntry {
                path: listed_path.clone(),
                expected_stage: exp_stage.clone(),
                reason: "listed file not found under directory".to_string(),
            });
        }
    }

    let summary = CheckDirSummary {
        schema: CHECK_DIR_SCHEMA,
        compiler_version: buildlang::VERSION,
        root: normalize_rel_path(dir),
        jobs: effective_workers,
        total,
        passed,
        expected_fail,
        unexpected_fail,
        stale_expected,
        elapsed_ms,
        files: file_entries,
        stale_entries,
    };

    if json {
        let rendered = serde_json::to_string_pretty(&summary).map_err(|e| {
            eprintln!("Error serializing check-dir JSON: {}", e);
            1
        })?;
        println!("{}", rendered);
    } else {
        println!(
            "buildc {} check-dir: {} files ({} jobs, {} ms): {} passed, {} expected_fail, {} unexpected_fail, {} stale_expected",
            summary.compiler_version,
            summary.total,
            summary.jobs,
            summary.elapsed_ms,
            summary.passed,
            summary.expected_fail,
            summary.unexpected_fail,
            summary.stale_expected
        );
        if summary.unexpected_fail > 0 {
            println!("\nUnexpected failures:");
            for entry in &summary.files {
                if entry.status == "unexpected_fail" {
                    let st = entry.stage.as_deref().unwrap_or("unknown");
                    let detail = entry
                        .diagnostic
                        .as_deref()
                        .map(|d| format!(" ({})", d))
                        .unwrap_or_default();
                    println!("  + {} {}{}", entry.path, st, detail);
                }
            }
        }
        if !summary.stale_entries.is_empty() {
            println!("\nStale expected-failure entries:");
            for stale in &summary.stale_entries {
                println!(
                    "  - {} {} ({})",
                    stale.path, stale.expected_stage, stale.reason
                );
            }
        }
    }

    if summary.unexpected_fail == 0 && summary.stale_expected == 0 {
        Ok(())
    } else {
        Err(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fail_list_skips_comments_and_normalizes_paths() {
        let sample = "\
# Header comment
   # Indented comment

axiom/lib.bld types
./foundation\\bad.bld parse
";
        let parsed = parse_fail_list(sample).expect("valid fail list must parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed.get("axiom/lib.bld").map(String::as_str),
            Some("types")
        );
        assert_eq!(
            parsed.get("foundation/bad.bld").map(String::as_str),
            Some("parse")
        );
    }

    #[test]
    fn parse_fail_list_rejects_malformed_lines() {
        assert!(parse_fail_list("only_path_no_stage.bld\n").is_err());
        assert!(parse_fail_list("a.bld types extra_token\n").is_err());
    }

    #[test]
    fn canonicalize_diagnostic_tyvars_is_thread_order_independent() {
        assert_eq!(
            canonicalize_diagnostic_tyvars("cannot unify ?T119243 with ?T9001 and ?T119243"),
            "cannot unify ?T0 with ?T1 and ?T0"
        );
        assert_eq!(
            canonicalize_diagnostic_tyvars("type `MutexGuard<?T103697>` has no field `count`"),
            "type `MutexGuard<?T0>` has no field `count`"
        );
    }
}
