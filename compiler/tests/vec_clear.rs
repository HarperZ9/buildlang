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
