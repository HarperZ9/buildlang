//! A `use` of a module file brings the real item.
//!
//! `use shapes::Square;` with `shapes.bld` beside the program used to bind
//! `Square` as a value of unknown type. The check accepted any use of it, and
//! the C build then stopped because no struct and no function stood behind
//! the imported names. The module file is now merged into the program, once,
//! and the import names the item itself.
//!
//! Checks written before the runs: each program passes `buildc check`,
//! builds, runs and prints the stated line, or is refused where stated.
//! 3 * 3 = 9, 2 * 9 = 18, 41 + 1 = 42.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

const SHAPES: &str = "pub struct Square {\n    pub side: i64,\n}\n\npub fn area(s: Square) -> i64 {\n    s.side * s.side\n}\n";

fn program(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-useimport-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().expect("a parent folder")).expect("program folder");
        std::fs::write(path, text).expect("source file");
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
fn an_imported_struct_and_function_build_and_run() {
    let dir = program(
        "basic",
        &[
            ("shapes.bld", SHAPES),
            (
                "main.bld",
                "use shapes::{Square, area};\n\nfn main() ~ Console {\n    let s = Square { side: 3 };\n    println!(\"{}\", area(s));\n}\n",
            ),
        ],
    );
    checks_and_prints(&dir, "9");
}

#[test]
fn an_imported_function_keeps_its_signature() {
    // `area` takes a `Square`. An integer argument passed the check while the
    // import was a value of unknown type.
    let dir = program(
        "signature",
        &[
            ("shapes.bld", SHAPES),
            (
                "main.bld",
                "use shapes::area;\n\nfn main() ~ Console {\n    let n: i64 = area(5);\n    println!(\"{}\", n);\n}\n",
            ),
        ],
    );
    let check = buildc("check", &dir.join("main.bld"));
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert!(log.contains("Square"), "no `Square` in\n{log}");
}

#[test]
fn a_method_calls_an_imported_function_by_its_bare_name() {
    let dir = program(
        "method",
        &[
            ("shapes.bld", SHAPES),
            (
                "main.bld",
                "use shapes::{Square, area};\n\nstruct Tile {\n    w: i64,\n}\n\nimpl Tile {\n    fn cover(self) -> i64 {\n        area(Square { side: self.w })\n    }\n}\n\nfn main() ~ Console {\n    let t = Tile { w: 3 };\n    println!(\"{}\", t.cover());\n}\n",
            ),
        ],
    );
    checks_and_prints(&dir, "9");
}

#[test]
fn a_module_in_a_folder_is_imported_by_its_path() {
    let dir = program(
        "folder",
        &[
            ("geo/shapes.bld", SHAPES),
            (
                "main.bld",
                "use geo::shapes::{Square, area};\n\nfn main() ~ Console {\n    let s = Square { side: 3 };\n    println!(\"{}\", area(s));\n}\n",
            ),
        ],
    );
    checks_and_prints(&dir, "9");
}

#[test]
fn a_module_imported_by_two_files_is_merged_once() {
    let dir = program(
        "twice",
        &[
            ("shapes.bld", SHAPES),
            (
                "helper.bld",
                "use shapes::{Square, area};\n\npub fn double(side: i64) -> i64 {\n    2 * area(Square { side: side })\n}\n",
            ),
            (
                "main.bld",
                "use shapes::{Square, area};\nuse helper::double;\n\nfn main() ~ Console {\n    let s = Square { side: 3 };\n    println!(\"{} {}\", area(s), double(3));\n}\n",
            ),
        ],
    );
    checks_and_prints(&dir, "9 18");
}

#[test]
fn two_files_that_import_each_other_do_not_recurse() {
    let dir = program(
        "mutual",
        &[
            (
                "a.bld",
                "use b::fb;\n\npub fn fa() -> i64 {\n    fb() + 1\n}\n",
            ),
            ("b.bld", "use a::fa;\n\npub fn fb() -> i64 {\n    41\n}\n"),
            (
                "main.bld",
                "use a::fa;\n\nfn main() ~ Console {\n    println!(\"{}\", fa());\n}\n",
            ),
        ],
    );
    checks_and_prints(&dir, "42");
}

#[test]
fn a_standard_library_function_is_imported_by_name() {
    // `stdlib/math.bld` declares `lerp_f64`; halfway from 0 to 10 is 5.
    let dir = program(
        "std",
        &[(
            "main.bld",
            "use std::math::lerp_f64;\n\nfn main() ~ Console {\n    if lerp_f64(0.0, 10.0, 0.5) == 5.0 {\n        println!(\"halfway\");\n    }\n}\n",
        )],
    );
    checks_and_prints(&dir, "halfway");
}

#[test]
fn a_module_file_with_a_parse_error_is_not_imported() {
    let dir = program(
        "broken",
        &[
            ("broken.bld", "pub fn f( -> i64 {\n    1\n}\n"),
            (
                "main.bld",
                "use broken::f;\n\nfn main() ~ Console {\n    println!(\"{}\", f());\n}\n",
            ),
        ],
    );
    let check = buildc("check", &dir.join("main.bld"));
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert!(log.contains("BL0611"), "no BL0611 in\n{log}");
    assert!(
        log.contains("has a lex or parse error"),
        "the reason is missing\n{log}"
    );
}
