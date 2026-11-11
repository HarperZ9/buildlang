//! G32: the minimum of a signed type written as a negated literal.
//! `-128_i8` was rejected with "integer literal `128i8` is out of range",
//! because the literal was checked before the minus sign was applied. It is
//! accepted for every signed width, with and without a suffix. One past the
//! minimum, and the positive literal alone, are still rejected.

use std::path::PathBuf;
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn has_cc() -> bool {
    ["cc", "gcc", "clang"].iter().any(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn buildc(verb: &str, tag: &str, src: &str) -> Output {
    let dir = std::env::temp_dir().join(format!("bl-minlit-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg(verb)
        .arg(&file)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
    .replace("\r\n", "\n")
}

/// Every signed width, suffixed, and the value one above each minimum, so a
/// minimum that wrapped to another value would show.
#[test]
fn the_minimum_of_every_signed_width_is_a_valid_literal() {
    if !has_cc() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let src = "const M8: i8 = -128_i8;\n\
               fn id16(x: i16) -> i16 { x }\n\
               fn main() ~ Console {\n\
               let a: i8 = -128_i8;\n\
               let b: i16 = -32768_i16;\n\
               let c: i32 = -2147483648_i32;\n\
               let d: i64 = -9223372036854775808_i64;\n\
               let e: isize = -9223372036854775808_isize;\n\
               let f: i128 = -170141183460469231731687303715884105728_i128;\n\
               println!(\"{} {} {} {} {}\", a, b, c, d, e);\n\
               println!(\"{} {} {} {}\", a as i64 + 1, b as i64 + 1, c as i64 + 1, d + 1);\n\
               println!(\"{} {} {}\", f + 1_i128, M8, id16(-32768_i16));\n\
               }\n";
    let out = buildc("run", "suffixed", src);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        "-128 -32768 -2147483648 -9223372036854775808 -9223372036854775808\n\
         -127 -32767 -2147483647 -9223372036854775807\n\
         -170141183460469231731687303715884105727 -128 -32768\n"
    );
}

/// Without a suffix the literal takes the annotated type, and parentheses
/// around the literal change nothing.
#[test]
fn an_unsuffixed_minimum_takes_the_annotated_type() {
    if !has_cc() {
        return;
    }
    let src = "fn main() ~ Console {\n\
               let f: i8 = -128;\n\
               let g: i16 = -(32768_i16);\n\
               let h: i32 = -2147483648;\n\
               let i: i64 = -9223372036854775808;\n\
               let j = -2147483648;\n\
               println!(\"{} {} {} {} {}\", f, g, h, i, j);\n\
               }\n";
    let out = buildc("run", "plain", src);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "-128 -32768 -2147483648 -9223372036854775808 -2147483648"
    );
}

/// The range check still holds on both sides of the minimum.
#[test]
fn values_outside_the_range_are_still_rejected() {
    let cases = [
        ("below", "let a: i8 = -129_i8;", "`129i8` is out of range"),
        ("positive", "let a: i8 = 128_i8;", "`128i8` is out of range"),
        (
            "double",
            "let a: i8 = -(-128_i8);",
            "integer overflow in unary `-`",
        ),
        (
            "i16",
            "let a: i16 = -32769_i16;",
            "`32769i16` is out of range",
        ),
        (
            "i64",
            "let a: i64 = 9223372036854775808_i64;",
            "is out of range",
        ),
        ("sum", "let a: i8 = -128_i8 - 1_i8;", "integer overflow"),
    ];
    for (tag, stmt, wanted) in cases {
        let src = format!("fn main() ~ Console {{\n{stmt}\nprintln!(\"{{}}\", a);\n}}\n");
        let out = buildc("check", tag, &src);
        let text = text(&out);
        assert!(!out.status.success(), "{tag} must be rejected: {text}");
        assert!(text.contains(wanted), "{tag}: {text}");
    }
}
