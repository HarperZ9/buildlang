//! Every tool package in the registry, built offline from its own Build.lock
//! and run against the outputs its original produced.
//!
//! A package opts in with `tests/cases.txt`. Each line is
//! `MODE GOLDEN ARGS...`: run the package's program with ARGS from the
//! package folder and compare its stdout with the GOLDEN file. MODE `exact`
//! wants the same bytes. MODE `float:ABS:REL` splits both outputs on
//! whitespace; tokens that parse as numbers must agree within ABS or within
//! REL of the golden value, and other tokens must be equal. The goldens were
//! written by `tests/reference.py` from the original tool (see SURVEY.md in
//! the registry).

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn has_c_compiler() -> bool {
    ["gcc", "clang", "cc", "cl"].into_iter().any(|cc| {
        Command::new(cc)
            .arg(if cc == "cl" { "/?" } else { "--version" })
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn copy_dir(src: &Path, dest: &Path) {
    std::fs::create_dir_all(dest).expect("create dir");
    for entry in std::fs::read_dir(src).expect("read dir") {
        let path = entry.expect("entry").path();
        let target = dest.join(path.file_name().expect("name"));
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy file");
        }
    }
}

/// (name, version, folder) of every registry version with tests/cases.txt.
fn tool_packages() -> Vec<(String, String, PathBuf)> {
    let registry = repo_root().join("registry");
    let text = std::fs::read_to_string(registry.join("index.json")).expect("index");
    let index: serde_json::Value = serde_json::from_str(&text).expect("index json");
    let mut out = Vec::new();
    for (name, entry) in index["packages"].as_object().expect("packages") {
        let Some(versions) = entry["versions"].as_object() else {
            continue;
        };
        for (version, v) in versions {
            let dir = registry.join(v["path"].as_str().expect("path"));
            if dir.join("tests/cases.txt").is_file() {
                out.push((name.clone(), version.clone(), dir));
            }
        }
    }
    out
}

fn numbers_agree(got: &str, want: &str, abs: f64, rel: f64) -> bool {
    match (got.parse::<f64>(), want.parse::<f64>()) {
        (Ok(g), Ok(w)) => g == w || (g - w).abs() <= abs || (g - w).abs() <= rel * w.abs(),
        _ => got == want,
    }
}

fn compare(mode: &str, got: &str, want: &str) -> Result<(), String> {
    if mode == "exact" {
        return if got == want {
            Ok(())
        } else {
            let line = got
                .lines()
                .zip(want.lines())
                .position(|(a, b)| a != b)
                .map(|i| i + 1)
                .unwrap_or(got.lines().count().min(want.lines().count()) + 1);
            Err(format!("output differs from the golden at line {}", line))
        };
    }
    let parts: Vec<&str> = mode.split(':').collect();
    assert!(
        parts.len() == 3 && parts[0] == "float",
        "unknown mode {}",
        mode
    );
    let abs: f64 = parts[1].parse().expect("abs bound");
    let rel: f64 = parts[2].parse().expect("rel bound");
    let g: Vec<&str> = got.split_whitespace().collect();
    let w: Vec<&str> = want.split_whitespace().collect();
    if g.len() != w.len() {
        return Err(format!("{} tokens, golden has {}", g.len(), w.len()));
    }
    for (i, (a, b)) in g.iter().zip(&w).enumerate() {
        if !numbers_agree(a, b, abs, rel) {
            return Err(format!("token {}: got {}, golden {}", i, a, b));
        }
    }
    Ok(())
}

#[test]
fn every_tool_package_matches_its_original() {
    if !has_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let packages = tool_packages();
    assert!(
        !packages.is_empty(),
        "no registry package has tests/cases.txt"
    );
    let mut failures = Vec::new();
    for (name, version, dir) in packages {
        let work = std::env::temp_dir().join(format!(
            "buildlang-tool-{}-{}-{}",
            name,
            version,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&work);
        copy_dir(&dir, &work);
        let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
            .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
            .args(["pkg", "build"])
            .arg(&work)
            .arg("--registry")
            .arg(repo_root().join("registry"))
            .output()
            .expect("buildc runs");
        if !out.status.success() {
            failures.push(format!(
                "{} {}: build failed\n{}{}",
                name,
                version,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
            continue;
        }
        let exe = work.join("target/pkg").join(if cfg!(windows) {
            format!("{}.exe", name)
        } else {
            name.clone()
        });
        let cases = std::fs::read_to_string(work.join("tests/cases.txt")).unwrap();
        for line in cases
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        {
            let words: Vec<&str> = line.split_whitespace().collect();
            let (mode, golden, args) = (words[0], words[1], &words[2..]);
            let run = Command::new(&exe)
                .args(args)
                .current_dir(&work)
                .output()
                .expect("tool runs");
            let got = String::from_utf8_lossy(&run.stdout).to_string();
            let want = std::fs::read_to_string(work.join(golden)).expect("golden");
            if let Err(why) = compare(mode, &got, &want) {
                failures.push(format!("{} {} `{}`: {}", name, version, line, why));
            } else {
                eprintln!("ok: {} {} {}", name, version, line);
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
