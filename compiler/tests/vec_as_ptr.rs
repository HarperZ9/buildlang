//! `Vec::as_ptr` and `Vec::as_mut_ptr` hand a vector's element buffer to C.
//!
//! `tests/vec-as-ptr/main.bld` copies one `Vec<u8>` into another with the C
//! library's `memcpy` (through `as_mut_ptr` and `as_ptr`), compares them with
//! `memcmp`, reads a slice built over `as_ptr`, and reads a `Vec<u32>`'s buffer
//! as bytes. Accept: the printed lines. Control: a copy of the generated C whose
//! buffer helper returns the address one element past the start must print
//! different lines, so the output depends on the pointer being the buffer's
//! start. Refusal: `as_ptr` on a `Vec<f32>`, whose runtime storage is 8 bytes
//! per element, stops with the layout message rather than handing C the wrong
//! stride. Without a gcc- or clang-style compiler the test says so and returns.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

const EXPECTED: &str = "\
copied 7 100 224
equal true
view 162 8
u32 bytes 2 1 0 0
";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn posix_cc() -> Option<&'static str> {
    ["cc", "gcc", "clang"].into_iter().find(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn emit_c(dir: &Path, source: &str) -> String {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("main.bld"), source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["build", "--emit", "c"])
        .arg(dir)
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(dir.join("target/debug/main.c")).expect("main.c written")
}

/// Compile `c` and run it; returns (exit success, stdout then stderr).
fn build_and_run(cc: &str, dir: &Path, name: &str, c: &str) -> (bool, String) {
    let src = dir.join(format!("{name}.c"));
    std::fs::write(&src, c).unwrap();
    let exe = dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    });
    let mut cmd = Command::new(cc);
    cmd.arg("-std=c11")
        .arg("-O1")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .arg("-lm");
    if cfg!(windows) {
        cmd.arg("-lws2_32");
    }
    let built = cmd.output().expect("the C compiler runs");
    assert!(
        built.status.success(),
        "{name} does not build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&exe).output().expect("the program runs");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    (run.status.success(), all.replace("\r\n", "\n"))
}

#[test]
fn as_ptr_hands_the_buffer_to_c() {
    let Some(cc) = posix_cc() else {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    };
    let dir = std::env::temp_dir().join(format!("buildlang-asptr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let source = std::fs::read_to_string(repo_root().join("tests/vec-as-ptr/main.bld")).unwrap();
    let c = emit_c(&dir, &source);
    let (ok, out) = build_and_run(cc, &dir, "accept", &c);
    assert!(ok, "the program failed:\n{out}");
    assert_eq!(out, EXPECTED);

    // Control: the buffer helper points one element past the start.
    let helper = "    if (h.inner->elem_size != es) bl_vec_layout_fail(h.inner->elem_size, es);\n    return h.inner->ptr;";
    assert!(c.contains(helper), "the control found the buffer helper");
    let shifted = c.replacen(
        helper,
        "    if (h.inner->elem_size != es) bl_vec_layout_fail(h.inner->elem_size, es);\n    return (char*)h.inner->ptr + es;",
        1,
    );
    let (_, out) = build_and_run(cc, &dir, "control", &shifted);
    assert_ne!(
        out, EXPECTED,
        "a pointer one element in gave the same output"
    );

    // Refusal: a Vec<f32> is stored as f64s, so its buffer is not an f32 array.
    let f32_dir = dir.join("f32");
    let c = emit_c(
        &f32_dir,
        "fn main() ~ Console {\n    let mut w: Vec<f32> = Vec::new();\n    w.push(1.5);\n    let p = w.as_ptr();\n    println!(\"unreachable\");\n}\n",
    );
    let (ok, out) = build_and_run(cc, &f32_dir, "f32", &c);
    assert!(!ok, "as_ptr on a Vec<f32> must stop:\n{out}");
    assert!(
        out.contains("a vector holds 8-byte elements where its type needs 4 bytes"),
        "layout message: {out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
