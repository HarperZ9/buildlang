//! M4: `HashMap<str, T>` for any `T`.
//!
//! Every program in `tests/hashmap` must build, run and print its `.expected`
//! file. On 1.8.0 each one fails:
//! - `values.bld`: a `u64`, `u8` or tuple value, or any method on a map reached
//!   through a reference, did not build (lowering and the C backend spelled
//!   the per-type wrapper names differently);
//! - `order.bld`: `keys()` came back in bucket order; it is now insertion
//!   order, checked against a JS `Map` (the expected file is node's output);
//! - `remove_cluster.bld`: removing one key of a probe cluster cut the chain,
//!   so the other keys of the cluster were no longer found.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

#[test]
fn hashmap_programs_print_their_expected_output() {
    let dir = repo_root().join("tests/hashmap");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/hashmap exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("bld"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 3, "hashmap programs went missing");
    let mut failures = Vec::new();
    for file in &files {
        let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
            .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
            .arg("run")
            .arg(file)
            .output()
            .expect("buildc runs");
        let got = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        let want = std::fs::read_to_string(file.with_extension("expected"))
            .expect("expected file")
            .replace("\r\n", "\n");
        if !out.status.success() {
            failures.push(format!(
                "{}: failed\n{}",
                file.display(),
                String::from_utf8_lossy(&out.stderr)
            ));
        } else if got.trim_end() != want.trim_end() {
            failures.push(format!("{}:\n got:\n{got}\n want:\n{want}", file.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
