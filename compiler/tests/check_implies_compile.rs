//! M1 exit criterion: a program that passes `buildc check` compiles.
//!
//! For every `.bld` program in `tests/programs`, `examples`, `semantic-corpus` and
//! `tests/agent-probe` (programs an AI model wrote in a 2026-10-09 probe, including
//! ones that must be rejected), run `buildc check`. When it passes, emit C and
//! compile it with the host C compiler (compile only, implicit declarations are
//! errors). The test fails if any program passes the check and then fails to
//! compile: that is a program the checker should have rejected.
//!
//! Skipped, with the reason printed: programs whose entry points are GPU or shader
//! stages (C is not their target) and programs that include an external C header.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("bld") {
            out.push(path);
        }
    }
}

fn skip_reason(src: &str) -> Option<&'static str> {
    let stage = src.contains("#[fragment]") || src.contains("#[vertex]");
    let kernel_only =
        (src.contains("#[compute]") || src.contains("#[kernel]")) && !src.contains("fn main");
    if stage || kernel_only {
        return Some("GPU or shader entry points; C is not their target");
    }
    for line in src.lines() {
        let line = line.trim_start();
        if line.starts_with("//") {
            continue;
        }
        if let Some(pos) = line.find("header \"") {
            let header = &line[pos + 8..];
            let std = [
                "<stdio.h>",
                "<stdlib.h>",
                "<math.h>",
                "<string.h>",
                "stdio.h",
                "math.h",
            ];
            if !std.iter().any(|h| header.starts_with(h)) {
                return Some("includes an external C header");
            }
        }
    }
    None
}

/// The host C compiler and its compile-only arguments, or None.
fn c_compiler() -> Option<(String, Vec<String>)> {
    let gnu_like = |cc: &str| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if let Ok(cc) = std::env::var("CC") {
        if gnu_like(&cc) {
            return Some((
                cc,
                vec!["-c".into(), "-Werror=implicit-function-declaration".into()],
            ));
        }
    }
    for cc in ["gcc", "clang", "cc"] {
        if gnu_like(cc) {
            return Some((
                cc.to_string(),
                vec!["-c".into(), "-Werror=implicit-function-declaration".into()],
            ));
        }
    }
    None
}

#[test]
fn every_program_that_passes_check_compiles() {
    let Some((cc, cc_args)) = c_compiler() else {
        eprintln!("skipping: no gcc, clang or cc on PATH");
        return;
    };
    let root = repo_root();
    let mut files = Vec::new();
    for dir in [
        "tests/programs",
        "examples",
        "semantic-corpus",
        "tests/agent-probe",
    ] {
        collect(&root.join(dir), &mut files);
    }
    assert!(
        files.len() > 200,
        "expected the full corpus, found {}",
        files.len()
    );

    let buildc = PathBuf::from(env!("CARGO_BIN_EXE_buildc"));
    let stdlib = root.join("stdlib");
    let work = std::env::temp_dir().join(format!(
        "buildlang_check_implies_compile_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&work).expect("create work dir");

    let queue = Arc::new(Mutex::new(files.clone()));
    let failures = Arc::new(Mutex::new(Vec::<String>::new()));
    let counts = Arc::new(Mutex::new((0usize, 0usize, 0usize))); // checked, rejected, skipped
    let threads: Vec<_> = (0..8)
        .map(|t| {
            let (queue, failures, counts) = (queue.clone(), failures.clone(), counts.clone());
            let (buildc, stdlib, work, cc, cc_args) = (
                buildc.clone(),
                stdlib.clone(),
                work.clone(),
                cc.clone(),
                cc_args.clone(),
            );
            std::thread::spawn(move || loop {
                let Some(file) = queue.lock().unwrap().pop() else {
                    break;
                };
                let src = std::fs::read_to_string(&file).unwrap_or_default();
                if let Some(reason) = skip_reason(&src) {
                    eprintln!("skip {}: {reason}", file.display());
                    counts.lock().unwrap().2 += 1;
                    continue;
                }
                let dir = file.parent().unwrap();
                let check = Command::new(&buildc)
                    .env("BUILDLANG_STDLIB", &stdlib)
                    .current_dir(dir)
                    .arg("check")
                    .arg(&file)
                    .output()
                    .expect("run buildc check");
                if !check.status.success() {
                    counts.lock().unwrap().1 += 1;
                    continue;
                }
                counts.lock().unwrap().0 += 1;
                let c_path = work.join(format!("t{t}.c"));
                let obj = work.join(format!("t{t}.o"));
                let emit = Command::new(&buildc)
                    .env("BUILDLANG_STDLIB", &stdlib)
                    .current_dir(dir)
                    .arg(&file)
                    .arg("-o")
                    .arg(&c_path)
                    .output()
                    .expect("run buildc emit");
                if !emit.status.success() {
                    failures.lock().unwrap().push(format!(
                        "{}: check passed, C emission failed:\n{}",
                        file.display(),
                        String::from_utf8_lossy(&emit.stderr)
                    ));
                    continue;
                }
                let out = Command::new(&cc)
                    .args(&cc_args)
                    .arg("-o")
                    .arg(&obj)
                    .arg(&c_path)
                    .output()
                    .expect("run C compiler");
                if !out.status.success() {
                    let errors: String = String::from_utf8_lossy(&out.stderr)
                        .lines()
                        .filter(|l| l.contains("error"))
                        .take(3)
                        .collect::<Vec<_>>()
                        .join("\n");
                    failures.lock().unwrap().push(format!(
                        "{}: check passed, C compile failed:\n{errors}",
                        file.display()
                    ));
                }
            })
        })
        .collect();
    for t in threads {
        t.join().expect("worker thread");
    }
    let _ = std::fs::remove_dir_all(&work);
    let (checked, rejected, skipped) = *counts.lock().unwrap();
    eprintln!(
        "{} files: {checked} passed check, {rejected} rejected, {skipped} skipped",
        files.len()
    );
    let failures = failures.lock().unwrap();
    assert!(
        failures.is_empty(),
        "{} program(s) passed `buildc check` but do not compile:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
