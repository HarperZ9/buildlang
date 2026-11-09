//! `Vec::clear` empties a vector and leaves it usable.
//!
//! The method used to lower to the runtime's free routine, so the next push
//! wrote through a freed handle. The program below pushes and reads after each
//! clear; it must print the expected totals and exit 0.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn clear_keeps_the_vector_usable() {
    let programs = repo_root().join("tests").join("programs");
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(programs.join("136_vec_clear.bld"))
        .output()
        .expect("buildc runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("no C compiler") {
        return;
    }
    let want = std::fs::read_to_string(programs.join("136_vec_clear.expected"))
        .unwrap()
        .replace("\r\n", "\n");
    let got = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert_eq!(got, want);
}

#[test]
fn clear_does_not_free_the_handle() {
    let dir = std::env::temp_dir().join(format!("bl-vclear-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfile = dir.join("main.c");
    let emit = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("-o")
        .arg(&cfile)
        .arg(repo_root().join("tests/programs/136_vec_clear.bld"))
        .output()
        .expect("buildc runs");
    assert!(emit.status.success());
    let c = std::fs::read_to_string(&cfile).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let main = &c[c.rfind("main(int argc").expect("main is emitted")..];
    assert!(main.contains("build_hvec_clear("));
    assert!(!main.contains("build_hvec_free("));
}

/// `release` is the call that frees: the emitted C frees the handle, and a
/// program that builds and releases a vector many times runs to the end.
#[test]
fn release_frees_the_vector() {
    let dir = std::env::temp_dir().join(format!("bl-vrelease-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("main.bld");
    std::fs::write(
        &source,
        "fn total(n: i64) -> i64 {\n    let mut v: Vec<i64> = Vec::new();\n    let mut i = 0;\n    \
         while i < n { v.push(i); i = i + 1; }\n    let mut s = 0;\n    let mut j = 0;\n    \
         while j < n { s = s + v[j as usize]; j = j + 1; }\n    v.release();\n    s\n}\n\
         fn main() ~ Console {\n    let mut k = 0;\n    let mut sum = 0;\n    \
         while k < 2000 { sum = sum + total(100); k = k + 1; }\n    println!(\"{}\", sum);\n}\n",
    )
    .unwrap();
    let cfile = dir.join("main.c");
    let emit = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("-o")
        .arg(&cfile)
        .arg(&source)
        .output()
        .expect("buildc runs");
    assert!(
        emit.status.success(),
        "{}",
        String::from_utf8_lossy(&emit.stderr)
    );
    let c = std::fs::read_to_string(&cfile).unwrap();
    let body = &c[c.find("total(").expect("total is emitted")..];
    assert!(body.contains("build_hvec_free("));
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&source)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    if String::from_utf8_lossy(&out.stderr).contains("no C compiler") {
        return;
    }
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "9900000");
}
