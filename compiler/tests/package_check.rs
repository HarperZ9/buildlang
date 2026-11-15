//! `buildc check <dir>` and `buildc lint <dir>`: a package checked as one unit.
//!
//! A package is `Build.toml` plus `src/*.bld`. The compiler joins the files
//! into one text and checks that. Checks written before the runs:
//! 1. A package whose `main.bld` calls a function of another file passes.
//! 2. An error is reported at the file and line it is written at, never at a
//!    line of the joined text. The fixtures put a three-line file ahead of the
//!    faulty one, so the joined line (14) differs from the true line (7).
//! 3. The JSON report carries the original file, line and byte offsets, for a
//!    file with CRLF line ends too.
//! 4. Errors in two files come out of one run.
//! 5. Every package in the registry checks as a directory.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

const MANIFEST: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n";
const AAA: &str = "fn helper_one() -> i32 {\n    1\n}\n";
const LIB_OK: &str = "// a library file\nfn area(w: i32, h: i32) -> i32 {\n    w * h\n}\n";
/// The mismatch is on line 7, column 5.
const LIB_BROKEN: &str = "// a library file\nfn area(w: i32, h: i32) -> i32 {\n    w * h\n}\n\nfn broken() -> i32 {\n    let n: i32 = \"text\";\n    n\n}\n";
const MAIN: &str = "fn main() ~ Console {\n    println!(\"{}\", area(3, 4));\n}\n";
/// The parse error is on line 5, column 9.
const SHAPE_BROKEN: &str =
    "fn perimeter(w: i32, h: i32) -> i32 {\n    2 * (w + h)\n}\n\nfn bad( -> i32 {\n    1\n}\n";

/// A package in a fresh folder: `Build.toml` plus the named files of `src/`.
fn package(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-pkgcheck-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("package folder");
    std::fs::write(dir.join("Build.toml"), MANIFEST).expect("manifest");
    for (file, text) in files {
        std::fs::write(dir.join("src").join(file), text).expect("source file");
    }
    dir
}

