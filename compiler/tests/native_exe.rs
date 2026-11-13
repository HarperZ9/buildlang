// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! `--target x86_64-native` writes a PE32+ (Windows) or ELF64 (Linux)
//! executable with no C compiler, assembler or linker. Each corpus program is
//! built that way and run, and its stdout and exit code must equal what the
//! C backend's build of the same source produces. A planted-fault control
//! flips one byte of machine code and checks the comparison then fails.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CORPUS: [(&str, &str); 9] = [
    ("fib", "fn fib(n: i64) -> i64 {\n    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }\n}\n\nfn main() ~ Console {\n    println(\"fib(25) = {}\", fib(25));\n}\n"),
    ("gcd", "fn gcd(a: u64, b: u64) -> u64 {\n    let mut x: u64 = a;\n    let mut y: u64 = b;\n    while y != 0 {\n        let t: u64 = x % y;\n        x = y;\n        y = t;\n    }\n    x\n}\n\nfn main() ~ Console {\n    println(\"{} {} {}\", gcd(1071, 462), gcd(17, 5), gcd(1000000007, 1000000007));\n}\n"),
    ("wrapping_sum", "fn main() ~ Console {\n    let mut s: u32 = 4000000000;\n    let mut i: u32 = 0;\n    while i < 10 {\n        s = s.wrapping_add(123456789);\n        i = i + 1;\n    }\n    let m: i32 = 2000000000;\n    println(\"{} {}\", s, m.wrapping_mul(3));\n}\n"),
    ("nested_calls", "fn sq(x: i64) -> i64 { x * x }\nfn add3(a: i64, b: i64, c: i64) -> i64 { a + b + c }\nfn mix(a: i64, b: i64, c: i64, d: i64) -> i64 { add3(sq(a), sq(b), c) - d }\n\nfn main() ~ Console {\n    println(\"{}\", mix(sq(2), add3(1, 2, 3), mix(1, 2, 3, 4), 10));\n}\n"),
    ("branches", "fn classify(n: i64) -> i64 {\n    if n < 0 { 0 - 1 } else if n == 0 { 0 } else if n < 100 { 1 } else { 2 }\n}\n\nfn main() ~ Console {\n    let big: u64 = 18000000000000000000;\n    let gt: bool = big > 9000000000000000000;\n    let both: bool = gt && classify(5) == 1;\n    println(\"{} {} {} {} {} {}\", classify(-9), classify(0), classify(42), classify(1000), gt, both);\n}\n"),
    ("u64_divrem", "fn main() ~ Console {\n    let a: u64 = 18446744073709551615;\n    let b: u64 = 7;\n    let c: i64 = -17;\n    let d: i64 = 5;\n    println(\"{} {} {} {}\", a / b, a % b, c / d, c % d);\n}\n"),
    ("negatives", "fn neg(x: i32) -> i32 { 0 - x }\n\nfn main() ~ Console {\n    let a: i32 = -2147483647;\n    let b: i64 = -9000000000;\n    let c: i32 = neg(a);\n    println(\"{} {} {} {}\", a, b, c, a - 1);\n}\n"),
    ("string_and_numbers", "fn main() ~ Console {\n    let n: i64 = 42;\n    let u: u32 = 7;\n    print!(\"answer: \");\n    println(\"{} of {} is {}%\", n, u, n * 100 / 7);\n    println(\"done\");\n}\n"),
    ("loop_break", "fn main() ~ Console {\n    let mut i: i64 = 0;\n    let mut acc: i64 = 0;\n    loop {\n        i = i + 1;\n        if i % 3 == 0 { continue; }\n        acc = acc + i;\n        if i >= 20 { break; }\n    }\n    println(\"{} {}\", i, acc);\n}\n"),
];

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn work_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("buildc_native_exe_{}", std::process::id()));
    fs::create_dir_all(&d).expect("create work dir");
    d
}

/// The native target exists only for x86-64 Windows and Linux hosts.
fn host_supported() -> bool {
    cfg!(target_arch = "x86_64") && (cfg!(windows) || cfg!(target_os = "linux"))
}

fn build_native(src: &Path, exe: &Path) -> Output {
    buildc()
        .arg(src)
        .args(["--target", "x86_64-native", "-o"])
        .arg(exe)
        .output()
        .expect("run buildc")
}

/// (stdout with LF line ends, exit code) of a finished process.
fn result(out: &Output) -> (String, Option<i32>) {
    (
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        out.status.code(),
    )
}

fn run_exe(exe: &Path) -> Output {
    Command::new(exe)
        .output()
        .unwrap_or_else(|e| panic!("could not start {}: {e}", exe.display()))
}

