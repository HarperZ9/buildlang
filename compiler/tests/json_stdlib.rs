//! M4: JSON in the standard library (`stdlib/json.bld`, `mod json;`).
//!
//! - Reader and writer against JS: `tests/json/docs.txt` holds 54 documents,
//!   18 of them invalid. For each valid one, `json_stringify` and
//!   `json_stringify_sorted` must equal JS `JSON.stringify` of `JSON.parse`
//!   (keys as written, then sorted); each invalid one must be refused. The
//!   expected file comes from `tools/json_fixture.mjs`.
//! - Typed structs: `tests/json/typed.bld` reads moves into structs with the
//!   checked field accessors; a missing key, a string where an integer
//!   belongs, a fraction for an integer and a truncated document each give
//!   an Err naming the problem.
//! - JSON lines: `tests/json/lines.bld` writes five records shaped like Low
//!   Water's combat trace (fixed key order, sorted nested keys, no spaces).
//!   The output must equal what JS wrote for the same records, byte for byte,
//!   and two runs must give identical bytes.
//! - Modules: a `format!` inside a `mod` file printed text from the main file
//!   on 1.8.0 (macro arguments were read back by span from the wrong file).

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

fn run(file: &Path, args: &[&str]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(file);
    if !args.is_empty() {
        cmd.arg("--").args(args);
    }
    let out = cmd.output().expect("buildc runs");
    assert!(
        out.status.success(),
        "{} failed:\n{}{}",
        file.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

fn assert_lines(name: &str, got: &str, want_file: &Path) {
    let want = std::fs::read_to_string(want_file)
        .unwrap()
        .replace("\r\n", "\n");
    let got_lines: Vec<&str> = got.lines().collect();
    let want_lines: Vec<&str> = want.lines().collect();
    let bad: Vec<String> = got_lines
        .iter()
        .zip(&want_lines)
        .enumerate()
        .filter(|(_, (g, w))| g != w)
        .map(|(i, (g, w))| format!("line {}:\n  got  {g}\n  want {w}", i + 1))
        .collect();
    assert!(
        bad.is_empty() && got_lines.len() == want_lines.len(),
        "{name}: {} lines differ ({} got, {} wanted)\n{}",
        bad.len(),
        got_lines.len(),
        want_lines.len(),
        bad.join("\n")
    );
}

#[test]
fn reader_and_writer_match_json_parse_and_stringify() {
    let dir = repo_root().join("tests/json");
    let docs = dir.join("docs.txt");
    let got = run(&dir.join("roundtrip.bld"), &[docs.to_str().unwrap()]);
    assert_lines("roundtrip", &got, &dir.join("docs.expected"));
    let errors = got.lines().filter(|l| *l == "error").count();
    assert_eq!(errors, 36, "18 invalid documents, two lines each");
}

#[test]
fn typed_structs_read_with_checked_errors() {
    let dir = repo_root().join("tests/json");
    let got = run(&dir.join("typed.bld"), &[]);
    assert_lines("typed", &got, &dir.join("typed.expected"));
}

#[test]
fn json_lines_match_js_and_are_byte_stable() {
    let dir = repo_root().join("tests/json");
    let first = run(&dir.join("lines.bld"), &[]);
    assert_lines("lines", &first, &dir.join("lines.expected"));
    let second = run(&dir.join("lines.bld"), &[]);
    assert_eq!(first.as_bytes(), second.as_bytes(), "two runs differ");
}

#[test]
fn format_inside_a_module_reads_its_own_file() {
    let dir = std::env::temp_dir().join(format!("buildlang-modfmt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("helper.bld"),
        "fn greet(n: i64) -> str {\n    format!(\"hello {}\", n)\n}\n\nfn show(n: i64) ~ Console {\n    println!(\"n = {}\", n);\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.bld"),
        "mod helper;\n\nfn main() ~ Console {\n    let g = greet(5);\n    println!(\"{}\", g);\n    show(7);\n}\n",
    )
    .unwrap();
    let got = run(&dir.join("main.bld"), &[]);
    assert_eq!(got.trim_end(), "hello 5\nn = 7");
    let _ = std::fs::remove_dir_all(&dir);
}
