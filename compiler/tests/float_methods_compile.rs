//! Every f64 method the checker accepts must also compile and give the right
//! value. `x.log(base)` checked and then failed in the C compiler: the lowering
//! passed two arguments to the one-argument portable `log`. This sweep runs each
//! accepted method once, through an unannotated binding, and compares the
//! printed value with Rust's own result, so the next method that checks but
//! does not build fails here rather than in a user's program.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

#[test]
fn every_accepted_f64_method_compiles_and_matches_rust() {
    let x = 2.25f64;
    let y = -0.5f64;
    let calls: Vec<(&str, f64)> = vec![
        ("x.abs()", x.abs()),
        ("x.sqrt()", x.sqrt()),
        ("x.cbrt()", x.cbrt()),
        ("x.ceil()", x.ceil()),
        ("x.floor()", x.floor()),
        ("x.round()", x.round()),
        ("y.trunc()", y.trunc()),
        ("x.fract()", x.fract()),
        ("x.sin()", x.sin()),
        ("x.cos()", x.cos()),
        ("x.tan()", x.tan()),
        ("y.asin()", y.asin()),
        ("y.acos()", y.acos()),
        ("x.atan()", x.atan()),
        ("x.sinh()", x.sinh()),
        ("x.cosh()", x.cosh()),
        ("x.tanh()", x.tanh()),
        ("x.exp()", x.exp()),
        ("x.exp2()", x.exp2()),
        ("x.ln()", x.ln()),
        ("x.log2()", x.log2()),
        ("x.log10()", x.log10()),
        ("x.log(3.0)", x.log(3.0)),
        ("x.log(10.0)", x.log(10.0)),
        ("x.to_radians()", x.to_radians()),
        ("x.to_degrees()", x.to_degrees()),
        ("x.powi(3)", x.powi(3)),
        ("x.powf(0.5)", x.powf(0.5)),
        ("x.atan2(y)", x.atan2(y)),
        ("x.hypot(y)", x.hypot(y)),
        ("y.copysign(x)", y.copysign(x)),
        ("x.clamp(0.0, 1.0)", x.clamp(0.0, 1.0)),
        ("x.mul_add(2.0, 1.0)", x.mul_add(2.0, 1.0)),
        ("x.min(y)", x.min(y)),
        ("x.max(y)", x.max(y)),
    ];
    let dir = std::env::temp_dir().join(format!("buildlang-floatmethods-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    let mut body =
        String::from("fn main() ~ Console {\n    let x: f64 = 2.25;\n    let y: f64 = -0.5;\n");
    for (i, (call, _)) in calls.iter().enumerate() {
        body.push_str(&format!(
            "    let r{i} = {call};\n    println!(\"{{}}\", r{i});\n"
        ));
    }
    body.push_str("}\n");
    std::fs::write(&file, &body).unwrap();

    let buildc = env!("CARGO_BIN_EXE_buildc");
    let check = Command::new(buildc)
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("check")
        .arg(&file)
        .output()
        .expect("buildc runs");
    assert!(
        check.status.success(),
        "the sweep program must check:\n{}",
        String::from_utf8_lossy(&check.stderr)
    );

    let out = Command::new(buildc)
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success() && stderr.contains("No C compiler") {
        eprintln!("skipping: no C compiler ({stderr})");
        return;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "a program that checks must build and run:\n{stderr}"
    );
    let got: Vec<f64> = text
        .lines()
        .filter_map(|l| l.trim().parse::<f64>().ok())
        .collect();
    assert_eq!(got.len(), calls.len(), "output:\n{text}");
    for ((call, want), g) in calls.iter().zip(&got) {
        let tol = 1e-12 * want.abs().max(1.0);
        assert!((g - want).abs() <= tol, "{call}: got {g}, want {want}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
