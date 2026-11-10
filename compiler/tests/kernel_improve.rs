//! The improvement loop of the `sum-det` kernel package: its frozen set and
//! its ledger chain, checked by the loop kernel itself on every CI run.
//!
//! The loop kernel (`improve/kernel`, written in BuildLang) is built here and
//! asked two things from a copy of the package: do the frozen files still have
//! their listed digests, and is the ledger chain whole. Each answer has planted
//! controls: one changed byte in a frozen file, a frozen file dropped from the
//! list, and a ledger with a removed or edited line must each be refused.

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

/// Run the loop kernel in `work` and return (exit code, stdout).
fn ask(kernel: &Path, work: &Path, what: &str) -> (i32, String) {
    let out = Command::new(kernel)
        .arg(what)
        .current_dir(work)
        .output()
        .expect("loop kernel runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

fn build_kernel(work: &Path) -> PathBuf {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["pkg", "build"])
        .arg(work.join("improve/kernel"))
        .arg("--registry")
        .arg(repo_root().join("registry"))
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "loop kernel build failed\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    work.join("improve/kernel/target/pkg")
        .join(if cfg!(windows) {
            "improve-kernel.exe"
        } else {
            "improve-kernel"
        })
}

fn rewrite(path: &Path, edit: impl Fn(&str) -> String) -> String {
    let old = std::fs::read_to_string(path).expect("read");
    std::fs::write(path, edit(&old)).expect("write");
    old
}

#[test]
fn sum_det_frozen_set_and_ledger_hold_and_planted_faults_are_refused() {
    if !has_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let pkg = repo_root().join("registry/packages/sum-det/0.1.0");
    let work = std::env::temp_dir().join(format!("buildlang-kimprove-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    copy_dir(&pkg, &work);
    let kernel = build_kernel(&work);

    let (rc, text) = ask(&kernel, &work, "frozen");
    assert_eq!(rc, 0, "frozen set does not match its digests: {}", text);
    let (rc, text) = ask(&kernel, &work, "verify");
    assert_eq!(rc, 0, "ledger chain is broken: {}", text);
    let ledger = work.join("improve/LEDGER.jsonl");
    let lines = std::fs::read_to_string(&ledger)
        .expect("ledger")
        .lines()
        .count();
    assert!(lines >= 2, "the ledger holds no run");

    // Control 1: one changed byte in a frozen file.
    let rule = work.join("bench/bench.toml");
    let old = rewrite(&rule, |s| {
        s.replace("threshold_ppm = 50000", "threshold_ppm = 10")
    });
    assert!(
        old.contains("threshold_ppm = 50000"),
        "threshold line moved"
    );
    let (rc, text) = ask(&kernel, &work, "frozen");
    assert!(
        rc != 0 && text.contains("changed [bench/bench.toml]"),
        "{}",
        text
    );
    std::fs::write(&rule, old).expect("restore");

    // Control 2: a file that must be frozen is dropped from the list.
    let list = work.join("improve/FROZEN.sha256");
    let old = rewrite(&list, |s| {
        let keep: Vec<&str> = s
            .lines()
            .filter(|l| !l.ends_with("  holdout/sizes.txt"))
            .collect();
        keep.join("\n") + "\n"
    });
    let (rc, text) = ask(&kernel, &work, "frozen");
    assert!(
        rc != 0 && text.contains("not-listed [holdout/sizes.txt]"),
        "{}",
        text
    );
    std::fs::write(&list, old).expect("restore");

    // Control 3: a ledger line removed. Control 4: a refusal turned into a pass.
    let old = rewrite(&ledger, |s| {
        s.lines().skip(1).collect::<Vec<_>>().join("\n") + "\n"
    });
    assert_ne!(ask(&kernel, &work, "verify").0, 0, "a removed line passed");
    std::fs::write(&ledger, &old).expect("restore");
    rewrite(&ledger, |s| {
        s.replacen("\"decision\":\"refuse\"", "\"decision\":\"eligible\"", 1)
    });
    assert_ne!(ask(&kernel, &work, "verify").0, 0, "an edited line passed");
    std::fs::write(&ledger, old).expect("restore");
    assert_eq!(ask(&kernel, &work, "verify").0, 0);
    let _ = std::fs::remove_dir_all(&work);
}
