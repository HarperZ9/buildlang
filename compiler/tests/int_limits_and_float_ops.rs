//! Regressions for three miscompiles found by translation validation and the
//! checked-arithmetic boundary tests:
//! - #212: `i8::MAX` lowered to an undeclared `i8_MAX` (C compile error).
//! - #215: `u64::MAX - literal` was stored in an `int32_t` temporary.
//! - #203: an integer-literal local unified with a float was `double` in C
//!   but incremented through `bl_iadd_i32`, truncating it.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Run `src` with `buildc run`; `None` when no C compiler is available.
fn run(tag: &str, src: &str) -> Option<(i32, String, String)> {
    let dir = std::env::temp_dir().join(format!("bl-limits-{}-{tag}", std::process::id()));
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
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    if stderr.contains("no C compiler") {
        return None;
    }
    Some((out.status.code().unwrap_or(-1), stdout, stderr))
}

#[test]
fn every_integer_limit_compiles_with_its_value() {
    let cases = [
        ("i8", "MIN", "-128"),
        ("i8", "MAX", "127"),
        ("i16", "MIN", "-32768"),
        ("i16", "MAX", "32767"),
        ("i32", "MIN", "-2147483648"),
        ("i32", "MAX", "2147483647"),
        ("i64", "MIN", "-9223372036854775808"),
        ("i64", "MAX", "9223372036854775807"),
        ("u8", "MIN", "0"),
        ("u8", "MAX", "255"),
        ("u16", "MIN", "0"),
        ("u16", "MAX", "65535"),
        ("u32", "MIN", "0"),
        ("u32", "MAX", "4294967295"),
        ("u64", "MIN", "0"),
        ("u64", "MAX", "18446744073709551615"),
    ];
    let mut body = String::new();
    for (t, b, _) in &cases {
        body.push_str(&format!("    let v_{t}_{b}: {t} = {t}::{b};\n    println!(\"{{}}\", v_{t}_{b});\n"));
    }
    let src = format!("fn main() ~ Console {{\n{body}}}\n");
    let Some((code, out, err)) = run("all", &src) else { return };
    assert_eq!(code, 0, "stderr: {err}");
    let want: Vec<&str> = cases.iter().map(|c| c.2).collect();
    assert_eq!(out.lines().collect::<Vec<_>>(), want);
}

#[test]
fn i8_max_in_arithmetic_issue_212() {
    let src = "fn main() ~ Console {\n    let t: i8 = i8::MAX / 2 + 1;\n    println!(\"{}\", t);\n}\n";
    let Some((code, out, err)) = run("212", src) else { return };
    assert_eq!(code, 0, "stderr: {err}");
    assert_eq!(out, "64\n");
}

#[test]
fn u64_max_minus_literal_keeps_its_width_issue_215() {
    let src = "fn mul(a: u64, b: u64) -> u64 { a * b }\n\
               fn main() ~ Console {\n\
               \x20   let s: u64 = u64::MAX - 1;\n\
               \x20   println!(\"{}\", s);\n\
               \x20   let r: u64 = mul((u64::MAX - 9223372036854775807), (2 as u64));\n\
               \x20   println!(\"{}\", r);\n\
               }\n";
    let Some((code, out, err)) = run("215", src) else { return };
    assert_eq!(out, "18446744073709551614\n", "stderr: {err}");
    assert_eq!(code, 101, "2^63 * 2 must trap; stdout {out}");
    assert!(err.contains("attempt to multiply with overflow"), "{err}");
}

#[test]
fn literal_local_unified_with_float_adds_as_float_issue_203() {
    let src = "fn main() ~ Console {\n\
               \x20   let mut x = 0;\n\
               \x20   let h = x * 0.5 + 0.5;\n\
               \x20   x = h;\n\
               \x20   x = x + 1;\n\
               \x20   println!(\"{}\", x);\n\
               }\n";
    let Some((code, out, err)) = run("203", src) else { return };
    assert_eq!(code, 0, "stderr: {err}");
    assert_eq!(out, "1.5\n");
}
