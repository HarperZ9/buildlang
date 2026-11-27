// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 111C integration tests for unified cross-backend integer semantics
//! across C, HLSL, GLSL, WGSL, MSL, and SPIR-V.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "buildlang_wave111c_{}_{}_{}",
            std::process::id(),
            id,
            label
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, content).expect("write fixture");
        p
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate lives under repo root")
        .to_path_buf()
}

fn diag(out: &Output) -> String {
    format!(
        "status={:?}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn tool_available(bin: &str, arg: &str) -> bool {
    Command::new(bin)
        .arg(arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn sha256_round_ref(
    a: u32,
    b: u32,
    c: u32,
    d: u32,
    e: u32,
    f: u32,
    g: u32,
    h: u32,
    k: u32,
    w: u32,
) -> (u32, u32) {
    let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
    let ch = (e & f) ^ ((!e) & g);
    let t1 = h
        .wrapping_add(s1)
        .wrapping_add(ch)
        .wrapping_add(k)
        .wrapping_add(w);
    let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
    let maj = (a & b) ^ (a & c) ^ (b & c);
    let t2 = s0.wrapping_add(maj);
    (t1.wrapping_add(t2), d.wrapping_add(t1))
}

fn sha256_gpu_kernel_src() -> &'static str {
    r#"#[compute(workgroup_size = [64, 1, 1])]
fn sha256_round_compute(
    k_round: u32,
    w_msg: u32,
    a_buf: &[u32],
    b_buf: &[u32],
    c_buf: &[u32],
    d_buf: &[u32],
    e_buf: &[u32],
    f_buf: &[u32],
    g_buf: &[u32],
    h_buf: &[u32],
    a_out: &mut [u32],
    e_out: &mut [u32],
) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let a: u32 = a_buf[i];
    let b: u32 = b_buf[i];
    let c: u32 = c_buf[i];
    let d: u32 = d_buf[i];
    let e: u32 = e_buf[i];
    let f: u32 = f_buf[i];
    let g: u32 = g_buf[i];
    let h: u32 = h_buf[i];

    let all_ones: u32 = 4294967295;
    let r6: u32 = 6;
    let l26: u32 = 26;
    let r11: u32 = 11;
    let l21: u32 = 21;
    let r25: u32 = 25;
    let l7: u32 = 7;
    let s1_a: u32 = (e >> r6) | (e << l26);
    let s1_b: u32 = (e >> r11) | (e << l21);
    let s1_c: u32 = (e >> r25) | (e << l7);
    let sigma1: u32 = s1_a ^ s1_b ^ s1_c;
    let ch: u32 = (e & f) ^ ((e ^ all_ones) & g);
    let t1: u32 = h
        .wrapping_add(sigma1)
        .wrapping_add(ch)
        .wrapping_add(k_round)
        .wrapping_add(w_msg);

    let r2: u32 = 2;
    let l30: u32 = 30;
    let r13: u32 = 13;
    let l19: u32 = 19;
    let r22: u32 = 22;
    let l10: u32 = 10;
    let s0_a: u32 = (a >> r2) | (a << l30);
    let s0_b: u32 = (a >> r13) | (a << l19);
    let s0_c: u32 = (a >> r22) | (a << l10);
    let sigma0: u32 = s0_a ^ s0_b ^ s0_c;
    let maj: u32 = (a & b) ^ (a & c) ^ (b & c);
    let t2: u32 = sigma0.wrapping_add(maj);

    a_out[i] = t1.wrapping_add(t2);
    e_out[i] = d.wrapping_add(t1);
}
"#
}

fn sha256_cpu_program_src() -> &'static str {
    r#"fn sha256_round_a(
    a: u32,
    b: u32,
    c: u32,
    e: u32,
    f: u32,
    g: u32,
    h: u32,
    k_round: u32,
    w_msg: u32,
) -> u32 {
    let all_ones: u32 = 4294967295;
    let s1: u32 = ((e >> 6) | (e << 26)) ^ ((e >> 11) | (e << 21)) ^ ((e >> 25) | (e << 7));
    let ch: u32 = (e & f) ^ ((e ^ all_ones) & g);
    let t1: u32 = h
        .wrapping_add(s1)
        .wrapping_add(ch)
        .wrapping_add(k_round)
        .wrapping_add(w_msg);
    let s0: u32 = ((a >> 2) | (a << 30)) ^ ((a >> 13) | (a << 19)) ^ ((a >> 22) | (a << 10));
    let maj: u32 = (a & b) ^ (a & c) ^ (b & c);
    let t2: u32 = s0.wrapping_add(maj);
    t1.wrapping_add(t2)
}

