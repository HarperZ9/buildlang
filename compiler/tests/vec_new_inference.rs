//! M2: an unannotated `let v = vec_new();` takes its element type from the
//! first value pushed into it. Before, it was always a vector of `i32`, and
//! pushing a string was rejected with BL0901, the commonest agent error in the
//! probes. A later push of another type is still rejected, so the inference
//! never turns a type error into a silent one.

use std::process::Command;

fn buildc() -> Command {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", root.join("stdlib"));
    cmd
}

fn have_c_compiler() -> bool {
    let probe = |cc: &str, arg: &str| {
        Command::new(cc)
            .arg(arg)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    ["gcc", "clang", "cc"]
        .iter()
        .any(|cc| probe(cc, "--version"))
        || probe("cl", "/?")
}

fn write(name: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("buildlang_vec_infer_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, src).unwrap();
    path
}

#[test]
fn first_push_sets_the_element_type() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let src = "fn main() ~ Console {\n    let mut words = vec_new();\n    for w in \"the quick brown fox\".split(\" \") {\n        words.push(w);\n    }\n    let mut out = vec_new();\n    let mut i = words.len();\n    while i > 0 {\n        i = i - 1;\n        vec_push(&mut out, words[i]);\n    }\n    println!(\"{} {} {} {}\", out[0], out[1], out[2], out[3]);\n    let mut cs = vec_new();\n    for c in \"ab\".chars() {\n        cs.push(c);\n    }\n    println!(\"{}{}\", cs[1], cs[0]);\n    let mut nums = vec_new();\n    nums.push(7);\n    nums.push(8);\n    println!(\"{}\", nums[0] + nums[1]);\n    let empty = vec_new();\n    println!(\"{}\", empty.len());\n}\n";
    let file = write("infer.bld", src);
    let check = buildc().arg("check").arg(&file).output().unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
    let run = buildc().arg("run").arg(&file).output().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let out = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert_eq!(out.trim_end(), "fox brown quick the\nba\n15\n0");
}

#[test]
fn a_later_push_of_another_type_is_still_rejected() {
    let src = "fn main() ~ Console {\n    let mut v = vec_new();\n    v.push(\"a\");\n    v.push(3);\n    println!(\"{}\", v.len());\n}\n";
    let file = write("mixed.bld", src);
    let out = buildc()
        .arg("check")
        .arg(&file)
        .arg("--error-format=json")
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "a mixed vector must not pass the check"
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let codes: Vec<&str> = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert!(!codes.is_empty(), "{report}");
}

/// The method form `v.push(x)` makes the same element check as `vec_push`:
/// on 1.6.0, `v.push(3)` on a `Vec<str>` passed the check and failed in gcc.
#[test]
fn pushing_the_wrong_type_into_an_annotated_vector_fails_the_check() {
    let src = "fn main() ~ Console {\n    let mut v: Vec<str> = vec_new();\n    v.push(\"a\");\n    v.push(3);\n    println!(\"{}\", v.len());\n}\n";
    let file = write("mixed_annotated.bld", src);
    let out = buildc().arg("check").arg(&file).output().unwrap();
    assert!(!out.status.success(), "must be rejected at check time");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("BL0901"), "{text}");
}
