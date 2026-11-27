// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 83C integration tests auditing and hardening W3C WGSL (`wgsl.rs`) and
//! Apple Metal 3.1 (`msl.rs`) compute codegen across all 5 official GPU
//! examples (`vec_add`, `saxpy`, `matmul`, `reduce`, `stencil`), all 6 Wave 82F
//! compute kernels (`pbr_ggx_brdf`, `ntc610_decode`, `splat_3dgs_project`,
//! `jonswap_wave`, `indirect_frustum_cull`, `sdf_raymarch`), live `naga` W3C
//! WGSL validation + WGSL-to-SPIR-V/MSL cross-translation + `spirv-val`,
//! `buildc verify-cert --shader`, and planted fail-closed negative controls.

use std::path::{Path, PathBuf};
use std::process::Command;

use buildlang::codegen::gpu_polyhedral::{GpuTvCertificate, GPU_TV_CERT_SCHEMA};

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

fn gpu_example(name: &str) -> PathBuf {
    repo_root().join("examples").join("gpu").join(name)
}

fn make_temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "buildlang_wave83c_{}_{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tool_available(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn validate_wgsl_with_naga_if_available(label: &str, wgsl_path: &Path, dir: &Path) {
    if !tool_available("naga", "--version") {
        eprintln!("skipping naga in {label}: naga not on PATH");
        return;
    }
    let check = Command::new("naga")
        .arg(wgsl_path)
        .output()
        .expect("run naga validate");
    assert!(
        check.status.success(),
        "naga rejected {label}:\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr),
        std::fs::read_to_string(wgsl_path).unwrap_or_default()
    );

    let spv_out = dir.join(format!("{label}.naga.spv"));
    let to_spv = Command::new("naga")
        .args([wgsl_path.to_str().unwrap(), spv_out.to_str().unwrap()])
        .output()
        .expect("run naga WGSL->SPV");
    assert!(
        to_spv.status.success(),
        "naga WGSL->SPV failed for {label}:\n{}",
        String::from_utf8_lossy(&to_spv.stderr)
    );

    let msl_out = dir.join(format!("{label}.naga.metal"));
    let to_msl = Command::new("naga")
        .args([
            "--metal-version",
            "3.1",
            wgsl_path.to_str().unwrap(),
            msl_out.to_str().unwrap(),
        ])
        .output()
        .expect("run naga WGSL->MSL 3.1");
    assert!(
        to_msl.status.success(),
        "naga WGSL->MSL 3.1 failed for {label}:\n{}",
        String::from_utf8_lossy(&to_msl.stderr)
    );

    // Naga 30.0.1 emits ArrayStride on Workgroup arrays without the explicit
    // layout extension (upstream VUID-StandaloneSpirv-None-10684); BuildLang's
    // own SPIR-V backend validates `reduce.bld` directly with `spirv-val` in
    // `tests/gpu.rs`. Validate every non-workgroup naga SPIR-V output here.
    if label != "reduce" && tool_available("spirv-val", "--version") {
        let sval = Command::new("spirv-val")
            .args(["--target-env", "vulkan1.0", spv_out.to_str().unwrap()])
            .output()
            .expect("run spirv-val");
        assert!(
            sval.status.success(),
            "spirv-val rejected naga SPIR-V for {label}:\n{}",
            String::from_utf8_lossy(&sval.stderr)
        );
    }
}