fn sha256_round_e(
    d: u32,
    e: u32,
    f: u32,
    g: u32,
    h: u32,
    k_round: u32,
    w_msg: u32,
) -> u32 {
    let all_ones: u32 = 4294967295;
    let s1: u32 = ((e >> 6) | (e << 26)) ^ ((e >> 11) | (e << 21)) ^ ((e >> 25) | (e << 7));
    let ch: u32 = (e & f) ^ ((e ^ all_ones) & g);
    let t1: u32 = h
        .wrapping_add(s1)
        .wrapping_add(ch)
        .wrapping_add(k_round)
        .wrapping_add(w_msg);
    d.wrapping_add(t1)
}

fn main() ~ Console {
    // Canonical FIPS 180-4 SHA-256 IV + K[0] + W[0] for "abc".
    let a: u32 = 1779033703;
    let b: u32 = 3144134277;
    let c: u32 = 1013904242;
    let d: u32 = 2773480762;
    let e: u32 = 1359893119;
    let f: u32 = 2600822924;
    let g: u32 = 528734635;
    let h: u32 = 1541459225;
    let k0: u32 = 1116352408;
    let w0: u32 = 1633837952;
    let a_new: u32 = sha256_round_a(a, b, c, e, f, g, h, k0, w0);
    let e_new: u32 = sha256_round_e(d, e, f, g, h, k0, w0);
    println!("{} {}", a_new, e_new);
}
"#
}

/// Extract SPIR-V opcodes (lower 16 bits of each instruction header word after the 5-word header).
fn extract_spirv_opcodes(bytes: &[u8]) -> Vec<u16> {
    assert!(bytes.len() >= 20 && bytes.len() % 4 == 0);
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    assert_eq!(words[0], 0x0723_0203, "invalid SPIR-V magic");
    let mut ops = Vec::new();
    let mut idx = 5usize;
    while idx < words.len() {
        let word = words[idx];
        let count = (word >> 16) as usize;
        let op = (word & 0xffff) as u16;
        assert!(count >= 1 && idx + count <= words.len());
        ops.push(op);
        idx += count;
    }
    ops
}

#[test]
fn sha256_round_wrapping_compiles_all_6_targets_and_matches_fips_vector() {
    let td = TempDirGuard::new("sha256_6targets");

    // 1. CPU C backend: compile, execute, and verify exact FIPS 180-4 round-0 words.
    let cpu_file = td.write("sha256_cpu.bld", sha256_cpu_program_src());
    let cpu_run = buildc()
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["run", "--release"])
        .arg(&cpu_file)
        .output()
        .expect("run sha256_cpu");
    assert!(cpu_run.status.success(), "{}", diag(&cpu_run));
    let (want_a, want_e) = sha256_round_ref(
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
        0x428a_2f98,
        0x6162_6380,
    );
    let cpu_stdout = String::from_utf8_lossy(&cpu_run.stdout).trim().to_string();
    assert_eq!(
        cpu_stdout,
        format!("{want_a} {want_e}"),
        "CPU SHA-256 round output did not match FIPS 180-4 reference"
    );

    // 2. All 5 GPU targets (HLSL, GLSL, WGSL, MSL, SPIR-V) from identical .bld source.
    let gpu_file = td.write("sha256_gpu.bld", sha256_gpu_kernel_src());
    for (target, ext) in [
        ("hlsl", "hlsl"),
        ("glsl", "comp"),
        ("wgsl", "wgsl"),
        ("msl", "metal"),
        ("spirv", "spv"),
    ] {
        let out_path = td.0.join(format!("sha256.{ext}"));
        let res = buildc()
            .arg(&gpu_file)
            .args(["--target", target, "-o"])
            .arg(&out_path)
            .output()
            .expect("compile gpu target");
        assert!(res.status.success(), "{target}: {}", diag(&res));

        if target != "spirv" {
            let text = std::fs::read_to_string(&out_path).expect("read shader text");
            assert!(
                !text.contains("??"),
                "{target} emitted invalid fallback operator `??`:\n{text}"
            );
            assert!(
                text.contains('+'),
                "{target} missing wrapping add `+`:\n{text}"
            );
        } else {
            let spv_bytes = std::fs::read(&out_path).expect("read spv");
            let ops = extract_spirv_opcodes(&spv_bytes);
            assert!(
                ops.contains(&128),
                "SPIR-V SHA-256 shader must contain OpIAdd (128)"
            );
            if tool_available("spirv-val", "--version") {
                let v = Command::new("spirv-val")
                    .arg(&out_path)
                    .output()
                    .expect("run spirv-val");
                assert!(v.status.success(), "spirv-val failed: {}", diag(&v));
            }
        }
    }
}