fn buildc(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .env_remove("BUILDLANG_REGISTRY")
        .args(args)
        .arg(dir)
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

/// The folder as diagnostics print it: `/` separators on every platform.
fn shown(dir: &Path) -> String {
    dir.to_string_lossy().replace('\\', "/")
}

#[test]
fn a_package_of_several_files_passes() {
    let dir = package(
        "ok",
        &[("aaa.bld", AAA), ("lib.bld", LIB_OK), ("main.bld", MAIN)],
    );
    let out = buildc(&["check"], &dir);
    let log = text(&out);
    assert!(out.status.success(), "the check failed\n{log}");
    assert!(log.contains("Type checking... OK"), "{log}");
}

#[test]
fn a_type_error_is_reported_at_its_own_file_and_line() {
    let dir = package(
        "types",
        &[
            ("aaa.bld", AAA),
            ("lib.bld", LIB_BROKEN),
            ("main.bld", MAIN),
        ],
    );
    let out = buildc(&["check"], &dir);
    let log = text(&out);
    assert!(!out.status.success(), "the check passed\n{log}");
    let wanted = format!(
        "{}/src/lib.bld:7:5: [BL0301] type mismatch: expected `i32`, found `str`",
        shown(&dir)
    );
    assert!(log.contains(&wanted), "no `{wanted}` in\n{log}");
    assert!(!log.contains(":14:"), "a joined-text line leaked\n{log}");
}

#[test]
fn a_parse_error_is_reported_at_its_own_file_and_line() {
    let dir = package(
        "parse",
        &[
            ("aaa.bld", AAA),
            ("shape.bld", SHAPE_BROKEN),
            ("main.bld", MAIN),
        ],
    );
    let out = buildc(&["check"], &dir);
    let log = text(&out);
    assert!(!out.status.success(), "the check passed\n{log}");
    let wanted = format!(
        "error[{}/src/shape.bld:5:9]: [BL0202] expected pattern, found `->`",
        shown(&dir)
    );
    assert!(log.contains(&wanted), "no `{wanted}` in\n{log}");
    assert!(log.contains("  5 | fn bad( -> i32 {"), "{log}");
}

#[test]
fn errors_in_two_files_come_out_of_one_run() {
    let second = "fn also_broken() -> bool {\n    let flag: bool = 7;\n    flag\n}\n";
    let dir = package(
        "two",
        &[
            ("lib.bld", LIB_BROKEN),
            ("zeta.bld", second),
            ("main.bld", MAIN),
        ],
    );
    let out = buildc(&["check"], &dir);
    let log = text(&out);
    assert!(!out.status.success(), "the check passed\n{log}");
    for wanted in ["/src/lib.bld:7:5: [BL0301]", "/src/zeta.bld:2:5: [BL0301]"] {
        assert!(log.contains(wanted), "no `{wanted}` in\n{log}");
    }
}

#[test]
fn the_json_report_carries_the_original_file_line_and_bytes() {
    let crlf = LIB_BROKEN.replace('\n', "\r\n");
    let dir = package(
        "json",
        &[
            ("aaa.bld", AAA),
            ("lib.bld", crlf.as_str()),
            ("main.bld", MAIN),
        ],
    );
    let receipt = dir.join("receipt.json");
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .env_remove("BUILDLANG_REGISTRY")
        .args(["check", "--error-format=json", "--receipt"])
        .arg(&receipt)
        .arg(&dir)
        .output()
        .expect("buildc runs");
    assert!(!out.status.success(), "the check passed\n{}", text(&out));
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}\n{}", text(&out)));
    assert_eq!(report["error_count"], 1, "{report}");
    let d = &report["diagnostics"][0];
    assert_eq!(d["code"], "BL0301", "{report}");
    assert_eq!(
        d["file"].as_str(),
        Some(format!("{}/src/lib.bld", shown(&dir)).as_str()),
        "{report}"
    );
    // `let n: i32 = "text";` starts at this byte of the CRLF file and is 20
    // bytes long.
    let start = crlf.find("let n").expect("the statement is in the file") as u64;
    let span = &d["span"];
    assert_eq!(span["line"], 7, "{report}");
    assert_eq!(span["column"], 5, "{report}");
    assert_eq!(span["end_line"], 7, "{report}");
    assert_eq!(span["end_column"], 25, "{report}");
    assert_eq!(span["byte_start"], start, "{report}");
    assert_eq!(span["byte_end"], start + 20, "{report}");

    let receipt: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt).expect("receipt written"))
            .expect("receipt json");
    let rd = &receipt["diagnostics"][0];
    assert_eq!(rd["line"], 7, "{receipt}");
    assert_eq!(
        rd["file"].as_str(),
        Some(format!("{}/src/lib.bld", shown(&dir)).as_str()),
        "{receipt}"
    );
}

#[test]
fn a_library_package_with_no_main_is_checked() {
    let dir = package("libonly", &[("lib.bld", LIB_OK)]);
    let out = buildc(&["check"], &dir);
    assert!(out.status.success(), "the check failed\n{}", text(&out));

    let dir = package("libonly-bad", &[("lib.bld", LIB_BROKEN)]);
    let out = buildc(&["check"], &dir);
    let log = text(&out);
    assert!(!out.status.success(), "the check passed\n{log}");
    assert!(log.contains("/src/lib.bld:7:5: [BL0301]"), "{log}");
}

#[test]
fn a_directory_with_no_manifest_is_refused() {
    let dir = package("nomanifest", &[("main.bld", MAIN)]);
    std::fs::remove_file(dir.join("Build.toml")).expect("manifest removed");
    for command in ["check", "lint"] {
        let out = buildc(&[command], &dir);
        let log = text(&out);
        assert!(!out.status.success(), "{command} passed\n{log}");
        assert!(log.contains("is a directory with no Build.toml"), "{log}");
    }
}

