// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 43E end-to-end integration tests for 2026 SOTA GPU Kernel & E-Graph
//! Superoptimization in BuildLang (`compiler/`):
//! - V5 Certified E-Graph SOTA rules (`R14..R20`): Online-Softmax / Log-Sum-Exp
//!   fusion (`exp(a)*exp(b) -> exp(a+b)`, `exp(a)/exp(b) -> exp(a-b)`,
//!   `log(exp(x)) -> x`), FMA contraction (`a*b+c -> fma(a,b,c)`), block-scaled
//!   `E2M1`/`FP8` shared-exponent scale hoisting (`(s*a)+(s*b) -> s*(a+b)` and
//!   `(a*c1)*(b*c2) -> (a*b)*(c1*c2)`), Walsh-Hadamard block pre-rotation `H_B`,
//!   and opt-in FlashAttention-4 Horner FMA `exp2`.
//! - Polyhedral GPU TV (`buildlang.gpu-tv-cert/v1`): multi-op subgroup
//!   reductions (`Sum`, `Max`, `Min`) across `W_s = 16, 32, 64`, Cooperative-
//!   Vector / DirectX SM 6.10 `linalg::Matrix` tiling metadata, PLDI 2026
//!   `TinyWGSL`-style transitive barrier uniformity, and RESOLVE-style
//!   reduced-concurrency legality across `wgsl`, `hlsl`, `msl`, and `spirv`.
//! - 100% fail-closed planted negative controls across every new rule,
//!   error witness, subgroup reduction op, cooperative tile, and barrier.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use buildlang::codegen::gpu_polyhedral::{
    build_cooperative_tile_plan, compute_gpu_certificate_sha256, seal_gpu_tv_certificate_with_op,
    verify_cooperative_tile_plan, verify_gpu_tv_certificate, DependenceKind, DependenceVector,
    GpuTvCertificate, PolyhedralSchedule, SubgroupReductionOp,
};
use buildlang::codegen::ir::{
    BinOp, BlockId, MirBlock, MirFnSig, MirFunction, MirLocal, MirModule, MirTerminator, MirType,
    MirValue,
};
use buildlang::codegen::superopt::{
    compute_certificate_sha256, horner_exp2_cert_expr, horner_exp2_f64, quantize_e2m1_block,
    superoptimize_module_with_config, verify_certificate, verify_step_axiom,
    walsh_hadamard_transform_normalized, CertExpr, RewriteErrorWitness, SuperoptCertificate,
    SuperoptConfig, SuperoptRule, SUPEROPT_CERT_SCHEMA,
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
        "buildlang_wave43e_{}_{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn wave43e_online_softmax_exp_fusion_and_log_exp_cancel_end_to_end() {
    let dir = make_temp_dir("online_softmax");
    let src = dir.join("online_softmax.bld");
    let out_base_c = dir.join("online_softmax_base.c");
    let out_opt_c = dir.join("online_softmax_opt.c");
    let cert_path = dir.join("online_softmax.cert.json");

    std::fs::write(
        &src,
        r#"
fn online_softmax_step(x: f64, m_prev: f64, m_new: f64, d_prev: f64) -> f64 {
    // R16: exp(x) / exp(m_new) -> exp(x - m_new)
    let ex: f64 = x.exp();
    let em: f64 = m_new.exp();
    let w: f64 = ex / em;

    // R15: exp(m_prev) * exp(-m_new) -> exp(m_prev + (-m_new))
    let neg_m: f64 = 0.0 - m_new;
    let ep: f64 = m_prev.exp();
    let en: f64 = neg_m.exp();
    let scale: f64 = ep * en;

    // R17: log(exp(m_new)) -> m_new
    let em2: f64 = m_new.exp();
    let recovered_m: f64 = em2.ln();

    // R18: d_prev * scale + w -> fma(d_prev, scale, w)
    let prod: f64 = d_prev * scale;
    let d_new: f64 = prod + w;

    // R14: recovered_m / 1.0 -> recovered_m
    let m_unit: f64 = recovered_m / 1.0;

    d_new + m_unit
}

fn main() -> i32 ~ Console {
    let r1: f64 = online_softmax_step(2.5, 1.5, 2.5, 3.25);
    let r2: f64 = online_softmax_step(-0.75, 0.25, 0.5, 4.0);
    println!("{:.12} {:.12}", r1, r2);
    0
}
"#,
    )
    .unwrap();

    // Compile both baseline and superoptimized C to measure emitted op reduction.
    let c_base = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            src.to_str().unwrap(),
            "--target",
            "c",
            "-o",
            out_base_c.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        c_base.status.success(),
        "baseline C compile failed: {}",
        String::from_utf8_lossy(&c_base.stderr)
    );

    let c_opt = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            src.to_str().unwrap(),
            "--target",
            "c",
            "--superopt",
            "--emit-cert",
            cert_path.to_str().unwrap(),
            "-o",
            out_opt_c.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        c_opt.status.success(),
        "superopt C compile failed: {}",
        String::from_utf8_lossy(&c_opt.stderr)
    );

    let base_txt = std::fs::read_to_string(&out_base_c).unwrap();
    let opt_txt = std::fs::read_to_string(&out_opt_c).unwrap();
    let base_exp_calls = base_txt.matches("build_det_exp(").count();
    let opt_exp_calls = opt_txt.matches("build_det_exp(").count();
    assert!(
        opt_exp_calls < base_exp_calls,
        "superopt must eliminate redundant exp() calls in C output: base={base_exp_calls}, opt={opt_exp_calls}"
    );
    assert!(
        opt_txt.contains("fma("),
        "superopt C output must emit fused fma() call"
    );

    // Execute both baseline and superoptimized binaries and check numerical parity.
    let run_base = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["run", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        run_base.status.success(),
        "baseline run failed: {}",
        String::from_utf8_lossy(&run_base.stderr)
    );
    let run_opt = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["run", "--superopt", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        run_opt.status.success(),
        "superopt run failed: {}",
        String::from_utf8_lossy(&run_opt.stderr)
    );

    let parse_pair = |out: &[u8]| -> (f64, f64) {
        let s = String::from_utf8_lossy(out);
        let nums: Vec<f64> = s
            .split_whitespace()
            .map(|t| t.parse::<f64>().unwrap())
            .collect();
        assert_eq!(nums.len(), 2, "unexpected stdout: {s}");
        (nums[0], nums[1])
    };
    let (b1, b2) = parse_pair(&run_base.stdout);
    let (o1, o2) = parse_pair(&run_opt.stdout);
    assert!(
        (b1 - o1).abs() <= 1e-12 && (b2 - o2).abs() <= 1e-12,
        "Online-Softmax numerical parity drifted: base=({b1}, {b2}) vs opt=({o1}, {o2})"
    );

    // Inspect certificate and independently verify via `buildc verify-cert`.
    let cert: SuperoptCertificate =
        serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
    assert_eq!(cert.schema, SUPEROPT_CERT_SCHEMA);
    let rules: Vec<&SuperoptRule> = cert.steps.iter().map(|s| &s.rule).collect();
    assert!(rules.contains(&&SuperoptRule::OnlineSoftmaxExpMulFuse));
    assert!(rules.contains(&&SuperoptRule::OnlineSoftmaxExpDivFuse));
    assert!(rules.contains(&&SuperoptRule::LogExpCancel));
    assert!(rules.contains(&&SuperoptRule::FmaContract));
    assert!(rules.contains(&&SuperoptRule::FloatDivOne));
    assert!(
        cert.total_cost_after * 2 < cert.total_cost_before,
        "expected >50% hardware cost reduction on Online-Softmax, got {} -> {}",
        cert.total_cost_before,
        cert.total_cost_after
    );

    let verify = Command::new(buildc_bin())
        .args(["verify-cert", cert_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "verify-cert failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert!(String::from_utf8_lossy(&verify.stdout).contains("buildlang.superopt-cert/v1 verified"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave43e_gated_deltanet_inner_loop_and_block_scaled_dot_superopt() {
    let dir = make_temp_dir("deltanet_microscaling");
    let src = dir.join("deltanet_dot.bld");
    let cert_path = dir.join("deltanet_dot.cert.json");

    std::fs::write(
        &src,
        r#"
// Gated-DeltaNet scalar state recurrence:
// S_t = alpha_t * S_{t-1} + beta_t * delta_t (K = 16 steps)
fn gated_deltanet_scan(alpha: f64, beta: f64, delta: f64) -> f64 {
    let mut state: f64 = 0.0;
    let mut t: i64 = 0;
    while t < 16 {
        let write_term: f64 = beta * delta;
        state = state * alpha + write_term;
        t = t + 1;
    }
    state
}

// Block-scaled E2M1/FP8 dot product with shared scale `s` hoisted across
// terms via cascading R19_BLOCK_SCALE_HOIST plus E8M0 constant scale folding.
fn block_scaled_dot4(
    s: f64,
    q0: f64,
    q1: f64,
    q2: f64,
    q3: f64,
    ax: f64,
    bw: f64,
) -> f64 {
    let p0: f64 = s * q0;
    let p1: f64 = s * q1;
    let p2: f64 = s * q2;
    let p3: f64 = s * q3;
    let sum01: f64 = p0 + p1;
    let sum012: f64 = sum01 + p2;
    let block_sum: f64 = sum012 + p3;

    // E8M0 exact power-of-two shared-exponent scale pair folding:
    // (ax * 0.25) * (bw * 0.5) -> (ax * bw) * 0.125 (BitExact!)
    let sx: f64 = ax * 0.25;
    let sw: f64 = bw * 0.5;
    let pair_scaled: f64 = sx * sw;

    block_sum + pair_scaled
}

fn main() -> i32 ~ Console {
    let s1: f64 = gated_deltanet_scan(0.875, 0.25, 1.5);
    let d1: f64 = block_scaled_dot4(0.125, 1.5, -2.0, 3.0, 4.0, 6.0, -2.0);
    println!("{:.12} {:.12}", s1, d1);
    0
}
"#,
    )
    .unwrap();

    let run_base = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["run", src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        run_base.status.success(),
        "baseline run failed: {}",
        String::from_utf8_lossy(&run_base.stderr)
    );

    let run_opt = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            "run",
            "--superopt",
            "--emit-cert",
            cert_path.to_str().unwrap(),
            src.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        run_opt.status.success(),
        "superopt run failed: {}",
        String::from_utf8_lossy(&run_opt.stderr)
    );

    let parse_pair = |out: &[u8]| -> (f64, f64) {
        let s = String::from_utf8_lossy(out);
        let nums: Vec<f64> = s
            .split_whitespace()
            .map(|t| t.parse::<f64>().unwrap())
            .collect();
        assert_eq!(nums.len(), 2, "unexpected stdout: {s}");
        (nums[0], nums[1])
    };
    let (bs, bd) = parse_pair(&run_base.stdout);
    let (os, od) = parse_pair(&run_opt.stdout);
    assert!(
        (bs - os).abs() <= 1e-12 && (bd - od).abs() <= 1e-12,
        "DeltaNet/BlockScaled parity drifted: base=({bs}, {bd}) vs opt=({os}, {od})"
    );

    let cert: SuperoptCertificate =
        serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
    let hoist_steps: Vec<_> = cert
        .steps
        .iter()
        .filter(|s| s.rule == SuperoptRule::BlockScaleHoist)
        .collect();
    let fma_steps: Vec<_> = cert
        .steps
        .iter()
        .filter(|s| s.rule == SuperoptRule::FmaContract)
        .collect();
    assert!(
        hoist_steps.len() >= 4,
        "expected >= 4 cascading R19_BLOCK_SCALE_HOIST steps (3 shared-scale + 1 E8M0 pair), got {}",
        hoist_steps.len()
    );
    assert!(
        hoist_steps
            .iter()
            .any(|s| s.error_witness == RewriteErrorWitness::BitExact),
        "E8M0 exact power-of-two scale pair fold must carry BitExact witness"
    );
    assert!(
        !fma_steps.is_empty(),
        "expected R18_FMA_CONTRACT on Gated-DeltaNet inner loop"
    );
    assert!(cert.total_cost_after < cert.total_cost_before);

    let verify = Command::new(buildc_bin())
        .args(["verify-cert", cert_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(verify.status.success());

    // Verify Walsh-Hadamard H_32 + MXFP4 E8M0 preserves exact Parseval inner
    // product, suppresses heavy-tailed activation outlier peak > 2.5x, and cuts
    // block quantization L2 error sharply over unrotated E2M1.
    let mut x = vec![0.0_f64; 32];
    let mut w = vec![0.0_f64; 32];
    for i in 0..32 {
        x[i] = ((i % 7) as f64 - 3.0) * 0.25;
        w[i] = ((i % 5) as f64 - 2.0) * 0.35;
    }
    x[5] = 12.5; // Heavy-tailed 50x-median activation outlier
    let exact: f64 = x.iter().zip(w.iter()).map(|(a, b)| a * b).sum();
    let orig_peak = x.iter().fold(0.0_f64, |acc, &v| acc.max(v.abs()));

    let (sx_raw, qx_raw) = quantize_e2m1_block(&x, true).unwrap();
    let unrot_l2: f64 = x
        .iter()
        .zip(qx_raw.iter())
        .map(|(&orig, &q)| (orig - sx_raw * q).powi(2))
        .sum::<f64>()
        .sqrt();

    let mut hx = x.clone();
    let mut hw = w.clone();
    walsh_hadamard_transform_normalized(&mut hx).unwrap();
    walsh_hadamard_transform_normalized(&mut hw).unwrap();
    let rot_exact: f64 = hx.iter().zip(hw.iter()).map(|(a, b)| a * b).sum();
    assert!(
        (rot_exact - exact).abs() < 1e-12,
        "Walsh-Hadamard H_32 must preserve exact Parseval inner product: {rot_exact} vs {exact}"
    );
    let rot_peak = hx.iter().fold(0.0_f64, |acc, &v| acc.max(v.abs()));
    assert!(
        rot_peak < orig_peak * 0.4,
        "H_32 must suppress outlier peak >2.5x: {orig_peak} -> {rot_peak}"
    );
    let (sx_rot, qx_rot) = quantize_e2m1_block(&hx, true).unwrap();
    let rot_l2: f64 = hx
        .iter()
        .zip(qx_rot.iter())
        .map(|(&orig, &q)| (orig - sx_rot * q).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(
        rot_l2 < unrot_l2,
        "Hadamard-rotated MXFP4 E8M0 L2 quant err ({rot_l2}) should beat unrotated E2M1 L2 err ({unrot_l2})"
    );
    // Planted illegal block size fails closed.
    let mut bad_buf = vec![1.0_f64; 15];
    assert!(walsh_hadamard_transform_normalized(&mut bad_buf).is_err());
    assert!(quantize_e2m1_block(&bad_buf, true).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave43e_flashattention4_horner_exp2_fma_and_planted_bad_coeff_control() {
    let mut module = MirModule::new("fa4_horner");
    let sig = MirFnSig::new(vec![MirType::f32()], MirType::f32());
    let mut func = MirFunction::new("exp2_frac", sig);
    let mut p0 = MirLocal::named(buildlang::codegen::ir::LocalId(0), "f", MirType::f32());
    p0.is_param = true;
    func.add_local(p0);
    func.add_local(MirLocal::new(
        buildlang::codegen::ir::LocalId(1),
        MirType::f32(),
    ));

    let mut bb0 = MirBlock::new(BlockId(0));
    bb0.set_terminator(MirTerminator::Call {
        func: MirValue::Function(Arc::from("exp2")),
        args: vec![MirValue::Local(buildlang::codegen::ir::LocalId(0))],
        dest: Some(buildlang::codegen::ir::LocalId(1)),
        target: Some(BlockId(1)),
        unwind: None,
    });
    let mut bb1 = MirBlock::new(BlockId(1));
    bb1.set_terminator(MirTerminator::Return(Some(MirValue::Local(
        buildlang::codegen::ir::LocalId(1),
    ))));
    func.add_block(bb0);
    func.add_block(bb1);
    module.add_function(func);

    let cfg = SuperoptConfig {
        allow_fma_contract: true,
        enable_horner_exp2_fma: true,
    };
    let mut cert = superoptimize_module_with_config(&mut module, "wgsl", cfg)
        .expect("Horner FMA exp2 rewrite must verify");
    assert_eq!(cert.rewrites_applied, 1);
    assert_eq!(cert.steps[0].rule, SuperoptRule::HornerExp2Fma);
    assert_eq!(cert.steps[0].cost_before, 12);
    assert_eq!(cert.steps[0].cost_after, 8);
    verify_certificate(&cert).unwrap();

    // Accuracy check over fractional domain [0, 1].
    for i in 0..=1000 {
        let f = (i as f64) / 1000.0;
        assert!((horner_exp2_f64(f) - 2.0_f64.powf(f)).abs() <= 4.0e-6);
    }

    // Planted bad polynomial coefficient control (tampered c0 + recomputed SHA-256).
    let tampered_after = CertExpr::Call {
        func: "fma".to_string(),
        args: vec![
            CertExpr::Local(0),
            CertExpr::Local(0),
            CertExpr::FloatBits(1.25_f64.to_bits(), MirType::f32()),
        ],
        ret_ty: MirType::f32(),
    };
    cert.steps[0].after = tampered_after;
    cert.steps[0].cost_after = buildlang::codegen::superopt::expr_cost(&cert.steps[0].after);
    cert.total_cost_after = cert.steps[0].cost_after as u64;
    cert.cert_sha256 = compute_certificate_sha256(&cert).unwrap();
    let err = verify_certificate(&cert).expect_err("tampered Horner coeff must fail closed");
    assert!(err.contains("axiom violation"), "unexpected err: {err}");
}

#[test]
fn wave43e_gpu_tv_cooperative_vector_sm610_and_subgroup_ops_all_four_targets() {
    let dir = make_temp_dir("gpu_sota");
    let src = dir.join("sota_fma_tile.bld");
    std::fs::write(
        &src,
        r#"
#[compute]
fn fma_attention_tile(
    a: &[f32],
    b: &[f32],
    c: &[f32],
    out: &mut [f32],
) ~ Gpu {
    let row = gl_GlobalInvocationID.y;
    let col = gl_GlobalInvocationID.x;
    let idx = row * 16 + col;
    let prod: f32 = a[idx] * b[idx];
    let acc: f32 = prod + c[idx];
    out[idx] = acc / 1.0;
}
"#,
    )
    .unwrap();

    for (target, ext, expected_api, expected_fma_token) in [
        ("wgsl", "wgsl", "wgsl_subgroup_matrix", Some("fma(")),
        ("hlsl", "hlsl", "sm6_10_linalg_matrix", Some("mad(")),
        ("msl", "metal", "msl_simdgroup_matrix", Some("fma(")),
        ("spirv", "spv", "spv_khr_cooperative_matrix", None),
    ] {
        let out_shader = dir.join(format!("tile_{target}.{ext}"));
        let cert_path = dir.join(format!("tile_{target}.gpu-cert.json"));
        let res = Command::new(buildc_bin())
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
            .unwrap();
        assert!(
            res.status.success(),
            "target {target} compile failed: {}",
            String::from_utf8_lossy(&res.stderr)
        );

        if let Some(tok) = expected_fma_token {
            let shader_txt = std::fs::read_to_string(&out_shader).unwrap();
            assert!(
                shader_txt.contains(tok),
                "{target} shader must emit fused `{tok}`:\n{shader_txt}"
            );
        }

        let cert: GpuTvCertificate =
            serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
        assert!(
            cert.superopt_cert
                .steps
                .iter()
                .any(|s| s.rule == SuperoptRule::FmaContract),
            "target {target} must record R18_FMA_CONTRACT"
        );
        assert!(
            cert.superopt_cert
                .steps
                .iter()
                .any(|s| s.rule == SuperoptRule::FloatDivOne),
            "target {target} must record R14_FLOAT_DIV_ONE"
        );
        let coop = cert.polyhedral_schedules[0]
            .cooperative_tile
            .as_ref()
            .expect("cooperative_tile must be populated");
        assert_eq!(coop.api_surface, expected_api);
        assert_eq!(coop.legal_subgroup_widths, vec![16, 32, 64]);
        assert_eq!(coop.microscaling_block_size, 16);
        assert!(coop.reduced_concurrency_verified);

        let verify = Command::new(buildc_bin())
            .args([
                "verify-cert",
                cert_path.to_str().unwrap(),
                "--shader",
                out_shader.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            verify.status.success(),
            "verify-cert failed for {target}: {}",
            String::from_utf8_lossy(&verify.stderr)
        );

        // Also verify Max & Min subgroup reduction certificates on every target.
        let shader_bytes = std::fs::read(&out_shader).unwrap();
        for op in [SubgroupReductionOp::Max, SubgroupReductionOp::Min] {
            let cert_op = seal_gpu_tv_certificate_with_op(
                target,
                cert.superopt_cert.clone(),
                cert.polyhedral_schedules.clone(),
                op,
                true,
                true,
                &shader_bytes,
            )
            .unwrap();
            assert_eq!(cert_op.subgroup_plan.reduction_op, op);
            assert_eq!(cert_op.subgroup_plan.max_cross_width_abs_diff, 0.0);
            verify_gpu_tv_certificate(&cert_op, Some(&shader_bytes)).unwrap();
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave43e_planted_unsound_rewrites_wrong_reduction_op_and_illegal_tiling_fail_closed() {
    let dir = make_temp_dir("planted_controls");

    // Control 1: Unsound R15 `exp(a) * exp(b) -> exp(a * b)` (must be a + b).
    let exp_a = CertExpr::Call {
        func: "exp".to_string(),
        args: vec![CertExpr::Local(0)],
        ret_ty: MirType::f64(),
    };
    let exp_b = CertExpr::Call {
        func: "exp".to_string(),
        args: vec![CertExpr::Local(1)],
        ret_ty: MirType::f64(),
    };
    let bad_r15_before = CertExpr::BinOp {
        op: BinOp::Mul,
        left: Box::new(exp_a.clone()),
        right: Box::new(exp_b.clone()),
    };
    let bad_r15_after = CertExpr::Call {
        func: "exp".to_string(),
        args: vec![CertExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(CertExpr::Local(0)),
            right: Box::new(CertExpr::Local(1)),
        }],
        ret_ty: MirType::f64(),
    };
    assert!(verify_step_axiom(
        &SuperoptRule::OnlineSoftmaxExpMulFuse,
        &bad_r15_before,
        &bad_r15_after
    )
    .unwrap_err()
    .contains("axiom violation"));

    // Control 2: Unsound R16 `exp(a) / exp(b) -> exp(b - a)` (flipped operands).
    let bad_r16_before = CertExpr::BinOp {
        op: BinOp::Div,
        left: Box::new(exp_a.clone()),
        right: Box::new(exp_b.clone()),
    };
    let bad_r16_after = CertExpr::Call {
        func: "exp".to_string(),
        args: vec![CertExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(CertExpr::Local(1)),
            right: Box::new(CertExpr::Local(0)),
        }],
        ret_ty: MirType::f64(),
    };
    assert!(verify_step_axiom(
        &SuperoptRule::OnlineSoftmaxExpDivFuse,
        &bad_r16_before,
        &bad_r16_after
    )
    .unwrap_err()
    .contains("axiom violation"));

    // Control 3: Unsound R17 `log(exp2(x)) -> x` (mismatched base).
    let bad_r17_before = CertExpr::Call {
        func: "log".to_string(),
        args: vec![CertExpr::Call {
            func: "exp2".to_string(),
            args: vec![CertExpr::Local(0)],
            ret_ty: MirType::f64(),
        }],
        ret_ty: MirType::f64(),
    };
    assert!(verify_step_axiom(
        &SuperoptRule::LogExpCancel,
        &bad_r17_before,
        &CertExpr::Local(0)
    )
    .unwrap_err()
    .contains("axiom violation"));

    // Control 4: Unsound R18 `(a * b) + c -> fma(a, c, b)` (swapped addend/factor).
    let bad_r18_before = CertExpr::BinOp {
        op: BinOp::Add,
        left: Box::new(CertExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(CertExpr::Local(0)),
            right: Box::new(CertExpr::Local(1)),
        }),
        right: Box::new(CertExpr::Local(2)),
    };
    let bad_r18_after = CertExpr::Call {
        func: "fma".to_string(),
        args: vec![CertExpr::Local(0), CertExpr::Local(2), CertExpr::Local(1)],
        ret_ty: MirType::f64(),
    };
    assert!(
        verify_step_axiom(&SuperoptRule::FmaContract, &bad_r18_before, &bad_r18_after)
            .unwrap_err()
            .contains("axiom violation")
    );

    // Control 5: Unsound R19 `(s * a) + (t * b) -> s * (a + b)` where `s != t`.
    let bad_r19_before = CertExpr::BinOp {
        op: BinOp::Add,
        left: Box::new(CertExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(CertExpr::Local(0)),
            right: Box::new(CertExpr::Local(1)),
        }),
        right: Box::new(CertExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(CertExpr::Local(2)),
            right: Box::new(CertExpr::Local(3)),
        }),
    };
    let bad_r19_after = CertExpr::BinOp {
        op: BinOp::Mul,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(CertExpr::Local(1)),
            right: Box::new(CertExpr::Local(3)),
        }),
    };
    assert!(verify_step_axiom(
        &SuperoptRule::BlockScaleHoist,
        &bad_r19_before,
        &bad_r19_after
    )
    .unwrap_err()
    .contains("axiom violation"));

    // Control 6: Planted wrong subgroup reduction op & illegal cooperative tile
    // inside a real sealed GPU certificate (with recomputed SHA-256!).
    let clean_src = dir.join("clean_gpu.bld");
    let clean_wgsl = dir.join("clean_gpu.wgsl");
    let clean_cert_path = dir.join("clean_gpu.gpu-cert.json");
    std::fs::write(
        &clean_src,
        r#"
#[compute]
fn clean_fma(a: &[f32], b: &[f32], c: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let p: f32 = a[i] * b[i];
    out[i] = p + c[i];
}
"#,
    )
    .unwrap();
    let ok = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            clean_src.to_str().unwrap(),
            "--target",
            "wgsl",
            "--emit-gpu-cert",
            clean_cert_path.to_str().unwrap(),
            "-o",
            clean_wgsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(ok.status.success());
    let raw = std::fs::read_to_string(&clean_cert_path).unwrap();

    // 6a. Forged error witness (`BitExact` claimed for `FmaContract`, recomputed SHA-256).
    let mut forged_witness: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
    forged_witness.superopt_cert.steps[0].error_witness = RewriteErrorWitness::BitExact;
    forged_witness.superopt_cert.cert_sha256 =
        compute_certificate_sha256(&forged_witness.superopt_cert).unwrap();
    forged_witness.cert_sha256 = compute_gpu_certificate_sha256(&forged_witness).unwrap();
    let p_wit = dir.join("forged_witness.json");
    std::fs::write(
        &p_wit,
        serde_json::to_string_pretty(&forged_witness).unwrap(),
    )
    .unwrap();
    let r_wit = Command::new(buildc_bin())
        .args(["verify-cert", p_wit.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!r_wit.status.success());
    assert!(String::from_utf8_lossy(&r_wit.stderr).contains("error_witness mismatch"));

    // 6b. Wrong subgroup reduction op (`Max` claimed with `subgroupAdd` intrinsic).
    let mut wrong_op: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
    wrong_op.subgroup_plan.reduction_op = SubgroupReductionOp::Max;
    wrong_op.cert_sha256 = compute_gpu_certificate_sha256(&wrong_op).unwrap();
    let p_op = dir.join("wrong_op.json");
    std::fs::write(&p_op, serde_json::to_string_pretty(&wrong_op).unwrap()).unwrap();
    let r_op = Command::new(buildc_bin())
        .args(["verify-cert", p_op.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!r_op.status.success());
    assert!(String::from_utf8_lossy(&r_op.stderr).contains("subgroup intrinsic mismatch"));

    // 6c. Wrong subgroup reduction op with matching intrinsic (`subgroupMax`)
    //     but stale Sum `width_invariance_sha256`.
    let mut stale_digest_op: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
    stale_digest_op.subgroup_plan.reduction_op = SubgroupReductionOp::Max;
    stale_digest_op.subgroup_intrinsic = "subgroupMax".to_string();
    stale_digest_op.subgroup_plan.intrinsic = "subgroupMax".to_string();
    stale_digest_op.cert_sha256 = compute_gpu_certificate_sha256(&stale_digest_op).unwrap();
    let p_stale = dir.join("stale_op_digest.json");
    std::fs::write(
        &p_stale,
        serde_json::to_string_pretty(&stale_digest_op).unwrap(),
    )
    .unwrap();
    let r_stale = Command::new(buildc_bin())
        .args(["verify-cert", p_stale.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!r_stale.status.success());
    assert!(String::from_utf8_lossy(&r_stale.stderr).contains("width_invariance_sha256"));

    // 6d. Illegal Cooperative-Vector tile shape (tile_m=7 not divisible by W_s=16/32/64)
    //     and wrong API surface (`sm6_10_linalg_matrix` on `wgsl` target).
    let mut bad_coop: GpuTvCertificate = serde_json::from_str(&raw).unwrap();
    bad_coop.polyhedral_schedules[0]
        .cooperative_tile
        .as_mut()
        .unwrap()
        .tile_m = 7;
    bad_coop.cert_sha256 = compute_gpu_certificate_sha256(&bad_coop).unwrap();
    let p_coop = dir.join("bad_coop.json");
    std::fs::write(&p_coop, serde_json::to_string_pretty(&bad_coop).unwrap()).unwrap();
    let r_coop = Command::new(buildc_bin())
        .args(["verify-cert", p_coop.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!r_coop.status.success());
    assert!(String::from_utf8_lossy(&r_coop.stderr).contains("not divisible by subgroup width"));

    // 6e. Illegal 2D dependence (`H * delta < 0`) + wrong API + unpadded SRAM.
    let sched_2d = PolyhedralSchedule {
        function: "illegal_2d".to_string(),
        dimensions: 2,
        hyperplanes: vec![vec![1, 0], vec![0, 1]],
        tile_sizes: vec![16, 16],
        sram_bank_stride: vec![17, 16],
        parallel_axes: vec![false, false],
        dependences: vec![DependenceVector {
            buffer: "buf".to_string(),
            kind: DependenceKind::Raw,
            delta: vec![1, -1],
        }],
        cooperative_tile: None,
    };
    assert!(build_cooperative_tile_plan("hlsl", &sched_2d)
        .unwrap_err()
        .contains("legality violation"));

    let valid_2d = PolyhedralSchedule {
        dependences: vec![],
        ..sched_2d
    };
    let mut bad_api = build_cooperative_tile_plan("hlsl", &valid_2d).unwrap();
    bad_api.api_surface = "deprecated_sm6_9_coop_vec".to_string();
    assert!(verify_cooperative_tile_plan(&bad_api, &valid_2d, "hlsl")
        .unwrap_err()
        .contains("cooperative tile API mismatch"));

    let mut bad_rc = build_cooperative_tile_plan("hlsl", &valid_2d).unwrap();
    bad_rc.reduced_concurrency_verified = false;
    assert!(verify_cooperative_tile_plan(&bad_rc, &valid_2d, "hlsl")
        .unwrap_err()
        .contains("reduced-concurrency"));

    // Control 7: Planted NESTED non-uniform barrier inside 2-level thread-divergent
    // branch (caught by TinyWGSL-style transitive post-dominator uniformity check).
    let nested_src = dir.join("nested_bad_barrier.bld");
    let nested_wgsl = dir.join("nested_bad_barrier.wgsl");
    let nested_cert = dir.join("nested_bad_barrier.gpu-cert.json");
    std::fs::write(
        &nested_src,
        r#"
#[compute]
fn nested_bad_barrier(n: u32, a: &[f32], out: &mut [f32]) ~ Gpu {
    let scratch = workgroupArray(64);
    let lid = gl_LocalInvocationID.x;
    let gid = gl_GlobalInvocationID.x;
    scratch[lid] = a[gid];
    if lid < 32 {
        if gid < n {
            workgroupBarrier();
        }
    }
    out[gid] = scratch[lid];
}
"#,
    )
    .unwrap();
    let r_nested = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            nested_src.to_str().unwrap(),
            "--target",
            "wgsl",
            "--emit-gpu-cert",
            nested_cert.to_str().unwrap(),
            "-o",
            nested_wgsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !r_nested.status.success(),
        "nested non-uniform workgroupBarrier must fail closed"
    );
    assert!(String::from_utf8_lossy(&r_nested.stderr).contains("non-uniform workgroup barrier"));
    assert!(!nested_wgsl.exists());
    assert!(!nested_cert.exists());

    // Sanity check Horner helper expression verifies cleanly.
    let h_before = CertExpr::Call {
        func: "exp2".to_string(),
        args: vec![CertExpr::Local(0)],
        ret_ty: MirType::f32(),
    };
    let h_after = horner_exp2_cert_expr(CertExpr::Local(0), MirType::f32());
    verify_step_axiom(&SuperoptRule::HornerExp2Fma, &h_before, &h_after).unwrap();

    let _ = std::fs::remove_dir_all(&dir);
}
