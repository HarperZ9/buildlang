//! C ABI slices. A C host passes a pointer and a length; inside the export,
//! `unsafe { slice::from_raw_parts(p, n) }` gives a `&[T]` whose indexing is
//! checked against `n`. `tests/c-abi-slices/main.bld` exports four functions,
//! one of them over a grid whose side is known only at run time, and
//! `tests/c-abi-slices/driver.c` calls them with lengths it chooses.
//!
//! Accept: the driver's output matches `driver.expected`, and an index inside
//! the slice reads. Control: an index equal to the length the host passed
//! traps (exit 101, "index out of bounds") instead of reading past the end.
//! Refusals: `from_raw_parts` outside `unsafe` (BL0382), a `&[T]` in an
//! export signature (BL0901, with the pointer-and-length fix), and `&arr`
//! where the array's element type differs from the slice's.

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

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("buildlang-slices-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn buildc(args: &[&str], path: &Path, cc: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", repo_root().join("stdlib"));
    if let Some(cc) = cc {
        cmd.env("BUILDLANG_CC", cc);
    }
    cmd.args(args).arg(path).output().expect("buildc runs")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Build the fixture library and link the driver; returns the driver path.
fn linked_driver(cc: &str, project: &Path) -> PathBuf {
    let fixtures = repo_root().join("tests/c-abi-slices");
    std::fs::copy(fixtures.join("main.bld"), project.join("main.bld")).unwrap();
    let out = buildc(&["build", "--emit", "staticlib"], project, Some(cc));
    assert!(
        out.status.success(),
        "staticlib build failed:\n{}",
        text(&out)
    );
    let lib_dir = project.join("target/debug");
    let flags = std::fs::read_to_string(lib_dir.join("main.link")).expect("main.link written");
    let exe = lib_dir.join(if cfg!(windows) {
        "driver.exe"
    } else {
        "driver"
    });
    let link = Command::new(cc)
        .arg(fixtures.join("driver.c"))
        .arg("-I")
        .arg(&lib_dir)
        .arg(lib_dir.join("libmain.a"))
        .args(flags.lines().filter(|l| !l.trim().is_empty()))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("C compiler runs");
    assert!(
        link.status.success(),
        "C host failed to link:\n{}",
        text(&link)
    );
    exe
}

#[test]
fn a_pointer_and_length_from_c_read_as_a_checked_slice() {
    let Some(cc) = posix_cc() else {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    };
    let project = scratch("accept");
    let exe = linked_driver(cc, &project);
    let expected =
        std::fs::read_to_string(repo_root().join("tests/c-abi-slices/driver.expected")).unwrap();

    let run = Command::new(&exe).output().expect("driver runs");
    assert!(run.status.success(), "driver exited with {:?}", run.status);
    assert_eq!(
        normalize(&String::from_utf8_lossy(&run.stdout)),
        normalize(&expected)
    );

    // the last index inside the slice reads
    let inside = Command::new(&exe).arg("2").output().expect("driver runs");
    assert!(
        inside.status.success(),
        "index 2 of 3 must read: {}",
        text(&inside)
    );
    assert!(String::from_utf8_lossy(&inside.stdout).contains("read 3.2500"));

    // control: the index equal to the length the host passed traps
    let past = Command::new(&exe).arg("3").output().expect("driver runs");
    assert_eq!(
        past.status.code(),
        Some(101),
        "index 3 of 3 must trap: {}",
        text(&past)
    );
    assert!(
        text(&past).contains("index out of bounds: the len is 3 but the index is 3"),
        "trap message: {}",
        text(&past)
    );
    assert!(!String::from_utf8_lossy(&past.stdout).contains("read "));
    let _ = std::fs::remove_dir_all(&project);
}

fn check_source(name: &str, source: &str) -> Output {
    let dir = scratch(name);
    let file = dir.join("main.bld");
    std::fs::write(&file, source).unwrap();
    buildc(&["check"], &file, None)
}

#[test]
fn from_raw_parts_outside_unsafe_is_refused() {
    let out = check_source(
        "unsafe",
        "extern \"C\" fn n_of(p: *const f32, n: usize) -> usize {\n    let xs = slice::from_raw_parts(p, n);\n    xs.len()\n}\n",
    );
    let log = text(&out);
    assert!(!out.status.success(), "must not check:\n{log}");
    assert_eq!(log.matches("BL0382").count(), 1, "reported once:\n{log}");

    // the same body inside `unsafe` checks
    let ok = check_source(
        "unsafe-ok",
        "extern \"C\" fn n_of(p: *const f32, n: usize) -> usize {\n    let xs = unsafe { slice::from_raw_parts(p, n) };\n    xs.len()\n}\n",
    );
    assert!(ok.status.success(), "{}", text(&ok));
}

#[test]
fn a_slice_in_an_export_signature_names_the_pointer_and_length_fix() {
    let out = check_source(
        "sig",
        "extern \"C\" fn total(xs: &[f32]) -> f32 {\n    xs[0]\n}\n",
    );
    let log = text(&out);
    assert!(!out.status.success(), "must not check:\n{log}");
    assert!(log.contains("BL0901"), "{log}");
    assert!(log.contains("slice::from_raw_parts(p, n)"), "{log}");
}

#[test]
fn an_array_of_another_element_type_is_not_read_as_the_slice() {
    let dir = scratch("width");
    let file = dir.join("main.bld");
    std::fs::write(
        &file,
        "fn first(xs: &[i64]) -> i64 {\n    xs[0]\n}\n\nfn main() ~ Console {\n    let c = [4, 9];\n    let v = first(&c);\n    println!(\"{}\", v);\n}\n",
    )
    .unwrap();
    let out = buildc(&["run"], &file, None);
    let log = text(&out);
    assert!(
        !out.status.success(),
        "i32 storage must not be read as i64:\n{log}"
    );
    assert!(
        log.contains("this array holds `i32` but the parameter is a slice of `i64`"),
        "{log}"
    );
}
