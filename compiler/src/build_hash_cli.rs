// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! `buildc build-hash FILE`: the hash of the C this compiler emits for FILE,
//! measured over several separate compiler processes.
//!
//! Each run is a fresh process, so per-process state (hash-map seeds, the
//! process id, the clock) differs between runs. The command prints one
//! SHA-256 when every run emitted the same bytes and exits 0. When any run
//! differs it names the run and the first differing line and exits 1.

use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

/// Where a run first departs from run 0.
#[derive(Debug, PartialEq)]
pub struct Difference {
    pub run: usize,
    /// 1-based line in the emitted C.
    pub line: usize,
}

#[derive(Debug, PartialEq)]
pub struct Verdict {
    pub hashes: Vec<String>,
    pub first_difference: Option<Difference>,
}

impl Verdict {
    pub fn deterministic(&self) -> bool {
        self.first_difference.is_none()
    }
}

fn first_differing_line(a: &[u8], b: &[u8]) -> usize {
    let mut la = a.split(|c| *c == b'\n');
    let mut lb = b.split(|c| *c == b'\n');
    let mut line = 1;
    loop {
        match (la.next(), lb.next()) {
            (Some(x), Some(y)) if x == y => line += 1,
            _ => return line,
        }
    }
}

/// Call `emit` once per run and compare every emission with the first.
pub fn compare_runs<F>(runs: usize, mut emit: F) -> Result<Verdict, i32>
where
    F: FnMut(usize) -> Result<Vec<u8>, i32>,
{
    let mut hashes = Vec::with_capacity(runs);
    let mut first: Option<Vec<u8>> = None;
    let mut first_difference = None;
    for run in 0..runs {
        let bytes = emit(run)?;
        hashes.push(sha256_hex(&bytes));
        match &first {
            None => first = Some(bytes),
            Some(base) if first_difference.is_none() && *base != bytes => {
                let line = first_differing_line(base, &bytes);
                first_difference = Some(Difference { run, line });
            }
            Some(_) => {}
        }
    }
    Ok(Verdict {
        hashes,
        first_difference,
    })
}

/// One compiler process: FILE to C at a per-run path, bytes read back.
fn emit_with_fresh_process(file: &Path, dir: &Path, run: usize) -> Result<Vec<u8>, i32> {
    let exe = std::env::current_exe().map_err(|e| {
        eprintln!("Error locating buildc: {e}");
        2
    })?;
    let c_path = dir.join(format!("run-{run}.c"));
    let out = Command::new(exe)
        .arg(file)
        .arg("-o")
        .arg(&c_path)
        .args(["--target", "c"])
        .output()
        .map_err(|e| {
            eprintln!("Error starting buildc: {e}");
            2
        })?;
    if !out.status.success() {
        eprint!("{}", String::from_utf8_lossy(&out.stderr));
        eprintln!("'{}' does not compile to C", file.display());
        return Err(2);
    }
    std::fs::read(&c_path).map_err(|e| {
        eprintln!("Error reading '{}': {e}", c_path.display());
        2
    })
}

fn report(file: &Path, source: &[u8], v: &Verdict) -> serde_json::Value {
    let difference = v
        .first_difference
        .as_ref()
        .map(|d| serde_json::json!({ "run": d.run, "line": d.line }));
    serde_json::json!({
        "schema": "buildlang-build-hash/v1",
        "file": file.to_string_lossy().replace('\\', "/"),
        "compiler": format!("buildc {}", env!("CARGO_PKG_VERSION")),
        "target": "c",
        "runs": v.hashes.len(),
        "source_sha256": sha256_hex(source),
        "deterministic": v.deterministic(),
        "c_sha256": if v.deterministic() { v.hashes.first().cloned() } else { None },
        "run_sha256": v.hashes,
        "first_difference": difference,
    })
}

pub fn cmd_build_hash(file: &Path, runs: usize, json: bool) -> Result<(), i32> {
    if runs < 2 {
        eprintln!("--runs must be at least 2: one emission cannot show a difference");
        return Err(2);
    }
    let source = std::fs::read(file).map_err(|e| {
        eprintln!("Error reading '{}': {e}", file.display());
        2
    })?;
    let dir = std::env::temp_dir().join(format!("buildc-build-hash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| {
        eprintln!("Error creating '{}': {e}", dir.display());
        2
    })?;
    let verdict = compare_runs(runs, |run| emit_with_fresh_process(file, &dir, run));
    let _ = std::fs::remove_dir_all(&dir);
    let verdict = verdict?;
    if json {
        let doc = report(file, &source, &verdict);
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
    } else if let Some(d) = &verdict.first_difference {
        println!(
            "NOT DETERMINISTIC: run {} differs from run 0 at line {} of the emitted C",
            d.run, d.line
        );
    } else {
        println!("{}  {} runs, same bytes", verdict.hashes[0], runs);
    }
    if verdict.deterministic() {
        Ok(())
    } else {
        Err(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_emissions_give_one_hash() {
        let v = compare_runs(3, |_| Ok(b"int main(void) { return 0; }\n".to_vec())).unwrap();
        assert!(v.deterministic());
        assert_eq!(v.hashes.len(), 3);
        assert!(v.hashes.iter().all(|h| h == &v.hashes[0] && h.len() == 64));
    }

    /// Planted defect: an emitter that writes the run number into line 3.
    #[test]
    fn a_planted_run_counter_is_caught_and_located() {
        let v = compare_runs(3, |run| {
            Ok(format!("a\nb\n// emitted by run {run}\nc\n").into_bytes())
        })
        .unwrap();
        assert!(!v.deterministic());
        assert_eq!(v.first_difference, Some(Difference { run: 1, line: 3 }));
        assert_ne!(v.hashes[0], v.hashes[1]);
    }

    /// Planted defect: two declarations swap places in the last run only.
    #[test]
    fn a_planted_reordering_in_a_late_run_is_caught() {
        let v = compare_runs(4, |run| {
            let text = if run == 3 { "x\nB\nA\n" } else { "x\nA\nB\n" };
            Ok(text.as_bytes().to_vec())
        })
        .unwrap();
        assert_eq!(v.first_difference, Some(Difference { run: 3, line: 2 }));
    }

    #[test]
    fn a_truncated_emission_is_caught() {
        let v = compare_runs(2, |run| {
            Ok(if run == 0 {
                b"a\nb\n".to_vec()
            } else {
                b"a\n".to_vec()
            })
        })
        .unwrap();
        assert_eq!(v.first_difference, Some(Difference { run: 1, line: 2 }));
    }

    #[test]
    fn a_failed_emission_stops_the_comparison() {
        let r = compare_runs(3, |run| if run == 1 { Err(2) } else { Ok(vec![]) });
        assert_eq!(r, Err(2));
    }
}
