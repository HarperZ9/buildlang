//! The generated C builds where the compiler has no 128-bit integer (MSVC).
//!
//! `tests/c-no-int128/main.bld` uses 64-bit wrapping arithmetic, integer
//! parsing and float infinities, the runtime paths that once needed
//! `unsigned __int128` or a constant `1.0 / 0.0` (which MSVC refuses). The
//! test compiles the same C twice with gcc or clang: as it is, and with
//! `__SIZEOF_INT128__` undefined and `__int128` defined to an undeclared name
//! from the start of the runtime to its end marker, so any use of the type in
//! the runtime outside its `BL_HAVE_INT128` guard fails to compile. The macro
//! is lifted around each system header the runtime includes, since those may
//! use the type themselves. Both builds must print the expected lines. The control
//! compiles a copy with one guard removed and must fail to build, so the
//! hidden-type build can catch an unguarded use. Without a gcc- or
//! clang-style compiler the test says so and returns.

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

fn emit_c(dir: &Path) -> String {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::copy(
        repo_root().join("tests/c-no-int128/main.bld"),
        dir.join("main.bld"),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["build", "--emit", "c"])
        .arg(dir)
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(dir.join("target/debug/main.c")).expect("main.c written")
}

/// The C with __int128 hidden over the runtime, from its banner to its end marker. The macro is
/// lifted around each system header the runtime includes, since those may use the type.
fn hide_int128(c: &str) -> String {
    const HIDE: &str = "#define __int128 bl_no_int128_here";
    let banner = c
        .find("// BuildLang Runtime Library")
        .expect("the runtime banner");
    let start = c[..banner].rfind('\n').map_or(0, |i| i + 1);
    let marker = c
        .find("End BuildLang Runtime")
        .expect("the runtime end marker");
    let end = c[..marker].rfind('\n').map_or(0, |i| i + 1);
    let mut out = String::from(&c[..start]);
    out.push_str("#undef __SIZEOF_INT128__\n");
    out.push_str(HIDE);
    out.push('\n');
    for line in c[start..end].lines() {
        if line.starts_with("#include ") {
            out.push_str("#undef __int128\n");
            out.push_str(line);
            out.push('\n');
            out.push_str(HIDE);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out.push_str("#undef __int128\n");
    out.push_str(&c[end..]);
    out
}

/// Compile `c` and run it; Err with the compiler's message when it does not build.
fn build_and_run(cc: &str, dir: &Path, name: &str, c: &str, hide: bool) -> Result<String, String> {
    let src = dir.join(format!("{name}.c"));
    std::fs::write(&src, if hide { hide_int128(c) } else { c.to_string() }).unwrap();
    let exe = dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    });
    let mut cmd = Command::new(cc);
    cmd.arg("-std=c11").arg("-O1");
    cmd.arg(&src).arg("-o").arg(&exe).arg("-lm");
    if cfg!(windows) {
        cmd.arg("-lws2_32");
    }
    let built = cmd.output().expect("the C compiler runs");
    if !built.status.success() {
        return Err(String::from_utf8_lossy(&built.stderr).to_string());
    }
    let run = Command::new(&exe).output().expect("the program runs");
    assert!(run.status.success(), "{name} exited with {:?}", run.status);
    Ok(String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n"))
}

const EXPECTED: &str = "\
12638153115695167455
-9223372036854775808
9223372036854775805
1
-9223372036854775808
9223372036854775807
124
inf
-inf
4612811918334230528
";

#[test]
fn generated_c_builds_and_agrees_without_int128() {
    let Some(cc) = posix_cc() else {
        eprintln!("skipping: no gcc/clang-style C compiler on PATH");
        return;
    };
    let dir = std::env::temp_dir().join(format!("buildlang-noi128-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let c = emit_c(&dir);
    assert!(
        c.contains("#define BL_HAVE_INT128"),
        "the runtime names its 128-bit guard"
    );
    assert!(
        !c.contains("1.0 / 0.0") && !c.contains("1.0f / 0.0f"),
        "a constant division by zero is left in the runtime"
    );

    let with = build_and_run(cc, &dir, "with", &c, false).expect("builds with __int128");
    assert_eq!(with, EXPECTED);
    let without = build_and_run(cc, &dir, "without", &c, true)
        .unwrap_or_else(|e| panic!("does not build without __int128:\n{e}"));
    assert_eq!(without, EXPECTED);

    // Control: drop the guard around the 128-bit Option slot; the hidden-type build must refuse it.
    let unguarded = c.replacen(
        "#if BL_HAVE_INT128\n        __int128 i128;",
        "#if 1\n        __int128 i128;",
        1,
    );
    assert_ne!(unguarded, c, "the control found the guard to remove");
    assert!(
        build_and_run(cc, &dir, "control", &unguarded, true).is_err(),
        "an unguarded __int128 built with the type hidden, so the check proves nothing"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
