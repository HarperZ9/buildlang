// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 106A regression tests for native GLSL 450 IEEE-754 `fma(a, b, c)`
//! lowering and 5-way GPU FMA parity across `wgsl`, `hlsl`, `msl`, `spirv`,
//! and `glsl`, plus modular `u32` wrapping-add compute lowering and fail-closed
//! planted negative controls.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use buildlang::codegen::backend::glsl::GlslBackend;
use buildlang::codegen::gpu_polyhedral::{GpuTvCertificate, GPU_TV_CERT_SCHEMA};
use buildlang::codegen::ir::{
    BlockId, FloatSize, LocalId, MirBlock, MirFnSig, MirFunction, MirLocal, MirModule,
    MirTerminator, MirType, MirValue, ShaderStage,
};
use buildlang::codegen::superopt::SuperoptConfig;

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
        "buildlang_wave106a_glsl_fma_{}_{}",
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

fn count_fma_calls(source: &str, token: &str) -> usize {
    source.matches(token).count()
}

fn validate_glsl_and_check_spirv_fma(
    test_name: &str,
    glsl_path: &Path,
    spv_out: &Path,
    expected_fma_count: usize,
) {
    if !tool_available("glslangValidator", "--version") {
        assert!(
            !shader_tools_required(),
            "{test_name}: glslangValidator not on PATH and BUILDLANG_SHADER_TOOLS_REQUIRE=1"
        );
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

    if expected_fma_count > 0 && tool_available("spirv-dis", "--version") {
        let dis = Command::new("spirv-dis")
            .arg(spv_out.to_str().unwrap())
            .output()
            .expect("run spirv-dis");
        assert!(dis.status.success(), "spirv-dis failed");
        let disasm = String::from_utf8_lossy(&dis.stdout);
        let spv_fmas = disasm.matches(" Fma ").count();
        assert_eq!(
            spv_fmas, expected_fma_count,
            "{test_name}: expected {expected_fma_count} GLSL.std.450 Fma instructions in SPIR-V, got {spv_fmas}"
        );
    }
}

#[test]
fn glsl_fma_5way_parity_and_glslang_spirv_validation() {
    assert!(
        SuperoptConfig::for_target("glsl").allow_fma_contract,
        "SuperoptConfig::for_target(\"glsl\") must enable allow_fma_contract"
    );

    let dir = make_temp_dir("five_way_fma");
    let src = dir.join("pbr_fma_probe.bld");
    std::fs::write(
        &src,
        r#"
#[compute(workgroup_size = [64, 1, 1])]
fn pbr_fma_probe(
    w0: f32,
    w1: f32,
    bias: f32,
    x0_buf: &[f32],
    x1_buf: &[f32],
    out_buf: &mut [f32],
) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let z0: f32 = x0_buf[i];
    let z1: f32 = x1_buf[i];
    let h0: f32 = w0 * z0 + w1 * z1 + bias;
    let a0: f32 = 0.5 + (0.5 * h0) / (1.0 + h0 * h0);
    let h1: f32 = 0.85 * a0 - 0.35 * z1 + 0.15 * bias;
    let a1: f32 = 0.5 + (0.5 * h1) / (1.0 + h1 * h1);
    out_buf[i] = 0.75 * a1 + 0.25 * z0;
}
"#,
    )
    .unwrap();

    let mut fma_counts = Vec::new();
    let mut rewrite_counts = Vec::new();

    for (target, ext, fma_token) in [
        ("wgsl", "wgsl", "fma("),
        ("hlsl", "hlsl", "mad("),
        ("msl", "metal", "fma("),
        ("spirv", "spv", ""),
        ("glsl", "glsl", "fma("),
    ] {
        let out_shader = dir.join(format!("pbr_fma_probe.{ext}"));
        let cert_path = dir.join(format!("pbr_fma_probe.{target}.gpu-cert.json"));
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

        let verify = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
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

        let cert: GpuTvCertificate =
            serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
        assert_eq!(cert.schema, GPU_TV_CERT_SCHEMA);
        assert!(cert.verified);
        rewrite_counts.push((target, cert.superopt_cert.rewrites_applied));

        if !fma_token.is_empty() {
            let shader_src = std::fs::read_to_string(&out_shader).unwrap();
            let count = count_fma_calls(&shader_src, fma_token);
            assert!(
                count >= 5,
                "target {target} expected at least 5 `{fma_token}` fusions, got {count}\n{shader_src}"
            );
            fma_counts.push((target, count));
        }

        if target == "glsl" {
            let expected_glsl_fmas = fma_counts
                .iter()
                .find(|(t, _)| *t == "glsl")
                .map(|(_, c)| *c)
                .unwrap_or(0);
            let spv_from_glsl = dir.join("pbr_fma_probe_from_glsl.spv");
            validate_glsl_and_check_spirv_fma(
                "glsl_fma_5way_parity",
                &out_shader,
                &spv_from_glsl,
                expected_glsl_fmas,
            );
        }
    }

    let first_rewrites = rewrite_counts[0].1;
    assert!(
        first_rewrites >= 5,
        "expected >= 5 certified superopt rewrites, got {first_rewrites}"
    );
    for (target, rc) in &rewrite_counts {
        assert_eq!(
            *rc, first_rewrites,
            "target {target} rewrite count {rc} diverged from 5-way parity {first_rewrites}"
        );
    }
    assert_eq!(fma_counts.len(), 4);
    let first_fmas = fma_counts[0].1;
    for (target, count) in &fma_counts {
        assert_eq!(
            *count, first_fmas,
            "target {target} FMA count {count} diverged from 5-way parity {first_fmas}"
        );
    }
}

