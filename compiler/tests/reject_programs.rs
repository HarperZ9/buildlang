//! Programs `buildc check` must reject.
//!
//! Each file in `tests/reject` once passed `buildc check` and then failed to
//! build or crashed (issue #83: an unresolved `Box<T>` annotation segfaulted,
//! and a by-value recursive enum failed in the C compiler). The check must now
//! fail and report a stable BL code, so an agent sees the error before any C
//! is generated.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn has_bl_code(text: &str) -> bool {
    text.match_indices("[BL").any(|(i, _)| {
        let code = &text[i + 1..];
        code.len() >= 7
            && code[2..6].chars().all(|c| c.is_ascii_digit())
            && code.as_bytes()[6] == b']'
    })
}

#[test]
fn every_reject_program_fails_check_with_a_bl_code() {
    let dir = repo_root().join("tests/reject");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/reject exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("bld"))
        .collect();
    files.sort();
    assert!(
        files.len() >= 3,
        "reject programs went missing: {}",
        files.len()
    );

    let mut failures = Vec::new();
    for file in &files {
        let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
            .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
            .arg("check")
            .arg(file)
            .output()
            .expect("buildc runs");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if out.status.success() {
            failures.push(format!("{}: check passed", file.display()));
        } else if !has_bl_code(&text) {
            failures.push(format!("{}: no BL code in\n{text}", file.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn bl_code_detector_has_a_negative_control() {
    assert!(has_bl_code("  [BL0384] recursive type"));
    assert!(!has_bl_code("error: something [BLx]"));
    assert!(!has_bl_code("no code here"));
}
