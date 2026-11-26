// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 26D end-to-end integration tests for Certified GPU Polyhedral Loop
//! Tiling (`H * delta >= 0`), Uniformity-Safe Subgroup Auto-Vectorization
//! (`strict_bit_exact` vs `bounded_ulp` across `W_s = 16, 32, 64`), and
//! `buildlang.gpu-tv-cert/v1` across `wgsl`, `hlsl`, `msl`, and `spirv`.

use std::path::PathBuf;
use std::process::Command;

use buildlang::codegen::gpu_polyhedral::{
    compute_gpu_certificate_sha256, higham_f32_gamma_bound, simulate_bounded_ulp_subgroup_sum,
    simulate_strict_bit_exact_subgroup_sum, DependenceKind, DependenceVector, GpuTvCertificate,
    SubgroupMode, GPU_TV_CERT_SCHEMA,
};
use buildlang::codegen::ir::{BinOp, MirType};
use buildlang::codegen::superopt::{compute_certificate_sha256, CertExpr};

fn buildc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_buildc"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn stdlib_dir() -> PathBuf {
    repo_root().join("stdlib")
}

fn make_temp_dir(label: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("buildlang_gpu_tv_{}_{}", label, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn gpu_tv_cert_all_four_backends_wgsl_hlsl_msl_spirv() {
    let dir = make_temp_dir("four_backends");
    let src = dir.join("kernel_superopt.bld");
    std::fs::write(
        &src,
        r#"
#[compute]
fn scale_norm(input: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let v: f32 = input[i];
    let half: f32 = v / 2.0;
    let doubled: f32 = half * 2.0;
    out[i] = doubled * 1.0;
}
"#,
    )
    .unwrap();

    for (target, ext, expected_intrinsic) in [
        ("wgsl", "wgsl", "subgroupAdd"),
        ("hlsl", "hlsl", "WaveActiveSum"),
        ("msl", "metal", "simd_sum"),
        ("spirv", "spv", "OpGroupNonUniformFAdd"),
    ] {
        let out_shader = dir.join(format!("kernel_{target}.{ext}"));
        let cert_path = dir.join(format!("kernel_{target}.gpu-cert.json"));
        let compile = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                src.to_str().unwrap(),
                "--target",
                target,
                "--emit-gpu-cert",
                cert_path.to_str().unwrap(),
                "--subgroup-mode",
                "strict-bit-exact",
                "-o",
                out_shader.to_str().unwrap(),
            ])
            .output()
            .expect("gpu compile failed");
        assert!(
            compile.status.success(),
            "target {target} compile failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        assert!(out_shader.exists(), "missing shader for {target}");
        assert!(cert_path.exists(), "missing gpu cert for {target}");

        let raw = std::fs::read_to_string(&cert_path).unwrap();
        let cert: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
        assert_eq!(cert.schema, GPU_TV_CERT_SCHEMA);
        assert_eq!(cert.target, target);
        assert_eq!(cert.subgroup_intrinsic, expected_intrinsic);
        assert!(cert.verified);
        assert!(cert.superopt_cert.rewrites_applied >= 3);
        assert_eq!(cert.polyhedral_schedules.len(), 1);
        assert_eq!(cert.polyhedral_schedules[0].dimensions, 1);
        assert_eq!(
            cert.polyhedral_schedules[0].sram_bank_stride[0],
            cert.polyhedral_schedules[0].tile_sizes[0] + 1
        );
        assert!(cert.subgroup_plan.uniformity_verified);
        assert_eq!(cert.subgroup_plan.widths_verified, vec![16, 32, 64]);
        assert_eq!(cert.subgroup_plan.max_cross_width_abs_diff, 0.0);
        assert!(matches!(
            cert.subgroup_plan.mode,
            SubgroupMode::StrictBitExact { .. }
        ));

        let verify = Command::new(buildc_bin())
            .args([
                "verify-cert",
                cert_path.to_str().unwrap(),
                "--shader",
                out_shader.to_str().unwrap(),
            ])
            .output()
            .expect("verify-cert failed");
        assert!(
            verify.status.success(),
            "verify-cert failed for {target}: {}",
            String::from_utf8_lossy(&verify.stderr)
        );
        let stdout = String::from_utf8_lossy(&verify.stdout);
        assert!(
            stdout.contains("buildlang.gpu-tv-cert/v1 verified"),
            "unexpected stdout for {target}: {stdout}"
        );
    }

    // Also verify real 2D matmul + barrier-synchronized reduce examples with bounded-ulp mode
    let matmul_bld = repo_root().join("examples").join("gpu").join("matmul.bld");
    let reduce_bld = repo_root().join("examples").join("gpu").join("reduce.bld");
    for (label, bld_path, expected_dims) in [
        ("matmul", matmul_bld, 2usize),
        ("reduce", reduce_bld, 1usize),
    ] {
        let out_wgsl = dir.join(format!("{label}.wgsl"));
        let cert_path = dir.join(format!("{label}.gpu-cert.json"));
        let res = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                bld_path.to_str().unwrap(),
                "--target",
                "wgsl",
                "--emit-gpu-cert",
                cert_path.to_str().unwrap(),
                "--subgroup-mode",
                "bounded-ulp",
                "-o",
                out_wgsl.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            res.status.success(),
            "{label} wgsl compile failed: {}",
            String::from_utf8_lossy(&res.stderr)
        );
        let cert: GpuTvCertificate =
            serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
        assert_eq!(cert.polyhedral_schedules[0].dimensions, expected_dims);
        assert!(matches!(
            cert.subgroup_plan.mode,
            SubgroupMode::BoundedUlp { .. }
        ));
        let v = Command::new(buildc_bin())
            .args([
                "verify-cert",
                cert_path.to_str().unwrap(),
                "--shader",
                out_wgsl.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(v.status.success());
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planted_loop_carried_dependence_is_rejected_fail_closed() {
    let dir = make_temp_dir("planted_dep");
    let src = dir.join("prefix_bad.bld");
    let out_wgsl = dir.join("prefix_bad.wgsl");
    let cert_path = dir.join("prefix_bad.gpu-cert.json");
    // 1D prefix recurrence: `out[i] = out[i - 1] + b[i]` -> delta = +1 on
    // parallelized gl_GlobalInvocationID.x axis!
    std::fs::write(
        &src,
        r#"
#[compute]
fn prefix_racy(b: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    if i > 0 {
        out[i] = out[i - 1] + b[i];
    }
}
"#,
    )
    .unwrap();

    let res = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            src.to_str().unwrap(),
            "--target",
            "wgsl",
            "--emit-gpu-cert",
            cert_path.to_str().unwrap(),
            "-o",
            out_wgsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !res.status.success(),
        "loop-carried parallel dependence must be rejected fail-closed"
    );
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(
        stderr.contains("illegal loop-carried dependence"),
        "expected polyhedral loop-carried error, got: {stderr}"
    );
    assert!(!out_wgsl.exists(), "must write zero shader on refusal");
    assert!(!cert_path.exists(), "must write zero cert on refusal");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planted_non_uniform_barrier_is_rejected_fail_closed() {
    let dir = make_temp_dir("planted_barrier");
    let src = dir.join("bad_barrier.bld");
    let out_wgsl = dir.join("bad_barrier.wgsl");
    let cert_path = dir.join("bad_barrier.gpu-cert.json");
    std::fs::write(
        &src,
        r#"
#[compute]
fn bad_barrier(a: &[f32], out: &mut [f32]) ~ Gpu {
    let scratch = workgroupArray(64);
    let lid = gl_LocalInvocationID.x;
    let gid = gl_GlobalInvocationID.x;
    scratch[lid] = a[gid];
    if lid < 32 {
        workgroupBarrier();
    }
    out[gid] = scratch[lid];
}
"#,
    )
    .unwrap();

    let res = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            src.to_str().unwrap(),
            "--target",
            "wgsl",
            "--emit-gpu-cert",
            cert_path.to_str().unwrap(),
            "-o",
            out_wgsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !res.status.success(),
        "non-uniform workgroupBarrier must fail closed"
    );
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(
        stderr.contains("non-uniform workgroup barrier"),
        "expected non-uniform barrier rejection, got: {stderr}"
    );
    assert!(!out_wgsl.exists());
    assert!(!cert_path.exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planted_forged_rewrite_dependence_digest_and_shader_tamper_fail_closed() {
    let dir = make_temp_dir("tamper_suite");
    let src = dir.join("clean.bld");
    let out_wgsl = dir.join("clean.wgsl");
    let cert_path = dir.join("clean.gpu-cert.json");
    std::fs::write(
        &src,
        r#"
#[compute]
fn twice_gpu(input: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let v: f32 = input[i];
    out[i] = v * 2.0;
}
"#,
    )
    .unwrap();

    let ok = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            src.to_str().unwrap(),
            "--target",
            "wgsl",
            "--emit-gpu-cert",
            cert_path.to_str().unwrap(),
            "-o",
            out_wgsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(ok.status.success());

    let raw = std::fs::read_to_string(&cert_path).unwrap();

    // 1. Planted forged e-graph rewrite (`v * 2.0 -> v + 3.0`) with recomputed digests
    let mut forged_rw: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
    forged_rw.superopt_cert.steps[0].after = CertExpr::BinOp {
        op: BinOp::Add,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::FloatBits(3.0_f64.to_bits(), MirType::f32())),
    };
    forged_rw.superopt_cert.cert_sha256 =
        compute_certificate_sha256(&forged_rw.superopt_cert).unwrap();
    forged_rw.cert_sha256 = compute_gpu_certificate_sha256(&forged_rw).unwrap();
    let p1 = dir.join("forged_rw.json");
    std::fs::write(&p1, serde_json::to_string_pretty(&forged_rw).unwrap()).unwrap();
    let r1 = Command::new(buildc_bin())
        .args(["verify-cert", p1.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!r1.status.success());
    assert!(String::from_utf8_lossy(&r1.stderr).contains("axiom violation"));

    // 2. Planted illegal polyhedral dependence with recomputed cert digest
    let mut forged_dep: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
    forged_dep.polyhedral_schedules[0]
        .dependences
        .push(DependenceVector {
            buffer: "out".to_string(),
            kind: DependenceKind::Raw,
            delta: vec![1],
        });
    forged_dep.cert_sha256 = compute_gpu_certificate_sha256(&forged_dep).unwrap();
    let p2 = dir.join("forged_dep.json");
    std::fs::write(&p2, serde_json::to_string_pretty(&forged_dep).unwrap()).unwrap();
    let r2 = Command::new(buildc_bin())
        .args(["verify-cert", p2.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!r2.status.success());
    assert!(String::from_utf8_lossy(&r2.stderr).contains("illegal loop-carried dependence"));

    // 3. Tampered certificate SHA-256
    let mut bad_sha: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
    bad_sha.cert_sha256 = "f".repeat(64);
    let p3 = dir.join("bad_sha.json");
    std::fs::write(&p3, serde_json::to_string_pretty(&bad_sha).unwrap()).unwrap();
    let r3 = Command::new(buildc_bin())
        .args(["verify-cert", p3.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!r3.status.success());
    assert!(String::from_utf8_lossy(&r3.stderr).contains("SHA-256 mismatch"));

    // 4. Tampered emitted shader file pinned via `--shader`
    let bad_shader = dir.join("tampered.wgsl");
    let mut shader_txt = std::fs::read_to_string(&out_wgsl).unwrap();
    shader_txt.push_str("\n// tampered\n");
    std::fs::write(&bad_shader, shader_txt).unwrap();
    let r4 = Command::new(buildc_bin())
        .args([
            "verify-cert",
            cert_path.to_str().unwrap(),
            "--shader",
            bad_shader.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!r4.status.success());
    assert!(String::from_utf8_lossy(&r4.stderr).contains("emitted shader SHA-256 mismatch"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cross_width_16_32_64_strict_bit_exact_zero_drift_and_bounded_ulp() {
    let n = 1024usize;
    let vals: Vec<f32> = (0..n)
        .map(|i| {
            let s = if i % 7 == 0 { -1.0_f32 } else { 1.0_f32 };
            s * (((i * 53 + 19) % 127) as f32) * 0.015625_f32
        })
        .collect();

    // Strict bit-exact mode: 0-bit drift across W_s = 16, 32, 64
    let s16 = simulate_strict_bit_exact_subgroup_sum(&vals, 16, 16);
    let s32 = simulate_strict_bit_exact_subgroup_sum(&vals, 16, 32);
    let s64 = simulate_strict_bit_exact_subgroup_sum(&vals, 16, 64);
    assert_eq!(s16, s32, "W_s=16 and W_s=32 must have 0-bit drift");
    assert_eq!(s32, s64, "W_s=32 and W_s=64 must have 0-bit drift");

    // Bounded-ULP mode: all three widths stay within Higham gamma_m * sum(|x_i|)
    let exact: f64 = vals.iter().map(|&x| x as f64).sum();
    let abs_sum: f64 = vals.iter().map(|&x| (x as f64).abs()).sum();
    let bound = higham_f32_gamma_bound(n as u32) * abs_sum;
    for w in [16u32, 32u32, 64u32] {
        let got = simulate_bounded_ulp_subgroup_sum(&vals, w) as f64;
        assert!(
            (got - exact).abs() <= bound,
            "W_s={w} exceeded Higham bound {bound}: err={}",
            (got - exact).abs()
        );
    }
}