#[test]
fn glsl_wrapping_u32_add_sha256_round_validates_live() {
    let dir = make_temp_dir("wrapping_u32");
    let src = dir.join("sha256_round.bld");
    std::fs::write(
        &src,
        r#"
#[compute(workgroup_size = [64, 1, 1])]
fn sha256_wrap_step(
    k_round: u32,
    a_buf: &[u32],
    e_buf: &[u32],
    out_buf: &mut [u32],
) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let a: u32 = a_buf[i];
    let e: u32 = e_buf[i];
    let r6: u32 = 6;
    let l26: u32 = 26;
    let rot: u32 = (e >> r6) | (e << l26);
    out_buf[i] = a.wrapping_add(rot).wrapping_add(k_round);
}
"#,
    )
    .unwrap();

    let glsl_out = dir.join("sha256_wrap_step.glsl");
    let cert_out = dir.join("sha256_wrap_step.glsl.gpu-cert.json");
    let compile = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            src.to_str().unwrap(),
            "--target",
            "glsl",
            "--emit-gpu-cert",
            cert_out.to_str().unwrap(),
            "-o",
            glsl_out.to_str().unwrap(),
        ])
        .output()
        .expect("compile sha256_wrap_step");
    assert!(
        compile.status.success(),
        "glsl compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let glsl_src = std::fs::read_to_string(&glsl_out).unwrap();
    assert!(glsl_src.contains("layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;"));
    assert!(glsl_src.contains("uint "));

    let spv_out = dir.join("sha256_wrap_step.spv");
    validate_glsl_and_check_spirv_fma("glsl_wrapping_u32", &glsl_out, &spv_out, 0);
}

#[test]
fn glsl_planted_broken_fma_and_wrong_arity_rejected_fail_closed() {
    // Control A: Direct MIR codegen rejects 2-argument `fma(a, b)` fail-closed.
    let mut mir = MirModule::new(Arc::from("planted_bad_fma"));
    let mut func = MirFunction::new(
        Arc::from("bad_fma_kernel"),
        MirFnSig::new(
            vec![
                MirType::Float(FloatSize::F32),
                MirType::Float(FloatSize::F32),
            ],
            MirType::Void,
        ),
    );
    func.shader_stage = Some(ShaderStage::Compute);
    func.workgroup_size = Some([64, 1, 1]);
    func.locals = vec![
        MirLocal {
            id: LocalId(0),
            name: Some(Arc::from("a")),
            ty: MirType::Float(FloatSize::F32),
            is_mut: false,
            is_param: true,
            annotations: Vec::new(),
        },
        MirLocal {
            id: LocalId(1),
            name: Some(Arc::from("b")),
            ty: MirType::Float(FloatSize::F32),
            is_mut: false,
            is_param: true,
            annotations: Vec::new(),
        },
        MirLocal {
            id: LocalId(2),
            name: Some(Arc::from("res")),
            ty: MirType::Float(FloatSize::F32),
            is_mut: true,
            is_param: false,
            annotations: Vec::new(),
        },
    ];
    let mut entry = MirBlock::new(BlockId(0));
    entry.terminator = Some(MirTerminator::Call {
        func: MirValue::Function(Arc::from("fma")),
        args: vec![MirValue::Local(LocalId(0)), MirValue::Local(LocalId(1))],
        dest: Some(LocalId(2)),
        target: Some(BlockId(1)),
        unwind: None,
    });
    let mut ret_block = MirBlock::new(BlockId(1));
    ret_block.terminator = Some(MirTerminator::Return(None));
    func.blocks = Some(vec![entry, ret_block]);
    mir.functions.push(func);

    let mut backend = GlslBackend::new();
    let err = backend
        .generate(&mir)
        .expect_err("2-arg fma must be rejected fail-closed by GlslBackend");
    assert!(
        err.to_string().contains("requires exactly 3 arguments"),
        "unexpected error message: {err}"
    );

    // Control B: Live glslangValidator rejects a broken 2-arg fma(...) GLSL shader fail-closed.
    if tool_available("glslangValidator", "--version") {
        let dir = make_temp_dir("planted_bad_glsl");
        let bad_glsl = dir.join("broken_fma.glsl");
        let bad_spv = dir.join("broken_fma.spv");
        std::fs::write(
            &bad_glsl,
            r#"#version 450
layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;
layout(std430, set = 0, binding = 0) buffer Out { float out_buf[]; };
void main() {
    uint i = gl_GlobalInvocationID.x;
    out_buf[i] = fma(1.0, 2.0);
}
"#,
        )
        .unwrap();
        let out = Command::new("glslangValidator")
            .args([
                "-V",
                "-S",
                "comp",
                bad_glsl.to_str().unwrap(),
                "-o",
                bad_spv.to_str().unwrap(),
            ])
            .output()
            .expect("run glslangValidator on broken fma");
        assert!(
            !out.status.success(),
            "glslangValidator unexpectedly accepted broken 2-arg fma shader"
        );
    }
}