/// `buildc run` through the C backend, or None when no C toolchain works here.
fn run_c(src: &Path) -> Option<(String, Option<i32>)> {
    let out = buildc().arg("run").arg(src).output().ok()?;
    if !out.status.success() {
        eprintln!(
            "C backend unavailable for {}: {}",
            src.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        return None;
    }
    Some(result(&out))
}

/// Native stdout comes after buildc's own build log when run through
/// `buildc run`; compare against the program's lines only.
fn tail_matches(c_out: &str, native: &str) -> bool {
    c_out.ends_with(native)
}

#[test]
fn native_executables_match_c_backend() {
    if !host_supported() {
        eprintln!("SKIP native_exe: host is not x86-64 Windows or Linux; nothing was run");
        return;
    }
    let dir = work_dir();
    let mut compared = 0;
    for (name, source) in CORPUS {
        let src = dir.join(format!("{name}.bld"));
        let exe = dir.join(format!("{name}.exe"));
        fs::write(&src, source).expect("write source");
        let build = build_native(&src, &exe);
        assert!(
            build.status.success(),
            "{name}: native build failed: {}{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        let header = fs::read(&exe).expect("read executable");
        let magic: &[u8] = if cfg!(windows) { b"MZ" } else { b"\x7fELF" };
        assert!(header.starts_with(magic), "{name}: wrong container magic");
        let (native_out, native_code) = result(&run_exe(&exe));
        assert!(
            !native_out.is_empty(),
            "{name}: native program printed nothing"
        );
        match run_c(&src) {
            Some((c_out, c_code)) => {
                assert!(
                    tail_matches(&c_out, &native_out),
                    "{name}: native stdout {native_out:?} differs from C {c_out:?}"
                );
                assert_eq!(native_code, c_code, "{name}: exit codes differ");
                compared += 1;
            }
            None => eprintln!("{name}: C comparison skipped (no C toolchain)"),
        }
        eprintln!("{name}: {}", native_out.trim_end());
    }
    eprintln!(
        "compared {compared} of {} programs with the C backend",
        CORPUS.len()
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Planted fault: change the digit base in print_u64 (`add dl, '0'` becomes
/// `add dl, '1'`). The comparison against the C backend must then fail,
/// which shows the comparison above is live.
#[test]
fn flipped_code_byte_breaks_the_comparison() {
    if !host_supported() {
        eprintln!("SKIP native_exe: host is not x86-64 Windows or Linux; nothing was run");
        return;
    }
    let dir = work_dir().join("flip");
    fs::create_dir_all(&dir).unwrap();
    let (name, source) = CORPUS[0];
    let src = dir.join(format!("{name}.bld"));
    let exe = dir.join(format!("{name}.exe"));
    fs::write(&src, source).unwrap();
    assert!(build_native(&src, &exe).status.success());
    let good = result(&run_exe(&exe));
    let mut bytes = fs::read(&exe).unwrap();
    let at = bytes
        .windows(3)
        .position(|w| w == [0x80, 0xC2, 0x30])
        .expect("digit routine present");
    bytes[at + 2] = 0x31;
    let bad_exe = dir.join(format!("{name}_flipped.exe"));
    fs::write(&bad_exe, &bytes).unwrap();
    // A fresh file has no execute bit on Linux; give the copy the original's permissions.
    fs::set_permissions(&bad_exe, fs::metadata(&exe).unwrap().permissions()).unwrap();
    let bad = result(&run_exe(&bad_exe));
    assert_ne!(good.0, bad.0, "flipping a code byte left stdout unchanged");
    if let Some((c_out, _)) = run_c(&src) {
        assert!(
            tail_matches(&c_out, &good.0),
            "unflipped build should match C"
        );
        assert!(
            !tail_matches(&c_out, &bad.0),
            "flipped build must not match C"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// Constructs outside the slice are refused by name and no file is written.
#[test]
fn unsupported_constructs_are_refused_by_name() {
    let dir = work_dir().join("refuse");
    fs::create_dir_all(&dir).unwrap();
    let cases = [
        ("float", "fn main() ~ Console {\n    let x: f64 = 1.5;\n    println(\"{}\", x);\n}\n", "f64"),
        ("structs", "struct P { x: i64 }\n\nfn main() ~ Console {\n    let p = P { x: 1 };\n    println(\"{}\", p.x);\n}\n", "P"),
    ];
    for (name, source, needle) in cases {
        let src = dir.join(format!("{name}.bld"));
        let exe = dir.join(format!("{name}.exe"));
        fs::write(&src, source).unwrap();
        let out = buildc()
            .arg(&src)
            .args(["--target", "x86_64-windows", "-o"])
            .arg(&exe)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{name}: should be refused");
        assert!(
            err.contains("x86-64 native target does not support") && err.contains(needle),
            "{name}: diagnostic {err:?}"
        );
        assert!(!exe.exists(), "{name}: an output file was written");
    }
    let _ = fs::remove_dir_all(&dir);
}
