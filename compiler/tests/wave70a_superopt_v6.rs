// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 70A end-to-end integration tests for the V6 Certified E-Graph
//! Superoptimizer (`R21..R30`):
//! - Algebraic identities, unary constant folding, double-negation/involution
//!   elimination (`--x`, `~~x`, `!!b`), exact integer unit div/rem (`x / 1`, `x % 1`),
//!   unsigned power-of-two shift/mask strength reduction (`u / 2^k`, `u % 2^k`),
//!   zero-shift folding, integer associative constant folding, same-block pure CSE
//!   with commutative canonicalization and copy propagation, redundant pure load
//!   folding with strict store invalidation, and IEEE-754 signed-zero preservation.
//! - Direct V5-vs-V6 MIR cost benchmark proving >= 10% additional cost reduction
//!   over V5 with 0.0 runtime semantic drift and 100% `buildc verify-cert` PASS.
//! - Planted unsound-rewrite and forged-certificate rejection controls.

use std::path::PathBuf;
use std::process::Command;

use buildlang::codegen::ir::{
    BinOp, BlockId, LocalId, MirBlock, MirConst, MirFnSig, MirFunction, MirLocal, MirModule,
    MirRValue, MirStmt, MirStmtKind, MirTerminator, MirType, MirValue, UnaryOp,
};
use buildlang::codegen::lower::MirLowerer;
use buildlang::codegen::superopt::{
    compute_certificate_sha256, expr_cost, superoptimize_module, superoptimize_module_v5_baseline,
    verify_certificate, verify_step_axiom, CertExpr, RewriteErrorWitness, SuperoptCertificate,
    SuperoptRule, SUPEROPT_CERT_SCHEMA,
};
use buildlang::lexer::Lexer;
use buildlang::parser::Parser;
use buildlang::types::TypeContext;
use buildlang::SourceFile;

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
        "buildlang_wave70a_{}_{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn lower_source_to_mir(src: &str) -> MirModule {
    let sf = SourceFile::new("wave70a_bench.bld", src);
    let mut lexer = Lexer::new(&sf);
    let tokens = lexer.tokenize().expect("source must lex");
    let mut parser = Parser::new(&sf, tokens);
    let ast = parser.parse().expect("source must parse");
    let ctx = TypeContext::new();
    let lowerer = MirLowerer::new(&ctx);
    lowerer
        .lower_module_all(&ast)
        .expect("source must lower to MIR")
}

/// Measure total remaining pure-expression + load cost across a `MirModule`.
fn total_module_mir_cost(module: &MirModule) -> u64 {
    let mut total = 0u64;
    for func in &module.functions {
        let Some(blocks) = &func.blocks else {
            continue;
        };
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { value, .. } = &stmt.kind {
                    total += match value {
                        MirRValue::UnaryOp { .. } => 1,
                        MirRValue::BinaryOp { op, .. } => match op {
                            BinOp::Add
                            | BinOp::Sub
                            | BinOp::BitAnd
                            | BinOp::BitOr
                            | BinOp::BitXor
                            | BinOp::Shl
                            | BinOp::Shr => 1,
                            BinOp::Mul => 3,
                            BinOp::Div | BinOp::Rem => 8,
                            _ => 4,
                        },
                        MirRValue::FieldAccess { .. } | MirRValue::Deref { .. } => 2,
                        MirRValue::IndexAccess { .. } => 3,
                        _ => 0,
                    };
                }
            }
            if let Some(MirTerminator::Call { func: callee, .. }) = &block.terminator {
                if let MirValue::Function(n) | MirValue::Global(n) = callee {
                    total += match n.as_ref() {
                        "fma" => 2,
                        "exp" | "exp2" | "log" | "log2" => 12,
                        _ => 0,
                    };
                }
            }
        }
    }
    total
}

const V6_BENCH_PROGRAM: &str = r#"
struct Packet {
    a: i64,
    b: i64,
    u: u64,
}

