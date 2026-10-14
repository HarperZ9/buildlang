//! Programs `buildc check` used to accept that the C compiler then rejected.
//!
//! Found 2026-10-14 while checking the doc blocks: arguments of formatting
//! macros were never type-checked, and an integer literal unified with any
//! type. Each case must now fail the check with BL0301; the paired accepted
//! cases guard against rejecting valid programs.

use std::{path::PathBuf, process::Command};

fn buildc() -> Command {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", root.join("stdlib"));
    cmd
}

fn check(name: &str, body: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("buildlang_soundness_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{name}.bld"));
    std::fs::write(&file, body).unwrap();
    let out = buildc()
        .arg("check")
        .arg(&file)
        .arg("--error-format=json")
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

const PRELUDE: &str = "struct P { x: i32 }\nfn takes_p(p: P) -> i32 { p.x }\nfn takes_str(s: str) -> i32 { 1 }\nfn takes_int(n: i32) -> i32 { n }\n";

#[test]
fn rejected_programs_report_a_type_mismatch() {
    let cases = [
        (
            "macro_arg_str_for_int",
            "println!(\"{}\", takes_int(\"x\"));",
        ),
        ("macro_arg_int_for_struct", "println!(\"{}\", takes_p(3));"),
        ("macro_arg_int_for_str", "println!(\"{}\", takes_str(3));"),
        (
            "named_macro_arg",
            "println!(\"{v}\", v = takes_int(\"x\"));",
        ),
        ("assert_arg", "assert_eq!(takes_int(\"x\"), 1);"),
        ("let_int_as_str", "let s: str = 4;\nprintln!(\"{}\", s);"),
        ("let_int_as_struct", "let p: P = 3;\nprintln!(\"{}\", p.x);"),
        (
            "call_int_for_str",
            "let v = takes_str(3);\nprintln!(\"{}\", v);",
        ),
        (
            "let_float_as_int",
            "let n: i32 = 2.5;\nprintln!(\"{}\", n);",
        ),
        (
            "format_arg",
            "let s = format!(\"{}\", takes_p(1));\nprintln!(\"{}\", s);",
        ),
    ];
    for (name, stmt) in cases {
        let src = format!("{PRELUDE}fn main() ~ Console {{\n    {stmt}\n}}\n");
        let (ok, json) = check(name, &src);
        assert!(!ok, "{name}: the check must fail\n{src}");
        assert!(
            json.contains("\"code\": \"BL0301\""),
            "{name}: expected BL0301\n{json}"
        );
        assert!(
            !json.contains("DefId("),
            "{name}: types must print by name\n{json}"
        );
    }
}

#[test]
fn accepted_programs_still_pass() {
    let cases = [
        (
            "arith_in_macro",
            "let x = 2;\nprintln!(\"{} {}\", x + 1, takes_int(x));",
        ),
        ("named_arg", "println!(\"{v}\", v = takes_int(3));"),
        (
            "struct_field_in_macro",
            "let p = P { x: 1 };\nprintln!(\"{}\", takes_p(p));",
        ),
        (
            "vec_list",
            "let v = vec![1, 2, 3];\nprintln!(\"{}\", v.len());",
        ),
        (
            "vec_repeat",
            "let v = vec![0; 4];\nprintln!(\"{}\", v.len());",
        ),
        (
            "int_literal_as_float",
            "let f: f64 = 1;\nprintln!(\"{}\", f);",
        ),
        (
            "assert_ok",
            "assert_eq!(takes_int(2), 2);\nassert!(takes_int(1) > 0, \"positive {}\", 1);",
        ),
        ("trailing_comma", "println!(\"{}\", takes_int(1),);"),
        ("str_arg", "println!(\"{}\", takes_str(\"a\"));"),
    ];
    for (name, stmt) in cases {
        let src = format!("{PRELUDE}fn main() ~ Console {{\n    {stmt}\n}}\n");
        let (ok, json) = check(name, &src);
        assert!(ok, "{name}: the check must pass\n{src}\n{json}");
    }
}
