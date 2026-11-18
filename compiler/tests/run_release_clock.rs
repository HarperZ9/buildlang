//! `buildc run --release` and `clock_ns`.
//!
//! `tests/run-release/main.bld` calls a C file compiled into its own
//! translation unit (`opt.c`) that reports whether gcc or clang optimised it
//! (`__OPTIMIZE__`), then times four stretches of work with `clock_ns`.
//! Accept: plain `run` reports `opt 0` and `run --release` reports `opt 1`
//! (the plain run is the control for the flag: the same program, built the
//! default way, must say it was not optimised); both give the same results.
//! `clock_ns` is monotonic, resolves below a millisecond and agrees with
//! `clock_ms` to within a millisecond. Control for the clock: a copy of the
//! generated C whose `clock_ns` counts whole milliseconds must report
//! `sub-ms false`. Refusal: `--release` with `--emit-receipt`, whose verifier
//! rebuilds without optimisation. A compiler that does not say whether it
//! optimised (MSVC prints `opt -1`) skips the flag check.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

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

fn program() -> PathBuf {
    repo_root().join("tests/run-release/main.bld")
}

fn run(extra: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .args(extra)
        .arg(program())
        .output()
        .expect("buildc runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text.replace("\r\n", "\n"))
}

const RESULTS: &str = "ordered true\nsub-ms true\nagree true\nx 4\n";

#[test]
fn release_flag_optimises_and_clock_ns_is_fine_grained() {
    if posix_cc().is_none() {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    }
    let (ok, plain) = run(&[]);
    assert!(ok, "plain run failed:\n{plain}");
    let (ok, release) = run(&["--release"]);
    assert!(ok, "release run failed:\n{release}");
    if plain.starts_with("opt -1") {
        eprintln!("the C compiler does not report optimisation; flag check skipped");
    } else {
        assert!(
            plain.starts_with("opt 0\n"),
            "plain run was optimised:\n{plain}"
        );
        assert!(
            release.starts_with("opt 1\n"),
            "--release was not optimised:\n{release}"
        );
    }
    assert!(plain.ends_with(RESULTS), "plain run:\n{plain}");
    assert!(release.ends_with(RESULTS), "release run:\n{release}");

    let (ok, refused) = run(&["--release", "--emit-receipt", "-"]);
    assert!(!ok, "--release with --emit-receipt must be refused");
    assert!(refused.contains("--release is not supported"), "{refused}");
}

/// Compile `c` with the C file beside the program and run it.
fn build_and_run(cc: &str, dir: &Path, c: &str) -> String {
    let src = dir.join("control.c");
    std::fs::write(&src, c).unwrap();
    let exe = dir.join(if cfg!(windows) {
        "control.exe"
    } else {
        "control"
    });
    let mut cmd = Command::new(cc);
    cmd.arg("-std=c11").arg(&src).arg("-o").arg(&exe).arg("-lm");
    if cfg!(windows) {
        cmd.arg("-lws2_32");
    }
    let built = cmd.output().expect("the C compiler runs");
    assert!(
        built.status.success(),
        "control does not build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&exe).output().expect("the program runs");
    String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n")
}

#[test]
fn a_millisecond_clock_fails_the_resolution_check() {
    let Some(cc) = posix_cc() else {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    };
    let dir = std::env::temp_dir().join(format!("buildlang-clockns-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["main.bld", "opt.c"] {
        std::fs::copy(
            repo_root().join("tests/run-release").join(name),
            dir.join(name),
        )
        .unwrap();
    }
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
    let c = std::fs::read_to_string(dir.join("target/debug/main.c")).expect("main.c written");

    let accept = build_and_run(cc, &dir, &c);
    assert!(accept.ends_with(RESULTS), "accept build:\n{accept}");

    let head = "static int64_t build_clock_ns(void) {";
    assert!(c.contains(head), "the control found clock_ns");
    let coarse = c.replacen(
        head,
        "static int64_t build_clock_ns(void) { return build_clock_ms() * 1000000LL; }\nstatic int64_t bl_unused_clock_ns(void) {",
        1,
    );
    let control = build_and_run(cc, &dir, &coarse);
    assert!(
        control.contains("sub-ms false"),
        "a millisecond clock passed the resolution check:\n{control}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
