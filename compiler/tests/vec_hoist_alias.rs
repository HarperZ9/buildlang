//! A store that can replace a vector handle stops hoisting
//! (the C backend's `c/vec_hoist_stores.rs`).
//!
//! The language has no borrow checker, so `f(&h.data, &mut h, ..)` compiles
//! and the callee can replace the vector its first parameter points at. A
//! hoisted buffer pointer would then read the old vector. Each planted case
//! replaces the vector through a different store and must read the new one.

use std::path::PathBuf;
use std::process::Command;

fn buildc() -> Command {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", root.join("stdlib"));
    cmd
}

/// Emitted C and the program's stdout; `None` when no C compiler is available.
fn run(tag: &str, src: &str) -> Option<(String, String)> {
    let dir = std::env::temp_dir().join(format!("bl-vhalias-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, src).unwrap();
    let cfile = dir.join("main.c");
    let emit = buildc().arg("-o").arg(&cfile).arg(&file).output().unwrap();
    assert!(
        emit.status.success(),
        "{}",
        String::from_utf8_lossy(&emit.stderr)
    );
    let c = std::fs::read_to_string(&cfile).unwrap();
    let out = buildc().arg("run").arg(&file).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    if stderr.contains("no C compiler") {
        return None;
    }
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    Some((c, stdout))
}

/// The C text of the definition of function `name`.
fn c_fn<'a>(c: &'a str, name: &str) -> &'a str {
    let pat = format!(" {name}(");
    let head = c
        .match_indices(&pat)
        .map(|(i, _)| i)
        .find(|&i| c[i..].lines().next().unwrap().trim_end().ends_with('{'))
        .expect("function definition");
    let end = c[head..].find("\n}\n").map(|e| head + e).unwrap_or(c.len());
    &c[head..end]
}

const HOLDER: &str = "struct Holder { data: Vec<i64> }\n";

fn main_calling(call: &str, second: &str) -> String {
    format!(
        "fn main() ~ Console {{\n\
         \x20   let mut h = Holder {{ data: Vec::new() }};\n\
         \x20   h.data.push(111);\n\
         {second}\n\
         \x20   let r = {call};\n\
         \x20   println!(\"{{}}\", r);\n}}\n"
    )
}

#[test]
fn a_field_store_through_an_alias_reads_the_new_vector() {
    let src = format!(
        "{HOLDER}fn swap_in(v: &mut Vec<i64>, h: &mut Holder, w: Vec<i64>) -> i64 {{\n\
         \x20   h.data = w;\n\
         \x20   v[0]\n}}\n{}",
        main_calling(
            "swap_in(&mut h.data, &mut h, w)",
            "    let mut w: Vec<i64> = Vec::new();\n    w.push(222);"
        )
    );
    let Some((c, stdout)) = run("field", &src) else {
        return;
    };
    assert_eq!(stdout, "222\n", "the read follows the replaced vector");
    assert!(!c_fn(&c, "swap_in").contains("bl_vv_"), "not hoisted");
}

#[test]
fn a_whole_struct_store_through_an_alias_reads_the_new_vector() {
    let src = format!(
        "{HOLDER}fn swap_in(v: &Vec<i64>, h: &mut Holder, g: Holder) -> i64 {{\n\
         \x20   *h = g;\n\
         \x20   v[0]\n}}\n{}",
        main_calling(
            "swap_in(&h.data, &mut h, g)",
            "    let mut g = Holder { data: Vec::new() };\n    g.data.push(222);"
        )
    );
    let Some((c, stdout)) = run("whole", &src) else {
        return;
    };
    assert_eq!(stdout, "222\n", "the read follows the replaced struct");
    assert!(!c_fn(&c, "swap_in").contains("bl_vv_"), "not hoisted");
}

#[test]
fn a_store_of_plain_numbers_keeps_the_hoist() {
    let src = "struct Acc { sum: f64, n: i64 }\n\
        fn total(b: &Vec<f64>, a: &mut Acc) {\n\
        \x20   let mut i = 0;\n\
        \x20   while i < 3 { a.sum = a.sum + b[i]; a.n = a.n + 1; i = i + 1; }\n\
        }\n\
        fn main() ~ Console {\n\
        \x20   let mut b: Vec<f64> = Vec::new();\n\
        \x20   b.push(1.5); b.push(2.0); b.push(3.0);\n\
        \x20   let mut a = Acc { sum: 0.0, n: 0 };\n\
        \x20   total(&b, &mut a);\n\
        \x20   println!(\"{} {}\", a.sum, a.n);\n\
        }\n";
    let Some((c, stdout)) = run("plain", src) else {
        return;
    };
    assert_eq!(stdout, "6.5 3\n");
    assert!(
        c_fn(&c, "total").contains("bl_vat(bl_vv_"),
        "a numeric field store cannot replace a vector, so the hoist stays"
    );
}
