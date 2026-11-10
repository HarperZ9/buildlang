//! `assert!`, `assert_eq!`, `assert_ne!`, their `debug_` forms, `unreachable!`,
//! `todo!` and `unimplemented!` were dropped from the program with no
//! diagnostic (G15): a failing check let the program run on and exit 0. These
//! run real programs and check the exit status and stderr. A macro the
//! compiler does not implement must be a compile error that names it.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// `buildc run` on a `main` with `body`.
fn run(tag: &str, body: &str) -> Run {
    run_src(tag, &format!("fn main() ~ Console {{\n{body}\n}}\n"))
}

fn run_src(tag: &str, src: &str) -> Run {
    let dir = std::env::temp_dir().join(format!("bl-assert-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

/// The program must stop before `println!("reached")` and report `needle`.
fn expect_abort(tag: &str, body: &str, needle: &str) {
    let r = run(tag, &format!("{body}\nprintln!(\"reached\");"));
    assert!(!r.ok, "{tag}: exited 0\nstdout: {}", r.stdout);
    assert!(
        !r.stdout.contains("reached"),
        "{tag}: ran past the failing check"
    );
    assert!(
        r.stderr.contains(needle),
        "{tag}: stderr lacks `{needle}`:\n{}",
        r.stderr
    );
}

#[test]
fn passing_checks_let_the_program_continue() {
    let r = run(
        "pass",
        "let x: i32 = 4;\n\
         assert!(x == 4);\n\
         assert_eq!(x + 1, 5);\n\
         assert_ne!(x, 5);\n\
         debug_assert!(x > 0);\n\
         debug_assert_eq!(x, 4);\n\
         debug_assert_ne!(x, 0);\n\
         assert!(x == 4, \"never printed {}\", x);\n\
         println!(\"reached\");",
    );
    assert!(r.ok, "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim(), "reached");
    assert!(!r.stderr.contains("never printed"), "{}", r.stderr);
}

#[test]
fn failing_assert_aborts_with_source_text_and_line() {
    expect_abort(
        "assert",
        "let x: i32 = 3;\nassert!(x == 4);",
        "Assertion failed: assert!(x == 4) at line 3",
    );
}

#[test]
fn failing_assert_eq_and_assert_ne_abort() {
    expect_abort("eq", "assert_eq!(3, 4);", "assert_eq!(3, 4) at line 2");
    expect_abort("ne", "assert_ne!(2 + 2, 4);", "assert_ne!(2 + 2, 4)");
}

#[test]
fn debug_forms_are_always_checked() {
    expect_abort("dbg", "debug_assert!(1 > 2);", "debug_assert!(1 > 2)");
    expect_abort("dbgeq", "debug_assert_eq!(1, 2);", "debug_assert_eq!(1, 2)");
    expect_abort("dbgne", "debug_assert_ne!(1, 1);", "debug_assert_ne!(1, 1)");
}

#[test]
fn custom_message_is_printed_before_the_abort() {
    let r = run(
        "msg",
        "let n: i32 = 7;\nassert_eq!(n, 8, \"n was {}\", n);\nprintln!(\"reached\");",
    );
    assert!(!r.ok);
    assert!(r.stderr.contains("n was 7"), "{}", r.stderr);
    assert!(r.stderr.contains("assert_eq!(n, 8)"), "{}", r.stderr);
    assert!(!r.stdout.contains("reached"));
}

#[test]
fn quotes_in_the_checked_text_still_compile() {
    let r = run(
        "quote",
        "let s = \"a\";\nassert!(1 == 1);\nassert!(\"x\\\"y\" == \"x\\\"y\");\nprintln!(\"reached\");",
    );
    assert!(r.ok, "stderr: {}", r.stderr);
    expect_abort("quotefail", "assert!(\"a\" == \"b\");", "Assertion failed");
}

#[test]
fn unreachable_todo_and_unimplemented_abort() {
    expect_abort("todo", "todo!();", "todo!() reached at line 2");
    expect_abort("unimpl", "unimplemented!();", "unimplemented!() reached");
    let r = run_src(
        "unreach",
        "fn f(x: i32) -> i32 ~ Console {\n\
         if x > 0 { return 1; }\n\
         unreachable!(\"x was {}\", x);\n\
         }\n\
         fn main() ~ Console {\n\
         println!(\"{}\", f(1));\n\
         println!(\"{}\", f(0));\n\
         println!(\"reached\");\n\
         }\n",
    );
    assert!(!r.ok);
    assert!(r.stdout.contains('1') && !r.stdout.contains("reached"));
    assert!(r.stderr.contains("x was 0"), "{}", r.stderr);
    assert!(
        r.stderr.contains("unreachable!() reached at line 3"),
        "{}",
        r.stderr
    );
}

#[test]
fn unknown_macro_is_a_compile_error_naming_it() {
    for (tag, body) in [
        ("stmt", "frobnicate!(1, 2);"),
        ("expr", "let v = frobnicate!(1);"),
    ] {
        let r = run(tag, &format!("{body}\nprintln!(\"reached\");"));
        assert!(!r.ok, "{tag}: built and ran");
        assert!(!r.stdout.contains("reached"), "{tag}: program ran");
        let all = r.stderr + &r.stdout;
        assert!(
            all.contains("macro `frobnicate!` is not implemented"),
            "{tag}: {all}"
        );
    }
}

#[test]
fn assert_without_a_condition_is_a_compile_error() {
    let r = run("empty", "assert_eq!(1);\nprintln!(\"reached\");");
    assert!(!r.ok);
    assert!(!r.stdout.contains("reached"));
}

#[test]
fn a_check_written_over_several_lines_reports_on_one_line() {
    expect_abort(
        "multiline",
        "let x: i32 = 1;\nassert!(\n    x\n        == 2\n);",
        "Assertion failed: assert!(x == 2) at line 4",
    );
}
