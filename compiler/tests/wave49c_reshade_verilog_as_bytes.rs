// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 49C end-to-end integration tests:
//! - N16: ReShade `.fx` UV lowering (`generate_reshade`) ensuring screen UV /
//!   `TEXCOORD` binding by type (`vec2`) and regression guard against
//!   sampling pixel `color.xy` in `vignette_shader.fx`.
//! - N10: Tier-0 Open Silicon BuildLang-to-Verilog-2001 RTL emitter
//!   (`--target verilog`) emitting a synthesizable 2-stage pipelined fixed-point
//!   MAC/ALU core verified bit-for-bit across 1,000 random vectors against the
//!   compiled BuildLang C reference, plus planted bit-flip and non-synthesizable
//!   controls.
//! - Wave 48G: `String::as_bytes()` `0xC0000005` fix rejecting `Vec<u8>` type
//!   confusion at `buildc check`, verifying `s.as_bytes().to_vec()` and
//!   zero-copy `s.as_bytes()[i]`, and enforcing fail-closed C backend container
//!   assignment checks (zero silent `memcpy` type-punning).

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use buildlang::codegen::backend::c::CBackend;
use buildlang::codegen::backend::verilog::{
    simulate_verilog_2stage_pipeline, verify_synthesizable_verilog,
};
use buildlang::codegen::backend::Backend;
use buildlang::codegen::ir::{
    BlockId, LocalId, MirBlock, MirFnSig, MirFunction, MirLocal, MirModule, MirRValue, MirStmt,
    MirTerminator, MirType, MirValue,
};

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
    let dir = std::env::temp_dir().join(format!(
        "buildlang_wave49c_{}_{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn n16_generate_reshade_vignette_and_custom_uv_bind_texcoord_never_color_xy() {
    let dir = make_temp_dir("n16_reshade");
    let vignette_bld = repo_root()
        .join("examples")
        .join("quickstart")
        .join("vignette_shader.bld");
    let out_fx = dir.join("vignette_shader.fx");

    let compile = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            vignette_bld.to_str().unwrap(),
            "--target",
            "hlsl",
            "-o",
            out_fx.to_str().unwrap(),
        ])
        .output()
        .expect("vignette_shader.fx compile failed");
    assert!(
        compile.status.success(),
        "vignette_shader.fx stderr: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let fx = std::fs::read_to_string(&out_fx).expect("missing vignette_shader.fx");
    assert!(
        fx.contains("#include \"ReShade.fxh\""),
        "missing ReShade.fxh:\n{fx}"
    );
    assert!(
        fx.contains("float4 pos : SV_Position, float2 uv : TEXCOORD"),
        "missing SV_Position / TEXCOORD signature:\n{fx}"
    );
    assert!(
        fx.contains("tex2D(ReShade::BackBuffer, uv)"),
        "must sample BackBuffer with screen UV:\n{fx}"
    );
    assert!(
        fx.contains("vignette(uv.x, uv.y, 0.5, 0.6)"),
        "vignette must pass screen uv.x, uv.y:\n{fx}"
    );
    assert!(
        !fx.contains("vignette(color.x, color.y") && !fx.contains("color.xy"),
        "regression: generated .fx must never pass pixel color.xy as UV:\n{fx}"
    );

    // Custom vec2 parameter name (`screen_uv`) must still receive `: TEXCOORD`,
    // and a no-param fragment shader in ReShade mode must auto-add `TEXCOORD`.
    let custom_bld = dir.join("custom_uv.bld");
    let custom_fx = dir.join("custom_uv.fx");
    std::fs::write(
        &custom_bld,
        r#"
#[fragment]
fn PS_ScreenUv(screen_uv: vec2) -> vec4 {
    let c = tex2d(screen_uv);
    vec4(c.x * screen_uv.x, c.y * screen_uv.y, c.z, 1.0)
}
"#,
    )
    .unwrap();
    let res = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            custom_bld.to_str().unwrap(),
            "--target",
            "hlsl",
            "-o",
            custom_fx.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "custom_uv.fx failed: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    let custom_txt = std::fs::read_to_string(&custom_fx).unwrap();
    assert!(
        custom_txt.contains("float4 pos : SV_Position, float2 screen_uv : TEXCOORD"),
        "arbitrary vec2 fragment param must bind TEXCOORD:\n{custom_txt}"
    );
    assert!(
        custom_txt.contains("tex2D(ReShade::BackBuffer, screen_uv)"),
        "custom_uv.fx must sample BackBuffer at screen_uv:\n{custom_txt}"
    );

    // Ensure compiler/README.md no longer teaches the `vignette(color.x, color.y)` bug.
    let readme = std::fs::read_to_string(repo_root().join("compiler").join("README.md")).unwrap();
    assert!(
        !readme.contains("vignette(color.x, color.y"),
        "compiler/README.md still contains color.x/color.y UV bug"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn n10_verilog_rtl_pipelined_mac_alu_1000_vectors_bit_exact_and_planted_bitflip() {
    let dir = make_temp_dir("n10_verilog");
    let src = dir.join("fixed_mac_alu.bld");
    let out_v = dir.join("fixed_mac_alu.v");

    // Pipelined fixed-point 32-bit MAC/ALU core + deterministic 1,000-vector
    // SplitMix64 reference harness in `main()`.
    std::fs::write(
        &src,
        r#"
fn fixed_mac_alu(op: i32, a: i32, b: i32, acc: i32) -> i32 {
    let prod: i32 = a * b;
    let sh: i32 = b & 7;
    if op == 0 {
        acc + prod
    } else if op == 1 {
        acc - prod
    } else if op == 2 {
        a + b
    } else if op == 3 {
        a - b
    } else if op == 4 {
        a & b
    } else if op == 5 {
        a | b
    } else if op == 6 {
        a ^ b
    } else {
        a >> sh
    }
}

fn next_rng(state: i64) -> i64 {
    // Portable linear congruential PRNG (exact 31-bit modulus, zero UB overflow).
    (state * 1103515245 + 12345) & 2147483647
}

fn main() -> i32 ~ Console {
    let mut s: i64 = 424242;
    let mut i: i64 = 0;
    while i < 1000 {
        s = next_rng(s);
        let op: i32 = ((s >> 16) & 7) as i32;
        s = next_rng(s);
        let a: i32 = (((s >> 8) & 65535) - 32768) as i32;
        s = next_rng(s);
        let b: i32 = (((s >> 8) & 65535) - 32768) as i32;
        s = next_rng(s);
        let acc: i32 = (((s >> 8) & 131071) - 65536) as i32;
        let out: i32 = fixed_mac_alu(op, a, b, acc);
        println!("{} {} {} {} {}", op, a, b, acc, out);
        i = i + 1;
    }
    0
}
"#,
    )
    .unwrap();

    // 1. Emit synthesizable Verilog-2001 RTL via `buildc --target verilog`.
    let rtl_compile = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            src.to_str().unwrap(),
            "--target",
            "verilog",
            "-o",
            out_v.to_str().unwrap(),
        ])
        .output()
        .expect("verilog compile failed");
    assert!(
        rtl_compile.status.success(),
        "verilog compile stderr: {}",
        String::from_utf8_lossy(&rtl_compile.stderr)
    );
    let rtl = std::fs::read_to_string(&out_v).expect("missing fixed_mac_alu.v");
    verify_synthesizable_verilog(&rtl).expect("emitted RTL must be synthesizable");
    assert!(
        rtl.contains("module fixed_mac_alu ("),
        "missing module:\n{rtl}"
    );
    assert!(
        rtl.contains("always @(posedge clk or negedge rst_n)"),
        "missing clocked pipeline block:\n{rtl}"
    );

    // 2. Run the compiled BuildLang native binary to collect 1,000 reference vectors.
    let run = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["run", src.to_str().unwrap()])
        .output()
        .expect("native run failed");
    assert!(
        run.status.success(),
        "native run stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    let mut vectors: Vec<Vec<(&str, i64)>> = Vec::with_capacity(1000);
    let mut expected: Vec<i64> = Vec::with_capacity(1000);
    for line in stdout.lines() {
        let parts: Vec<i64> = line
            .split_whitespace()
            .map(|t| t.parse::<i64>().unwrap())
            .collect();
        if parts.len() == 5 {
            vectors.push(vec![
                ("op", parts[0]),
                ("a", parts[1]),
                ("b", parts[2]),
                ("acc", parts[3]),
            ]);
            expected.push(parts[4]);
        }
    }
    assert_eq!(vectors.len(), 1000, "expected 1,000 random test vectors");
    let mut op_hist = [0usize; 8];
    for v in &vectors {
        let op = v[0].1 as usize;
        op_hist[op] += 1;
    }
    for (op, &cnt) in op_hist.iter().enumerate() {
        assert!(
            cnt >= 80,
            "every ALU opcode 0..=7 must be thoroughly exercised, op {op} had only {cnt} vectors"
        );
    }

    // 3. Simulate the EMITTED Verilog-2001 2-stage pipeline across all 1,000 vectors.
    let rtl_outs = simulate_verilog_2stage_pipeline(&rtl, "fixed_mac_alu", &vectors)
        .expect("emitted Verilog simulation must succeed");
    assert_eq!(rtl_outs.len(), 1000);
    let mismatches = rtl_outs
        .iter()
        .zip(expected.iter())
        .filter(|(got, exp)| got != exp)
        .count();
    assert_eq!(
        mismatches, 0,
        "emitted Verilog RTL must match BuildLang reference on 1,000/1,000 random vectors"
    );

    // 4. Planted bit-flip controls:
    //    (a) Flip `+` to `-` in the emitted RTL datapath, and
    //    (b) Flip 1 LSB bit on `assign comb_result = ...;` (`^ $signed(32'h1)`).
    //    Assert the cycle-accurate differential verifier catches both fail-closed.
    let mod_start = rtl
        .find("module fixed_mac_alu (")
        .expect("fixed_mac_alu module missing");
    let (prefix, mac_mod) = rtl.split_at(mod_start);
    assert!(mac_mod.contains(" + "), "expected `+` in fixed_mac_alu RTL");
    let tampered_op_rtl = format!("{prefix}{}", mac_mod.replace(" + ", " - "));
    let bad_outs = simulate_verilog_2stage_pipeline(&tampered_op_rtl, "fixed_mac_alu", &vectors)
        .expect("tampered RTL still parses");
    let tampered_mismatches = bad_outs
        .iter()
        .zip(expected.iter())
        .filter(|(got, exp)| got != exp)
        .count();
    assert!(
        tampered_mismatches > 100,
        "planted RTL `+ -> -` mutation must be caught by 1,000-vector check, got {tampered_mismatches}"
    );

    let bitflip_rtl = format!(
        "{prefix}{}",
        mac_mod.replacen(
            "assign comb_result = ",
            "assign comb_result = $signed(32'h1) ^ ",
            1
        )
    );
    let bitflip_outs = simulate_verilog_2stage_pipeline(&bitflip_rtl, "fixed_mac_alu", &vectors)
        .expect("bit-flipped RTL still parses");
    let bitflip_mismatches = bitflip_outs
        .iter()
        .zip(expected.iter())
        .filter(|(got, exp)| got != exp)
        .count();
    assert_eq!(
        bitflip_mismatches, 1000,
        "planted 1-bit LSB flip on comb_result must fail all 1,000/1,000 vectors"
    );

    // 5. Planted non-synthesizable controls: reject `initial` / `#5` delay and reject
    //    floating-point BuildLang functions at compile time.
    let bad_initial = format!("{rtl}\ninitial $display(\"bad\");\n");
    assert!(verify_synthesizable_verilog(&bad_initial).is_err());

    let float_src = dir.join("bad_float.bld");
    std::fs::write(&float_src, "fn fadd(a: f64, b: f64) -> f64 { a + b }\n").unwrap();
    let bad_compile = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            float_src.to_str().unwrap(),
            "--target",
            "verilog",
            "-o",
            dir.join("bad_float.v").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !bad_compile.status.success(),
        "floating-point function must be rejected fail-closed by Verilog RTL backend"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave48g_string_as_bytes_type_error_to_vec_roundtrip_and_c_backend_guard() {
    let dir = make_temp_dir("wave48g_as_bytes");

    // 1. Planted control (exact Wave 48G 0xC0000005 reproducer):
    //    `let b: Vec<u8> = s.as_bytes();` must fail cleanly at `buildc check`.
    let bad_src = dir.join("repro_as_bytes_crash.bld");
    std::fs::write(
        &bad_src,
        r#"
fn main() ~ Console {
    let s: String = "hello_world_12345".to_string();
    let b: Vec<u8> = s.as_bytes();
    println("{}", b[0]);
}
"#,
    )
    .unwrap();
    let check_bad = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["check", bad_src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !check_bad.status.success(),
        "assigning s.as_bytes() (&[u8]) to Vec<u8> must fail type checking"
    );

    // 2. Valid `s.as_bytes().to_vec()` and zero-copy `let b = s.as_bytes(); b[0]`
    //    must pass `buildc check` and run cleanly in debug and `--release`.
    let good_src = dir.join("as_bytes_ok.bld");
    std::fs::write(
        &good_src,
        r#"
fn main() -> i32 ~ Console {
    let s: String = "hello_world_12345".to_string();
    let b = s.as_bytes();
    let v: Vec<u8> = s.as_bytes().to_vec();
    println!("{} {} {}", b[0], v.len(), v[0]);
    0
}
"#,
    )
    .unwrap();
    for args in [
        vec!["run", good_src.to_str().unwrap()],
        vec!["run", "--release", good_src.to_str().unwrap()],
    ] {
        let out = Command::new(buildc_bin())
            .env("BUILDLANG_STDLIB", stdlib_dir())
            .args(&args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "args {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        let txt = String::from_utf8_lossy(&out.stdout);
        assert_eq!(txt.trim(), "104 17 104", "unexpected output: {txt}");
    }

    // 3. CBackend fail-closed unit guard: raw MIR assigning `BuildString` into
    //    `Vec<u8>` must return Err(CodegenError::Unsupported), never emit `memcpy`.
    let mut mir = MirModule::new("bad_container_assign");
    let sig = MirFnSig::new(
        vec![MirType::Struct(Arc::from("BuildString"))],
        MirType::Void,
    );
    let mut func = MirFunction::new("bad_fn", sig);
    let mut p0 = MirLocal::named(LocalId(0), "s", MirType::Struct(Arc::from("BuildString")));
    p0.is_param = true;
    func.add_local(p0);
    func.add_local(MirLocal::named(
        LocalId(1),
        "b",
        MirType::Vec(Box::new(MirType::u8())),
    ));
    let mut bb = MirBlock::new(BlockId::ENTRY);
    bb.push_stmt(MirStmt::assign(
        LocalId(1),
        MirRValue::Use(MirValue::Local(LocalId(0))),
    ));
    bb.set_terminator(MirTerminator::Return(None));
    func.add_block(bb);
    mir.add_function(func);

    let mut c_backend = CBackend::new();
    let err = c_backend
        .generate(&mir)
        .expect_err("CBackend must reject BuildString -> Vec<u8> assignment fail-closed");
    let msg = format!("{err}");
    assert!(
        msg.contains("incompatible runtime container"),
        "expected incompatible runtime container refusal, got: {msg}"
    );

    // 4. Regression for `macros.rs` collect_as_string: `let chars: Vec<char> = s.chars().collect();`
    //    inside a `-> String` function must collect into `Vec<char>`, not `BuildString`,
    //    while returning `s.chars().rev().collect()` into `String` still produces `BuildString`.
    let chars_collect_src = dir.join("chars_collect_in_string_fn.bld");
    std::fs::write(
        &chars_collect_src,
        r#"
fn reverse_via_vec(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = chars.len();
    while i > 0 {
        i = i - 1;
        out.push(chars[i]);
    }
    out
}

fn main() -> i32 ~ Console {
    let r = reverse_via_vec("buildlang");
    println!("{}", r);
    0
}
"#,
    )
    .unwrap();
    let run_chars = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["run", chars_collect_src.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        run_chars.status.success(),
        "reverse_via_vec failed: {}",
        String::from_utf8_lossy(&run_chars.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run_chars.stdout).trim(),
        "gnaldliub"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