fn v6_integer_kernel(p: Packet, x: i64, y: i64, u: u64) -> i64 {
    // R29: Redundant field loads on immutable struct param
    let f1: i64 = p.a;
    let f2: i64 = p.a;
    let f3: i64 = p.b;
    let f4: i64 = p.b;

    // R28: Same-block pure CSE (commutative `x * y` and `y * x`, plus copy prop)
    let xc: i64 = x;
    let prod1: i64 = x * y;
    let prod2: i64 = y * xc;
    let sum_cse1: i64 = f1 + f3;
    let sum_cse2: i64 = f4 + f2;

    // R22: Double negation & bitwise involution
    let neg1: i64 = -x;
    let neg2: i64 = -neg1;
    let inv1: i64 = !y;
    let inv2: i64 = !inv1;

    // R23: Unit integer div / rem
    let d1: i64 = prod1 / 1;
    let r1: i64 = prod2 % 1;

    // R24 & R25: Unsigned power-of-two div -> shr and rem -> bitand
    let u_div: u64 = u / 16;
    let u_rem: u64 = u % 64;
    let u_div2: u64 = p.u / 32;
    let u_rem2: u64 = p.u % 128;

    // R26: Shift by zero identity
    let sh0: i64 = neg2 << 0;

    // R27: Integer associative constant folding
    let a1: i64 = x + 7;
    let a2: i64 = a1 + 9;
    let m1: i64 = y * 3;
    let m2: i64 = m1 * 5;
    let s1: i64 = x << 2;
    let s2: i64 = s1 << 3;
    let k1: i64 = y ^ 255;
    let k2: i64 = k1 ^ 255;

    d1 + r1 + sum_cse1 + sum_cse2 + sh0 + inv2 + a2 + m2 + s2 + k2
        + (u_div as i64) + (u_rem as i64) + (u_div2 as i64) + (u_rem2 as i64)
}

fn v6_signed_div_must_stay_exact(x: i64) -> i64 {
    // Signed division/remainder by power of 2 MUST NOT be rewritten to shift/mask!
    let q: i64 = x / 8;
    let r: i64 = x % 8;
    q * 100 + r
}

fn v6_float_signed_zero_and_involution(v: f64) -> f64 {
    let n1: f64 = -v;
    let n2: f64 = -n1;
    let z1: f64 = n2 - 0.0;
    let z2: f64 = z1 + (-0.0);
    z2
}

fn v6_store_invalidates_load(mut p: Packet) -> i64 {
    let before: i64 = p.a;
    p.a = before + 100;
    let after: i64 = p.a;
    before * 1000 + after
}

fn main() -> i32 ~ Console {
    let p = Packet { a: 17, b: 29, u: 1029 };
    let r_int: i64 = v6_integer_kernel(p, 11, 13, 509);
    let r_sdiv: i64 = v6_signed_div_must_stay_exact(-19);
    let r_store: i64 = v6_store_invalidates_load(Packet { a: 7, b: 3, u: 1 });
    let r_neg0: f64 = v6_float_signed_zero_and_involution(-0.0);
    let r_pos: f64 = v6_float_signed_zero_and_involution(3.25);
    // Verify sign bit of `-0.0` is preserved: `1.0 / (-0.0) < 0.0` whereas `1.0 / (+0.0) > 0.0`.
    let neg0_sign_preserved: i64 = if (1.0 / r_neg0) < 0.0 { 1 } else { 0 };
    println!("{} {} {} {} {:.2}", r_int, r_sdiv, r_store, neg0_sign_preserved, r_pos);
    0
}
"#;

