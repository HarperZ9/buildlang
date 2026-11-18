//! `*const T` keeps its `const` in the generated C header. A C host that holds
//! a pointer to read-only data passes it to a `*const` parameter without a
//! cast, and a C++ host can call the export at all (C++ refuses the implicit
//! `const T*` to `T*` conversion).
//!
//! `tests/c-abi-const/main.bld` exports a `*const u8` parameter, a `*mut u8`
//! parameter, a `*const u8` return and a `*const *const u8` parameter.
//! Accept: the header states `const` at each `*const` level and nowhere else,
//! and `driver.c`, which passes only const data to the `*const` parameters,
//! builds with `-Werror` and prints `driver.expected`. Control: the same
//! driver against a copy of the header with every `const` removed must fail to
//! build with `-Werror`, so the accept build depends on the qualifiers. Without
//! a gcc- or clang-style compiler the test says so and returns.

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

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Build `driver.c` against the header in `include` and the library in `lib`.
fn link(cc: &str, include: &Path, lib: &Path, exe: &Path) -> Output {
    let flags = std::fs::read_to_string(lib.join("main.link")).expect("main.link written");
    Command::new(cc)
        .args(["-Wall", "-Werror"])
        .arg(repo_root().join("tests/c-abi-const/driver.c"))
        .arg("-I")
        .arg(include)
        .arg(lib.join("libmain.a"))
        .args(flags.lines().filter(|l| !l.trim().is_empty()))
        .arg("-o")
        .arg(exe)
        .output()
        .expect("C compiler runs")
}

#[test]
fn const_pointers_keep_const_in_the_header() {
    let Some(cc) = posix_cc() else {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    };
    let project = std::env::temp_dir().join(format!("buildlang-cconst-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::copy(
        repo_root().join("tests/c-abi-const/main.bld"),
        project.join("main.bld"),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .env("BUILDLANG_CC", cc)
        .args(["build", "--emit", "staticlib"])
        .arg(&project)
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "staticlib build failed:\n{}",
        text(&out)
    );
    let lib = project.join("target/debug");
    let header = std::fs::read_to_string(lib.join("main.h")).expect("main.h written");
    for line in [
        "uint32_t cc_sum(const uint8_t* p, uintptr_t n);",
        "void cc_fill(uint8_t* p, uintptr_t n, uint8_t v);",
        "const uint8_t* cc_same(const uint8_t* p);",
        "uintptr_t cc_rows(const uint8_t* const* rows, uintptr_t n);",
    ] {
        assert!(header.contains(line), "header lacks `{line}`:\n{header}");
    }

    let exe = lib.join(if cfg!(windows) {
        "driver.exe"
    } else {
        "driver"
    });
    let built = link(cc, &lib, &lib, &exe);
    assert!(
        built.status.success(),
        "driver failed to build:\n{}",
        text(&built)
    );
    let run = Command::new(&exe).output().expect("driver runs");
    assert!(run.status.success(), "driver exited with {:?}", run.status);
    let expected =
        std::fs::read_to_string(repo_root().join("tests/c-abi-const/driver.expected")).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n"),
        expected.replace("\r\n", "\n")
    );

    // Control: the header without its qualifiers; the -Werror build must fail.
    let bare = project.join("bare");
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::write(bare.join("main.h"), header.replace("const ", "")).unwrap();
    let control = link(cc, &bare, &lib, &bare.join("driver.exe"));
    assert!(
        !control.status.success(),
        "the driver built against a header without const, so the check proves nothing"
    );
    let _ = std::fs::remove_dir_all(&project);
}
