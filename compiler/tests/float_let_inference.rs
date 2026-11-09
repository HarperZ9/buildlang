//! Issue #167: an unannotated `let` whose integer literal the checker unifies
//! with a float takes that float type. Before, lowering typed it from the
//! literal alone, so `let mut c = 0;` divided into an `f64` was `int32_t` and
//! the division was emitted as `bl_idiv_i32`: AVG-style code printed 60 for
//! 60.3 / 1.

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
    let dir = std::env::temp_dir().join(format!("buildlang_float_let_{}", std::process::id()));
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

/// The program from the issue: a float sum divided by an unannotated count.
#[test]
fn a_literal_let_divided_into_an_f64_is_an_f64() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let src = "fn main() ~ Console { let s = 60.3; let mut c = 0; c = c + 1; let r = s / c; println!(\"{}\", r); }
";
    let out = check_and_run("issue167.bld", src);
    let last = out.lines().last().unwrap_or("");
    assert_eq!(last, "60.3", "{out}");
}

/// An average over a loop, the shape BuildDB's AVG had: 7 / 2 is 3.5.
#[test]
fn an_average_keeps_its_fraction() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let src = "fn avg() -> f64 { let mut sum = 0.0; let mut n = 0; let mut k = 3; while k <= 4 { sum = sum + k; n = n + 1; k = k + 1; } sum / n }
fn main() ~ Console { println!(\"{}\", avg()); }
";
    let out = check_and_run("avg.bld", src);
    let last = out.lines().last().unwrap_or("");
    assert_eq!(last, "3.5", "{out}");
}

/// A literal used only as an integer stays an integer: 7 / 2 is 3.
#[test]
fn an_integer_division_stays_integer() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let src = "fn main() ~ Console { let a = 7; let b = 2; println!(\"{}\", a / b); }
";
    let out = check_and_run("intdiv.bld", src);
    let last = out.lines().last().unwrap_or("");
    assert_eq!(last, "3", "{out}");
}