#[test]
fn wave70a_v6_rules_end_to_end_zero_drift_and_verify_cert() {
    let dir = make_temp_dir("v6_e2e");
    let src_path = dir.join("v6_bench.bld");
    let cert_path = dir.join("v6_bench.superopt-cert.json");
    std::fs::write(&src_path, V6_BENCH_PROGRAM).unwrap();

    // 1. Run unoptimized baseline
    let baseline = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["run", src_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        baseline.status.success(),
        "baseline run failed: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let base_out = String::from_utf8_lossy(&baseline.stdout).trim().to_string();

    // 2. Run with V6 `--superopt --emit-cert`
    let opt = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args([
            "run",
            "--superopt",
            "--emit-cert",
            cert_path.to_str().unwrap(),
            src_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        opt.status.success(),
        "superopt run failed: {}",
        String::from_utf8_lossy(&opt.stderr)
    );
    let opt_out = String::from_utf8_lossy(&opt.stdout).trim().to_string();

    // 0.0 bit-exact runtime semantic drift (including negative signed div/rem,
    // post-store load invalidation, and IEEE-754 `-0.0` sign bit!).
    assert_eq!(
        base_out, opt_out,
        "V6 superopt drifted from baseline output: base='{base_out}' opt='{opt_out}'"
    );
    let fields: Vec<&str> = opt_out.split_whitespace().collect();
    assert_eq!(fields.len(), 5, "unexpected output format: {opt_out}");
    // -19 / 8 == -2, -19 % 8 == -3 -> -203
    assert_eq!(fields[1], "-203", "signed div/rem corrupted: {opt_out}");
    // before=7, after=107 -> 7107 (proves store invalidated redundant load cache!)
    assert_eq!(
        fields[2], "7107",
        "store-after-load aliasing bug: {opt_out}"
    );
    // IEEE-754 `-0.0` sign bit strictly preserved!
    assert_eq!(fields[3], "1", "IEEE-754 -0.0 sign bit lost: {opt_out}");
    assert_eq!(fields[4], "3.25");

    // 3. Verify emitted certificate via CLI `buildc verify-cert`
    let verify_cli = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["verify-cert", cert_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        verify_cli.status.success(),
        "buildc verify-cert failed: {}",
        String::from_utf8_lossy(&verify_cli.stderr)
    );

    // 4. Inspect certificate rules: confirm V6 rules fired and verified cleanly.
    let cert_raw = std::fs::read_to_string(&cert_path).unwrap();
    let cert: SuperoptCertificate = serde_json::from_str(&cert_raw).unwrap();
    assert_eq!(cert.schema, SUPEROPT_CERT_SCHEMA);
    verify_certificate(&cert).expect("V6 certificate must verify");

    let rules: Vec<&SuperoptRule> = cert.steps.iter().map(|s| &s.rule).collect();
    for required in [
        SuperoptRule::DoubleNegInvolution,
        SuperoptRule::IntDivRemId,
        SuperoptRule::UintDivPow2Shr,
        SuperoptRule::UintRemPow2And,
        SuperoptRule::IntShiftZero,
        SuperoptRule::IntAssocConstFold,
        SuperoptRule::PureCse,
        SuperoptRule::RedundantLoadFold,
        SuperoptRule::FloatSignedZeroId,
    ] {
        assert!(
            rules.contains(&&required),
            "expected V6 rule {:?} to fire, got rules: {:?}",
            required,
            rules
        );
    }

    // 5. Planted forged CLI certificate check: tamper cert_sha256 and ensure `buildc verify-cert` rejects.
    let mut bad_cert = cert.clone();
    bad_cert.cert_sha256 = "f".repeat(64);
    let bad_path = dir.join("forged.superopt-cert.json");
    std::fs::write(&bad_path, serde_json::to_string_pretty(&bad_cert).unwrap()).unwrap();
    let bad_cli = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["verify-cert", bad_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !bad_cli.status.success(),
        "tampered certificate must be rejected by buildc verify-cert"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wave70a_v6_vs_v5_benchmark_exceeds_10pct_additional_mir_cost_reduction() {
    // Compare exact V5 (`R1..R20`) vs V6 (`R1..R30`) on identical MIR modules:
    // the V6 benchmark program PLUS real graduated `universe/workstation/` kernels!
    let mut sources: Vec<(String, String)> =
        vec![("v6_bench_program".to_string(), V6_BENCH_PROGRAM.to_string())];
    for rel in [
        "universe/workstation/scale_algebra.bld",
        "universe/workstation/kcore_color.bld",
        "universe/workstation/conservation.bld",
    ] {
        let p = repo_root().join(rel);
        if p.exists() {
            sources.push((rel.to_string(), std::fs::read_to_string(&p).unwrap()));
        }
    }

    let mut total_raw_cost = 0u64;
    let mut total_v5_remaining = 0u64;
    let mut total_v6_remaining = 0u64;
    let mut total_v5_saved = 0u64;
    let mut total_v6_saved = 0u64;

    for (name, src) in &sources {
        let base_mir = lower_source_to_mir(src);
        let raw_cost = total_module_mir_cost(&base_mir);

        let mut v5_mir = base_mir.clone();
        let v5_cert = superoptimize_module_v5_baseline(&mut v5_mir, "c")
            .unwrap_or_else(|e| panic!("V5 superopt failed on {name}: {e}"));
        verify_certificate(&v5_cert).unwrap();
        let v5_rem = total_module_mir_cost(&v5_mir);

        let mut v6_mir = base_mir;
        let v6_cert = superoptimize_module(&mut v6_mir, "c")
            .unwrap_or_else(|e| panic!("V6 superopt failed on {name}: {e}"));
        verify_certificate(&v6_cert).unwrap();
        let v6_rem = total_module_mir_cost(&v6_mir);

        let v5_s = v5_cert
            .total_cost_before
            .saturating_sub(v5_cert.total_cost_after);
        let v6_s = v6_cert
            .total_cost_before
            .saturating_sub(v6_cert.total_cost_after);
        eprintln!(
            "  [{name}] raw={raw_cost} v5_rem={v5_rem} v6_rem={v6_rem} (delta={:.2}%) v5_saved={v5_s} v6_saved={v6_s}",
            if v5_rem > 0 { ((v5_rem as f64 - v6_rem as f64) / v5_rem as f64) * 100.0 } else { 0.0 }
        );
        assert!(
            v6_rem <= v5_rem,
            "V6 must never increase MIR cost over V5 on {name}: v5={v5_rem} v6={v6_rem}"
        );
        total_raw_cost += raw_cost;
        total_v5_remaining += v5_rem;
        total_v6_remaining += v6_rem;
        total_v5_saved += v5_s;
        total_v6_saved += v6_s;
    }

    assert!(total_v5_remaining > 0);
    let additional_reduction_pct =
        ((total_v5_remaining - total_v6_remaining) as f64 / total_v5_remaining as f64) * 100.0;
    let extra_saved_pct = if total_v5_saved > 0 {
        ((total_v6_saved - total_v5_saved) as f64 / total_v5_saved as f64) * 100.0
    } else {
        100.0
    };
    eprintln!(
        "[WAVE70A BENCH] raw_cost={total_raw_cost} v5_rem={total_v5_remaining} v6_rem={total_v6_remaining} \
         additional_mir_cost_reduction={additional_reduction_pct:.2}% \
         v5_saved={total_v5_saved} v6_saved={total_v6_saved} extra_saved={extra_saved_pct:.2}%"
    );
    assert!(
        additional_reduction_pct >= 10.0,
        "V6 must achieve >= 10% additional MIR cost reduction over V5, got {additional_reduction_pct:.2}%"
    );
}

#[test]
fn wave70a_planted_unsound_rewrites_and_forged_v6_certs_fail_closed() {
    // 1. Signed `i64 / 8 -> i64 >> 3` is UNSOUND for negative ints and MUST be rejected.
    let bad_sdiv_before = CertExpr::BinOp {
        op: BinOp::Div,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::Int(8, MirType::i64())),
    };
    let bad_sdiv_after = CertExpr::BinOp {
        op: BinOp::Shr,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::Int(3, MirType::i64())),
    };
    assert!(
        verify_step_axiom(
            &SuperoptRule::UintDivPow2Shr,
            &bad_sdiv_before,
            &bad_sdiv_after
        )
        .is_err(),
        "signed i64 / 8 -> i64 >> 3 must be rejected fail-closed"
    );

    // 2. Signed `i64 % 8 -> i64 & 7` is UNSOUND for negative ints and MUST be rejected.
    let bad_srem_before = CertExpr::BinOp {
        op: BinOp::Rem,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::Int(8, MirType::i64())),
    };
    let bad_srem_after = CertExpr::BinOp {
        op: BinOp::BitAnd,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::Int(7, MirType::i64())),
    };
    assert!(
        verify_step_axiom(
            &SuperoptRule::UintRemPow2And,
            &bad_srem_before,
            &bad_srem_after
        )
        .is_err(),
        "signed i64 % 8 -> i64 & 7 must be rejected fail-closed"
    );

    // 3. Float `x + (+0.0) -> x` and `x - (-0.0) -> x` violate IEEE-754 signed-zero semantics.
    let bad_pos_zero_add = CertExpr::BinOp {
        op: BinOp::Add,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::FloatBits(0.0_f64.to_bits(), MirType::f64())),
    };
    assert!(
        verify_step_axiom(
            &SuperoptRule::FloatSignedZeroId,
            &bad_pos_zero_add,
            &CertExpr::Local(0)
        )
        .is_err(),
        "x + (+0.0) -> x violates IEEE-754 -0.0 and must be rejected"
    );
    let bad_neg_zero_sub = CertExpr::BinOp {
        op: BinOp::Sub,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::FloatBits((-0.0_f64).to_bits(), MirType::f64())),
    };
    assert!(
        verify_step_axiom(
            &SuperoptRule::FloatSignedZeroId,
            &bad_neg_zero_sub,
            &CertExpr::Local(0)
        )
        .is_err(),
        "x - (-0.0) -> x violates IEEE-754 -0.0 and must be rejected"
    );

    // 4. Float associative constant reassociation `(x + 1e16) + (-1e16) -> x + 0.0` MUST fail.
    let bad_float_assoc_before = CertExpr::BinOp {
        op: BinOp::Add,
        left: Box::new(CertExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(CertExpr::Local(0)),
            right: Box::new(CertExpr::FloatBits(1e16_f64.to_bits(), MirType::f64())),
        }),
        right: Box::new(CertExpr::FloatBits((-1e16_f64).to_bits(), MirType::f64())),
    };
    let bad_float_assoc_after = CertExpr::BinOp {
        op: BinOp::Add,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::FloatBits(0.0_f64.to_bits(), MirType::f64())),
    };
    assert!(
        verify_step_axiom(
            &SuperoptRule::IntAssocConstFold,
            &bad_float_assoc_before,
            &bad_float_assoc_after
        )
        .is_err(),
        "float associative constant folding is unsound and must be rejected"
    );

    // 5. `0 / x -> 0` traps when `x == 0` and MUST be rejected.
    let bad_zero_div = CertExpr::BinOp {
        op: BinOp::Div,
        left: Box::new(CertExpr::Int(0, MirType::i64())),
        right: Box::new(CertExpr::Local(0)),
    };
    assert!(
        verify_step_axiom(
            &SuperoptRule::IntDivRemId,
            &bad_zero_div,
            &CertExpr::Int(0, MirType::i64())
        )
        .is_err(),
        "0 / x -> 0 traps on x == 0 and must be rejected"
    );

    // 6. Mismatched double unary `Neg(BitNot(x)) -> x` MUST be rejected.
    let bad_involution = CertExpr::UnaryOp {
        op: UnaryOp::Neg,
        operand: Box::new(CertExpr::UnaryOp {
            op: UnaryOp::BitNot,
            operand: Box::new(CertExpr::Local(0)),
        }),
    };
    assert!(
        verify_step_axiom(
            &SuperoptRule::DoubleNegInvolution,
            &bad_involution,
            &CertExpr::Local(0)
        )
        .is_err(),
        "mismatched Neg(BitNot(x)) -> x must be rejected"
    );

    // 7. Non-commutative CSE forgery: `a - b` reusing `b - a` MUST be rejected.
    let bad_cse_before = CertExpr::BinOp {
        op: BinOp::Sub,
        left: Box::new(CertExpr::Local(0)),
        right: Box::new(CertExpr::Local(1)),
    };
    let bad_cse_after = CertExpr::ReuseLocal {
        local: 2,
        equivalent: Box::new(CertExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(CertExpr::Local(1)),
            right: Box::new(CertExpr::Local(0)),
        }),
    };
    assert!(
        verify_step_axiom(&SuperoptRule::PureCse, &bad_cse_before, &bad_cse_after).is_err(),
        "non-commutative Sub CSE swap must be rejected"
    );

    // 8. Mismatched field load forgery: `p.b` reusing `p.a` MUST be rejected.
    let bad_load_before = CertExpr::FieldLoad {
        base: Box::new(CertExpr::Local(0)),
        field_name: "b".to_string(),
    };
    let bad_load_after = CertExpr::ReuseLocal {
        local: 2,
        equivalent: Box::new(CertExpr::FieldLoad {
            base: Box::new(CertExpr::Local(0)),
            field_name: "a".to_string(),
        }),
    };
    assert!(
        verify_step_axiom(
            &SuperoptRule::RedundantLoadFold,
            &bad_load_before,
            &bad_load_after
        )
        .is_err(),
        "mismatched FieldLoad CSE must be rejected"
    );

    // 9. Self-aliasing ReuseLocal (even with recomputed SHA-256!) MUST be rejected.
    let mut module = MirModule::new("self_alias_unit");
    let sig = MirFnSig::new(vec![MirType::i64(), MirType::i64()], MirType::i64());
    let mut func = MirFunction::new("f", sig);
    let mut p0 = MirLocal::named(LocalId(0), "a", MirType::i64());
    p0.is_param = true;
    func.add_local(p0);
    let mut p1 = MirLocal::named(LocalId(1), "b", MirType::i64());
    p1.is_param = true;
    func.add_local(p1);
    func.add_local(MirLocal::new(LocalId(2), MirType::i64()));
    func.add_local(MirLocal::new(LocalId(3), MirType::i64()));
    let mut bb = MirBlock::new(BlockId::ENTRY);
    bb.push_stmt(MirStmt::assign(
        LocalId(2),
        MirRValue::BinaryOp {
            op: BinOp::Mul,
            left: MirValue::Local(LocalId(0)),
            right: MirValue::Local(LocalId(1)),
        },
    ));
    bb.push_stmt(MirStmt::assign(
        LocalId(3),
        MirRValue::BinaryOp {
            op: BinOp::Mul,
            left: MirValue::Local(LocalId(1)),
            right: MirValue::Local(LocalId(0)),
        },
    ));
    bb.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(3)))));
    func.add_block(bb);
    module.add_function(func);

    let mut cert = superoptimize_module(&mut module, "c").unwrap();
    assert_eq!(cert.steps.len(), 1);
    assert_eq!(cert.steps[0].rule, SuperoptRule::PureCse);
    assert_eq!(cert.steps[0].error_witness, RewriteErrorWitness::BitExact);
    assert_eq!(expr_cost(&cert.steps[0].before), 3);
    assert_eq!(expr_cost(&cert.steps[0].after), 0);

    // Corrupt local in ReuseLocal to self-alias dest_local (LocalId(3)) and recompute SHA-256.
    let self_dest = cert.steps[0].dest_local;
    if let CertExpr::ReuseLocal { ref mut local, .. } = cert.steps[0].after {
        *local = self_dest;
    }
    cert.cert_sha256 = compute_certificate_sha256(&cert).unwrap();
    let err = verify_certificate(&cert).expect_err("self-aliasing ReuseLocal must be rejected");
    assert!(err.contains("self-aliasing"), "unexpected error: {err}");

    // Also verify UnaryConstFold works directly on MIR constants (`-42`, `!true`, `~0u64`).
    let mut umod = MirModule::new("unary_const_unit");
    let usig = MirFnSig::new(vec![], MirType::i64());
    let mut ufunc = MirFunction::new("uf", usig);
    ufunc.add_local(MirLocal::new(LocalId(0), MirType::i64()));
    ufunc.add_local(MirLocal::new(LocalId(1), MirType::Bool));
    ufunc.add_local(MirLocal::new(LocalId(2), MirType::u64()));
    let mut ubb = MirBlock::new(BlockId::ENTRY);
    ubb.push_stmt(MirStmt::assign(
        LocalId(0),
        MirRValue::UnaryOp {
            op: UnaryOp::Neg,
            operand: MirValue::Const(MirConst::Int(42, MirType::i64())),
        },
    ));
    ubb.push_stmt(MirStmt::assign(
        LocalId(1),
        MirRValue::UnaryOp {
            op: UnaryOp::Not,
            operand: MirValue::Const(MirConst::Bool(true)),
        },
    ));
    ubb.push_stmt(MirStmt::assign(
        LocalId(2),
        MirRValue::UnaryOp {
            op: UnaryOp::BitNot,
            operand: MirValue::Const(MirConst::Uint(0, MirType::u64())),
        },
    ));
    ubb.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(0)))));
    ufunc.add_block(ubb);
    umod.add_function(ufunc);
    let ucert = superoptimize_module(&mut umod, "c").unwrap();
    assert_eq!(ucert.rewrites_applied, 3);
    assert!(ucert
        .steps
        .iter()
        .all(|s| s.rule == SuperoptRule::UnaryConstFold));
}