const WAVE82F_KERNELS: [(&str, &str); 6] = [
    (
        "pbr_ggx_brdf_compute",
        r#"
#[compute(workgroup_size = [64, 1, 1])]
fn pbr_ggx_brdf_compute(
    roughness: f32,
    f0: f32,
    albedo: f32,
    ndl_buf: &[f32],
    ndv_buf: &[f32],
    ndh_buf: &[f32],
    out_buf: &mut [f32],
) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let mut ndl: f32 = ndl_buf[i];
    let mut ndv: f32 = ndv_buf[i];
    let mut ndh: f32 = ndh_buf[i];
    if ndl < 0.001 {
        ndl = 0.001;
    }
    if ndv < 0.001 {
        ndv = 0.001;
    }
    if ndh < 0.0 {
        ndh = 0.0;
    }
    let alpha: f32 = roughness * roughness;
    let a2: f32 = alpha * alpha;
    let denom_d: f32 = ndh * ndh * (a2 - 1.0) + 1.0;
    let inv_pi: f32 = 0.31830989;
    let d_ggx: f32 = a2 * inv_pi / (denom_d * denom_d + 0.00001);
    let om: f32 = 1.0 - ndh;
    let om2: f32 = om * om;
    let om5: f32 = om2 * om2 * om;
    let fresnel: f32 = f0 + (1.0 - f0) * om5;
    let k: f32 = (roughness + 1.0) * (roughness + 1.0) * 0.125;
    let g_smith: f32 = (ndl / (ndl * (1.0 - k) + k)) * (ndv / (ndv * (1.0 - k) + k));
    let spec: f32 = (d_ggx * fresnel * g_smith) / (4.0 * ndl * ndv + 0.0001);
    let mut radiance: f32 = ((1.0 - fresnel) * albedo * inv_pi + spec) * ndl;
    if radiance < 0.0 {
        radiance = 0.0;
    }
    if radiance > 1.0 {
        radiance = 1.0;
    }
    out_buf[i] = radiance;
}
"#,
    ),
    (
        "ntc610_decode_compute",
        r#"
#[compute(workgroup_size = [64, 1, 1])]
fn ntc610_decode_compute(
    w0: f32,
    w1: f32,
    bias: f32,
    g0_buf: &[f32],
    g1_buf: &[f32],
    out_buf: &mut [f32],
) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let z0: f32 = g0_buf[i];
    let z1: f32 = g1_buf[i];
    let h0: f32 = w0 * z0 + w1 * z1 + bias;
    let a0: f32 = 0.5 + (0.5 * h0) / (1.0 + h0 * h0);
    let h1: f32 = 0.85 * a0 - 0.35 * z1 + 0.15 * bias;
    let a1: f32 = 0.5 + (0.5 * h1) / (1.0 + h1 * h1);
    let mut y: f32 = 0.75 * a1 + 0.25 * z0;
    if y < 0.0 {
        y = 0.0;
    }
    if y > 1.0 {
        y = 1.0;
    }
    out_buf[i] = y;
}
"#,
    ),
    (
        "splat_3dgs_project_compute",
        r#"
#[compute(workgroup_size = [64, 1, 1])]
fn splat_3dgs_project_compute(
    inv_xx: f32,
    inv_xy: f32,
    inv_yy: f32,
    opacity: f32,
    dx_buf: &[f32],
    dy_buf: &[f32],
    out_buf: &mut [f32],
) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let dx: f32 = dx_buf[i];
    let dy: f32 = dy_buf[i];
    let mut q: f32 = 0.5 * (inv_xx * dx * dx + 2.0 * inv_xy * dx * dy + inv_yy * dy * dy);
    if q < 0.0 {
        q = 0.0;
    }
    let q2: f32 = q * q;
    let g: f32 = 1.0 / (1.0 + q + 0.5 * q2 + 0.16666667 * q2 * q);
    let mut alpha: f32 = opacity * g;
    if alpha > 0.99 {
        alpha = 0.99;
    }
    if alpha < 0.00392157 {
        alpha = 0.0;
    }
    out_buf[i] = alpha;
}
"#,
    ),
    (
        "jonswap_wave_compute",
        r#"
#[compute(workgroup_size = [64, 1, 1])]
fn jonswap_wave_compute(
    alpha_pm: f32,
    omega_p: f32,
    gamma_j: f32,
    k_buf: &[f32],
    out_buf: &mut [f32],
) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let k: f32 = k_buf[i];
    let mut energy: f32 = 0.0;
    if k > 0.0001 {
        let gk: f32 = 9.81 * k;
        let mut omega: f32 = 0.5 * (gk + 1.0);
        omega = 0.5 * (omega + gk / omega);
        omega = 0.5 * (omega + gk / omega);
        let w2: f32 = omega * omega;
        let w5: f32 = w2 * w2 * omega + 0.00001;
        let ratio: f32 = omega_p / (omega + 0.00001);
        let r2: f32 = ratio * ratio;
        let r4: f32 = r2 * r2;
        let pm_decay: f32 = 1.0 / (1.0 + 1.25 * r4 + 0.78125 * r4 * r4);
        let mut sigma: f32 = 0.09;
        if omega <= omega_p {
            sigma = 0.07;
        }
        let d_om: f32 = (omega - omega_p) / (sigma * omega_p + 0.00001);
        let bell: f32 = 1.0 / (1.0 + 0.5 * d_om * d_om);
        energy = (alpha_pm * 96.2361 / w5) * pm_decay * (1.0 + (gamma_j - 1.0) * bell);
        if energy < 0.0 {
            energy = 0.0;
        }
    }
    out_buf[i] = energy;
}
"#,
    ),
    (
        "indirect_frustum_cull_compute",
        r#"
#[compute(workgroup_size = [64, 1, 1])]
fn indirect_frustum_cull_compute(
    tan_half_fov: f32,
    near_z: f32,
    far_z: f32,
    px_buf: &[f32],
    py_buf: &[f32],
    pz_buf: &[f32],
    rad_buf: &[f32],
    vis_buf: &mut [f32],
) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let x: f32 = px_buf[i];
    let y: f32 = py_buf[i];
    let z: f32 = pz_buf[i];
    let r: f32 = rad_buf[i];
    let mut visible: f32 = 1.0;
    if r <= 0.0 {
        visible = 0.0;
    }
    if z + r < near_z {
        visible = 0.0;
    }
    if z - r > far_z {
        visible = 0.0;
    }
    let half_span: f32 = z * tan_half_fov + r;
    if x < 0.0 - half_span {
        visible = 0.0;
    }
    if x > half_span {
        visible = 0.0;
    }
    if y < 0.0 - half_span {
        visible = 0.0;
    }
    if y > half_span {
        visible = 0.0;
    }
    vis_buf[i] = visible;
}
"#,
    ),
    (
        "sdf_raymarch_compute",
        r#"
#[compute(workgroup_size = [64, 1, 1])]
fn sdf_raymarch_compute(
    max_steps: u32,
    sphere_r: f32,
    t_max: f32,
    dx_buf: &[f32],
    dy_buf: &[f32],
    dz_buf: &[f32],
    depth_buf: &mut [f32],
) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let rdx: f32 = dx_buf[i];
    let rdy: f32 = dy_buf[i];
    let rdz: f32 = dz_buf[i];
    let mut t: f32 = 0.0;
    let mut hit: f32 = 0.0;
    let mut step: u32 = 0;
    while step < max_steps {
        if hit < 0.5 {
            let px: f32 = rdx * t;
            let py: f32 = 1.0 + rdy * t;
            let pz: f32 = rdz * t - 3.0;
            let r2: f32 = px * px + py * py + pz * pz + 0.000001;
            let mut len: f32 = 0.5 * (r2 + 1.0);
            len = 0.5 * (len + r2 / len);
            len = 0.5 * (len + r2 / len);
            let d_sphere: f32 = len - sphere_r;
            let d_plane: f32 = py + 1.0;
            let mut d: f32 = d_sphere;
            if d_plane < d_sphere {
                d = d_plane;
            }
            if d < 0.001 {
                hit = 1.0;
            }
            if hit < 0.5 {
                t = t + d;
                if t > t_max {
                    t = t_max;
                    hit = 1.0;
                }
            }
        }
        step = step + 1;
    }
    depth_buf[i] = t;
}
"#,
    ),
];

