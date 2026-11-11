//! Helpers shared by `value_array_params.rs` and `global_stores.rs`: run a
//! BuildLang program with the built `buildc` and read what it printed.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

/// Whether a C compiler is on the path. Without one the run tests print a
/// note and return; the type-check tests still run.
pub fn has_cc() -> bool {
    let found = ["cc", "gcc", "clang"].iter().any(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    });
    if !found {
        eprintln!("skipping: no C compiler");
    }
    found
}

pub struct Ran {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// `buildc run` on a file; both streams with `\r\n` read as `\n`.
pub fn run_file(file: &Path) -> Ran {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(file)
        .output()
        .expect("buildc runs");
    Ran {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    }
}

/// `buildc run` on `src` written to a fresh temporary folder.
pub fn run(tag: &str, src: &str) -> Ran {
    let dir = std::env::temp_dir().join(format!("bl-g33-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temporary folder");
    let file = dir.join("main.bld");
    std::fs::write(&file, src).expect("program written");
    let ran = run_file(&file);
    let _ = std::fs::remove_dir_all(&dir);
    ran
}

/// The program must exit 0 and print exactly `expected`.
pub fn assert_prints(tag: &str, src: &str, expected: &str) {
    let ran = run(tag, src);
    assert_eq!(ran.code, Some(0), "{tag}: stderr: {}", ran.stderr);
    assert_eq!(ran.stdout, expected, "{tag}: stderr: {}", ran.stderr);
}

/// The program must stop with the trap status 101 and the given message,
/// after printing exactly `before`.
pub fn assert_traps(tag: &str, src: &str, before: &str, message: &str) {
    let ran = run(tag, src);
    assert_eq!(
        ran.code,
        Some(101),
        "{tag}: stdout: {} stderr: {}",
        ran.stdout,
        ran.stderr
    );
    assert_eq!(ran.stdout, before, "{tag}");
    assert!(
        ran.stderr.contains(message),
        "{tag}: stderr: {}",
        ran.stderr
    );
}

/// `got` equals `want`, with the first differing lines in the failure.
pub fn assert_same_lines(tag: &str, got: &str, want: &str) {
    if got == want {
        return;
    }
    let wrong = got
        .lines()
        .zip(want.lines())
        .enumerate()
        .filter(|(_, (g, w))| g != w)
        .take(5)
        .map(|(i, (g, w))| format!("  line {}\n  got  {g}\n  want {w}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    panic!(
        "{tag}: {} lines printed, {} expected; first differences:\n{wrong}",
        got.lines().count(),
        want.lines().count()
    );
}
