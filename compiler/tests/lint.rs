//! `buildc lint FILE`: type errors fail the command, style findings warn.

use std::path::PathBuf;
use std::process::{Command, Output};

fn lint(name: &str, source: &str) -> Output {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = root.parent().expect("the compiler sits in the repository");
    let dir = std::env::temp_dir().join(format!("bl-lint-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp directory");
    let file = dir.join("main.bld");
    std::fs::write(&file, source).expect("program written");
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", root.join("stdlib"))
        .arg("lint")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    out
}

const CLEAN: &str = "fn main() ~ Console {\n    println!(\"{}\", 1);\n}\n";

#[test]
fn a_clean_file_reports_no_issues() {
    let out = lint("clean", CLEAN);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("No issues found"));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("warning"));
}

/// The check used to fire only on indented lines.
#[test]
fn trailing_whitespace_is_reported_on_an_unindented_line() {
    let out = lint("ws", &CLEAN.replace("Console {\n", "Console {  \n"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "a warning does not fail the command"
    );
    assert!(stderr.contains("trailing whitespace"), "stderr: {stderr}");
    assert!(stderr.contains(":1"), "the warning names line 1: {stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("0 error(s), 1 warning(s)"));
}

#[test]
fn a_type_error_fails_the_command() {
    let out = lint("ty", "fn main() {\n    let x: bool = 3;\n}\n");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("type mismatch"));
}
