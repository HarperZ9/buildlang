//! M4: a stateful C library written in BuildLang. `examples/lowwater-rules`
//! implements raw-native's `raw/game/lowwater/rules_c.h` (the Low Water rules:
//! records, sheet, checks, meters) behind an opaque handle, and
//! `tests/c-abi-rules/driver.c` runs the checks of raw-native's
//! `tests/test_lowwater_c.cpp` against it through the generated header.
//!
//! What this pins: an export may take and return a pointer to a struct that is
//! not `#[repr(C)]` (an opaque handle), and the header forward-declares that
//! struct with no fields; C strings cross in and out; a module-level `static
//! mut` holds the last error. Needs a gcc- or clang-style compiler and `ar`;
//! without them it says so and returns.

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

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").trim_end().to_string()
}

fn build_staticlib(project: &Path, cc: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .env("BUILDLANG_CC", cc)
        .args(["build", "--emit", "staticlib"])
        .arg(project)
        .output()
        .expect("buildc runs")
}

#[test]
fn rules_module_behind_an_opaque_handle_passes_the_host_checks() {
    let Some(cc) = posix_cc() else {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    };
    let project = std::env::temp_dir().join(format!("buildlang-cabi-rules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).expect("scratch dir");
    std::fs::copy(
        repo_root().join("examples/lowwater-rules/main.bld"),
        project.join("main.bld"),
    )
    .unwrap();

    let out = build_staticlib(&project, cc);
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "staticlib build failed:\n{log}");
    let lib_dir = project.join("target/debug");
    let header = std::fs::read_to_string(lib_dir.join("main.h")).expect("main.h written");
    assert!(
        header.contains("typedef struct LwRules LwRules;"),
        "the handle must be forward-declared:\n{header}"
    );
    assert!(
        !header.contains("typedef struct LwRules {"),
        "an opaque handle's fields must stay out of the header:\n{header}"
    );
    assert!(header.contains("LwRules* lw_rules_create(uint8_t* lwd_text, uint32_t seed);"));

    let link_flags = std::fs::read_to_string(lib_dir.join("main.link")).expect("main.link written");
    let exe = lib_dir.join(if cfg!(windows) {
        "driver.exe"
    } else {
        "driver"
    });
    let fixtures = repo_root().join("tests/c-abi-rules");
    let link = Command::new(cc)
        .arg(fixtures.join("driver.c"))
        .arg("-I")
        .arg(&lib_dir)
        .arg(lib_dir.join("libmain.a"))
        .args(link_flags.lines().filter(|l| !l.trim().is_empty()))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("C compiler runs");
    assert!(
        link.status.success(),
        "C host failed to link:\n{}",
        String::from_utf8_lossy(&link.stderr)
    );
    let run = Command::new(&exe).output().expect("driver runs");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "driver failed ({:?}):\n{stdout}",
        run.status
    );
    let expected = std::fs::read_to_string(fixtures.join("driver.expected")).unwrap();
    assert_eq!(normalize(&stdout), normalize(&expected));
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn a_struct_without_repr_c_by_value_is_still_refused() {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("check")
        .arg(repo_root().join("tests/reject/c_export_without_repr_c.bld"))
        .output()
        .expect("buildc runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "by-value non-repr(C) struct passed check:\n{text}"
    );
    assert!(text.contains("BL0901"), "{text}");
}
