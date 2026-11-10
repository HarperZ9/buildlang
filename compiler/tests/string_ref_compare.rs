//! Comparing a borrowed string (`&str`, `&String`) on the left of `==`, `!=`
//! or an ordering operator. The left operand used to stay a pointer: against a
//! string value the generated C did not compile, and against another borrowed
//! string the two addresses were compared. These run real programs.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// `buildc run` on `src`; returns stdout, and panics with stderr on failure.
fn run(tag: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("bl-strref-{}-{tag}", std::process::id()));
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

#[test]
fn a_borrowed_string_compares_with_a_literal() {
    let src = "fn is_x(cmd: &str) -> bool { cmd == \"x\" }\n\
               fn not_x(cmd: &str) -> bool { cmd != \"x\" }\n\
               fn main() ~ Console {\n\
               let a = String::from(\"x\");\n\
               let b = String::from(\"y\");\n\
               println!(\"{} {} {} {}\", is_x(&a), is_x(&b), not_x(&a), not_x(&b));\n\
               }\n";
    assert_eq!(run("literal", src).trim(), "true false false true");
}

/// Two separate strings with equal bytes live at different addresses, so an
/// address comparison prints `false true` here.
#[test]
fn two_borrowed_strings_compare_by_content_not_address() {
    let src = "fn same(a: &str, b: &str) -> bool { a == b }\n\
               fn differ(a: &str, b: &str) -> bool { a != b }\n\
               fn main() ~ Console {\n\
               let a = String::from(\"alpha\");\n\
               let b = String::from(\"al\") + \"pha\";\n\
               let c = String::from(\"beta\");\n\
               println!(\"{} {} {} {}\", same(&a, &b), differ(&a, &b), same(&a, &c), differ(&a, &c));\n\
               }\n";
    assert_eq!(run("content", src).trim(), "true false false true");
}

#[test]
fn a_borrowed_string_orders_against_values_and_borrows() {
    let src = "fn lt(a: &str, b: &str) -> bool { a < b }\n\
               fn ge_lit(a: &str) -> bool { a >= \"m\" }\n\
               fn main() ~ Console {\n\
               let a = String::from(\"apple\");\n\
               let z = String::from(\"zebra\");\n\
               println!(\"{} {} {} {}\", lt(&a, &z), lt(&z, &a), ge_lit(&a), ge_lit(&z));\n\
               }\n";
    assert_eq!(run("order", src).trim(), "true false false true");
}

/// The forms that already worked keep their results.
#[test]
fn string_values_and_a_borrow_on_the_right_still_compare() {
    let src = "fn rhs(a: &str) -> bool { let s = String::from(\"x\"); s == a }\n\
               fn main() ~ Console {\n\
               let a = String::from(\"x\");\n\
               let b = String::from(\"y\");\n\
               println!(\"{} {} {} {}\", a == \"x\", a == b, rhs(&a), rhs(&b));\n\
               }\n";
    assert_eq!(run("kept", src).trim(), "true false true false");
}

/// Two raw pointers to equal strings at different addresses are not equal:
/// content comparison applies to references only.
#[test]
fn raw_pointers_still_compare_addresses() {
    let src = "fn main() ~ Console {\n\
               let a = String::from(\"x\");\n\
               let b = String::from(\"x\");\n\
               let p: *const String = &a;\n\
               let q: *const String = &b;\n\
               let r: *const String = &a;\n\
               println!(\"{} {}\", p == q, p == r);\n\
               }\n";
    assert_eq!(run("raw", src).trim(), "false true");
}

#[test]
fn references_bound_by_let_compare_by_content() {
    let src = "fn main() ~ Console {\n\
               let a = String::from(\"alpha\");\n\
               let b = String::from(\"al\") + \"pha\";\n\
               let r = &a;\n\
               let s: &String = &b;\n\
               let t = r;\n\
               println!(\"{} {} {} {}\", r == s, t == s, r != s, &a == &b);\n\
               }\n";
    assert_eq!(run("let", src).trim(), "true true false true");
}
