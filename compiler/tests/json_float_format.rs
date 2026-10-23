//! M4: float text for traces and JSON. `x.to_json()` must write a double the
//! way JS `JSON.stringify` does: the shortest digits that read back to the
//! same bits, `-0` as `0`, NaN and the infinities as `null`, and JS's switch to
//! exponent form below 1e-6 and from 1e21.
//!
//! `tests/json-float/corpus.txt` holds 3,308 doubles as bit patterns beside the
//! text node wrote for each (`tools/json_float_corpus.mjs`). The test builds a
//! BuildLang program that rebuilds each double with `f64::from_bits` and prints
//! `to_json()`, then compares line by line. A control checks that the
//! comparison catches a single wrong line.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn corpus() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(repo_root().join("tests/json-float/corpus.txt"))
        .expect("corpus present");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (bits, json) = l.trim_end().split_once(' ').expect("bits and text");
            (bits.to_string(), json.to_string())
        })
        .collect()
}

fn program(rows: &[(String, String)]) -> String {
    let mut src =
        String::from("fn p(v: f64) ~ Console {\n    println!(\"{}\", v.to_json());\n}\n\n");
    let chunks: Vec<_> = rows.chunks(250).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        src.push_str(&format!("fn part{i}() ~ Console {{\n"));
        for (bits, _) in chunk.iter() {
            src.push_str(&format!("    p(f64::from_bits(0x{bits}_u64));\n"));
        }
        src.push_str("}\n\n");
    }
    src.push_str("fn main() ~ Console {\n");
    for i in 0..chunks.len() {
        src.push_str(&format!("    part{i}();\n"));
    }
    src.push_str("}\n");
    src
}

fn mismatches(rows: &[(String, String)], got: &str) -> Vec<String> {
    let lines: Vec<&str> = got.lines().map(|l| l.trim_end_matches('\r')).collect();
    let mut bad: Vec<String> = rows
        .iter()
        .zip(lines.iter())
        .filter(|((_, want), got)| want != *got)
        .map(|((bits, want), got)| format!("{bits}: got {got}, want {want}"))
        .collect();
    if lines.len() != rows.len() {
        bad.push(format!("{} lines for {} rows", lines.len(), rows.len()));
    }
    bad
}

#[test]
fn to_json_matches_json_stringify_on_the_corpus() {
    let rows = corpus();
    assert!(rows.len() > 3000, "corpus went missing: {}", rows.len());
    let dir = std::env::temp_dir().join(format!("buildlang-jsonfloat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("corpus.bld");
    std::fs::write(&file, program(&rows)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "corpus program failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bad = mismatches(&rows, &stdout);
    assert!(
        bad.is_empty(),
        "{} of {} values differ from JSON.stringify:\n{}",
        bad.len(),
        rows.len(),
        bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );

    // Control: one altered line must be caught.
    let mut altered: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();
    altered[1] = "-0".to_string();
    assert_eq!(mismatches(&rows, &altered.join("\n")).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
