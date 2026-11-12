//! The modern-feature matrix.
//!
//! `tests/features` holds one small program for each language feature a
//! programmer coming from Rust, C++, Swift or Zig expects, each with the output
//! it must print. `tests/features/GAPS.txt` lists the ones the compiler does not
//! handle yet, with a class:
//!
//! - `rejected`: `buildc check` refuses the program;
//! - `broken`: the check passes and the program then fails to build, crashes or
//!   prints something else.
//!
//! Two rules, checked here:
//!
//! 1. Every program not listed in GAPS.txt passes the check, builds, runs and
//!    prints its `.expected` text.
//! 2. Every line of GAPS.txt is still true. A feature that starts working fails
//!    this test until its line is removed, so the list cannot go stale and a
//!    fixed feature cannot silently break again.
//!
//! Output is compared after joining on whitespace.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn buildc() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", repo_root().join("stdlib"));
    cmd
}

fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Outcome {
    Pass,
    Rejected,
    Broken,
}

fn outcome(file: &Path) -> (Outcome, String) {
    let check = buildc()
        .arg("check")
        .arg(file)
        .output()
        .expect("buildc runs");
    if !check.status.success() {
        return (
            Outcome::Rejected,
            String::from_utf8_lossy(&check.stdout).into_owned(),
        );
    }
    let run = buildc().arg("run").arg(file).output().expect("buildc runs");
    let got = words(&String::from_utf8_lossy(&run.stdout));
    let want = words(&std::fs::read_to_string(file.with_extension("expected")).expect("expected"));
    if run.status.success() && got == want {
        (Outcome::Pass, String::new())
    } else {
        (
            Outcome::Broken,
            format!(
                "want [{want}] got [{got}] status {:?}\n{}",
                run.status.code(),
                String::from_utf8_lossy(&run.stderr)
            ),
        )
    }
}

fn gaps() -> BTreeMap<String, Outcome> {
    let text = std::fs::read_to_string(repo_root().join("tests/features/GAPS.txt")).expect("GAPS");
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let name = parts.next().expect("a name").to_string();
        let class = match parts.next() {
            Some("rejected") => Outcome::Rejected,
            Some("broken") => Outcome::Broken,
            other => panic!("GAPS.txt: `{name}` has class {other:?}; use rejected or broken"),
        };
        assert!(
            map.insert(name.clone(), class).is_none(),
            "GAPS.txt lists `{name}` twice"
        );
    }
    map
}

#[test]
fn every_feature_program_has_the_outcome_the_matrix_records() {
    let dir = repo_root().join("tests/features");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/features exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("bld"))
        .collect();
    files.sort();
    assert!(
        files.len() >= 70,
        "feature programs went missing: {} left",
        files.len()
    );
    let gaps = gaps();
    let names: Vec<String> = files
        .iter()
        .map(|f| f.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    for name in gaps.keys() {
        assert!(
            names.contains(name),
            "GAPS.txt names `{name}`, which has no program"
        );
    }

    // Run the programs on a few threads; each is its own compiler process.
    let results: Vec<(String, Outcome, String)> = std::thread::scope(|scope| {
        let chunks: Vec<_> = files.chunks(files.len().div_ceil(4)).collect();
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|f| {
                            let (o, detail) = outcome(f);
                            (
                                f.file_stem().unwrap().to_string_lossy().into_owned(),
                                o,
                                detail,
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });

    let mut failures = Vec::new();
    let (mut pass, mut rejected, mut broken) = (0, 0, 0);
    for (name, got, detail) in &results {
        match got {
            Outcome::Pass => pass += 1,
            Outcome::Rejected => rejected += 1,
            Outcome::Broken => broken += 1,
        }
        let want = gaps.get(name).copied().unwrap_or(Outcome::Pass);
        if *got == want {
            continue;
        }
        failures.push(match (want, got) {
            (Outcome::Pass, _) => format!("{name}: must pass, is {got:?}\n{detail}"),
            (_, Outcome::Pass) => {
                format!("{name}: works now. Remove its line from tests/features/GAPS.txt")
            }
            _ => {
                format!("{name}: GAPS.txt says {want:?}, it is {got:?}. Update the line\n{detail}")
            }
        });
    }
    println!(
        "feature matrix: {pass} pass, {rejected} rejected, {broken} broken, of {}",
        results.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
