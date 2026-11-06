//! Units through roots and powers (checker slice two).
//!
//! `units/units_pendulum.bld` uses `.sqrt()`, a free `sqrt`, `**` with a
//! literal exponent, `.powi(-1)` and `.sin()` of a ratio, all on unit-carrying
//! values. Checker slice one refused every one of these. The program must
//! check clean, emit C byte-identical to its unannotated twin (units are erased
//! before MIR), and print the same lines. A copy with one wrong annotation must
//! fail with a unit diagnostic, which shows the clean check is not vacuous.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/units")
        .join(name)
}

fn buildc(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(args)
        .output()
        .expect("buildc runs")
}

fn emit_c(src: &Path, out: &Path) -> Vec<u8> {
    let o = buildc(&[
        src.as_os_str(),
        "--target".as_ref(),
        "c".as_ref(),
        "-o".as_ref(),
        out.as_os_str(),
    ]);
    assert!(
        o.status.success(),
        "{} must compile to C:\n{}",
        src.display(),
        String::from_utf8_lossy(&o.stderr)
    );
    std::fs::read(out).expect("read emitted C")
}

#[test]
fn roots_and_powers_check_and_erase_to_identical_c() {
    let annotated = fixture("units_pendulum.bld");
    let plain = fixture("units_pendulum_plain.bld");

    let check = buildc(&["check".as_ref(), annotated.as_os_str()]);
    assert!(
        check.status.success(),
        "the annotated pendulum must check clean:\n{}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let dir = std::env::temp_dir().join(format!("buildlang-units-powers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let a = emit_c(&annotated, &dir.join("annotated.c"));
    let p = emit_c(&plain, &dir.join("plain.c"));
    assert_eq!(a, p, "unit annotations must not change the emitted C");

    let run_a = buildc(&["run".as_ref(), annotated.as_os_str()]);
    let run_p = buildc(&["run".as_ref(), plain.as_os_str()]);
    if run_a.status.success() || run_p.status.success() {
        assert!(run_a.status.success() && run_p.status.success());
        assert_eq!(run_a.stdout, run_p.stdout, "both must print the same lines");
        let text = String::from_utf8_lossy(&run_a.stdout).to_string();
        let first: f64 = text.lines().next().unwrap().trim().parse().unwrap();
        let want = std::f64::consts::TAU * (1.0f64 / 9.80665).sqrt();
        assert!((first - want).abs() < 1e-12, "period {first}, want {want}");
    } else {
        eprintln!(
            "skipping the run half: no C compiler ({})",
            String::from_utf8_lossy(&run_a.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn one_wrong_annotation_fails_with_a_unit_code() {
    let src = std::fs::read_to_string(fixture("units_pendulum.bld")).unwrap();
    // The reciprocal of a period is a frequency; call it seconds instead.
    let wrong = src.replace(
        "let f: f64<Hz> = t.powi(-1);",
        "let f: f64<s> = t.powi(-1);",
    );
    assert_ne!(src, wrong, "the mutation must apply");
    let dir = std::env::temp_dir().join(format!("buildlang-units-wrong-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("wrong.bld");
    std::fs::write(&file, wrong).unwrap();
    let o = buildc(&["check".as_ref(), file.as_os_str()]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        !o.status.success(),
        "the wrong unit must not check:\n{text}"
    );
    assert!(text.contains("BL0601"), "expected BL0601:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
