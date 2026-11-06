//! Scalar maths builtins return f64. An unannotated `let r = exp(x);` was typed
//! i32 by the lowering because `exp` was missing from its f64 list, so the C
//! output stored the result in an `int32_t` and the program still checked
//! (found by the superframe audio kernel). Every builtin below must keep its
//! fractional part through an unannotated binding.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

#[test]
fn unannotated_maths_results_keep_their_fraction() {
    let dir = std::env::temp_dir().join(format!("buildlang-mathtypes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    let calls = [
        ("exp(1.0)", 2.718281828459045),
        ("exp2(0.5)", std::f64::consts::SQRT_2),
        ("log(10.0)", std::f64::consts::LN_10),
        ("log2(3.0)", 3f64.log2()),
        ("log10(3.0)", 3f64.log10()),
        ("sinh(0.5)", 0.5f64.sinh()),
        ("cosh(0.5)", 0.5f64.cosh()),
        ("tanh(0.5)", 0.5f64.tanh()),
        ("asin(0.5)", 0.5f64.asin()),
        ("acos(0.5)", 0.5f64.acos()),
        ("atan(0.5)", 0.5f64.atan()),
        ("atan2(1.0, 2.0)", 1f64.atan2(2.0)),
    ];
    let mut body = String::from("fn main() ~ Console {\n");
    for (i, (call, _)) in calls.iter().enumerate() {
        body.push_str(&format!(
            "    let r{i} = {call};\n    println!(\"{{}}\", r{i});\n"
        ));
    }
    body.push_str("}\n");
    std::fs::write(&file, body).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() && text.trim().is_empty() {
        eprintln!(
            "skipping: no C compiler ({})",
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    }
    let got: Vec<f64> = text
        .lines()
        .filter_map(|l| l.trim().parse::<f64>().ok())
        .collect();
    assert_eq!(got.len(), calls.len(), "output:\n{text}");
    for ((call, want), g) in calls.iter().zip(&got) {
        assert!((g - want).abs() < 1e-12, "{call}: got {g}, want {want}");
        assert!(g.fract() != 0.0, "{call} lost its fraction: {g}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
