//! An import that names nothing is an error (BL0611).
//!
//! `use nowhere::thing::Widget;` used to pass `buildc check` with no
//! `nowhere/thing.bld` anywhere: the name stayed unknown and later lines
//! failed with errors about something else. Each program in `tests/imports`
//! named `missing_*` or `several` has imports that resolve nowhere and must be
//! refused with BL0611, naming the path. `resolved.bld` is the control: its
//! imports all resolve and the check must pass.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

/// Run `buildc check` on a program of `tests/imports`; return whether it
/// passed and everything it printed.
fn check(name: &str) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("check")
        .arg(repo_root().join("tests/imports").join(name))
        .output()
        .expect("buildc runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn refused_with(name: &str, wanted: &[&str]) {
    let (passed, text) = check(name);
    assert!(!passed, "{name}: the check passed\n{text}");
    for w in wanted {
        assert!(text.contains(w), "{name}: no `{w}` in\n{text}");
    }
}

#[test]
fn a_module_with_no_file_is_refused() {
    refused_with(
        "missing_module.bld",
        &[
            "[BL0611]",
            "unresolved import `nowhere::thing::Widget`",
            "nowhere/thing.bld",
        ],
    );
}

#[test]
fn a_standard_module_that_does_not_exist_is_refused() {
    refused_with(
        "missing_std_module.bld",
        &[
            "[BL0611]",
            "unresolved import `core::simd::f32x4`",
            "no module `simd`",
        ],
    );
}

#[test]
fn a_name_the_module_file_does_not_declare_is_refused() {
    refused_with(
        "missing_item.bld",
        &[
            "[BL0611]",
            "unresolved import `shapes::Hexagon`",
            "no top-level item `Hexagon`",
        ],
    );
}

#[test]
fn a_mod_declaration_with_no_file_is_refused() {
    refused_with(
        "missing_mod_file.bld",
        &["[BL0611]", "no file for `mod absent;`", "absent.bld"],
    );
}

#[test]
fn every_unresolved_import_is_reported_in_one_run() {
    let (passed, text) = check("several.bld");
    assert!(!passed, "several.bld: the check passed\n{text}");
    assert_eq!(
        text.matches("[BL0611]").count(),
        3,
        "expected three BL0611 reports in\n{text}"
    );
    for path in [
        "`nowhere::thing::Widget`",
        "`std::memory::Arena`",
        "`shapes::Hexagon`",
    ] {
        assert!(text.contains(path), "no {path} in\n{text}");
    }
    assert!(
        !text.contains("`shapes::Square`"),
        "shapes::Square resolves and must not be reported\n{text}"
    );
}

#[test]
fn imports_that_resolve_pass() {
    let (passed, text) = check("resolved.bld");
    assert!(passed, "resolved.bld was refused\n{text}");
    assert!(!text.contains("BL0611"), "{text}");
}
