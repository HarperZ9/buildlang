//! M4: DETMATH float rules (Low Water `design/DETMATH.md`).
//!
//! - `tests/detmath/goldens.bld` implements the DETMATH algorithms in
//!   BuildLang and prints the twelve golden calls as bit patterns. The
//!   expected file is the spec's table. One differing bit fails.
//! - `tests/detmath/ops.bld` prints sqrt, floor, abs, min and max results whose
//!   expected bits JS wrote (`tools/detmath_ops_expected.mjs`). On 1.8.0 a free
//!   `min(2.5, -1.25)` truncated to i32 and `max` dropped a NaN.
//! - The generated C must keep `a * b + c` unfused even when a host compiles it
//!   with GNU C, `-O2` and `-mfma`. A control copy with the pragmas removed must
//!   fuse under the same flags, which shows the test can fail on this machine;
//!   when it does not (no FMA, no gcc), the test says so and returns.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").trim_end().to_string()
}

fn run_program(file: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(file)
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "{} failed:\n{}{}",
        file.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    normalize(&String::from_utf8_lossy(&out.stdout))
}

fn expect_lines(name: &str) {
    let dir = repo_root().join("tests/detmath");
    let got = run_program(&dir.join(format!("{name}.bld")));
    let want = normalize(&std::fs::read_to_string(dir.join(format!("{name}.expected"))).unwrap());
    let diffs: Vec<String> = got
        .lines()
        .zip(want.lines())
        .filter(|(g, w)| g != w)
        .map(|(g, w)| format!("  got  {g}\n  want {w}"))
        .collect();
    assert!(
        diffs.is_empty() && got.lines().count() == want.lines().count(),
        "{name}: bits differ from the expected table\n{}\n(full output:\n{got})",
        diffs.join("\n")
    );
}

#[test]
fn detmath_goldens_match_bit_for_bit() {
    expect_lines("goldens");
}

#[test]
fn sqrt_floor_abs_min_max_match_js_bit_for_bit() {
    expect_lines("ops");
}

fn gcc() -> Option<&'static str> {
    ["gcc", "cc"].into_iter().find(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).contains("clang"))
            .unwrap_or(false)
    })
}

fn compile_and_feed(cc: &str, c_file: &Path, exe: &Path) -> Option<String> {
    let mut cmd = Command::new(cc);
    cmd.args(["-w", "-O2", "-mfma", "-std=gnu99"])
        .arg(c_file)
        .arg("-o")
        .arg(exe)
        .arg("-lm");
    if cfg!(windows) {
        cmd.arg("-lws2_32");
    }
    let built = cmd.output().ok()?;
    if !built.status.success() {
        return None;
    }
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    use std::io::Write;
    child.stdin.take()?.write_all(b"0123456789\n").ok()?;
    let out = child.wait_with_output().ok()?;
    out.status
        .success()
        .then(|| normalize(&String::from_utf8_lossy(&out.stdout)))
}

#[test]
fn generated_c_stays_unfused_under_host_fma_flags() {
    let Some(cc) = gcc() else {
        eprintln!("skipping: no gcc on PATH");
        return;
    };
    let dir = std::env::temp_dir().join(format!("buildlang-detmath-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        repo_root().join("tests/detmath/contract.bld"),
        dir.join("main.bld"),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["build", "--emit", "c"])
        .arg(&dir)
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let c_file = dir.join("target/debug/main.c");
    let c_text = std::fs::read_to_string(&c_file).expect("main.c written");
    assert!(c_text.contains("#pragma GCC optimize(\"fp-contract=off\")"));
    assert!(c_text.contains("#pragma STDC FP_CONTRACT OFF"));
    assert!(c_text.contains("#pragma fp_contract(off)"));

    // Control: the same C without the pragmas, same flags.
    let control: String = c_text
        .lines()
        .filter(|l| !l.contains("fp-contract=off") && !l.contains("FP_CONTRACT"))
        .collect::<Vec<_>>()
        .join("\n");
    let control_file = dir.join("control.c");
    std::fs::write(&control_file, control).unwrap();

    let ext = if cfg!(windows) { ".exe" } else { "" };
    let Some(fused) = compile_and_feed(cc, &control_file, &dir.join(format!("control{ext}")))
    else {
        eprintln!("skipping: {cc} could not build or run the -mfma control");
        return;
    };
    let got = compile_and_feed(cc, &c_file, &dir.join(format!("guarded{ext}")))
        .expect("the guarded C builds and runs with the same flags");
    // 0.1 * n - 1 for n = 10 or 11 (the line length with or without '\r').
    let unfused = ["0000000000000000", "3fb99999999999a0"];
    assert!(
        unfused.contains(&got.as_str()),
        "the generated C fused a * b + c under -mfma: got {got}"
    );
    if fused == got {
        eprintln!("note: {cc} did not contract the control either (no FMA here); the check above is weak on this machine");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
