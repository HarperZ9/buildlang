// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 76A integration tests for `--target glsl --emit-gpu-cert`, bringing all
//! 5/5 GPU shader targets (`wgsl`, `hlsl`, `msl`, `spirv`, `glsl`) under
//! verified `buildlang.gpu-tv-cert/v1` translation-validation certificates,
//! valid GLSL 450 compute shader emission, live `glslangValidator` checks,
//! and fail-closed planted forged-certificate / tampered-shader rejection.

use std::path::{Path, PathBuf};
use std::process::Command;

use buildlang::codegen::gpu_polyhedral::{
    compute_gpu_certificate_sha256, GpuTvCertificate, SubgroupMode, GPU_TV_CERT_SCHEMA,
};

fn buildc_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_buildc"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn stdlib_dir() -> PathBuf {
    repo_root().join("stdlib")
}

fn make_temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_wave76a_glsl_{}_{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn shader_tools_required() -> bool {
    std::env::var("BUILDLANG_SHADER_TOOLS_REQUIRE").as_deref() == Ok("1")
}

fn tool_available(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn validate_glsl_compute_if_available(test_name: &str, glsl_path: &Path, spv_out: &Path) {
    if !tool_available("glslangValidator", "--version") {
        assert!(
            !shader_tools_required(),
            "{test_name}: glslangValidator not on PATH and BUILDLANG_SHADER_TOOLS_REQUIRE=1"
        );
        eprintln!("skipping glslangValidator in {test_name}: tool not on PATH");
        return;
    }
    let out = Command::new("glslangValidator")
        .args([
            "-V",
            "-S",
            "comp",
            glsl_path.to_str().unwrap(),
            "-o",
            spv_out.to_str().unwrap(),
        ])
        .output()
        .expect("run glslangValidator");
    assert!(
        out.status.success(),
        "glslangValidator rejected {}:\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
        glsl_path.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
        std::fs::read_to_string(glsl_path).unwrap_or_default()
    );

    if tool_available("spirv-val", "--version") {
        let sval = Command::new("spirv-val")
            .args(["--target-env", "vulkan1.0", spv_out.to_str().unwrap()])
            .output()
            .expect("run spirv-val");
        assert!(
            sval.status.success(),
            "spirv-val rejected GLSL-compiled SPIR-V for {}:\n{}",
            glsl_path.display(),
            String::from_utf8_lossy(&sval.stderr)
        );
    }
}

#[test]
fn gpu_tv_cert_all_five_backends_wgsl_hlsl_msl_spirv_glsl() {
    let dir = make_temp_dir("five_backends");
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

    for (target, ext, expected_intrinsic, expected_coop_api) in [
        ("wgsl", "wgsl", "subgroupAdd", "wgsl_subgroup_matrix"),
        ("hlsl", "hlsl", "WaveActiveSum", "sm6_10_linalg_matrix"),
        ("msl", "metal", "simd_sum", "msl_simdgroup_matrix"),
        (
            "spirv",
            "spv",
            "OpGroupNonUniformFAdd",
            "spv_khr_cooperative_matrix",
        ),
        ("glsl", "glsl", "subgroupAdd", "glsl_khr_cooperative_matrix"),
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
        assert_eq!(
            cert.polyhedral_schedules[0]
                .cooperative_tile
                .as_ref()
                .expect("cooperative tile plan missing")
                .api_surface,
            expected_coop_api
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

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn glsl_compute_kernels_emit_valid_glsl450_and_verify_cert() {
    let dir = make_temp_dir("glsl_kernels");
    let custom_math = dir.join("clamp_affine.bld");
    std::fs::write(
        &custom_math,
        r#"
#[compute]
fn clamp_affine(gain: f32, bias: f32, src: &[f32], dst: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let x: f32 = src[i];
    let mut y: f32 = (x * gain + bias) * 1.0 + 0.0;
    if y < 0.0 {
        y = 0.0;
    }
    if y > 1.0 {
        y = 1.0;
    }
    dst[i] = y;
}
"#,
    )
    .unwrap();

    let cases: [(&str, PathBuf, &str, usize); 4] = [
        ("clamp_affine", custom_math, "strict-bit-exact", 1),
        (
            "saxpy",
            repo_root().join("examples").join("gpu").join("saxpy.bld"),
            "strict-bit-exact",
            1,
        ),
        (
            "matmul",
            repo_root().join("examples").join("gpu").join("matmul.bld"),
            "bounded-ulp",
            2,
        ),
        (
            "reduce",
            repo_root().join("examples").join("gpu").join("reduce.bld"),
            "bounded-ulp",
            1,
        ),
    ];

    for (label, bld_path, mode, expected_dims) in cases {
        let out_glsl = dir.join(format!("{label}.glsl"));
        let cert_path = dir.join(format!("{label}.gpu-cert.json"));
        let spv_path = dir.join(format!("{label}.spv"));

        let compile = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                bld_path.to_str().unwrap(),
                "--target",
                "glsl",
                "--emit-gpu-cert",
                cert_path.to_str().unwrap(),
                "--subgroup-mode",
                mode,
                "-o",
                out_glsl.to_str().unwrap(),
            ])
            .output()
            .expect("glsl compile failed");
        assert!(
            compile.status.success(),
            "GLSL compile failed for {label}: {}",
            String::from_utf8_lossy(&compile.stderr)
        );

        let glsl_src = std::fs::read_to_string(&out_glsl).unwrap();
        assert!(
            glsl_src.contains("#version 450"),
            "{label}: missing #version 450"
        );
        assert!(
            glsl_src.contains("layout(local_size_x ="),
            "{label}: missing compute local_size layout"
        );
        assert!(
            glsl_src.contains("void main()"),
            "{label}: missing void main()"
        );
        if label == "reduce" {
            assert!(
                glsl_src.contains("shared float"),
                "reduce: missing shared workgroup array"
            );
            assert!(
                glsl_src.contains("barrier();"),
                "reduce: missing GLSL barrier()"
            );
        }

        let cert: GpuTvCertificate =
            serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
        assert_eq!(cert.schema, GPU_TV_CERT_SCHEMA);
        assert_eq!(cert.target, "glsl");
        assert_eq!(cert.subgroup_intrinsic, "subgroupAdd");
        assert_eq!(cert.polyhedral_schedules[0].dimensions, expected_dims);
        assert_eq!(
            cert.polyhedral_schedules[0]
                .cooperative_tile
                .as_ref()
                .unwrap()
                .api_surface,
            "glsl_khr_cooperative_matrix"
        );

        let verify = Command::new(buildc_bin())
            .args([
                "verify-cert",
                cert_path.to_str().unwrap(),
                "--shader",
                out_glsl.to_str().unwrap(),
            ])
            .output()
            .expect("verify-cert failed");
        assert!(
            verify.status.success(),
            "verify-cert failed for {label}: {}",
            String::from_utf8_lossy(&verify.stderr)
        );

        validate_glsl_compute_if_available(label, &out_glsl, &spv_path);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planted_forged_glsl_shader_sha256_and_tampered_shader_fail_closed() {
    let dir = make_temp_dir("planted_forged");
    let src = dir.join("scale_glsl.bld");
    let out_glsl = dir.join("scale_glsl.glsl");
    let cert_path = dir.join("scale_glsl.gpu-cert.json");
    std::fs::write(
        &src,
        r#"
#[compute]
fn scale_glsl(input: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let v: f32 = input[i];
    out[i] = (v * 2.0) * 1.0;
}
"#,
    )
    .unwrap();

    let ok = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            src.to_str().unwrap(),
            "--target",
            "glsl",
            "--emit-gpu-cert",
            cert_path.to_str().unwrap(),
            "-o",
            out_glsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "baseline glsl compile failed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );

    // Baseline must verify cleanly before tamper.
    let base_v = Command::new(buildc_bin())
        .args([
            "verify-cert",
            cert_path.to_str().unwrap(),
            "--shader",
            out_glsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(base_v.status.success());

    let raw = std::fs::read_to_string(&cert_path).unwrap();

    // Planted Control 1: flip 1 hex char in shader_sha256 without recomputing cert_sha256.
    let mut flipped_raw: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
    let replacement = if flipped_raw.shader_sha256.starts_with('0') {
        '1'
    } else {
        '0'
    };
    flipped_raw.shader_sha256 = format!("{replacement}{}", &flipped_raw.shader_sha256[1..]);
    let p_flip = dir.join("forged_raw_shader_sha.json");
    std::fs::write(&p_flip, serde_json::to_string_pretty(&flipped_raw).unwrap()).unwrap();
    let r1 = Command::new(buildc_bin())
        .args([
            "verify-cert",
            p_flip.to_str().unwrap(),
            "--shader",
            out_glsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !r1.status.success(),
        "flipped shader_sha256 must be rejected fail-closed (RC != 0)"
    );

    // Planted Control 2: forge shader_sha256 AND recompute cert_sha256 so self-hash matches,
    // verifying `--shader` catches the mismatch against actual GLSL bytes.
    let mut forged_rehash: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
    forged_rehash.shader_sha256 = format!("{replacement}{}", &forged_rehash.shader_sha256[1..]);
    forged_rehash.cert_sha256 = compute_gpu_certificate_sha256(&forged_rehash).unwrap();
    let p_rehash = dir.join("forged_rehash_shader_sha.json");
    std::fs::write(
        &p_rehash,
        serde_json::to_string_pretty(&forged_rehash).unwrap(),
    )
    .unwrap();
    let r2 = Command::new(buildc_bin())
        .args([
            "verify-cert",
            p_rehash.to_str().unwrap(),
            "--shader",
            out_glsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !r2.status.success(),
        "forged shader_sha256 with recomputed cert_sha256 must fail --shader binding"
    );
    assert!(
        String::from_utf8_lossy(&r2.stderr).contains("emitted shader SHA-256 mismatch"),
        "expected shader SHA-256 mismatch diagnostic, got: {}",
        String::from_utf8_lossy(&r2.stderr)
    );

    // Planted Control 3: tamper the emitted GLSL file itself.
    let tampered_glsl = dir.join("tampered.glsl");
    let mut txt = std::fs::read_to_string(&out_glsl).unwrap();
    txt.push_str("\n// tampered\n");
    std::fs::write(&tampered_glsl, txt).unwrap();
    let r3 = Command::new(buildc_bin())
        .args([
            "verify-cert",
            cert_path.to_str().unwrap(),
            "--shader",
            tampered_glsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !r3.status.success(),
        "tampered GLSL file must be rejected fail-closed"
    );
    assert!(
        String::from_utf8_lossy(&r3.stderr).contains("emitted shader SHA-256 mismatch"),
        "expected shader SHA-256 mismatch diagnostic, got: {}",
        String::from_utf8_lossy(&r3.stderr)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planted_glsl_non_uniform_barrier_and_loop_carried_dependence_fail_closed() {
    let dir = make_temp_dir("planted_reject");

    // 1. Loop-carried parallel dependence on --target glsl --emit-gpu-cert
    let dep_src = dir.join("dep_bad.bld");
    let dep_glsl = dir.join("dep_bad.glsl");
    let dep_cert = dir.join("dep_bad.gpu-cert.json");
    std::fs::write(
        &dep_src,
        r#"
#[compute]
fn prefix_bad(b: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    if i > 0 {
        out[i] = out[i - 1] + b[i];
    }
}
"#,
    )
    .unwrap();
    let r_dep = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            dep_src.to_str().unwrap(),
            "--target",
            "glsl",
            "--emit-gpu-cert",
            dep_cert.to_str().unwrap(),
            "-o",
            dep_glsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !r_dep.status.success(),
        "loop-carried dependence on GLSL target must fail closed"
    );
    assert!(
        String::from_utf8_lossy(&r_dep.stderr).contains("illegal loop-carried dependence"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&r_dep.stderr)
    );
    assert!(!dep_glsl.exists());
    assert!(!dep_cert.exists());

    // 2. Non-uniform workgroup barrier on --target glsl --emit-gpu-cert
    let bar_src = dir.join("bar_bad.bld");
    let bar_glsl = dir.join("bar_bad.glsl");
    let bar_cert = dir.join("bar_bad.gpu-cert.json");
    std::fs::write(
        &bar_src,
        r#"
#[compute]
fn barrier_bad(a: &[f32], out: &mut [f32]) ~ Gpu {
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
    let r_bar = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            bar_src.to_str().unwrap(),
            "--target",
            "glsl",
            "--emit-gpu-cert",
            bar_cert.to_str().unwrap(),
            "-o",
            bar_glsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !r_bar.status.success(),
        "non-uniform barrier on GLSL target must fail closed"
    );
    assert!(
        String::from_utf8_lossy(&r_bar.stderr).contains("non-uniform workgroup barrier"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&r_bar.stderr)
    );
    assert!(!bar_glsl.exists());
    assert!(!bar_cert.exists());

    let _ = std::fs::remove_dir_all(&dir);
}
