// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! Wave 19D Universal Hardware Backends tests:
//! - Apple Metal (`--target msl`)
//! - WebGPU (`--target wgsl`)
//! - 64-bit RISC-V (`--target riscv64`)
//! Covers golden codegen, CLI `build` and direct compile paths, `--entry` selection,
//! workgroup shared memory & barriers, nested control flow, LP64D calling convention,
//! and fail-closed planted unsupported-construct refusals.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate lives under repo root")
        .to_path_buf()
}

fn gpu_example(name: &str) -> PathBuf {
    repo_root().join("examples").join("gpu").join(name)
}

fn temp_dir(tag: &str) -> PathBuf {
    let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "buildlang_wave19d_{}_{}_{}",
        std::process::id(),
        n,
        tag
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_bld(dir: &Path, name: &str, src: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, src).expect("write source");
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

#[test]
fn msl_emits_compute_kernels_structs_loops_and_barriers() {
    let dir = temp_dir("msl_compute");

    // 1. vec_add.bld
    let out_vec = dir.join("vec_add.metal");
    let res = buildc()
        .arg(gpu_example("vec_add.bld"))
        .args(["--target", "msl", "-o"])
        .arg(&out_vec)
        .output()
        .expect("run buildc msl vec_add");
    assert!(res.status.success(), "{}", diag(&res));
    let msl = std::fs::read_to_string(&out_vec).expect("read vec_add.metal");
    assert!(msl.contains("#include <metal_stdlib>"), "{msl}");
    assert!(msl.contains("using namespace metal;"), "{msl}");
    assert!(msl.contains("kernel void vec_add("), "{msl}");
    assert!(msl.contains("const device float* a [[buffer(0)]]"), "{msl}");
    assert!(msl.contains("const device float* b [[buffer(1)]]"), "{msl}");
    assert!(msl.contains("device float* out [[buffer(2)]]"), "{msl}");
    assert!(msl.contains("[[thread_position_in_grid]]"), "{msl}");
    assert!(msl.contains("out[i] ="), "{msl}");

    // 2. matmul.bld (2D grid + nested if + while loop + scalar constant buffers)
    let out_mm = dir.join("matmul.metal");
    let res = buildc()
        .arg(gpu_example("matmul.bld"))
        .args(["--target", "msl", "-o"])
        .arg(&out_mm)
        .output()
        .expect("run buildc msl matmul");
    assert!(res.status.success(), "{}", diag(&res));
    let mm = std::fs::read_to_string(&out_mm).expect("read matmul.metal");
    assert!(mm.contains("kernel void matmul("), "{mm}");
    assert!(mm.contains("constant uint& m [[buffer(0)]]"), "{mm}");
    assert!(mm.contains("while (true) {"), "{mm}");
    assert!(mm.contains("break;"), "{mm}");
    assert!(mm.contains("// workgroup_size: [16, 16, 1]"), "{mm}");

    // 3. reduce.bld (threadgroup shared memory + threadgroup_barrier + local/workgroup IDs)
    let out_red = dir.join("reduce.metal");
    let res = buildc()
        .arg(gpu_example("reduce.bld"))
        .args(["--target", "msl", "-o"])
        .arg(&out_red)
        .output()
        .expect("run buildc msl reduce");
    assert!(res.status.success(), "{}", diag(&res));
    let red = std::fs::read_to_string(&out_red).expect("read reduce.metal");
    assert!(red.contains("threadgroup float scratch[64];"), "{red}");
    assert!(
        red.contains("threadgroup_barrier(mem_flags::mem_threadgroup);"),
        "{red}"
    );
    assert!(red.contains("[[thread_position_in_threadgroup]]"), "{red}");
    assert!(red.contains("[[threadgroup_position_in_grid]]"), "{red}");

    // 4. --entry filtering on multi-entry compute_entries.bld
    let out_entry = dir.join("cs_grid.metal");
    let res = buildc()
        .arg(gpu_example("compute_entries.bld"))
        .args(["--target", "msl", "--entry", "cs_grid", "-o"])
        .arg(&out_entry)
        .output()
        .expect("run buildc msl --entry");
    assert!(res.status.success(), "{}", diag(&res));
    let eg = std::fs::read_to_string(&out_entry).expect("read cs_grid.metal");
    assert!(eg.contains("kernel void cs_grid("), "{eg}");
    assert!(!eg.contains("kernel void cs_scale("), "{eg}");
    assert!(eg.contains("// workgroup_size: [8, 8, 1]"), "{eg}");
}

#[test]
fn wgsl_emits_compute_bindings_workgroup_size_loops_and_barriers() {
    let dir = temp_dir("wgsl_compute");

    // 1. vec_add.bld
    let out_vec = dir.join("vec_add.wgsl");
    let res = buildc()
        .arg(gpu_example("vec_add.bld"))
        .args(["--target", "wgsl", "-o"])
        .arg(&out_vec)
        .output()
        .expect("run buildc wgsl vec_add");
    assert!(res.status.success(), "{}", diag(&res));
    let wgsl = std::fs::read_to_string(&out_vec).expect("read vec_add.wgsl");
    assert!(
        wgsl.contains("@group(0) @binding(0) var<storage, read> a: array<f32>;"),
        "{wgsl}"
    );
    assert!(
        wgsl.contains("@group(0) @binding(1) var<storage, read> b: array<f32>;"),
        "{wgsl}"
    );
    assert!(
        wgsl.contains("@group(0) @binding(2) var<storage, read_write> out: array<f32>;"),
        "{wgsl}"
    );
    assert!(
        wgsl.contains("@compute @workgroup_size(64, 1, 1)"),
        "{wgsl}"
    );
    assert!(
        wgsl.contains("@builtin(global_invocation_id) _bl_gid: vec3<u32>"),
        "{wgsl}"
    );
    assert!(wgsl.contains("out[i] ="), "{wgsl}");

    // 2. matmul.bld (uniform params struct + nested loop/if)
    let out_mm = dir.join("matmul.wgsl");
    let res = buildc()
        .arg(gpu_example("matmul.bld"))
        .args(["--target", "wgsl", "-o"])
        .arg(&out_mm)
        .output()
        .expect("run buildc wgsl matmul");
    assert!(res.status.success(), "{}", diag(&res));
    let mm = std::fs::read_to_string(&out_mm).expect("read matmul.wgsl");
    assert!(mm.contains("struct _Params_matmul {"), "{mm}");
    assert!(
        mm.contains("var<uniform> _params_matmul: _Params_matmul;"),
        "{mm}"
    );
    assert!(mm.contains("@compute @workgroup_size(16, 16, 1)"), "{mm}");
    assert!(mm.contains("loop {"), "{mm}");
    assert!(mm.contains("break;"), "{mm}");

    // 3. reduce.bld (var<workgroup> + workgroupBarrier())
    let out_red = dir.join("reduce.wgsl");
    let res = buildc()
        .arg(gpu_example("reduce.bld"))
        .args(["--target", "wgsl", "-o"])
        .arg(&out_red)
        .output()
        .expect("run buildc wgsl reduce");
    assert!(res.status.success(), "{}", diag(&res));
    let red = std::fs::read_to_string(&out_red).expect("read reduce.wgsl");
    assert!(
        red.contains("var<workgroup> scratch: array<f32, 64>;"),
        "{red}"
    );
    assert!(red.contains("workgroupBarrier();"), "{red}");
    assert!(
        red.contains("@builtin(local_invocation_id) _bl_lid: vec3<u32>"),
        "{red}"
    );
    assert!(
        red.contains("@builtin(workgroup_id) _bl_wid: vec3<u32>"),
        "{red}"
    );

    // 4. Multi-entry compute_entries.bld + --entry cs_grid
    let out_all = dir.join("all.wgsl");
    let res = buildc()
        .arg(gpu_example("compute_entries.bld"))
        .args(["--target", "wgsl", "-o"])
        .arg(&out_all)
        .output()
        .expect("run buildc wgsl multi");
    assert!(res.status.success(), "{}", diag(&res));
    let all = std::fs::read_to_string(&out_all).expect("read all.wgsl");
    assert!(all.contains("@compute @workgroup_size(64, 1, 1)"), "{all}");
    assert!(all.contains("@compute @workgroup_size(8, 8, 1)"), "{all}");
    assert!(all.contains("fn cs_scale("), "{all}");
    assert!(all.contains("fn cs_grid("), "{all}");
    assert!(all.contains("fn cs_bare("), "{all}");
}

#[test]
fn riscv64_emits_lp64d_int_float_branches_and_calls() {
    let dir = temp_dir("riscv64_asm");
    let src = write_bld(
        &dir,
        "math.bld",
        r#"
fn add_mul(a: i64, b: i64) -> i64 {
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < b {
        acc = acc + a;
        i = i + 1;
    }
    return acc;
}

fn lerp_f64(a: f64, b: f64, t: f64) -> f64 {
    return a + (b - a) * t;
}

fn main() -> i64 {
    let r: i64 = add_mul(6, 7);
    let _f: f64 = lerp_f64(1.0, 5.0, 0.5);
    if r == 42 {
        return 0;
    }
    return 1;
}
"#,
    );

    let out_s = dir.join("math.s");
    let res = buildc()
        .arg(&src)
        .args(["--target", "riscv64", "-o"])
        .arg(&out_s)
        .output()
        .expect("run buildc riscv64");
    assert!(res.status.success(), "{}", diag(&res));
    let asm = std::fs::read_to_string(&out_s).expect("read math.s");
    assert!(asm.contains(".option arch, rv64imafd"), "{asm}");
    assert!(asm.contains("sd ra,"), "{asm}");
    assert!(asm.contains("sd s0,"), "{asm}");
    assert!(asm.contains("addi sp, sp, -"), "{asm}");
    assert!(asm.contains("sd a0,"), "{asm}");
    assert!(asm.contains("sd a1,"), "{asm}");
    assert!(asm.contains("fsd fa0,"), "{asm}");
    assert!(asm.contains("fadd.d"), "{asm}");
    assert!(asm.contains("fmul.d"), "{asm}");
    assert!(asm.contains("call add_mul"), "{asm}");
    assert!(asm.contains("call lerp_f64"), "{asm}");
    assert!(asm.contains("bnez t0,"), "{asm}");
    assert!(asm.contains("ret\n"), "{asm}");
}

#[test]
fn build_subcommand_supports_msl_wgsl_and_riscv64() {
    let dir = temp_dir("build_subcmd");
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");

    // 1. Shader project built with `buildc build --target msl` and `--target wgsl`
    write_bld(
        &src_dir,
        "main.bld",
        r#"
#[compute(workgroup_size = [32, 1, 1])]
fn scale(alpha: f32, x: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    out[i] = x[i] * alpha;
}
"#,
    );

    for (target, expected_file, needle) in [
        ("msl", "main.metal", "kernel void scale("),
        ("wgsl", "main.wgsl", "@compute @workgroup_size(32, 1, 1)"),
    ] {
        let res = buildc()
            .args(["build"])
            .arg(&dir)
            .args(["--target", target])
            .output()
            .expect("run buildc build shader");
        assert!(res.status.success(), "target={target}: {}", diag(&res));
        let artifact = dir.join("target").join("debug").join(expected_file);
        assert!(artifact.exists(), "missing {}", artifact.display());
        let text = std::fs::read_to_string(&artifact).expect("read artifact");
        assert!(text.contains(needle), "target={target}:\n{text}");
    }

    // 2. CPU project built with `buildc build --target riscv64`
    let rv_dir = temp_dir("build_subcmd_rv");
    let rv_src = rv_dir.join("src");
    std::fs::create_dir_all(&rv_src).expect("create rv src dir");
    write_bld(&rv_src, "main.bld", "fn main() -> i64 { return 7 * 6; }\n");
    let res = buildc()
        .args(["build"])
        .arg(&rv_dir)
        .args(["--target", "riscv64"])
        .output()
        .expect("run buildc build riscv64");
    assert!(res.status.success(), "{}", diag(&res));
    let rv_artifact = rv_dir.join("target").join("debug").join("main.riscv64.s");
    assert!(rv_artifact.exists(), "missing {}", rv_artifact.display());
    let rv_asm = std::fs::read_to_string(&rv_artifact).expect("read main.riscv64.s");
    assert!(rv_asm.contains(".option arch, rv64imafd"), "{rv_asm}");
    assert!(rv_asm.contains("ret\n"), "{rv_asm}");
}

#[test]
fn planted_unsupported_constructs_are_refused_fail_closed() {
    let dir = temp_dir("planted_refusals");

    // 1. f64 in MSL shader must fail closed and write no file
    let f64_shader = write_bld(
        &dir,
        "bad_f64.bld",
        r#"
#[compute]
fn bad(a: &[f64], out: &mut [f64]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    out[i] = a[i] + 1.0;
}
"#,
    );
    for (target, ext) in [("msl", "metal"), ("wgsl", "wgsl")] {
        let out = dir.join(format!("bad_f64.{ext}"));
        let _ = std::fs::remove_file(&out);
        let res = buildc()
            .arg(&f64_shader)
            .args(["--target", target, "-o"])
            .arg(&out)
            .output()
            .expect("run buildc f64 refusal");
        assert!(
            !res.status.success(),
            "f64 must be refused on {target}\n{}",
            diag(&res)
        );
        let stderr = String::from_utf8_lossy(&res.stderr);
        assert!(
            stderr.contains("f64"),
            "expected f64 refusal diagnostic on {target}, got:\n{stderr}"
        );
        assert!(
            !out.exists(),
            "refused {target} build must not write output"
        );
    }

    // 2. i64 in WGSL shader must fail closed
    let i64_shader = write_bld(
        &dir,
        "bad_i64.bld",
        r#"
#[compute]
fn bad_i64(a: &[i64], out: &mut [i64]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    out[i] = a[i] + 1;
}
"#,
    );
    let out_i64 = dir.join("bad_i64.wgsl");
    let res = buildc()
        .arg(&i64_shader)
        .args(["--target", "wgsl", "-o"])
        .arg(&out_i64)
        .output()
        .expect("run buildc wgsl i64 refusal");
    assert!(!res.status.success(), "i64 must be refused on wgsl");
    assert!(
        !out_i64.exists(),
        "refused wgsl i64 build must not write output"
    );

    // 3. GPU #[compute] shader on riscv64 CPU target must fail closed
    let out_rv_gpu = dir.join("bad_gpu.s");
    let res = buildc()
        .arg(gpu_example("vec_add.bld"))
        .args(["--target", "riscv64", "-o"])
        .arg(&out_rv_gpu)
        .output()
        .expect("run buildc riscv64 gpu refusal");
    assert!(
        !res.status.success(),
        "#[compute] shader must be refused on riscv64\n{}",
        diag(&res)
    );
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(
        stderr.contains("cannot be lowered to CPU target `riscv64`"),
        "expected riscv64 shader refusal diagnostic, got:\n{stderr}"
    );
    assert!(
        !out_rv_gpu.exists(),
        "refused riscv64 build must not write output"
    );

    // 4. --entry on CPU target riscv64 must fail closed
    let res = buildc()
        .arg(gpu_example("compute_entries.bld"))
        .args(["--target", "riscv64", "--entry", "cs_scale", "-o"])
        .arg(dir.join("bad_entry.s"))
        .output()
        .expect("run buildc riscv64 --entry refusal");
    assert!(!res.status.success(), "--entry must be refused on riscv64");
    assert!(String::from_utf8_lossy(&res.stderr).contains("not a shader target"));
}
