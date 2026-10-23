//! M4: C ABI export. `buildc build --emit staticlib` turns a BuildLang project
//! into a static library and a C header, and a C host links against both.
//!
//! `tests/c-abi/main.bld` exports three `extern "C"` functions over two
//! `#[repr(C)]` structs (one nested). `tests/c-abi/driver.c` includes the
//! generated `main.h`, links the generated library, and prints values that
//! depend on the struct layout agreeing on both sides (`sizeof`, a nested
//! field written through a pointer, a bool). The test needs a gcc- or
//! clang-style compiler and `ar`; without them it says so and returns. The
//! host links the flags the build lists in `main.link`.

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

fn scratch_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("buildlang-cabi-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
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
fn staticlib_and_header_link_into_a_c_host() {
    let Some(cc) = posix_cc() else {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    };
    let fixtures = repo_root().join("tests/c-abi");
    let project = scratch_project("link");
    std::fs::copy(fixtures.join("main.bld"), project.join("main.bld")).unwrap();

    let out = build_staticlib(&project, cc);
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "staticlib build failed:\n{log}");
    let lib_dir = project.join("target/debug");
    let lib = lib_dir.join("libmain.a");
    let header = lib_dir.join("main.h");
    assert!(lib.exists(), "no {} in\n{log}", lib.display());
    let header_text = std::fs::read_to_string(&header).expect("main.h written");
    let fighter_def = header_text
        .find("typedef struct Fighter {")
        .expect("header defines Fighter");
    let vec2_def = header_text
        .find("typedef struct Vec2 {")
        .expect("header defines Vec2");
    assert!(
        vec2_def < fighter_def,
        "Vec2 must precede Fighter, which embeds it"
    );
    assert!(header_text.contains("Fighter lw_damage(Fighter f, double amount);"));

    let link_flags = std::fs::read_to_string(lib_dir.join("main.link")).expect("main.link written");
    let exe = lib_dir.join(if cfg!(windows) {
        "driver.exe"
    } else {
        "driver"
    });
    let link = Command::new(cc)
        .arg(fixtures.join("driver.c"))
        .arg("-I")
        .arg(&lib_dir)
        .arg(&lib)
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
    assert!(run.status.success(), "driver exited with {:?}", run.status);
    let expected = std::fs::read_to_string(fixtures.join("driver.expected")).unwrap();
    assert_eq!(
        normalize(&String::from_utf8_lossy(&run.stdout)),
        normalize(&expected)
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn staticlib_refuses_a_program_with_main() {
    let Some(cc) = posix_cc() else {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    };
    let project = scratch_project("main");
    std::fs::write(
        project.join("main.bld"),
        "extern \"C\" fn one() -> i32 {\n    1\n}\n\nfn main() ~ Console {\n    println!(\"x\");\n}\n",
    )
    .unwrap();
    let out = build_staticlib(&project, cc);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a library with fn main must not build"
    );
    assert!(err.contains("fn main"), "error should name fn main: {err}");
    let _ = std::fs::remove_dir_all(&project);
}
