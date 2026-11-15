//! A module file is loaded once, and its functions are named after the
//! module's own path.
//!
//! `mod geom;` in two files used to merge `geom.bld` twice: the check
//! reported `Sheet` as defined twice (at a position taken from the wrong
//! file) and the C compiler stopped on the second `struct Sheet`. A module
//! declared by another module got its functions prefixed with the loader
//! chain, so the path call `geom::area` inside that module named a function
//! that did not exist.
//!
//! Checks written before the runs: each program below passes `buildc check`,
//! builds, runs, and prints the stated line. 3 * 4 = 12 and 2 * 12 = 24.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

const GEOM: &str = "pub struct Sheet {\n    w: i32,\n    h: i32,\n}\n\npub fn area(s: Sheet) -> i32 {\n    s.w * s.h\n}\n";
/// Declares `geom` itself and calls it by path.
const USE2_PATH: &str = "mod geom;\n\npub fn double_area(w: i32, h: i32) -> i32 {\n    let s = Sheet { w: w, h: h };\n    2 * geom::area(s)\n}\n";
/// Declares `geom` itself and calls it by bare name.
const USE2_BARE: &str = "mod geom;\n\npub fn double_area(w: i32, h: i32) -> i32 {\n    let s = Sheet { w: w, h: h };\n    2 * area(s)\n}\n";
const MAIN_BOTH: &str = "mod geom;\nmod use2;\n\nfn main() ~ Console {\n    let s = Sheet { w: 3, h: 4 };\n    println!(\"{} {}\", geom::area(s), use2::double_area(3, 4));\n}\n";
const MAIN_BOTH_REVERSED: &str = "mod use2;\nmod geom;\n\nfn main() ~ Console {\n    let s = Sheet { w: 3, h: 4 };\n    println!(\"{} {}\", geom::area(s), use2::double_area(3, 4));\n}\n";
const MAIN_NESTED: &str =
    "mod use2;\n\nfn main() ~ Console {\n    println!(\"{}\", use2::double_area(3, 4));\n}\n";

fn program(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-modonce-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("program folder");
    for (file, text) in files {
        std::fs::write(dir.join(file), text).expect("source file");
    }
    dir
}

fn buildc(command: &str, file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg(command)
        .arg(file)
        .output()
        .expect("buildc runs")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Check, build and run `main.bld` of `dir`; its stdout must be `wanted`.
fn checks_and_prints(dir: &Path, wanted: &str) {
    let main = dir.join("main.bld");
    let check = buildc("check", &main);
    assert!(check.status.success(), "the check failed\n{}", text(&check));

    let run = buildc("run", &main);
    let log = text(&run);
    // A machine with no C compiler cannot answer. CI has one, and there a
    // missing compiler is a failure, not a pass.
    if log.contains("no C compiler") {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI has no C compiler\n{log}"
        );
        return;
    }
    assert_eq!(run.status.code(), Some(0), "{log}");
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert_eq!(stdout.trim_end(), wanted, "{log}");
}

#[test]
fn a_module_declared_in_two_files_is_loaded_once() {
    let dir = program(
        "both",
        &[
            ("geom.bld", GEOM),
            ("use2.bld", USE2_PATH),
            ("main.bld", MAIN_BOTH),
        ],
    );
    checks_and_prints(&dir, "12 24");
}

#[test]
fn the_order_of_the_two_declarations_does_not_matter() {
    let dir = program(
        "reversed",
        &[
            ("geom.bld", GEOM),
            ("use2.bld", USE2_PATH),
            ("main.bld", MAIN_BOTH_REVERSED),
        ],
    );
    checks_and_prints(&dir, "12 24");
}

#[test]
fn a_path_call_inside_a_module_reaches_the_module_it_declares() {
    let dir = program(
        "nested-path",
        &[
            ("geom.bld", GEOM),
            ("use2.bld", USE2_PATH),
            ("main.bld", MAIN_NESTED),
        ],
    );
    checks_and_prints(&dir, "24");
}

#[test]
fn a_bare_call_inside_a_module_reaches_the_module_it_declares() {
    let dir = program(
        "nested-bare",
        &[
            ("geom.bld", GEOM),
            ("use2.bld", USE2_BARE),
            ("main.bld", MAIN_NESTED),
        ],
    );
    checks_and_prints(&dir, "24");
    let dir = program(
        "both-bare",
        &[
            ("geom.bld", GEOM),
            ("use2.bld", USE2_BARE),
            ("main.bld", MAIN_BOTH),
        ],
    );
    checks_and_prints(&dir, "12 24");
}
