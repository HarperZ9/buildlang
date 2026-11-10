//! A `const` or `static` keeps its declared type wherever the code that uses
//! it sits in the source. A `u64` constant declared after the function that
//! adds to it was stored in a 32-bit signed temporary (G13), and a
//! `static mut TICKS: u64` incremented by a bare literal used the 32-bit add
//! helper (G6). Both printed wrong values with no diagnostic.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Run `src` as a program; `None` when no C compiler is available.
fn run(tag: &str, src: &str) -> Option<(i32, String, String)> {
    let dir = std::env::temp_dir().join(format!("bl-gorder-{}-{tag}", std::process::id()));
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

fn expect(tag: &str, src: &str, want: &str) {
    let Some((code, stdout, stderr)) = run(tag, src) else {
        eprintln!("skipped {tag}: no C compiler");
        return;
    };
    assert_eq!(code, 0, "{tag} failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
    assert_eq!(stdout, want, "{tag}: wrong output\nstderr:\n{stderr}");
}

#[test]
fn u64_const_declared_last_keeps_its_width() {
    let src = "fn a(v: u64) -> u64 { BASE + v % 1024_u64 }\n\
               fn d(v: u64) -> u64 { v % 1024_u64 + BASE }\n\
               fn k() -> u64 { BASE + 8_u64 }\n\
               fn main() ~ Console {\n\
               println!(\"{:x} {:x} {:x}\", a(568_u64), d(568_u64), k());\n\
               }\n\
               const BASE: u64 = 0x80000000_u64;\n";
    expect("last", src, "80000238 80000238 80000008\n");
}

#[test]
fn several_late_consts_of_mixed_widths() {
    let src = "fn f() -> u64 { HI + LO + 1_u64 }\n\
               fn g() -> i64 { NEG + 1_i64 }\n\
               fn h() -> u32 { SMALL + 1_u32 }\n\
               fn main() ~ Console {\n\
               println!(\"{} {} {}\", f(), g(), h());\n\
               }\n\
               const HI: u64 = 0xFFFFFFFF00000000_u64;\n\
               const LO: u64 = 0x80000000_u64;\n\
               const NEG: i64 = -5000000000_i64;\n\
               const SMALL: u32 = 4000000000_u32;\n";
    expect(
        "multi",
        src,
        "18446744071562067969 -4999999999 4000000001\n",
    );
}

#[test]
fn u64_static_mut_counts_past_2_pow_32() {
    let src = "static mut TICKS: u64 = 0;\n\
               fn tick() -> u64 { unsafe { TICKS = TICKS + 1; TICKS } }\n\
               fn main() ~ Console {\n\
               unsafe { TICKS = 4294967290_u64; }\n\
               let mut i = 0;\n\
               while i < 10 { tick(); i = i + 1; }\n\
               println!(\"{}\", tick());\n\
               }\n";
    expect("ticks", src, "4294967301\n");
}

#[test]
fn u64_static_mut_declared_last() {
    let src = "fn bump() -> u64 { unsafe { COUNT = COUNT + 2; COUNT } }\n\
               fn main() ~ Console {\n\
               unsafe { COUNT = 0xFFFFFFFF_u64; }\n\
               println!(\"{}\", bump());\n\
               }\n\
               static mut COUNT: u64 = 0;\n";
    expect("static-last", src, "4294967297\n");
}
