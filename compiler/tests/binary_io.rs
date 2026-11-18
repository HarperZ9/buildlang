//! Byte buffers reach the disk unchanged: `write_file_bytes`, `read_file_bytes`
//! and the little-endian helpers of `stdlib/bytes.bld`.
//!
//! `tests/binary-io/main.bld` writes every byte value 0 to 255 (so a zero byte,
//! 0x0A, 0x1A and 0xFF) followed by a u16, u32, u64, f32 and f64 in
//! little-endian order, reads the file back, and prints what it decodes. The
//! test builds the program's C with gcc or clang and checks three things: the
//! printed lines, the file's bytes against the same bytes built here, and the
//! file's SHA-256 against a reference digest computed outside this test. A
//! second run must write identical bytes.
//!
//! Controls, each a copy of the generated C with one change, must fail the
//! byte check:
//! - the writer stops at the first zero byte, as a `strlen`-based string
//!   writer does (the gap this feature closes), on every platform;
//! - on Windows, the file is opened in text mode (`"w"`), which turns each
//!   0x0A into 0x0D 0x0A. Elsewhere text and binary mode are the same, so this
//!   control only runs on Windows.
//!
//! Without a gcc- or clang-style compiler the test says so and returns.

use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// SHA-256 of the 282 expected bytes, computed with Python's hashlib from
/// `bytes(range(256)) + struct.pack('<HIQfd', 0xBEEF, 0xDEADBEEF,
/// 0x0123456789ABCDEF, 1.5, -0.1)`.
const REFERENCE_SHA256: &str = "3bf2561b36ff31631b97e6739e142e357786d1a15540b60b4372086fa3f109e9";

const EXPECTED: &str = "\
write true
len 282
match true
zero 0 ff 255 lf 10 sub 26
u16 48879
u32 3735928559
u64 81985529216486895
f32 1.5
f64 -0.1
missing 0
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

fn expected_bytes() -> Vec<u8> {
    let mut b: Vec<u8> = (0..=255u8).collect();
    b.extend_from_slice(&0xBEEFu16.to_le_bytes());
    b.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    b.extend_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
    b.extend_from_slice(&1.5f32.to_le_bytes());
    b.extend_from_slice(&(-0.1f64).to_le_bytes());
    b
}

fn emit_c(dir: &Path) -> String {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::copy(
        repo_root().join("tests/binary-io/main.bld"),
        dir.join("main.bld"),
    )
    .unwrap();
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

/// What one run of a compiled program gave.
struct Run {
    ok: bool,
    stdout: String,
    bytes: Vec<u8>,
}

/// Compile `c`, run it with `<name>.bin` as its argument, and return its exit
/// status, its stdout and the bytes it wrote.
fn build_and_run(cc: &str, dir: &Path, name: &str, c: &str) -> Run {
    let src = dir.join(format!("{name}.c"));
    std::fs::write(&src, c).unwrap();
    let exe = dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    });
    let mut cmd = Command::new(cc);
    cmd.arg("-std=c11").arg("-O1");
    cmd.arg(&src).arg("-o").arg(&exe).arg("-lm");
    if cfg!(windows) {
        cmd.arg("-lws2_32");
    }
    let built = cmd.output().expect("the C compiler runs");
    assert!(
        built.status.success(),
        "{name} does not build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let out = dir.join(format!("{name}.bin"));
    let run = Command::new(&exe)
        .arg(&out)
        .output()
        .expect("the program runs");
    Run {
        ok: run.status.success(),
        stdout: String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n"),
        bytes: std::fs::read(&out).unwrap_or_default(),
    }
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn byte_buffers_round_trip_through_files() {
    let Some(cc) = posix_cc() else {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    };
    let dir = std::env::temp_dir().join(format!("buildlang-binio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let c = emit_c(&dir);
    let want = expected_bytes();
    assert_eq!(
        hex(&Sha256::digest(&want)),
        REFERENCE_SHA256,
        "the expected bytes drifted"
    );

    let first = build_and_run(cc, &dir, "first", &c);
    assert!(first.ok, "the program failed:\n{}", first.stdout);
    assert_eq!(first.stdout, EXPECTED);
    assert_eq!(
        first.bytes, want,
        "the file's bytes differ from the expected bytes"
    );
    assert_eq!(hex(&Sha256::digest(&first.bytes)), REFERENCE_SHA256);
    let second = build_and_run(cc, &dir, "second", &c);
    assert_eq!(second.bytes, first.bytes, "two runs wrote different bytes");

    // Control 1: a writer that stops at the first zero byte, like a string writer.
    let writer = "size_t w = n ? fwrite(v.inner->ptr, 1, n, f) : 0;";
    assert!(c.contains(writer), "the control found the byte writer");
    let to_zero = c.replacen(
        writer,
        "{ const char* z = (const char*)memchr(v.inner->ptr, 0, n); \
         if (z) n = (size_t)(z - (const char*)v.inner->ptr); } \
         size_t w = n ? fwrite(v.inner->ptr, 1, n, f) : 0;",
        1,
    );
    let zero = build_and_run(cc, &dir, "control_zero", &to_zero);
    assert!(
        zero.bytes.is_empty(),
        "the first byte is zero, so nothing should be written"
    );
    // The program then reads byte 0 of an empty file and stops at the index check.
    assert!(
        !zero.ok || zero.stdout != EXPECTED,
        "a writer that stops at a zero byte passed the round trip"
    );

    // Control 2 (Windows): text mode turns the one 0x0A into 0x0D 0x0A.
    if cfg!(windows) {
        let call = "return bl_write_vec_bytes(path, v, \"wb\");";
        assert!(c.contains(call), "the control found the binary-mode open");
        let text_mode = c.replacen(call, "return bl_write_vec_bytes(path, v, \"w\");", 1);
        let text = build_and_run(cc, &dir, "control_text", &text_mode);
        assert_ne!(text.bytes, want, "a text-mode write kept every byte");
        assert_eq!(
            text.bytes.len(),
            want.len() + 1,
            "one 0x0A should gain a 0x0D"
        );
        assert_ne!(
            text.stdout, EXPECTED,
            "a text-mode write passed the round trip"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
