//! `buildc build --emit staticlib` (M4 C ABI export).
//!
//! Compiles the generated C for a project with no `fn main` into an object
//! file and archives it: `libmain.a` with gcc or clang and `ar`, or `main.lib`
//! with MSVC `cl` and `lib`. The generated header `main.h` sits beside it.
//! `BUILDLANG_CC` picks the C compiler; otherwise the build's own choice is
//! used. The host must also link the libraries the BuildLang runtime uses;
//! the build lists them, one flag a line, in `main.link`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn is_msvc(compiler: &str) -> bool {
    let base = Path::new(compiler)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(compiler);
    base.eq_ignore_ascii_case("cl")
}

fn run(cmd: &mut Command, what: &str) -> Result<(), i32> {
    let out = cmd.output().map_err(|e| {
        eprintln!("error: could not start {what}: {e}");
        1
    })?;
    if out.status.success() {
        return Ok(());
    }
    eprintln!("error: {what} failed ({})", out.status);
    eprintln!("{}", String::from_utf8_lossy(&out.stdout));
    eprintln!("{}", String::from_utf8_lossy(&out.stderr));
    Err(1)
}

/// The archiver that matches a gcc- or clang-style compiler.
fn find_archiver(compiler: &str) -> Option<String> {
    let mut candidates = vec![
        "ar".to_string(),
        "gcc-ar".to_string(),
        "llvm-ar".to_string(),
    ];
    if compiler.contains("clang") {
        candidates.rotate_left(2);
    }
    candidates.into_iter().find(|ar| {
        Command::new(ar)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Build the static library from `c_file` into `out_dir`; returns its path and
/// whether the MSVC toolchain built it (which decides the link-flag format).
pub(crate) fn build_static_library(
    c_file: &Path,
    out_dir: &Path,
    release: bool,
    default_compiler: Option<String>,
) -> Result<(PathBuf, bool), i32> {
    let compiler = std::env::var("BUILDLANG_CC")
        .ok()
        .filter(|c| !c.is_empty())
        .or(default_compiler)
        .ok_or_else(|| {
            eprintln!("error: no C compiler found; set BUILDLANG_CC or install gcc, clang or MSVC");
            1
        })?;

    if is_msvc(&compiler) {
        let obj = out_dir.join("main.obj");
        let lib = out_dir.join("main.lib");
        run(
            Command::new(&compiler)
                .args(["/nologo", "/c", "/std:c11", "/W0", "/fp:precise"])
                .arg(if release { "/O2" } else { "/Zi" })
                .arg(c_file)
                .arg(format!("/Fo{}", obj.display())),
            "cl /c",
        )?;
        run(
            Command::new("lib")
                .arg("/nologo")
                .arg(format!("/OUT:{}", lib.display()))
                .arg(&obj),
            "lib",
        )?;
        let _ = std::fs::remove_file(&obj);
        return Ok((lib, true));
    }

    let obj = out_dir.join("main.o");
    let lib = out_dir.join("libmain.a");
    run(
        Command::new(&compiler)
            .args(["-c", "-std=c99"])
            .arg(if release { "-O2" } else { "-g" })
            .arg(c_file)
            .arg("-o")
            .arg(&obj),
        &format!("{compiler} -c"),
    )?;
    let ar = find_archiver(&compiler).ok_or_else(|| {
        eprintln!("error: no archiver found (tried ar, gcc-ar, llvm-ar)");
        1
    })?;
    let _ = std::fs::remove_file(&lib);
    run(
        Command::new(&ar).arg("rcs").arg(&lib).arg(&obj),
        &format!("{ar} rcs"),
    )?;
    let _ = std::fs::remove_file(&obj);
    Ok((lib, false))
}