#[test]
fn wrapping_add_sub_mul_u32_and_i32_all_6_targets_and_distinct_spirv_ops() {
    let td = TempDirGuard::new("wrap_ops");

    // CPU C backend execution of edge-case wrapping_add / wrapping_sub / wrapping_mul
    let cpu_src = r#"fn main() ~ Console {
    let u_max: u32 = 4294967295;
    let u_zero: u32 = 0;
    let u_a: u32 = u_max.wrapping_add(1);
    let u_s: u32 = u_zero.wrapping_sub(1);
    let u_m: u32 = (65536 as u32).wrapping_mul(65536 as u32);
    let i_max: i32 = 2147483647;
    let i_min: i32 = -2147483648;
    let i_a: i32 = i_max.wrapping_add(1);
    let i_s: i32 = i_min.wrapping_sub(1);
    let i_m: i32 = i_min.wrapping_mul(-1);
    println!("{} {} {} {} {} {}", u_a, u_s, u_m, i_a, i_s, i_m);
}
"#;
    let cpu_file = td.write("wrap_cpu.bld", cpu_src);
    let cpu_out = buildc()
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["run", "--release"])
        .arg(&cpu_file)
        .output()
        .expect("run wrap_cpu");
    assert!(cpu_out.status.success(), "{}", diag(&cpu_out));
    assert_eq!(
        String::from_utf8_lossy(&cpu_out.stdout).trim(),
        "0 4294967295 0 -2147483648 2147483647 -2147483648"
    );

    // GPU compute kernel exercising u32 + i32 wrapping_add, wrapping_sub, wrapping_mul
    let gpu_src = r#"#[compute(workgroup_size = [64, 1, 1])]
fn wrap_all_ops(
    u_a: &[u32],
    u_b: &[u32],
    i_a: &[i32],
    i_b: &[i32],
    u_out: &mut [u32],
    i_out: &mut [i32],
) ~ Gpu {
    let idx = gl_GlobalInvocationID.x;
    let ua: u32 = u_a[idx];
    let ub: u32 = u_b[idx];
    let u1: u32 = ua.wrapping_add(ub);
    let u2: u32 = u1.wrapping_sub(ub);
    let u3: u32 = u2.wrapping_mul(ub);
    u_out[idx] = u3;

    let ia: i32 = i_a[idx];
    let ib: i32 = i_b[idx];
    let i1: i32 = ia.wrapping_add(ib);
    let i2: i32 = i1.wrapping_sub(ib);
    let i3: i32 = i2.wrapping_mul(ib);
    i_out[idx] = i3;
}
"#;
    let gpu_file = td.write("wrap_gpu.bld", gpu_src);
    for (target, ext) in [
        ("hlsl", "hlsl"),
        ("glsl", "comp"),
        ("wgsl", "wgsl"),
        ("msl", "metal"),
        ("spirv", "spv"),
    ] {
        let out_path = td.0.join(format!("wrap_ops.{ext}"));
        let res = buildc()
            .arg(&gpu_file)
            .args(["--target", target, "-o"])
            .arg(&out_path)
            .output()
            .expect("compile wrap_gpu");
        assert!(res.status.success(), "{target}: {}", diag(&res));

        if target == "hlsl" {
            let text = std::fs::read_to_string(&out_path).expect("read hlsl");
            assert!(!text.contains("??"), "HLSL emitted ??:\n{text}");
            assert!(
                text.contains("asint((asuint("),
                "HLSL signed wrapping must use asint/asuint:\n{text}"
            );
        } else if target == "msl" {
            let text = std::fs::read_to_string(&out_path).expect("read msl");
            assert!(
                text.contains("static_cast<int>((static_cast<uint>("),
                "MSL signed wrapping must avoid signed C++ UB:\n{text}"
            );
        } else if target == "glsl" {
            let text = std::fs::read_to_string(&out_path).expect("read glsl");
            assert!(!text.contains("??"), "GLSL emitted ??:\n{text}");
        } else if target == "spirv" {
            let spv_bytes = std::fs::read(&out_path).expect("read spv");
            let ops = extract_spirv_opcodes(&spv_bytes);
            // OpIAdd = 128, OpISub = 130, OpIMul = 132
            assert!(ops.contains(&128), "missing OpIAdd (128)");
            assert!(ops.contains(&130), "missing OpISub (130) for wrapping_sub");
            assert!(ops.contains(&132), "missing OpIMul (132) for wrapping_mul");
            if tool_available("spirv-val", "--version") {
                let v = Command::new("spirv-val")
                    .arg(&out_path)
                    .output()
                    .expect("run spirv-val");
                assert!(v.status.success(), "spirv-val failed: {}", diag(&v));
            }
        }
    }
}

