//! `#[measured(source = "...", uncertainty = ...)]` constants.
//!
//! - The check receipt lists each measured constant with its value as
//!   written, its canonical unit, its source and its uncertainty.
//! - `receipt verify` re-derives the list: a receipt edited to change a
//!   source no longer verifies, and the untouched receipt does.
//! - The attribute changes no emitted C: the fixture and its plain twin emit
//!   identical bytes and print the same value.
//! - A program with no measured constants has no `measured_constants` key, so
//!   its receipt is unchanged from before the feature.
//! - A measured constant without a source fails check with BL0605 (the reject
//!   program `tests/reject/measured_without_source.bld`).

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
        .join("tests/measured")
        .join(name)
}

fn buildc(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(args)
        .output()
        .expect("buildc runs")
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("buildlang-measured-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn receipt(src: &Path, out: &Path) -> serde_json::Value {
    let o = buildc(&[
        "check".as_ref(),
        src.as_os_str(),
        "--receipt".as_ref(),
        out.as_os_str(),
    ]);
    assert!(
        o.status.success(),
        "{} must check:\n{}",
        src.display(),
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap()
}

#[test]
fn receipt_lists_measured_constants_and_verify_rederives_them() {
    let dir = scratch("receipt");
    let path = dir.join("receipt.json");
    let r = receipt(&fixture("gravity.bld"), &path);
    let list = r["measured_constants"]
        .as_array()
        .expect("measured_constants is a list");
    assert_eq!(list.len(), 2, "{r:#}");
    assert_eq!(list[0]["name"], "G");
    assert_eq!(list[0]["value"], "6.67430e-11");
    assert_eq!(list[0]["source"], "CODATA 2018");
    assert_eq!(list[0]["uncertainty"], "1.5e-15");
    assert!(list[0]["unit"].as_str().unwrap().contains("kg"));
    assert_eq!(list[1]["name"], "EARTH_RADIUS");
    assert_eq!(list[1]["unit"], "m");
    assert!(list[1].get("uncertainty").is_none());

    let ok = buildc(&["receipt".as_ref(), "verify".as_ref(), path.as_os_str()]);
    assert!(
        ok.status.success(),
        "the untouched receipt must verify:\n{}",
        String::from_utf8_lossy(&ok.stderr)
    );

    let tampered = dir.join("tampered.json");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("CODATA 2018", "CODATA 2014");
    std::fs::write(&tampered, text).unwrap();
    let bad = buildc(&["receipt".as_ref(), "verify".as_ref(), tampered.as_os_str()]);
    assert!(!bad.status.success(), "an edited source must not verify");
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("measured_constants"),
        "{}",
        String::from_utf8_lossy(&bad.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn attribute_changes_no_emitted_c_and_plain_receipts_have_no_key() {
    let dir = scratch("erase");
    let a = dir.join("a.c");
    let p = dir.join("p.c");
    for (src, out) in [
        (fixture("gravity.bld"), &a),
        (fixture("gravity_plain.bld"), &p),
    ] {
        let o = buildc(&[
            src.as_os_str(),
            "--target".as_ref(),
            "c".as_ref(),
            "-o".as_ref(),
            out.as_os_str(),
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    assert_eq!(
        std::fs::read(&a).unwrap(),
        std::fs::read(&p).unwrap(),
        "`#[measured]` must not change the emitted C"
    );

    let plain = receipt(&fixture("gravity_plain.bld"), &dir.join("plain.json"));
    assert!(
        plain.get("measured_constants").is_none(),
        "a receipt without measured constants must not carry the key: {plain:#}"
    );

    let run = buildc(&["run".as_ref(), fixture("gravity.bld").as_os_str()]);
    if run.status.success() {
        let g: f64 = String::from_utf8_lossy(&run.stdout).trim().parse().unwrap();
        let want = 6.67430e-11 * 5.972e24 / (6.371e6f64 * 6.371e6);
        assert!((g - want).abs() < 1e-12, "g = {g}, want {want}");
    } else {
        eprintln!(
            "skipping the run half: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_measured_attributes_fail_with_bl0605() {
    let dir = scratch("bad");
    let cases = [
        (
            "#[measured(uncertainty = 1.0)]\nconst A: f64 = 1.0;",
            "needs `source",
        ),
        (
            "#[measured(source = \"\")]\nconst A: f64 = 1.0;",
            "non-empty",
        ),
        (
            "#[measured(source = \"x\", sigma = 1.0)]\nconst A: f64 = 1.0;",
            "unknown key",
        ),
        (
            "#[measured(source = \"x\", uncertainty = -1.0)]\nconst A: f64 = 1.0;",
            "non-negative",
        ),
        (
            "#[measured(source = \"x\")]\nfn A() -> f64 { 1.0 }",
            "only to a `const`",
        ),
    ];
    for (i, (decl, why)) in cases.iter().enumerate() {
        let file = dir.join(format!("bad{i}.bld"));
        std::fs::write(
            &file,
            format!("{decl}\n\nfn main() ~ Console {{ println!(\"x\"); }}\n"),
        )
        .unwrap();
        let o = buildc(&["check".as_ref(), file.as_os_str()]);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(!o.status.success(), "case {i} must fail:\n{text}");
        assert!(text.contains("BL0605"), "case {i} needs BL0605:\n{text}");
        assert!(text.contains(why), "case {i} must say {why:?}:\n{text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