#[test]
fn wgsl_all_wave82f_and_example_kernels_pass_live_naga_and_spirv_val() {
    let dir = make_temp_dir("wgsl_matrix");
    let mut cases: Vec<(String, PathBuf)> = vec![
        ("vec_add".to_string(), gpu_example("vec_add.bld")),
        ("saxpy".to_string(), gpu_example("saxpy.bld")),
        ("matmul".to_string(), gpu_example("matmul.bld")),
        ("reduce".to_string(), gpu_example("reduce.bld")),
        ("stencil".to_string(), gpu_example("stencil.bld")),
    ];
    for (name, code) in WAVE82F_KERNELS {
        let p = dir.join(format!("{name}.bld"));
        std::fs::write(&p, code).unwrap();
        cases.push((name.to_string(), p));
    }

    for (label, bld_path) in &cases {
        let out_wgsl = dir.join(format!("{label}.wgsl"));
        let cert_path = dir.join(format!("{label}.wgsl.gpu-cert.json"));
        let compile = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                bld_path.to_str().unwrap(),
                "--target",
                "wgsl",
                "--emit-gpu-cert",
                cert_path.to_str().unwrap(),
                "-o",
                out_wgsl.to_str().unwrap(),
            ])
            .output()
            .expect("wgsl compile");
        assert!(
            compile.status.success(),
            "WGSL compile failed for {label}:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );

        let wgsl_src = std::fs::read_to_string(&out_wgsl).unwrap();
        assert!(wgsl_src.contains("@compute @workgroup_size("), "{label}");
        assert!(
            !wgsl_src.contains("} else {\n    }"),
            "{label}: redundant empty else block emitted"
        );
        if label == "reduce" {
            assert!(
                wgsl_src.contains("(s > 0u)"),
                "reduce.wgsl must match u32 literal width: {wgsl_src}"
            );
            assert!(
                wgsl_src.contains("(lid == 0u)"),
                "reduce.wgsl must match u32 literal width: {wgsl_src}"
            );
        }
        if label == "stencil" {
            assert!(
                wgsl_src.contains("(i == 0u)"),
                "stencil.wgsl must match u32 literal width: {wgsl_src}"
            );
        }

        let cert: GpuTvCertificate =
            serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
        assert_eq!(cert.schema, GPU_TV_CERT_SCHEMA);
        assert_eq!(cert.target, "wgsl");
        assert!(cert.verified);

        let verify = Command::new(buildc_bin())
            .args([
                "verify-cert",
                cert_path.to_str().unwrap(),
                "--shader",
                out_wgsl.to_str().unwrap(),
            ])
            .output()
            .expect("verify-cert");
        assert!(
            verify.status.success(),
            "verify-cert failed for {label}: {}",
            String::from_utf8_lossy(&verify.stderr)
        );

        validate_wgsl_with_naga_if_available(label, &out_wgsl, &dir);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn msl_all_wave82f_and_example_kernels_conform_to_metal31_and_verify_cert() {
    let dir = make_temp_dir("msl_matrix");
    let mut cases: Vec<(String, PathBuf)> = vec![
        ("vec_add".to_string(), gpu_example("vec_add.bld")),
        ("saxpy".to_string(), gpu_example("saxpy.bld")),
        ("matmul".to_string(), gpu_example("matmul.bld")),
        ("reduce".to_string(), gpu_example("reduce.bld")),
        ("stencil".to_string(), gpu_example("stencil.bld")),
    ];
    for (name, code) in WAVE82F_KERNELS {
        let p = dir.join(format!("{name}.bld"));
        std::fs::write(&p, code).unwrap();
        cases.push((name.to_string(), p));
    }

    for (label, bld_path) in &cases {
        let out_msl = dir.join(format!("{label}.metal"));
        let cert_path = dir.join(format!("{label}.msl.gpu-cert.json"));
        let compile = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                bld_path.to_str().unwrap(),
                "--target",
                "msl",
                "--emit-gpu-cert",
                cert_path.to_str().unwrap(),
                "-o",
                out_msl.to_str().unwrap(),
            ])
            .output()
            .expect("msl compile");
        assert!(
            compile.status.success(),
            "MSL compile failed for {label}:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );

        let msl_src = std::fs::read_to_string(&out_msl).unwrap();
        assert!(msl_src.contains("#include <metal_stdlib>"), "{label}");
        assert!(msl_src.contains("using namespace metal;"), "{label}");
        assert!(msl_src.contains("kernel void "), "{label}");
        assert!(msl_src.contains("[[buffer(0)]]"), "{label}");
        assert!(msl_src.contains("[[thread_position_in_grid]]"), "{label}");
        assert!(
            !msl_src.contains("} else {\n    }"),
            "{label}: redundant empty else block emitted"
        );
        if label == "reduce" {
            assert!(msl_src.contains("threadgroup float scratch[64];"));
            assert!(msl_src.contains("threadgroup_barrier(mem_flags::mem_threadgroup);"));
            assert!(
                msl_src.contains("(s > 0u)"),
                "reduce.metal must match unsigned literal: {msl_src}"
            );
            assert!(
                msl_src.contains("(lid == 0u)"),
                "reduce.metal must match unsigned literal: {msl_src}"
            );
        }
        if label == "stencil" {
            assert!(
                msl_src.contains("(i == 0u)"),
                "stencil.metal must match unsigned literal: {msl_src}"
            );
        }
        if label == "sdf_raymarch_compute" {
            assert!(
                msl_src.contains("uint _bl_step;"),
                "sdf_raymarch_compute.metal must escape `step` to avoid metal::step shadowing: {msl_src}"
            );
            assert!(!msl_src.contains("uint step;"));
        }

        let cert: GpuTvCertificate =
            serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
        assert_eq!(cert.schema, GPU_TV_CERT_SCHEMA);
        assert_eq!(cert.target, "msl");
        assert!(cert.verified);

        let verify = Command::new(buildc_bin())
            .args([
                "verify-cert",
                cert_path.to_str().unwrap(),
                "--shader",
                out_msl.to_str().unwrap(),
            ])
            .output()
            .expect("verify-cert");
        assert!(
            verify.status.success(),
            "verify-cert failed for {label}: {}",
            String::from_utf8_lossy(&verify.stderr)
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planted_forged_wgsl_msl_certs_and_corrupt_wgsl_fail_closed() {
    let dir = make_temp_dir("planted_controls");
    let src = gpu_example("reduce.bld");
    let out_wgsl = dir.join("reduce.wgsl");
    let cert_wgsl = dir.join("reduce.wgsl.gpu-cert.json");
    let out_msl = dir.join("reduce.metal");
    let cert_msl = dir.join("reduce.msl.gpu-cert.json");

    for (target, out_p, cert_p) in [
        ("wgsl", &out_wgsl, &cert_wgsl),
        ("msl", &out_msl, &cert_msl),
    ] {
        let ok = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                src.to_str().unwrap(),
                "--target",
                target,
                "--emit-gpu-cert",
                cert_p.to_str().unwrap(),
                "-o",
                out_p.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(ok.status.success());
    }

    // Planted Control 1: 1-hex-char flip in WGSL shader_sha256 rejected fail-closed.
    let mut forged: GpuTvCertificate =
        serde_json::from_str(&std::fs::read_to_string(&cert_wgsl).unwrap()).unwrap();
    let flip = if forged.shader_sha256.starts_with('0') {
        '1'
    } else {
        '0'
    };
    forged.shader_sha256 = format!("{flip}{}", &forged.shader_sha256[1..]);
    let forged_p = dir.join("forged_wgsl.json");
    std::fs::write(&forged_p, serde_json::to_string_pretty(&forged).unwrap()).unwrap();
    let r1 = Command::new(buildc_bin())
        .args([
            "verify-cert",
            forged_p.to_str().unwrap(),
            "--shader",
            out_wgsl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!r1.status.success(), "forged WGSL cert must fail closed");

    // Planted Control 2: 1-line tampered Metal file rejected fail-closed.
    let tampered_msl = dir.join("tampered.metal");
    let mut txt = std::fs::read_to_string(&out_msl).unwrap();
    txt.push_str("\n// tampered\n");
    std::fs::write(&tampered_msl, txt).unwrap();
    let r2 = Command::new(buildc_bin())
        .args([
            "verify-cert",
            cert_msl.to_str().unwrap(),
            "--shader",
            tampered_msl.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!r2.status.success(), "tampered MSL shader must fail closed");

    // Planted Control 3: Synthetic WGSL signed/unsigned mismatch rejected by live naga.
    if tool_available("naga", "--version") {
        let bad_wgsl = dir.join("bad_u32_i32.wgsl");
        std::fs::write(
            &bad_wgsl,
            "@compute @workgroup_size(64, 1, 1)\nfn k(@builtin(global_invocation_id) gid: vec3<u32>) {\n    let b: bool = gid.x > 0i;\n}\n",
        )
        .unwrap();
        let naga_bad = Command::new("naga").arg(&bad_wgsl).output().unwrap();
        assert!(
            !naga_bad.status.success(),
            "naga must reject u32 > i32 type mismatch fail-closed"
        );
    }

    // Planted Control 4: Unsupported f64 in GPU compute fails closed on both WGSL and MSL.
    let f64_src = dir.join("bad_f64.bld");
    std::fs::write(
        &f64_src,
        "#[compute]\nfn bad_f64(a: &[f64], out: &mut [f64]) ~ Gpu {\n    let i = gl_GlobalInvocationID.x;\n    out[i] = a[i] + 1.0;\n}\n",
    )
    .unwrap();
    for target in ["wgsl", "msl"] {
        let out_p = dir.join(format!("bad_f64.{target}"));
        let r = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args([
                f64_src.to_str().unwrap(),
                "--target",
                target,
                "-o",
                out_p.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!r.status.success(), "{target} must refuse f64 fail-closed");
    }

    let _ = std::fs::remove_dir_all(&dir);
}
