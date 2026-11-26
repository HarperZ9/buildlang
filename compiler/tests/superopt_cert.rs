// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! End-to-end tests for the V5 certified e-graph superoptimizer wired into
//! `buildc` (`--superopt`, `--emit-cert`, `buildc verify-cert`), multi-target
//! emission (`c`, `wgsl`, `msl`, `riscv64`), fail-closed forged-certificate
//! rejection, and Wave 25D stdlib/typechecker additions (`last_mut`, `Vec3::exp`,
//! `length(Vec3)`).

use std::path::PathBuf;
use std::process::Command;

use buildlang::codegen::ir::{BinOp, MirType};
use buildlang::codegen::superopt::{
    compute_certificate_sha256, CertExpr, SuperoptCertificate, SUPEROPT_CERT_SCHEMA,
};

fn buildc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_buildc"))
}

fn stdlib_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("stdlib")
}

fn make_temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_superopt_{}_{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn superopt_c_execution_bit_exact_and_cert_verified() {
    let dir = make_temp_dir("c_exec");
    let src = dir.join("kernel.bld");
    let cert_path = dir.join("kernel.cert.json");
    std::fs::write(
        &src,
        r#"
fn affine_kernel(x: i64, y: f64) -> f64 {
    let a: i64 = x * 2;
    let b: i64 = x * 8;
    let c: i64 = (b + 0) - 0;
    let d: i64 = (a * 1) + (x - x);
    let e: f64 = (y / 4.0) * 2.0;
    let f: f64 = e * 1.0;
    (c + d) as f64 + f
}

fn main() -> i32 ~ Console {
    let r1: f64 = affine_kernel(7, 12.0);
    let r2: f64 = affine_kernel(-5, -3.5);
    let r3: f64 = affine_kernel(0, 0.25);
    println!("{} {} {}", r1, r2, r3);
    0
}
"#,
    )
    .unwrap();

    let baseline = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["run", src.to_str().unwrap()])
        .output()
        .expect("baseline run failed");
    assert!(
        baseline.status.success(),
        "baseline stderr: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );

    let superopt = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            "run",
            "--superopt",
            "--emit-cert",
            cert_path.to_str().unwrap(),
            src.to_str().unwrap(),
        ])
        .output()
        .expect("superopt run failed");
    assert!(
        superopt.status.success(),
        "superopt stderr: {}",
        String::from_utf8_lossy(&superopt.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&baseline.stdout),
        String::from_utf8_lossy(&superopt.stdout),
        "superoptimized program must match baseline bit-for-bit"
    );

    let cert_json = std::fs::read_to_string(&cert_path).expect("cert file missing");
    let cert: SuperoptCertificate = serde_json::from_str(&cert_json).expect("valid cert json");
    assert_eq!(cert.schema, SUPEROPT_CERT_SCHEMA);
    assert_eq!(cert.target, "c");
    assert!(cert.verified);
    assert!(
        cert.rewrites_applied >= 6,
        "expected >= 6 certified rewrites, got {}",
        cert.rewrites_applied
    );
    assert!(
        cert.total_cost_after < cert.total_cost_before,
        "cost should decrease: {} -> {}",
        cert.total_cost_before,
        cert.total_cost_after
    );

    let verify = Command::new(buildc_bin())
        .args(["verify-cert", cert_path.to_str().unwrap()])
        .output()
        .expect("verify-cert failed");
    assert!(
        verify.status.success(),
        "verify-cert failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let verify_out = String::from_utf8_lossy(&verify.stdout);
    assert!(
        verify_out.contains("buildlang.superopt-cert/v1 verified"),
        "unexpected verify stdout: {verify_out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn superopt_multi_target_c_wgsl_msl_riscv64() {
    let dir = make_temp_dir("multi_target");

    // Scalar/float kernel for C and RISC-V
    let scalar_src = dir.join("scalar.bld");
    std::fs::write(
        &scalar_src,
        r#"
fn scale_and_halve(x: i64, y: f64) -> f64 {
    let a: i64 = x * 8;
    let b: i64 = a + 0;
    let c: f64 = (y / 4.0) * 2.0;
    (b as f64) + c
}
"#,
    )
    .unwrap();

    for (target, ext) in [("c", "c"), ("riscv64", "s")] {
        let out_file = dir.join(format!("scalar_{target}.{ext}"));
        let cert_file = dir.join(format!("scalar_{target}.cert.json"));
        let res = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                scalar_src.to_str().unwrap(),
                "--target",
                target,
                "--superopt",
                "--emit-cert",
                cert_file.to_str().unwrap(),
                "-o",
                out_file.to_str().unwrap(),
            ])
            .output()
            .expect("multi-target compile failed");
        assert!(
            res.status.success(),
            "target {target} failed: {}",
            String::from_utf8_lossy(&res.stderr)
        );
        let cert_raw = std::fs::read_to_string(&cert_file).unwrap();
        let cert: SuperoptCertificate = serde_json::from_str(&cert_raw).unwrap();
        assert_eq!(cert.target, target);
        assert!(cert.rewrites_applied >= 3);
        let verify = Command::new(buildc_bin())
            .args(["verify-cert", cert_file.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            verify.status.success(),
            "verify-cert failed for {target}: {}",
            String::from_utf8_lossy(&verify.stderr)
        );
    }

    // Compute shader for WGSL and MSL
    let gpu_src = dir.join("shader.bld");
    std::fs::write(
        &gpu_src,
        r#"
#[compute]
fn scale_half(input: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let v: f32 = input[i];
    let half: f32 = v / 2.0;
    let doubled: f32 = half * 2.0;
    out[i] = doubled * 1.0;
}
"#,
    )
    .unwrap();

    for (target, ext) in [("wgsl", "wgsl"), ("msl", "metal")] {
        let out_file = dir.join(format!("shader_{target}.{ext}"));
        let cert_file = dir.join(format!("shader_{target}.cert.json"));
        let res = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                gpu_src.to_str().unwrap(),
                "--target",
                target,
                "--superopt",
                "--emit-cert",
                cert_file.to_str().unwrap(),
                "-o",
                out_file.to_str().unwrap(),
            ])
            .output()
            .expect("gpu multi-target compile failed");
        assert!(
            res.status.success(),
            "target {target} failed: {}",
            String::from_utf8_lossy(&res.stderr)
        );
        let emitted = std::fs::read_to_string(&out_file).unwrap();
        assert!(
            emitted.contains("0.5"),
            "{target} output should contain reciprocal 0.5 from R13_FLOAT_DIV_POW2_MUL:\n{emitted}"
        );
        let cert_raw = std::fs::read_to_string(&cert_file).unwrap();
        let cert: SuperoptCertificate = serde_json::from_str(&cert_raw).unwrap();
        assert_eq!(cert.target, target);
        assert!(cert.rewrites_applied >= 3);
        let verify = Command::new(buildc_bin())
            .args(["verify-cert", cert_file.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(verify.status.success());
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_cert_rejects_forged_rewrite_and_tampered_digest_fail_closed() {
    let dir = make_temp_dir("forged");
    let src = dir.join("forge_src.bld");
    let cert_path = dir.join("valid.cert.json");
    let forged_rewrite_path = dir.join("forged_rewrite.cert.json");
    let tampered_sha_path = dir.join("tampered_sha.cert.json");
    std::fs::write(
        &src,
        r#"
fn twice(x: i64) -> i64 {
    x * 2
}
"#,
    )
    .unwrap();

    let out_c = dir.join("forge_src.c");
    let compile = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            src.to_str().unwrap(),
            "--target",
            "c",
            "--superopt",
            "--emit-cert",
            cert_path.to_str().unwrap(),
            "-o",
            out_c.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(compile.status.success());

    let raw = std::fs::read_to_string(&cert_path).unwrap();
    let mut forged: SuperoptCertificate = serde_json::from_str(&raw).unwrap();
    assert!(!forged.steps.is_empty());

    // 1. Forge an unsound rewrite `x * 2 -> x + 3` AND recompute SHA-256
    forged.steps[0].after = CertExpr::BinOp {
        op: BinOp::Add,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::Int(3, MirType::i64())),
    };
    forged.cert_sha256 = compute_certificate_sha256(&forged).unwrap();
    std::fs::write(
        &forged_rewrite_path,
        serde_json::to_string_pretty(&forged).unwrap(),
    )
    .unwrap();

    let bad1 = Command::new(buildc_bin())
        .args(["verify-cert", forged_rewrite_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!bad1.status.success(), "forged rewrite must fail closed");
    let stderr1 = String::from_utf8_lossy(&bad1.stderr);
    assert!(
        stderr1.contains("axiom violation"),
        "expected axiom violation, got: {stderr1}"
    );

    // 2. Tamper 1 hex digit in a valid certificate digest
    let mut tampered: SuperoptCertificate = serde_json::from_str(&raw).unwrap();
    let replacement = if tampered.cert_sha256.starts_with('a') {
        "b"
    } else {
        "a"
    };
    tampered.cert_sha256 = format!("{replacement}{}", &tampered.cert_sha256[1..]);
    std::fs::write(
        &tampered_sha_path,
        serde_json::to_string_pretty(&tampered).unwrap(),
    )
    .unwrap();

    let bad2 = Command::new(buildc_bin())
        .args(["verify-cert", tampered_sha_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!bad2.status.success(), "tampered digest must fail closed");
    let stderr2 = String::from_utf8_lossy(&bad2.stderr);
    assert!(
        stderr2.contains("SHA-256 mismatch"),
        "expected SHA-256 mismatch, got: {stderr2}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave25d_vec3_exp_last_mut_and_borrowed_str_methods() {
    let dir = make_temp_dir("math_vec");
    let src = dir.join("vec_math.bld");
    std::fs::write(
        &src,
        r#"
use std::math::{Vec3, length};

fn check_borrowed_str(s: &String) -> i64 {
    if s.is_empty() { 0 } else { s.len() as i64 }
}

fn main() -> i32 ~ Console {
    let a = Vec3::new(3.0, 4.0, 0.0);
    let len = length(a);
    let e = Vec3::new(0.0, 0.0, 0.0).exp();
    let msg = "buildlang".to_string();
    let n = check_borrowed_str(&msg);
    println!("{} {} {}", len, e.x + e.y + e.z, n);
    0
}
"#,
    )
    .unwrap();

    let res = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["check", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&res.stderr)
    );

    let _ = std::fs::remove_dir_all(&dir);
}
