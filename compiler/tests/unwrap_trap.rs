//! `unwrap`, `expect` and `unwrap_err` on a missing value stop the program
//! with Rust's message and exit code 101, as an index out of bounds does.
//! Before, `None.unwrap()` read the empty payload slot and printed 0.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Run `main`'s body with `buildc run`; `None` when no C compiler is available.
fn run(tag: &str, body: &str) -> Option<(i32, String, String)> {
    let dir = std::env::temp_dir().join(format!("bl-unwrap-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, format!("fn main() ~ Console {{\n{body}\n}}\n")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    if stderr.contains("no C compiler") {
        return None;
    }
    Some((out.status.code().unwrap_or(-1), stdout, stderr))
}

fn assert_traps(tag: &str, body: &str, stdout: &str, message: &str) {
    let Some((code, out, err)) = run(tag, body) else {
        return;
    };
    assert_eq!(
        code, 101,
        "{tag}: must stop with 101; stdout {out} stderr {err}"
    );
    assert_eq!(out, stdout, "{tag}: stderr {err}");
    assert!(
        err.contains(message),
        "{tag}: want {message:?} in stderr:\n{err}"
    );
}

#[test]
fn unwrap_on_none_stops_the_program() {
    assert_traps(
        "none",
        "let a: Option<i32> = None;\nprintln!(\"before\");\nlet v = a.unwrap();\nprintln!(\"after {}\", v);",
        "before\n",
        "called `Option::unwrap()` on a `None` value",
    );
}

#[test]
fn expect_on_none_prints_its_message() {
    assert_traps(
        "expect-none",
        "let a: Option<i32> = None;\nprintln!(\"{}\", a.expect(\"need a value\"));",
        "",
        "need a value",
    );
}

#[test]
fn unwrap_on_err_prints_the_err_text() {
    assert_traps(
        "err-str",
        "let r: Result<i32, str> = \"x\".parse::<i32>();\nprintln!(\"{}\", r.unwrap());",
        "",
        "called `Result::unwrap()` on an `Err` value: \"invalid digit found in string\"",
    );
    assert_traps(
        "err-int",
        "let r: Result<i32, i32> = Err(7);\nprintln!(\"{}\", r.unwrap());",
        "",
        "called `Result::unwrap()` on an `Err` value: 7",
    );
}

#[test]
fn expect_on_err_prints_message_and_err_text() {
    assert_traps(
        "expect-err",
        "let r: Result<i32, str> = Err(\"bad input\");\nprintln!(\"{}\", r.expect(\"reading n\"));",
        "",
        "reading n: \"bad input\"",
    );
}

#[test]
fn unwrap_err_on_ok_stops_the_program() {
    assert_traps(
        "unwrap-err",
        "let r: Result<i32, str> = Ok(3);\nprintln!(\"{}\", r.unwrap_err());",
        "",
        "called `Result::unwrap_err()` on an `Ok` value",
    );
}

#[test]
fn present_values_unwrap_as_before() {
    let Some((code, out, err)) = run(
        "present",
        "let a: Option<i32> = Some(4);\nlet r: Result<f64, str> = Ok(1.5);\nlet e: Result<i32, str> = Err(\"no\");\nprintln!(\"{} {} {} {} {}\", a.unwrap(), a.expect(\"x\"), r.unwrap(), r.expect(\"y\"), e.unwrap_err());",
    ) else {
        return;
    };
    assert_eq!(code, 0, "stderr {err}");
    assert_eq!(out, "4 4 1.5 1.5 no\n", "stderr {err}");
}
