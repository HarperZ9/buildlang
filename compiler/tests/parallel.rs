//! M5: parallel by default.
//!
//! - Every program in `tests/parallel/run` prints its `.expected` file at 1, 2
//!   and 3 pool threads, so the output does not depend on the schedule.
//! - Every program in `tests/parallel/reject` is a planted unsafe parallel
//!   body (an effect, a `&mut`, a `static mut`, a write to an outer variable,
//!   a function value, a capture). `buildc check` must reject each with the
//!   code its first line names.
//! - The `BL0406` fix is exact edits: applying them must give a program that
//!   checks, builds and prints what the serial program would.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn programs(sub: &str) -> Vec<PathBuf> {
    let dir = repo_root().join("tests/parallel").join(sub);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/parallel exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("bld"))
        .collect();
    files.sort();
    files
}

fn buildc() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", repo_root().join("stdlib"));
    cmd
}

fn run(file: &PathBuf, threads: u32) -> Result<String, String> {
    let out = buildc()
        .env("BUILD_PAR_THREADS", threads.to_string())
        .arg("run")
        .arg(file)
        .output()
        .expect("buildc runs");
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[test]
fn parallel_programs_print_the_same_bytes_at_every_thread_count() {
    let files = programs("run");
    assert!(files.len() >= 2, "parallel programs went missing");
    let mut failures = Vec::new();
    for file in &files {
        let want = std::fs::read_to_string(file.with_extension("expected"))
            .expect("expected file")
            .replace("\r\n", "\n");
        for threads in [1, 2, 3] {
            match run(file, threads) {
                Ok(got) if got == want => {}
                Ok(got) => failures.push(format!(
                    "{} at {threads} threads:\n got:\n{got}\n want:\n{want}",
                    file.display()
                )),
                Err(e) => failures.push(format!("{} at {threads} threads: {e}", file.display())),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn check_json(file: &PathBuf) -> serde_json::Value {
    let out = buildc()
        .arg("check")
        .arg(file)
        .arg("--error-format=json")
        .output()
        .expect("buildc runs");
    assert!(!out.status.success(), "{}: check passed", file.display());
    serde_json::from_slice(&out.stdout).expect("JSON report")
}

fn expected_code(file: &PathBuf) -> String {
    let text = std::fs::read_to_string(file).expect("program");
    text.lines()
        .next()
        .and_then(|l| l.strip_prefix("// expect: "))
        .expect("first line names the expected code")
        .trim()
        .to_string()
}

#[test]
fn planted_parallel_bodies_are_rejected_with_their_code() {
    let files = programs("reject");
    assert!(files.len() >= 8, "planted programs went missing");
    let mut failures = Vec::new();
    for file in &files {
        let want = expected_code(file);
        let report = check_json(file);
        let diags = report["diagnostics"].as_array().expect("diagnostics");
        let Some(d) = diags.iter().find(|d| d["code"] == want.as_str()) else {
            failures.push(format!("{}: no {want} in {report}", file.display()));
            continue;
        };
        if want == "BL0406" {
            let s = &d["suggestions"][0];
            let edits = s["edits"].as_array().map_or(0, Vec::len);
            if edits == 0 {
                failures.push(format!("{}: BL0406 without edits", file.display()));
            }
            if s["applicability"] == "MachineApplicable" {
                failures.push(format!(
                    "{}: hoist fix claims MachineApplicable",
                    file.display()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Apply the first suggestion's edits of the first `BL0406` diagnostic.
fn apply_hoist(file: &PathBuf) -> String {
    let report = check_json(file);
    let d = report["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .find(|d| d["code"] == "BL0406")
        .expect("a BL0406 diagnostic")
        .clone();
    let mut edits: Vec<(usize, usize, String)> = d["suggestions"][0]["edits"]
        .as_array()
        .expect("edits")
        .iter()
        .map(|e| {
            (
                e["span"]["byte_start"].as_u64().unwrap() as usize,
                e["span"]["byte_end"].as_u64().unwrap() as usize,
                e["replacement"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    let mut text = std::fs::read_to_string(file).expect("program");
    for (s, e, r) in edits {
        text.replace_range(s..e, &r);
    }
    text
}

#[test]
fn hoisting_the_effect_out_gives_a_program_that_prints_the_serial_bytes() {
    let cases = [
        (
            "console_named.bld",
            "pixel 0\npixel 1\npixel 2\npixel 3\n1.5\n",
        ),
        ("console_closure.bld", "2\n4\n"),
        ("user_effect.bld", "note 3\n"),
    ];
    let dir = std::env::temp_dir().join(format!("bl-par-hoist-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut failures = Vec::new();
    for (name, want) in cases {
        // Byte offsets in the JSON report index the source after CRLF is
        // folded to LF, so the edits are applied to an LF copy (a Windows
        // checkout has CRLF).
        let original = repo_root().join("tests/parallel/reject").join(name);
        let file = dir.join(format!("lf-{name}"));
        let text = std::fs::read_to_string(&original).expect("program");
        std::fs::write(&file, text.replace("\r\n", "\n")).expect("write LF copy");
        let fixed = dir.join(name);
        std::fs::write(&fixed, apply_hoist(&file)).expect("write fixed program");
        for threads in [1, 3] {
            match run(&fixed, threads) {
                Ok(got) if got == want => {}
                Ok(got) => failures.push(format!("{name} at {threads}: got {got:?}")),
                Err(e) => failures.push(format!("{name} at {threads}: {e}")),
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
