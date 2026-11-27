// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! Regression tests for Wave 69B GPU codegen fixes:
//! 1. `--target hlsl` lowers `#[compute]` kernels with `[numthreads(...)]`,
//!    `StructuredBuffer`/`RWStructuredBuffer`, `cbuffer`, `groupshared`,
//!    `SV_DispatchThreadID`, and `GroupMemoryBarrierWithGroupSync()`, passing
//!    `dxc -T cs_6_0` when `dxc` is available on PATH.
//! 2. `--target spirv` emits basic blocks in strict reverse post-order (RPO)
//!    so dominator blocks precede dominated merge/continuation blocks even on
//!    sequential tail `if`-without-`else` clamps, passing `spirv-val`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn temp_dir(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "buildlang_wave69b_{}_{}_{}",
        std::process::id(),
        n,
        label
    ));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn example_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("examples")
        .join("gpu")
        .join(name)
}

fn run_dxc_if_available(hlsl: &Path, entry: &str, out_dir: &Path) {
    let dxil = out_dir.join(format!("{entry}.dxil"));
    match Command::new("dxc")
        .args(["-T", "cs_6_0", "-E", entry])
        .arg(hlsl)
        .arg("-Fo")
        .arg(&dxil)
        .output()
    {
        Ok(out) => {
            assert!(
                out.status.success(),
                "dxc -T cs_6_0 failed for `{entry}`:\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
                fs::read_to_string(hlsl).unwrap_or_default()
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("failed to spawn dxc: {e}"),
    }
}

fn run_spirv_val_if_available(spv: &Path) {
    match Command::new("spirv-val").arg(spv).output() {
        Ok(out) => {
            assert!(
                out.status.success(),
                "spirv-val failed:\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("failed to spawn spirv-val: {e}"),
    }
}

const ONLINE_SOFTMAX_SRC: &str = r#"#[compute]
fn online_softmax(m_prev: f32, l_prev: f32, x_buf: &[f32], out_buf: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let x: f32 = x_buf[i];
    let mut m_new: f32 = m_prev;
    if x > m_new {
        m_new = x;
    }
    let u0: f32 = -(m_prev - m_new);
    let e_old: f32 = 1.0 / (1.0 + u0 + 0.5 * u0 * u0 + 0.16666667 * u0 * u0 * u0);
    let u1: f32 = -(x - m_new);
    let e_cur: f32 = 1.0 / (1.0 + u1 + 0.5 * u1 * u1 + 0.16666667 * u1 * u1 * u1);
    let l_new: f32 = l_prev * e_old + e_cur;
    out_buf[i] = e_cur / l_new;
}
"#;

const DELTANET_STEP_SRC: &str = r#"#[compute]
fn deltanet_step(alpha: f32, beta: f32, s_prev: &[f32], k_buf: &[f32], v_buf: &[f32], out_buf: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let s: f32 = s_prev[i];
    let k: f32 = k_buf[i];
    let v: f32 = v_buf[i];
    let err: f32 = v - s * k;
    out_buf[i] = alpha * s + beta * err * k;
}
"#;

const NTC610_SEQ_CLAMP_SRC: &str = r#"#[compute]
fn ntc610_decode(w0: f32, w1: f32, bias: f32, g0: &[f32], g1: &[f32], out_buf: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let z0: f32 = g0[i];
    let z1: f32 = g1[i];
    let h: f32 = w0 * z0 + w1 * z1 + bias;
    let mut h_act: f32 = 0.0;
    if h > 0.0 {
        h_act = h;
    }
    let mut y: f32 = 0.75 * h_act + 0.25 * z0;
    if y < 0.0 {
        y = 0.0;
    }
    if y > 1.0 {
        y = 1.0;
    }
    out_buf[i] = y;
}
"#;

const SPLAT_3DGS_TAIL_IF_SRC: &str = r#"#[compute]
fn splat_3dgs(inv_sx: f32, inv_sy: f32, opacity: f32, dx_buf: &[f32], dy_buf: &[f32], out_buf: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let dx: f32 = dx_buf[i];
    let dy: f32 = dy_buf[i];
    let q: f32 = 0.5 * (dx * dx * inv_sx + dy * dy * inv_sy);
    let g: f32 = 1.0 / (1.0 + q + 0.5 * q * q + 0.16666667 * q * q * q);
    let mut alpha: f32 = opacity * g;
    if alpha > 0.99 {
        alpha = 0.99;
    }
    out_buf[i] = alpha;
}
"#;

#[test]
fn hlsl_compute_saxpy_and_reduce_emit_valid_sm60_bindings() {
    let dir = temp_dir("hlsl_examples");

    // 1. saxpy.bld (parameter named `out` must be escaped to `_out`)
    let saxpy_hlsl = dir.join("saxpy.hlsl");
    let saxpy_cert = dir.join("saxpy_hlsl_cert.json");
    let out = buildc()
        .args(["--target", "hlsl", "--superopt", "--emit-gpu-cert"])
        .arg(&saxpy_cert)
        .arg("-o")
        .arg(&saxpy_hlsl)
        .arg(example_path("saxpy.bld"))
        .output()
        .expect("run buildc saxpy hlsl");
    assert!(
        out.status.success(),
        "saxpy hlsl failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let src = fs::read_to_string(&saxpy_hlsl).expect("read saxpy.hlsl");
    assert!(
        src.contains("[numthreads(64, 1, 1)]"),
        "missing numthreads:\n{src}"
    );
    assert!(
        src.contains("StructuredBuffer<float> a : register(t0);"),
        "{src}"
    );
    assert!(
        src.contains("RWStructuredBuffer<float> _out : register(u0);"),
        "{src}"
    );
    assert!(
        src.contains("cbuffer _Params_saxpy : register(b0)"),
        "{src}"
    );
    assert!(src.contains("SV_DispatchThreadID"), "{src}");
    assert!(
        src.contains("_out[i] ="),
        "missing real buffer store:\n{src}"
    );

    let vc = buildc()
        .arg("verify-cert")
        .arg(&saxpy_cert)
        .arg("--shader")
        .arg(&saxpy_hlsl)
        .output()
        .expect("verify-cert saxpy");
    assert!(
        vc.status.success(),
        "verify-cert failed: {}",
        String::from_utf8_lossy(&vc.stderr)
    );
    run_dxc_if_available(&saxpy_hlsl, "saxpy", &dir);

    // 2. reduce.bld (groupshared + GroupMemoryBarrierWithGroupSync + while loop)
    let reduce_hlsl = dir.join("reduce.hlsl");
    let reduce_cert = dir.join("reduce_hlsl_cert.json");
    let out_r = buildc()
        .args(["--target", "hlsl", "--superopt", "--emit-gpu-cert"])
        .arg(&reduce_cert)
        .arg("-o")
        .arg(&reduce_hlsl)
        .arg(example_path("reduce.bld"))
        .output()
        .expect("run buildc reduce hlsl");
    assert!(
        out_r.status.success(),
        "reduce hlsl failed: {}",
        String::from_utf8_lossy(&out_r.stderr)
    );
    let rsrc = fs::read_to_string(&reduce_hlsl).expect("read reduce.hlsl");
    assert!(rsrc.contains("groupshared float scratch[64];"), "{rsrc}");
    assert!(
        rsrc.contains("GroupMemoryBarrierWithGroupSync();"),
        "{rsrc}"
    );
    assert!(rsrc.contains("SV_GroupThreadID"), "{rsrc}");
    assert!(rsrc.contains("SV_GroupID"), "{rsrc}");
    run_dxc_if_available(&reduce_hlsl, "sum_reduce", &dir);
}

#[test]
fn hlsl_compute_four_flagship_kernels_with_clamps_pass_dxc() {
    let dir = temp_dir("hlsl_flagships");
    let cases = [
        ("online_softmax", ONLINE_SOFTMAX_SRC),
        ("deltanet_step", DELTANET_STEP_SRC),
        ("ntc610_decode", NTC610_SEQ_CLAMP_SRC),
        ("splat_3dgs", SPLAT_3DGS_TAIL_IF_SRC),
    ];
    for (entry, code) in cases {
        let bld = dir.join(format!("{entry}.bld"));
        let hlsl = dir.join(format!("{entry}.hlsl"));
        let cert = dir.join(format!("{entry}_hlsl_cert.json"));
        fs::write(&bld, code).expect("write bld");

        let out = buildc()
            .args(["--target", "hlsl", "--superopt", "--emit-gpu-cert"])
            .arg(&cert)
            .arg("-o")
            .arg(&hlsl)
            .arg(&bld)
            .output()
            .expect("run buildc hlsl");
        assert!(
            out.status.success(),
            "hlsl compile failed for {entry}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let vc = buildc()
            .arg("verify-cert")
            .arg(&cert)
            .arg("--shader")
            .arg(&hlsl)
            .output()
            .expect("verify-cert hlsl");
        assert!(
            vc.status.success(),
            "verify-cert failed for {entry}: {}",
            String::from_utf8_lossy(&vc.stderr)
        );
        run_dxc_if_available(&hlsl, entry, &dir);
    }
}

#[test]
fn spirv_sequential_and_tail_if_clamps_emit_dominator_order() {
    let dir = temp_dir("spirv_rpo");
    let cases = [
        ("online_softmax", ONLINE_SOFTMAX_SRC),
        ("deltanet_step", DELTANET_STEP_SRC),
        ("ntc610_decode", NTC610_SEQ_CLAMP_SRC),
        ("splat_3dgs", SPLAT_3DGS_TAIL_IF_SRC),
    ];
    for (entry, code) in cases {
        let bld = dir.join(format!("{entry}.bld"));
        let spv = dir.join(format!("{entry}.spv"));
        let cert = dir.join(format!("{entry}_spv_cert.json"));
        fs::write(&bld, code).expect("write bld");

        let out = buildc()
            .args(["--target", "spirv", "--superopt", "--emit-gpu-cert"])
            .arg(&cert)
            .arg("-o")
            .arg(&spv)
            .arg(&bld)
            .output()
            .expect("run buildc spirv");
        assert!(
            out.status.success(),
            "spirv compile failed for {entry}:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        run_spirv_val_if_available(&spv);
        let vc = buildc()
            .arg("verify-cert")
            .arg(&cert)
            .arg("--shader")
            .arg(&spv)
            .output()
            .expect("verify-cert spirv");
        assert!(
            vc.status.success(),
            "verify-cert failed for {entry}: {}",
            String::from_utf8_lossy(&vc.stderr)
        );
    }
}

#[test]
fn planted_broken_compute_and_forged_gpucert_fail_closed() {
    let dir = temp_dir("planted_controls");

    // Planted control 1: syntax-broken compute shader rejected fail-closed
    let broken = dir.join("broken.bld");
    fs::write(
        &broken,
        "#[compute]\nfn bad(a: &[f32], out_buf: &mut [f32]) ~ Gpu {\n    let i = gl_GlobalInvocationID.x;\n    out_buf[i] = a[i] + ;\n}\n",
    )
    .expect("write broken");
    let out_bad = buildc()
        .args(["--target", "hlsl", "-o"])
        .arg(dir.join("broken.hlsl"))
        .arg(&broken)
        .output()
        .expect("run buildc broken");
    assert!(
        !out_bad.status.success(),
        "planted syntax-broken compute shader must fail closed"
    );

    // Planted control 2: forged shader_sha256 in gpu-tv-cert rejected fail-closed
    let good = dir.join("good.bld");
    let hlsl = dir.join("good.hlsl");
    let cert = dir.join("good_cert.json");
    fs::write(&good, DELTANET_STEP_SRC).expect("write good");
    let ok = buildc()
        .args(["--target", "hlsl", "--superopt", "--emit-gpu-cert"])
        .arg(&cert)
        .arg("-o")
        .arg(&hlsl)
        .arg(&good)
        .output()
        .expect("build good");
    assert!(ok.status.success());

    let mut cert_txt = fs::read_to_string(&cert).expect("read cert");
    let needle = "\"shader_sha256\": \"";
    let pos = cert_txt.find(needle).expect("shader_sha256 field") + needle.len();
    let replacement = if &cert_txt[pos..pos + 1] == "0" {
        "1"
    } else {
        "0"
    };
    cert_txt.replace_range(pos..pos + 1, replacement);
    let forged = dir.join("forged_cert.json");
    fs::write(&forged, cert_txt).expect("write forged");

    let vc = buildc()
        .arg("verify-cert")
        .arg(&forged)
        .arg("--shader")
        .arg(&hlsl)
        .output()
        .expect("verify-cert forged");
    assert!(
        !vc.status.success(),
        "forged shader_sha256 gpu-tv-cert must fail closed"
    );
}
