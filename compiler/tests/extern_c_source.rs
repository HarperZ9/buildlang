//! `extern "C" source "file.c" { ... }`: a C file compiled into the program's own
//! translation unit, resolved against the directory of the file that declares it.
//! Accept: a tiny C source called from BuildLang returns its value under `run`;
//! the audio binding in `runtime/audio` (null device) runs 50 periods with no
//! missed deadline. Control: a source path that does not exist fails the C
//! compile instead of linking something else.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn has_cc() -> bool {
    ["cc", "gcc", "clang"].iter().any(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn run(file: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(file)
        .args(args)
        .output()
        .expect("buildc runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("buildlang-csource-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const PROGRAM: &str = "extern \"C\" source \"add.c\" {\n    fn add3(x: i32) -> i32;\n}\n\nfn main() ~ Console + Foreign {\n    println!(\"{}\", add3(39));\n}\n";

#[test]
fn a_c_source_beside_the_program_is_compiled_in() {
    if !has_cc() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let dir = scratch("accept");
    std::fs::write(
        dir.join("add.c"),
        "int32_t add3(int32_t x) { return x + 3; }\n",
    )
    .unwrap();
    std::fs::write(dir.join("main.bld"), PROGRAM).unwrap();
    let (ok, text) = run(&dir.join("main.bld"), &[]);
    assert!(ok, "{text}");
    assert!(text.lines().any(|l| l.trim() == "42"), "{text}");

    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .args(["build", "--emit", "c"])
        .arg(&dir)
        .output()
        .expect("buildc runs");
    assert!(out.status.success());
    let c = std::fs::read_to_string(dir.join("target/debug/main.c")).unwrap();
    let line = c
        .lines()
        .find(|l| l.starts_with("#include \"") && l.ends_with("add.c\""))
        .expect("the source is included");
    assert!(
        std::path::Path::new(line.trim_start_matches("#include \"").trim_end_matches('"'))
            .is_absolute(),
        "{line}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_source_fails_the_build() {
    if !has_cc() {
        return;
    }
    let dir = scratch("missing");
    std::fs::write(dir.join("main.bld"), PROGRAM).unwrap();
    let (ok, text) = run(&dir.join("main.bld"), &[]);
    assert!(!ok, "must not build without add.c:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_audio_binding_meets_every_deadline_on_the_null_device() {
    if !has_cc() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let (ok, text) = run(&repo_root().join("examples/audio/main.bld"), &[]);
    assert!(ok, "{text}");
    assert!(text.contains("periods 50 misses 0"), "{text}");
}