#[test]
fn include_inside_a_package_is_refused_with_its_place() {
    let with_include = format!("{LIB_OK}include!(\"other.bld\");\n");
    let dir = package(
        "include",
        &[("lib.bld", with_include.as_str()), ("main.bld", MAIN)],
    );
    let out = buildc(&["check"], &dir);
    let log = text(&out);
    assert!(!out.status.success(), "the check passed\n{log}");
    let wanted = format!("{}/src/lib.bld:5", shown(&dir));
    assert!(
        log.contains("`include!` is not available inside a package"),
        "{log}"
    );
    assert!(log.contains(&wanted), "no `{wanted}` in\n{log}");
}

#[test]
fn lint_reports_each_finding_at_its_own_file_and_line() {
    let trailing = "fn padded() -> i32 {\n    2   \n}\n";
    let dir = package(
        "lint",
        &[
            ("aaa.bld", AAA),
            ("pad.bld", trailing),
            ("lib.bld", LIB_BROKEN),
            ("main.bld", MAIN),
        ],
    );
    let out = buildc(&["lint"], &dir);
    let log = text(&out);
    assert_eq!(out.status.code(), Some(1), "{log}");
    let base = shown(&dir);
    let error = format!("({base}/src/lib.bld:7:5)");
    let warning = format!("trailing whitespace ({base}/src/pad.bld:2)");
    assert!(log.contains(&error), "no `{error}` in\n{log}");
    assert!(log.contains(&warning), "no `{warning}` in\n{log}");
    assert!(log.contains("1 error(s), 1 warning(s)"), "{log}");

    let clean = package("lint-clean", &[("lib.bld", LIB_OK), ("main.bld", MAIN)]);
    let out = buildc(&["lint"], &clean);
    let log = text(&out);
    assert_eq!(out.status.code(), Some(0), "{log}");
    assert!(log.contains("No issues found"), "{log}");
}

/// Published versions the package check refuses, each with the diagnostic it
/// must show. Found by the first run of this check, 2026-11-15.
/// std-collections 0.1.0: the demo `fn main()` at the end of `src/lib.bld`
/// prints and declares no `Console` effect.
const KNOWN_FAULTS: &[(&str, &str, &str)] = &[(
    "std-collections",
    "0.1.0",
    "src/lib.bld:106:1: [BL0402] function `main` performs effect `Console` but does not declare it",
)];

/// Every published version in the registry, checked in place as a directory.
/// Packages with dependencies go through their `Build.lock` and the registry
/// found above them.
#[test]
fn every_registry_package_checks_as_a_directory() {
    let registry = repo_root().join("registry");
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(registry.join("index.json")).expect("index"))
            .expect("index json");
    let mut checked = 0;
    let mut failures = Vec::new();
    for (name, entry) in index["packages"].as_object().expect("packages") {
        for (version, v) in entry["versions"].as_object().expect("versions") {
            let dir = registry.join(v["path"].as_str().expect("path"));
            let out = buildc(&["check"], &dir);
            let log = text(&out);
            checked += 1;
            let known = KNOWN_FAULTS
                .iter()
                .find(|(n, ver, _)| n == name && ver == version);
            match known {
                // A published version cannot change, so its fault stays until
                // a new version replaces it. The line must stay true: the
                // package has to fail, at the stated place with the stated text.
                Some((_, _, fault)) => {
                    if out.status.success() || !log.contains(fault) {
                        failures.push(format!(
                            "{name} {version} is listed with the fault `{fault}` and that is no longer what happens\n{log}"
                        ));
                    }
                }
                None => {
                    if !out.status.success() {
                        failures.push(format!("{name} {version}\n{log}"));
                    }
                }
            }
        }
    }
    assert!(checked >= 31, "only {checked} registry packages were found");
    assert!(
        failures.is_empty(),
        "{} of {checked} registry packages fail `buildc check <dir>`:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
