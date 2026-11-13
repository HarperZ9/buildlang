//! A function returning `&[T]` may return a view of a parameter passed by
//! reference or as a slice, never a view of its own locals: `buildc check`
//! refuses that, naming the local, instead of handing back a dangling pointer.

use std::path::PathBuf;
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn check(name: &str, src: &str, run: bool) -> (Output, String) {
    let dir = std::env::temp_dir().join(format!("bl-slice-escape-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg(if run { "run" } else { "check" })
        .arg(&file)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out, text)
}

fn refused(name: &str, src: &str, needle: &str) {
    let (out, text) = check(name, src, false);
    assert!(!out.status.success(), "{name}: check must refuse:\n{text}");
    assert!(
        text.contains(needle),
        "{name}: expected `{needle}`:\n{text}"
    );
    assert!(
        text.contains("return the Vec itself"),
        "{name}: the help must name the fix:\n{text}"
    );
}

const MAIN: &str = "fn main() ~ Console { println!(\"ok\"); }\n";

#[test]
fn a_range_view_of_a_local_vec_is_refused() {
    let src =
        format!("fn make() -> &[i32] {{ let v: Vec<i32> = vec![1, 2, 3]; &v[0..2] }}\n{MAIN}");
    refused("tail", &src, "returns a slice view of the local `v`");
}

#[test]
fn an_early_return_of_a_local_view_is_refused() {
    let src = format!(
        "fn make(n: i32) -> &[i32] {{\n    let v: Vec<i32> = vec![1, 2, 3];\n    if n > 0 {{ return &v[1..]; }}\n    &[]\n}}\n{MAIN}"
    );
    refused("early", &src, "the local `v`");
}

#[test]
fn a_view_held_in_a_let_and_a_by_value_parameter_are_refused() {
    let src = format!(
        "fn make() -> &[i32] {{ let v: Vec<i32> = vec![1, 2]; let s = &v[..]; s }}\n{MAIN}"
    );
    refused("alias", &src, "the local `v`");
    let src = format!("fn make(v: Vec<i32>) -> &[i32] {{ &v[..] }}\n{MAIN}");
    refused("byvalue", &src, "the local `v`");
}

#[test]
fn from_raw_parts_over_a_local_buffer_is_refused() {
    let src = format!(
        "fn make() -> &[i32] {{\n    let v: Vec<i32> = vec![1, 2, 3];\n    unsafe {{ slice::from_raw_parts(v.as_ptr(), 3) }}\n}}\n{MAIN}"
    );
    refused("rawparts", &src, "the local `v`");
}

#[test]
fn a_view_that_cannot_be_traced_is_refused() {
    let src = format!(
        "fn make(n: i32) -> &[i32] {{ let mut s: &[i32] = &[]; if n > 0 {{ s = &[]; }} s }}\n{MAIN}"
    );
    refused("mut", &src, "cannot be shown to outlive the call");
}

#[test]
fn views_of_reference_and_slice_parameters_build_and_run() {
    let src = "struct Bag { items: Vec<i32> }\n\
        impl Bag { fn items(&self) -> &[i32] { &self.items[..] } }\n\
        fn mid(v: &Vec<i32>) -> &[i32] { &v[1..3] }\n\
        fn none() -> &[i32] { &[] }\n\
        fn raw(p: *const i32, n: usize) -> &[i32] { unsafe { slice::from_raw_parts(p, n) } }\n\
        fn main() ~ Console {\n\
        \x20   let v: Vec<i32> = vec![10, 20, 30, 40];\n\
        \x20   let b = Bag { items: vec![7, 8] };\n\
        \x20   println!(\"{} {}\", mid(&v).len(), mid(&v)[0]);\n\
        \x20   println!(\"{} {}\", mid(&v)[1], mid(&v)[1]);\n\
        \x20   println!(\"{} {}\", none().len(), b.items()[1]);\n\
        \x20   println!(\"{}\", raw(v.as_ptr(), 2)[1]);\n\
        }\n";
    let (out, text) = check("allowed", src, true);
    assert!(out.status.success(), "must build and run:\n{text}");
    assert!(text.contains("2 20\n30 30\n0 8\n20\n"), "{text}");
}