#[test]
fn planted_unmarked_overflowing_add_fails_consistently_across_all_6_targets() {
    let td = TempDirGuard::new("planted_overflow");

    // 1. CPU C backend: unmarked u32 + u32 on (u32::MAX, 1) traps at runtime with exit 101.
    let cpu_src = r#"fn unchecked_add(a: u32, b: u32) -> u32 { a + b }
fn main() ~ Console {
    let a: u32 = 4294967295;
    let b: u32 = 1;
    println!("{}", unchecked_add(a, b));
}
"#;
    let cpu_file = td.write("overflow_cpu.bld", cpu_src);
    let cpu_out = buildc()
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["run", "--release"])
        .arg(&cpu_file)
        .output()
        .expect("run overflow_cpu");
    assert_eq!(
        cpu_out.status.code(),
        Some(101),
        "CPU must trap with exit 101 on unmarked u32 + u32 overflow: {}",
        diag(&cpu_out)
    );
    assert!(
        String::from_utf8_lossy(&cpu_out.stderr).contains("attempt to add with overflow"),
        "{}",
        diag(&cpu_out)
    );

    // 2. All 5 GPU targets: unmarked u32 + u32 on buffer data MUST be rejected at compile time.
    let gpu_data_overflow_src = r#"#[compute(workgroup_size = [64, 1, 1])]
fn bad_overflow_add(a_buf: &[u32], b_buf: &[u32], out_buf: &mut [u32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let x: u32 = a_buf[i];
    let y: u32 = b_buf[i];
    out_buf[i] = x + y;
}
"#;
    let gpu_file = td.write("overflow_gpu.bld", gpu_data_overflow_src);

    // Also test compile-time constant overflow on GPU.
    let gpu_const_overflow_src = r#"#[compute(workgroup_size = [64, 1, 1])]
fn bad_const_overflow(out_buf: &mut [u32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    let max_u: u32 = 4294967295;
    let one_u: u32 = 1;
    out_buf[i] = max_u + one_u;
}
"#;
    let gpu_const_file = td.write("const_overflow_gpu.bld", gpu_const_overflow_src);

    for (label, path) in [
        ("data_overflow", &gpu_file),
        ("const_overflow", &gpu_const_file),
    ] {
        for (target, ext) in [
            ("hlsl", "hlsl"),
            ("glsl", "comp"),
            ("wgsl", "wgsl"),
            ("msl", "metal"),
            ("spirv", "spv"),
        ] {
            let out_path = td.0.join(format!("{label}_{target}.{ext}"));
            let res = buildc()
                .arg(path)
                .args(["--target", target, "-o"])
                .arg(&out_path)
                .output()
                .expect("compile planted overflow");
            assert!(
                !res.status.success(),
                "{label} on {target} MUST fail closed at compile time, but succeeded!"
            );
            let stderr = String::from_utf8_lossy(&res.stderr);
            assert!(
                stderr.contains("wrapping_add") || stderr.contains("overflow"),
                "{label} on {target} diagnostic should mention overflow/wrapping_add: {stderr}"
            );
        }
    }
}

#[test]
fn bounded_gpu_index_kernels_compile_without_regression() {
    let td = TempDirGuard::new("gpu_examples");
    for name in [
        "matmul.bld",
        "stencil.bld",
        "reduce.bld",
        "compute_entries.bld",
    ] {
        let src: PathBuf = repo_root().join("examples").join("gpu").join(name);
        assert!(
            Path::new(&src).exists(),
            "missing example {}",
            src.display()
        );
        for (target, ext) in [("spirv", "spv"), ("wgsl", "wgsl"), ("msl", "metal")] {
            let out_path = td.0.join(format!("{name}_{target}.{ext}"));
            let res = buildc()
                .arg(&src)
                .args(["--target", target, "-o"])
                .arg(&out_path)
                .output()
                .expect("compile gpu example");
            assert!(res.status.success(), "{name} on {target}: {}", diag(&res));
        }
    }
}
