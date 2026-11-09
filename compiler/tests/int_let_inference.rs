//! Issue #159: an unannotated integer `let` takes the integer type the checker
//! inferred from its uses. Before, lowering typed it from the initializer alone,
//! so `let mut i = 0;` compared with an `i64` and returned as one was emitted as
//! `int32_t`: `buildc check` passed and `buildc run` trapped with an overflow
//! once `i` passed 2^31.

use std::process::Command;

fn buildc() -> Command {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", root.join("stdlib"));
    cmd
}

fn have_c_compiler() -> bool {
    let probe = |cc: &str, arg: &str| {
        Command::new(cc)
            .arg(arg)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    ["gcc", "clang", "cc"]
        .iter()
        .any(|cc| probe(cc, "--version"))
        || probe("cl", "/?")
}

fn write(name: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("buildlang_int_let_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, src).unwrap();
    path
}

/// Check, then run, and return trimmed stdout. Panics with the output on failure.
fn check_and_run(name: &str, src: &str) -> String {
    let file = write(name, src);
    let check = buildc().arg("check").arg(&file).output().unwrap();
    assert!(
        check.status.success(),
        "check failed: {}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    let run = buildc().arg("run").arg(&file).output().unwrap();
    assert!(
        run.status.success(),
        "run failed ({:?}): {}{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout)
        .replace("\r\n", "\n")
        .trim_end()
        .to_string()
}

/// The program from the issue: `i` meets `n: i64` in a comparison. It steps
/// 0, 1e9, ..., 5e9 and breaks at 5e9 (the issue text says 6e9, a slip).
#[test]
fn a_let_compared_with_an_i64_is_an_i64() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let src = "fn lp(n: i64) -> i64 { let mut i = 0; loop { if i >= n { break; } i += 1000000000; } i }\nfn main() ~ Console { println!(\"{}\", lp(5000000000)); }\n";
    let out = check_and_run("issue159.bld", src);
    let last = out.lines().last().unwrap_or("");
    assert_eq!(last, "5000000000", "{out}");
}

/// Only the return type says `i64`: the tail expression is the one use.
#[test]
fn a_let_returned_as_an_i64_is_an_i64() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let src = "fn pow2() -> i64 { let mut x = 1; let mut k = 0; while k < 40 { x = x * 2; k += 1; } x }\nfn main() ~ Console { println!(\"{}\", pow2()); }\n";
    let out = check_and_run("tail_only.bld", src);
    let last = out.lines().last().unwrap_or("");
    assert_eq!(last, "1099511627776", "{out}");
}

/// A literal with no wider use keeps the default `i32`, which still traps on
/// overflow; the inference never widens a binding silently.
#[test]
fn a_let_with_no_wider_use_stays_i32() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let src = "fn main() ~ Console { let mut x = 1; let mut k = 0; while k < 40 { x = x * 2; k += 1; } println!(\"{}\", x); }\n";
    let file = write("stays_i32.bld", src);
    let run = buildc().arg("run").arg(&file).output().unwrap();
    assert!(
        !run.status.success(),
        "an i32 that doubles 40 times must trap: {}",
        String::from_utf8_lossy(&run.stdout)
    );
}
