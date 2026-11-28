// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! Wave 116B Cross-Backend MIR Opcode, Cast, and Intrinsic Parity Tests.
//!
//! Verifies bitwise ops, shift-count masking, signed arithmetic vs unsigned
//! logical right shift, float truncated remainder, float pow, IEEE-754 NaN
//! inequality, bool logical NOT/Eq/Ne, same-width int casts, bool-to-int casts,
//! integer/float math intrinsics, f32 bitcasts, explicit wrapping arithmetic,
//! and planted fail-closed controls across CPU C and all 5 GPU targets
//! (HLSL, GLSL, WGSL, MSL, SPIR-V) with live dxc, glslangValidator, naga,
//! and spirv-val validation.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn temp_dir(tag: &str) -> PathBuf {
    let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "buildlang_wave116b_{}_{}_{}",
        std::process::id(),
        n,
        tag
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, content).expect("write file");
    p
}

fn diag(out: &Output) -> String {
    format!(
        "status: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn shader_tools_required() -> bool {
    std::env::var("BUILDLANG_SHADER_TOOLS_REQUIRE").as_deref() == Ok("1")
}

fn find_tool(env_var: &str, default_name: &str, probe_arg: &str) -> Option<String> {
    let tool = std::env::var(env_var).unwrap_or_else(|_| default_name.to_string());
    let ok = Command::new(&tool)
        .arg(probe_arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    ok.then_some(tool)
}

fn validate_spirv(path: &Path) {
    let Some(tool) = find_tool("SPIRV_VAL", "spirv-val", "--version") else {
        assert!(
            !shader_tools_required(),
            "spirv-val not on PATH and BUILDLANG_SHADER_TOOLS_REQUIRE=1"
        );
        return;
    };
    let out = Command::new(tool)
        .arg(path)
        .output()
        .expect("run spirv-val");
    assert!(out.status.success(), "spirv-val failed:\n{}", diag(&out));
}

fn validate_wgsl(path: &Path) {
    // CI (`BUILDLANG_SHADER_TOOLS_REQUIRE=1`) installs spirv-val, glslangValidator,
    // and dxc, not naga (matching `tests/wave83c_msl_wgsl_compute.rs`). Run naga
    // whenever available on PATH, and only fail-closed on missing naga if
    // `BUILDLANG_NAGA_REQUIRE=1` is explicitly set.
    let Some(tool) = find_tool("NAGA", "naga", "--version") else {
        assert!(
            std::env::var("BUILDLANG_NAGA_REQUIRE").as_deref() != Ok("1"),
            "naga not on PATH and BUILDLANG_NAGA_REQUIRE=1"
        );
        eprintln!("skipping naga for {}: naga not on PATH", path.display());
        return;
    };
    let out = Command::new(tool).arg(path).output().expect("run naga");
    assert!(out.status.success(), "naga failed:\n{}", diag(&out));
}

fn validate_glsl(path: &Path) {
    let Some(tool) = find_tool("GLSLANG_VALIDATOR", "glslangValidator", "--version") else {
        assert!(
            !shader_tools_required(),
            "glslangValidator not on PATH and BUILDLANG_SHADER_TOOLS_REQUIRE=1"
        );
        return;
    };
    let out = Command::new(tool)
        .args(["-V", "-S", "comp"])
        .arg(path)
        .args(["-o", &format!("{}.spv", path.display())])
        .output()
        .expect("run glslangValidator");
    assert!(
        out.status.success(),
        "glslangValidator failed:\n{}",
        diag(&out)
    );
}

fn validate_hlsl(path: &Path, entry: &str) {
    let Some(tool) = find_tool("DXC", "dxc", "--version") else {
        assert!(
            !shader_tools_required(),
            "dxc not on PATH and BUILDLANG_SHADER_TOOLS_REQUIRE=1"
        );
        return;
    };
    let out = Command::new(tool)
        .args(["-T", "cs_6_0", "-E", entry])
        .arg(path)
        .output()
        .expect("run dxc");
    assert!(out.status.success(), "dxc failed:\n{}", diag(&out));
}

fn spirv_opcodes(bytes: &[u8]) -> Vec<u32> {
    assert_eq!(
        bytes.len() % 4,
        0,
        "SPIR-V byte length must be 4-byte aligned"
    );
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let mut ops = Vec::new();
    let mut i = 5;
    while i < words.len() {
        let word = words[i];
        let op = word & 0xFFFF;
        let wc = (word >> 16) as usize;
        if wc == 0 {
            break;
        }
        ops.push(op);
        i += wc;
    }
    ops
}

fn compile_gpu_target(src: &Path, target: &str, out: &Path) -> Output {
    buildc()
        .arg(src)
        .args(["--target", target, "-o"])
        .arg(out)
        .output()
        .unwrap_or_else(|e| panic!("failed to run buildc --target {target}: {e}"))
}

fn compile_and_run_cpu(dir: &Path, name: &str, src: &str) -> Output {
    let bld = write_file(dir, name, src);
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        format!("{name}.bin")
    };
    let exe = dir.join(exe_name);
    let build = buildc()
        .arg(&bld)
        .args(["-o"])
        .arg(&exe)
        .output()
        .expect("run buildc cpu");
    assert!(
        build.status.success(),
        "CPU build failed:\n{}",
        diag(&build)
    );
    Command::new(&exe)
        .output()
        .expect("run compiled cpu binary")
}

#[test]
fn bitwise_shifts_bool_ops_and_casts_all_6_targets_and_live_validators() {
    let dir = temp_dir("bitwise_shifts_casts");

    // 1. GPU compute shader exercising:
    //    - u32 logical right shift + shift-count masking (shift by 33 == shift by 1)
    //    - i32 arithmetic right shift + shift-count masking (shift by 34 == shift by 2)
    //    - bitwise &, |, ^, ~ (!u)
    //    - bool logical NOT (!flag), bool == bool, bool != bool
    //    - bool as i32 (OpSelect) and i32 as u32 (OpBitcast)
    let gpu_src = r#"
#[compute]
fn bit_shift_ops(
    u_in: &[u32],
    i_in: &[i32],
    shift_in: &[u32],
    u_out: &mut [u32],
    i_out: &mut [i32],
    n: u32,
) ~ Gpu {
    let idx: u32 = gl_GlobalInvocationID.x;
    if idx < n {
        let u: u32 = u_in[idx];
        let s: i32 = i_in[idx];
        let sh: u32 = shift_in[idx];

        let u_shr: u32 = u >> sh;
        let u_shl: u32 = u << sh;
        let u_mix: u32 = (u_shr ^ u_shl) | (u & 255u32);
        let u_not: u32 = !u_mix;

        let s_shr: i32 = s >> (sh as i32);
        let flag_a: bool = u > 0u32;
        let flag_b: bool = s < 0i32;
        let not_a: bool = !flag_a;
        let eq_flag: bool = flag_a == flag_b;
        let ne_flag: bool = not_a != eq_flag;
        let flag_i: i32 = ne_flag as i32;
        let s_as_u: u32 = s_shr as u32;

        u_out[idx] = u_not ^ s_as_u;
        i_out[idx] = s_shr.wrapping_add(flag_i);
    }
}
"#;
    let bld = write_file(&dir, "bit_shift_ops.bld", gpu_src);

    // HLSL + live DXC
    let hlsl_p = dir.join("bit_shift_ops.hlsl");
    let res = compile_gpu_target(&bld, "hlsl", &hlsl_p);
    assert!(res.status.success(), "HLSL failed:\n{}", diag(&res));
    let hlsl = std::fs::read_to_string(&hlsl_p).unwrap();
    assert!(!hlsl.contains("??"), "HLSL emitted placeholder ??:\n{hlsl}");
    assert!(
        hlsl.contains("& 31u"),
        "HLSL must mask shift count:\n{hlsl}"
    );
    validate_hlsl(&hlsl_p, "bit_shift_ops");

    // GLSL + live glslangValidator
    let glsl_p = dir.join("bit_shift_ops.comp");
    let res = compile_gpu_target(&bld, "glsl", &glsl_p);
    assert!(res.status.success(), "GLSL failed:\n{}", diag(&res));
    let glsl = std::fs::read_to_string(&glsl_p).unwrap();
    assert!(!glsl.contains("??"), "GLSL emitted placeholder ??:\n{glsl}");
    assert!(
        glsl.contains("& 31u"),
        "GLSL must mask shift count:\n{glsl}"
    );
    validate_glsl(&glsl_p);

    // WGSL + live naga
    let wgsl_p = dir.join("bit_shift_ops.wgsl");
    let res = compile_gpu_target(&bld, "wgsl", &wgsl_p);
    assert!(res.status.success(), "WGSL failed:\n{}", diag(&res));
    let wgsl = std::fs::read_to_string(&wgsl_p).unwrap();
    assert!(
        wgsl.contains("(u32(") && wgsl.contains("& 31u)"),
        "WGSL must cast and mask shift count:\n{wgsl}"
    );
    validate_wgsl(&wgsl_p);

    // MSL
    let msl_p = dir.join("bit_shift_ops.metal");
    let res = compile_gpu_target(&bld, "msl", &msl_p);
    assert!(res.status.success(), "MSL failed:\n{}", diag(&res));
    let msl = std::fs::read_to_string(&msl_p).unwrap();
    assert!(
        msl.contains("static_cast<uint>(") && msl.contains("& 31u"),
        "MSL must mask shift count:\n{msl}"
    );

    // SPIR-V + live spirv-val + opcode checks
    let spv_p = dir.join("bit_shift_ops.spv");
    let res = compile_gpu_target(&bld, "spirv", &spv_p);
    assert!(res.status.success(), "SPIR-V failed:\n{}", diag(&res));
    validate_spirv(&spv_p);
    let spv_ops = spirv_opcodes(&std::fs::read(&spv_p).unwrap());
    assert!(
        spv_ops.contains(&168),
        "SPIR-V must emit OpLogicalNot (168)"
    );
    assert!(
        spv_ops.contains(&164),
        "SPIR-V must emit OpLogicalEqual (164)"
    );
    assert!(
        spv_ops.contains(&165),
        "SPIR-V must emit OpLogicalNotEqual (165)"
    );
    assert!(
        spv_ops.contains(&194),
        "SPIR-V must emit OpShiftRightLogical (194)"
    );
    assert!(
        spv_ops.contains(&195),
        "SPIR-V must emit OpShiftRightArithmetic (195)"
    );
    assert!(
        spv_ops.contains(&169),
        "SPIR-V must emit OpSelect (169) for bool as i32"
    );
    assert!(
        spv_ops.contains(&124),
        "SPIR-V must emit OpBitcast (124) for i32 as u32"
    );

    // 2. CPU C parity check for shift masking, arithmetic vs logical shr, bool ops, casts
    let cpu_src = r#"
fn main() -> i32 {
    let u: u32 = 2147483648u32;
    let u_shr: u32 = u >> 33u32;
    if u_shr != 1073741824u32 { return 1; }

    let s: i32 = -16i32;
    let s_shr: i32 = s >> 34i32;
    if s_shr != -4i32 { return 2; }

    let flag_a: bool = u > 0u32;
    let flag_b: bool = s < 0i32;
    let not_a: bool = !flag_a;
    let eq_flag: bool = flag_a == flag_b;
    let ne_flag: bool = not_a != eq_flag;
    let flag_i: i32 = ne_flag as i32;
    if flag_i != 1i32 { return 3; }

    let s_as_u: u32 = s_shr as u32;
    if s_as_u != 4294967292u32 { return 4; }
    0
}
"#;
    let run = compile_and_run_cpu(&dir, "bit_shift_cpu.bld", cpu_src);
    assert!(run.status.success(), "CPU parity failed:\n{}", diag(&run));
}

#[test]
fn float_rem_pow_nan_neq_intrinsics_and_bitcasts_all_6_targets() {
    let dir = temp_dir("float_math_bitcast");

    let gpu_src = r#"
#[compute]
fn float_math_ops(
    a_in: &[f32],
    b_in: &[f32],
    i_a: &[i32],
    i_b: &[i32],
    f_out: &mut [f32],
    u_out: &mut [u32],
    i_out: &mut [i32],
    n: u32,
) ~ Gpu {
    let idx: u32 = gl_GlobalInvocationID.x;
    if idx < n {
        let a: f32 = a_in[idx];
        let b: f32 = b_in[idx];

        let rem_v: f32 = a % b;
        let pow_v: f32 = abs(a) ** 2.0f32;
        let tr_v: f32 = rem_v.trunc();
        let cl_v: f32 = clamp(pow_v + tr_v, -10.0f32, 10.0f32);
        let bits: u32 = cl_v.to_bits();
        let rt: f32 = f32::from_bits(bits);
        let is_ne: bool = a != b;
        let ne_i: i32 = is_ne as i32;

        let imin: i32 = min(i_a[idx], i_b[idx]);
        let imax: i32 = max(i_a[idx], i_b[idx]);

        f_out[idx] = rt;
        u_out[idx] = bits;
        i_out[idx] = imin.wrapping_add(imax).wrapping_add(ne_i);
    }
}
"#;
    let bld = write_file(&dir, "float_math_ops.bld", gpu_src);

    // HLSL + live DXC
    let hlsl_p = dir.join("float_math_ops.hlsl");
    let res = compile_gpu_target(&bld, "hlsl", &hlsl_p);
    assert!(res.status.success(), "HLSL failed:\n{}", diag(&res));
    let hlsl = std::fs::read_to_string(&hlsl_p).unwrap();
    assert!(
        hlsl.contains("fmod("),
        "HLSL float Rem must use fmod:\n{hlsl}"
    );
    assert!(
        hlsl.contains("asuint("),
        "HLSL f32::to_bits must use asuint:\n{hlsl}"
    );
    assert!(
        hlsl.contains("asfloat("),
        "HLSL f32::from_bits must use asfloat:\n{hlsl}"
    );
    validate_hlsl(&hlsl_p, "float_math_ops");

    // GLSL + live glslangValidator
    let glsl_p = dir.join("float_math_ops.comp");
    let res = compile_gpu_target(&bld, "glsl", &glsl_p);
    assert!(res.status.success(), "GLSL failed:\n{}", diag(&res));
    let glsl = std::fs::read_to_string(&glsl_p).unwrap();
    assert!(
        glsl.contains("trunc("),
        "GLSL float Rem must emit truncated remainder via trunc:\n{glsl}"
    );
    assert!(
        glsl.contains("floatBitsToUint("),
        "GLSL f32::to_bits must use floatBitsToUint:\n{glsl}"
    );
    assert!(
        glsl.contains("uintBitsToFloat("),
        "GLSL f32::from_bits must use uintBitsToFloat:\n{glsl}"
    );
    validate_glsl(&glsl_p);

    // WGSL + live naga
    let wgsl_p = dir.join("float_math_ops.wgsl");
    let res = compile_gpu_target(&bld, "wgsl", &wgsl_p);
    assert!(res.status.success(), "WGSL failed:\n{}", diag(&res));
    let wgsl = std::fs::read_to_string(&wgsl_p).unwrap();
    assert!(
        wgsl.contains("bitcast<u32>(") && wgsl.contains("bitcast<f32>("),
        "WGSL must lower f32 bitcasts via bitcast<T>:\n{wgsl}"
    );
    validate_wgsl(&wgsl_p);

    // MSL
    let msl_p = dir.join("float_math_ops.metal");
    let res = compile_gpu_target(&bld, "msl", &msl_p);
    assert!(res.status.success(), "MSL failed:\n{}", diag(&res));
    let msl = std::fs::read_to_string(&msl_p).unwrap();
    assert!(msl.contains("fmod("), "MSL float Rem must use fmod:\n{msl}");
    assert!(
        msl.contains("as_type<uint>(") && msl.contains("as_type<float>("),
        "MSL must lower f32 bitcasts via as_type<T>:\n{msl}"
    );

    // SPIR-V + live spirv-val + opcode verification
    let spv_p = dir.join("float_math_ops.spv");
    let res = compile_gpu_target(&bld, "spirv", &spv_p);
    assert!(res.status.success(), "SPIR-V failed:\n{}", diag(&res));
    validate_spirv(&spv_p);
    let spv_ops = spirv_opcodes(&std::fs::read(&spv_p).unwrap());
    assert!(spv_ops.contains(&140), "SPIR-V must emit OpFRem (140)");
    assert!(
        spv_ops.contains(&183),
        "SPIR-V float != must emit OpFUnordNotEqual (183)"
    );
    assert!(
        !spv_ops.contains(&182),
        "SPIR-V must not emit OpFOrdNotEqual (182) for float !="
    );
    assert!(spv_ops.contains(&124), "SPIR-V must emit OpBitcast (124)");

    // CPU C verification of negative float Rem, NaN != NaN, bitcast roundtrip, int min/max
    let cpu_src = r#"
fn main() -> i32 {
    let rem_v: f32 = -5.5f32 % 2.0f32;
    if rem_v != -1.5f32 { return 1; }

    let nan_v: f32 = 0.0f32 / 0.0f32;
    if !(nan_v != nan_v) { return 2; }

    let x: f32 = -3.25f32;
    let bits: u32 = x.to_bits();
    if bits != 3226468352u32 { return 3; }
    let rt: f32 = f32::from_bits(bits);
    if rt != x { return 4; }

    if min(-7i32, 3i32) != -7i32 { return 5; }
    if max(-7i32, 3i32) != 3i32 { return 6; }
    0
}
"#;
    let run = compile_and_run_cpu(&dir, "float_math_cpu.bld", cpu_src);
    assert!(run.status.success(), "CPU parity failed:\n{}", diag(&run));
}

#[test]
fn explicit_wrapping_ops_validate_all_6_targets_zero_placeholder_tokens() {
    let dir = temp_dir("wrapping_ops");

    let gpu_src = r#"
#[compute]
fn wrap_ops(
    u_a: &[u32],
    u_b: &[u32],
    i_a: &[i32],
    i_b: &[i32],
    u_out: &mut [u32],
    i_out: &mut [i32],
    n: u32,
) ~ Gpu {
    let idx: u32 = gl_GlobalInvocationID.x;
    if idx < n {
        let ua: u32 = u_a[idx];
        let ub: u32 = u_b[idx];
        u_out[idx] = ua.wrapping_add(ub).wrapping_sub(1u32).wrapping_mul(3u32);

        let ia: i32 = i_a[idx];
        let ib: i32 = i_b[idx];
        i_out[idx] = ia.wrapping_add(ib).wrapping_sub(1i32).wrapping_mul(3i32);
    }
}
"#;
    let bld = write_file(&dir, "wrap_ops.bld", gpu_src);

    let hlsl_p = dir.join("wrap_ops.hlsl");
    let res = compile_gpu_target(&bld, "hlsl", &hlsl_p);
    assert!(res.status.success(), "HLSL failed:\n{}", diag(&res));
    let hlsl = std::fs::read_to_string(&hlsl_p).unwrap();
    assert!(!hlsl.contains("??"), "HLSL emitted ??:\n{hlsl}");
    assert!(
        hlsl.contains("asint((asuint("),
        "HLSL signed wrapping must wrap via asuint/asint:\n{hlsl}"
    );
    validate_hlsl(&hlsl_p, "wrap_ops");

    let glsl_p = dir.join("wrap_ops.comp");
    let res = compile_gpu_target(&bld, "glsl", &glsl_p);
    assert!(res.status.success(), "GLSL failed:\n{}", diag(&res));
    let glsl = std::fs::read_to_string(&glsl_p).unwrap();
    assert!(!glsl.contains("??"), "GLSL emitted ??:\n{glsl}");
    validate_glsl(&glsl_p);

    let wgsl_p = dir.join("wrap_ops.wgsl");
    let res = compile_gpu_target(&bld, "wgsl", &wgsl_p);
    assert!(res.status.success(), "WGSL failed:\n{}", diag(&res));
    validate_wgsl(&wgsl_p);

    let msl_p = dir.join("wrap_ops.metal");
    let res = compile_gpu_target(&bld, "msl", &msl_p);
    assert!(res.status.success(), "MSL failed:\n{}", diag(&res));
    let msl = std::fs::read_to_string(&msl_p).unwrap();
    assert!(
        msl.contains("static_cast<int>((static_cast<uint>("),
        "MSL signed wrapping must avoid C++ signed overflow UB:\n{msl}"
    );

    let spv_p = dir.join("wrap_ops.spv");
    let res = compile_gpu_target(&bld, "spirv", &spv_p);
    assert!(res.status.success(), "SPIR-V failed:\n{}", diag(&res));
    validate_spirv(&spv_p);
    let spv_ops = spirv_opcodes(&std::fs::read(&spv_p).unwrap());
    assert!(spv_ops.contains(&128), "SPIR-V missing OpIAdd (128)");
    assert!(spv_ops.contains(&130), "SPIR-V missing OpISub (130)");
    assert!(spv_ops.contains(&132), "SPIR-V missing OpIMul (132)");
}

#[test]
fn planted_unsupported_gpu_ops_fail_closed_across_all_5_gpu_targets() {
    let dir = temp_dir("planted_fail_closed");

    let cases: &[(&str, &str)] = &[
        (
            "planted_saturating.bld",
            r#"
#[compute]
fn bad_sat(a: &[u32], b: &[u32], out: &mut [u32], n: u32) ~ Gpu {
    let i: u32 = gl_GlobalInvocationID.x;
    if i < n {
        out[i] = a[i].saturating_add(b[i]);
    }
}
"#,
        ),
        (
            "planted_int_pow.bld",
            r#"
#[compute]
fn bad_pow(a: &[i32], b: &[i32], out: &mut [i32], n: u32) ~ Gpu {
    let i: u32 = gl_GlobalInvocationID.x;
    if i < n {
        out[i] = a[i] ** b[i];
    }
}
"#,
        ),
    ];

    for (filename, src) in cases {
        let bld = write_file(&dir, filename, src);
        for (target, ext) in [
            ("hlsl", "hlsl"),
            ("glsl", "comp"),
            ("wgsl", "wgsl"),
            ("msl", "metal"),
            ("spirv", "spv"),
        ] {
            let out = dir.join(format!("{filename}.{ext}"));
            let res = compile_gpu_target(&bld, target, &out);
            let stderr = String::from_utf8_lossy(&res.stderr);
            assert!(
                !res.status.success(),
                "planted unsupported op `{filename}` must fail closed on `{target}`, but succeeded!\n{}",
                diag(&res)
            );
            assert!(
                stderr.contains("not supported") || stderr.contains("Unsupported"),
                "planted `{filename}` on `{target}` must fail with backend Unsupported error, not parse/type error:\n{}",
                diag(&res)
            );
        }
    }
}
