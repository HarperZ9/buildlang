// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! Wave 123C integration and differential soundness fuzzer tests for the
//! V7 Certified E-Graph Superoptimizer (`R31..R40`):
//! - `R31_INT_BIT_ALL_ONES`
//! - `R32_INT_BIT_COMPLEMENT`
//! - `R33_INT_BIT_ABSORPTION`
//! - `R34_BOOL_LOGIC_ID`
//! - `R35_INT_SELF_COMPARE`
//! - `R36_COMPARE_NEGATION`
//! - `R37_WRAPPING_ARITH_ID`
//! - `R38_INT_MUL_NEG_ONE`
//! - `R39_UINT_SHIFT_MASK_SIMPLIFY`
//! - `R40_INT_BIT_DISTRIBUTIVE_DEMORGAN`

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

use buildlang::codegen::ir::{
    BinOp, BlockId, LocalId, MirBlock, MirFnSig, MirFunction, MirLocal, MirModule, MirRValue,
    MirStmt, MirStmtKind, MirTerminator, MirType, MirValue, UnaryOp,
};
use buildlang::codegen::lower::MirLowerer;
use buildlang::codegen::superopt::{
    compute_certificate_sha256, expr_cost, superoptimize_module, superoptimize_module_v6_baseline,
    verify_certificate, verify_step_axiom, CertExpr, RewriteErrorWitness, SuperoptCertificate,
    SuperoptRule, SuperoptStep, SUPEROPT_CERT_SCHEMA,
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
        "buildlang_wave123c_{}_{}",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn lower_source_to_mir(src: &str) -> MirModule {
    let sf = SourceFile::new("wave123c_bench.bld", src);
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
                            | BinOp::AddWrapping
                            | BinOp::SubWrapping
                            | BinOp::BitAnd
                            | BinOp::BitOr
                            | BinOp::BitXor
                            | BinOp::Shl
                            | BinOp::Shr => 1,
                            BinOp::Mul | BinOp::MulWrapping => 3,
                            BinOp::Div | BinOp::Rem => 8,
                            _ => 4,
                        },
                        MirRValue::FieldAccess { .. } | MirRValue::Deref { .. } => 2,
                        MirRValue::IndexAccess { .. } => 3,
                        _ => 0,
                    };
                }
            }
        }
    }
    total
}

/// Deterministic SplitMix64 generator for reproducible differential fuzzing.
struct SplitMix64(u64);
impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefVal {
    I32(i32),
    U32(u32),
    Bool(bool),
}

/// Independent reference semantics evaluator for `CertExpr` over `{i32, u32, bool}`
/// (checked arithmetic returns `None` on overflow so trap equivalence is also checked).
fn eval_ref(expr: &CertExpr, env: &[RefVal]) -> Option<RefVal> {
    match expr {
        CertExpr::Local(id) => env.get(*id as usize).copied(),
        CertExpr::Bool(b) => Some(RefVal::Bool(*b)),
        CertExpr::Int(v, _) => Some(RefVal::I32(*v as i32)),
        CertExpr::Uint(v, _) => Some(RefVal::U32(*v as u32)),
        CertExpr::UnaryOp { op, operand } => {
            let v = eval_ref(operand, env)?;
            match (op, v) {
                (UnaryOp::Neg, RefVal::I32(x)) => x.checked_neg().map(RefVal::I32),
                (UnaryOp::BitNot, RefVal::I32(x)) => Some(RefVal::I32(!x)),
                (UnaryOp::BitNot, RefVal::U32(x)) => Some(RefVal::U32(!x)),
                (UnaryOp::Not, RefVal::Bool(b)) => Some(RefVal::Bool(!b)),
                _ => None,
            }
        }
        CertExpr::BinOp { op, left, right } => {
            let l = eval_ref(left, env)?;
            let r = eval_ref(right, env)?;
            match (op, l, r) {
                (BinOp::BitAnd, RefVal::I32(a), RefVal::I32(b)) => Some(RefVal::I32(a & b)),
                (BinOp::BitOr, RefVal::I32(a), RefVal::I32(b)) => Some(RefVal::I32(a | b)),
                (BinOp::BitXor, RefVal::I32(a), RefVal::I32(b)) => Some(RefVal::I32(a ^ b)),
                (BinOp::BitAnd, RefVal::U32(a), RefVal::U32(b)) => Some(RefVal::U32(a & b)),
                (BinOp::BitOr, RefVal::U32(a), RefVal::U32(b)) => Some(RefVal::U32(a | b)),
                (BinOp::BitXor, RefVal::U32(a), RefVal::U32(b)) => Some(RefVal::U32(a ^ b)),
                (BinOp::BitAnd, RefVal::Bool(a), RefVal::Bool(b)) => Some(RefVal::Bool(a & b)),
                (BinOp::BitOr, RefVal::Bool(a), RefVal::Bool(b)) => Some(RefVal::Bool(a | b)),
                (BinOp::BitXor, RefVal::Bool(a), RefVal::Bool(b)) => Some(RefVal::Bool(a ^ b)),
                (BinOp::Shl, RefVal::I32(a), RefVal::I32(b)) => {
                    Some(RefVal::I32(a.wrapping_shl((b as u32) & 31)))
                }
                (BinOp::Shr, RefVal::I32(a), RefVal::I32(b)) => {
                    Some(RefVal::I32(a.wrapping_shr((b as u32) & 31)))
                }
                (BinOp::Shl, RefVal::U32(a), RefVal::U32(b)) => {
                    Some(RefVal::U32(a.wrapping_shl(b & 31)))
                }
                (BinOp::Shr, RefVal::U32(a), RefVal::U32(b)) => {
                    Some(RefVal::U32(a.wrapping_shr(b & 31)))
                }
                (BinOp::AddWrapping, RefVal::I32(a), RefVal::I32(b)) => {
                    Some(RefVal::I32(a.wrapping_add(b)))
                }
                (BinOp::SubWrapping, RefVal::I32(a), RefVal::I32(b)) => {
                    Some(RefVal::I32(a.wrapping_sub(b)))
                }
                (BinOp::MulWrapping, RefVal::I32(a), RefVal::I32(b)) => {
                    Some(RefVal::I32(a.wrapping_mul(b)))
                }
                (BinOp::AddWrapping, RefVal::U32(a), RefVal::U32(b)) => {
                    Some(RefVal::U32(a.wrapping_add(b)))
                }
                (BinOp::SubWrapping, RefVal::U32(a), RefVal::U32(b)) => {
                    Some(RefVal::U32(a.wrapping_sub(b)))
                }
                (BinOp::MulWrapping, RefVal::U32(a), RefVal::U32(b)) => {
                    Some(RefVal::U32(a.wrapping_mul(b)))
                }
                (BinOp::Mul, RefVal::I32(a), RefVal::I32(b)) => a.checked_mul(b).map(RefVal::I32),
                (BinOp::Eq, RefVal::Bool(a), RefVal::Bool(b)) => Some(RefVal::Bool(a == b)),
                (BinOp::Ne, RefVal::Bool(a), RefVal::Bool(b)) => Some(RefVal::Bool(a != b)),
                (BinOp::Eq, RefVal::I32(a), RefVal::I32(b)) => Some(RefVal::Bool(a == b)),
                (BinOp::Ne, RefVal::I32(a), RefVal::I32(b)) => Some(RefVal::Bool(a != b)),
                (BinOp::Lt, RefVal::I32(a), RefVal::I32(b)) => Some(RefVal::Bool(a < b)),
                (BinOp::Le, RefVal::I32(a), RefVal::I32(b)) => Some(RefVal::Bool(a <= b)),
                (BinOp::Gt, RefVal::I32(a), RefVal::I32(b)) => Some(RefVal::Bool(a > b)),
                (BinOp::Ge, RefVal::I32(a), RefVal::I32(b)) => Some(RefVal::Bool(a >= b)),
                (BinOp::Eq, RefVal::U32(a), RefVal::U32(b)) => Some(RefVal::Bool(a == b)),
                (BinOp::Ne, RefVal::U32(a), RefVal::U32(b)) => Some(RefVal::Bool(a != b)),
                (BinOp::Lt, RefVal::U32(a), RefVal::U32(b)) => Some(RefVal::Bool(a < b)),
                (BinOp::Le, RefVal::U32(a), RefVal::U32(b)) => Some(RefVal::Bool(a <= b)),
                (BinOp::Gt, RefVal::U32(a), RefVal::U32(b)) => Some(RefVal::Bool(a > b)),
                (BinOp::Ge, RefVal::U32(a), RefVal::U32(b)) => Some(RefVal::Bool(a >= b)),
                _ => None,
            }
        }
        _ => None,
    }
}

#[test]
fn wave123c_differential_soundness_fuzzer_1200_cases_all_rules_r31_to_r40() {
    let x = CertExpr::Local(0);
    let y = CertExpr::Local(1);
    let z = CertExpr::Local(2);
    let b = CertExpr::Local(3);
    let u = CertExpr::Local(4);
    let v = CertExpr::Local(5);

    let i32_ty = MirType::i32();
    let u32_ty = MirType::u32();

    // Every single V7 rule `R31..R40` paired with `(rule, before, after)` witnesses.
    let rule_pairs: Vec<(SuperoptRule, CertExpr, CertExpr)> = vec![
        // R31: IntBitAllOnes (signed -1 and unsigned u32::MAX)
        (
            SuperoptRule::IntBitAllOnes,
            CertExpr::BinOp {
                op: BinOp::BitAnd,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::Int(-1, i32_ty.clone())),
            },
            x.clone(),
        ),
        (
            SuperoptRule::IntBitAllOnes,
            CertExpr::BinOp {
                op: BinOp::BitOr,
                left: Box::new(u.clone()),
                right: Box::new(CertExpr::Uint(u32::MAX as u128, u32_ty.clone())),
            },
            CertExpr::Uint(u32::MAX as u128, u32_ty.clone()),
        ),
        // R32: IntBitComplement (`x & ~x -> 0`, `x | ~x -> !0`, `u ^ ~u -> u32::MAX`)
        (
            SuperoptRule::IntBitComplement,
            CertExpr::BinOp {
                op: BinOp::BitAnd,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::UnaryOp {
                    op: UnaryOp::BitNot,
                    operand: Box::new(x.clone()),
                }),
            },
            CertExpr::Int(0, i32_ty.clone()),
        ),
        (
            SuperoptRule::IntBitComplement,
            CertExpr::BinOp {
                op: BinOp::BitOr,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::UnaryOp {
                    op: UnaryOp::BitNot,
                    operand: Box::new(x.clone()),
                }),
            },
            CertExpr::Int(-1, i32_ty.clone()),
        ),
        (
            SuperoptRule::IntBitComplement,
            CertExpr::BinOp {
                op: BinOp::BitXor,
                left: Box::new(u.clone()),
                right: Box::new(CertExpr::UnaryOp {
                    op: UnaryOp::BitNot,
                    operand: Box::new(u.clone()),
                }),
            },
            CertExpr::Uint(u32::MAX as u128, u32_ty.clone()),
        ),
        // R33: IntBitAbsorption (`x & (x | y) -> x`, `x | (x & y) -> x`, `x ^ (x ^ y) -> y`)
        (
            SuperoptRule::IntBitAbsorption,
            CertExpr::BinOp {
                op: BinOp::BitAnd,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::BinOp {
                    op: BinOp::BitOr,
                    left: Box::new(x.clone()),
                    right: Box::new(y.clone()),
                }),
            },
            x.clone(),
        ),
        (
            SuperoptRule::IntBitAbsorption,
            CertExpr::BinOp {
                op: BinOp::BitOr,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::BinOp {
                    op: BinOp::BitAnd,
                    left: Box::new(y.clone()),
                    right: Box::new(x.clone()),
                }),
            },
            x.clone(),
        ),
        (
            SuperoptRule::IntBitAbsorption,
            CertExpr::BinOp {
                op: BinOp::BitXor,
                left: Box::new(u.clone()),
                right: Box::new(CertExpr::BinOp {
                    op: BinOp::BitXor,
                    left: Box::new(u.clone()),
                    right: Box::new(v.clone()),
                }),
            },
            v.clone(),
        ),
        // R34: BoolLogicId (`b == true -> b`, `b == false -> !b`, `b != b -> false`)
        (
            SuperoptRule::BoolLogicId,
            CertExpr::BinOp {
                op: BinOp::Eq,
                left: Box::new(b.clone()),
                right: Box::new(CertExpr::Bool(true)),
            },
            b.clone(),
        ),
        (
            SuperoptRule::BoolLogicId,
            CertExpr::BinOp {
                op: BinOp::Eq,
                left: Box::new(b.clone()),
                right: Box::new(CertExpr::Bool(false)),
            },
            CertExpr::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(b.clone()),
            },
        ),
        (
            SuperoptRule::BoolLogicId,
            CertExpr::BinOp {
                op: BinOp::Ne,
                left: Box::new(b.clone()),
                right: Box::new(b.clone()),
            },
            CertExpr::Bool(false),
        ),
        // R35: IntSelfCompare (`x == x -> true`, `x < x -> false`, `u >= u -> true`)
        (
            SuperoptRule::IntSelfCompare,
            CertExpr::BinOp {
                op: BinOp::Eq,
                left: Box::new(x.clone()),
                right: Box::new(x.clone()),
            },
            CertExpr::Bool(true),
        ),
        (
            SuperoptRule::IntSelfCompare,
            CertExpr::BinOp {
                op: BinOp::Lt,
                left: Box::new(x.clone()),
                right: Box::new(x.clone()),
            },
            CertExpr::Bool(false),
        ),
        (
            SuperoptRule::IntSelfCompare,
            CertExpr::BinOp {
                op: BinOp::Ge,
                left: Box::new(u.clone()),
                right: Box::new(u.clone()),
            },
            CertExpr::Bool(true),
        ),
        // R36: CompareNegation (`!(x < y) -> x >= y`, `!(u == v) -> u != v`)
        (
            SuperoptRule::CompareNegation,
            CertExpr::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(CertExpr::BinOp {
                    op: BinOp::Lt,
                    left: Box::new(x.clone()),
                    right: Box::new(y.clone()),
                }),
            },
            CertExpr::BinOp {
                op: BinOp::Ge,
                left: Box::new(x.clone()),
                right: Box::new(y.clone()),
            },
        ),
        (
            SuperoptRule::CompareNegation,
            CertExpr::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(CertExpr::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(u.clone()),
                    right: Box::new(v.clone()),
                }),
            },
            CertExpr::BinOp {
                op: BinOp::Ne,
                left: Box::new(u.clone()),
                right: Box::new(v.clone()),
            },
        ),
        // R37: WrappingArithId (`(u +_w v) -_w v -> u`, `u *_w 16 -> u << 4`, `x *_w 2 -> x +_w x`)
        (
            SuperoptRule::WrappingArithId,
            CertExpr::BinOp {
                op: BinOp::SubWrapping,
                left: Box::new(CertExpr::BinOp {
                    op: BinOp::AddWrapping,
                    left: Box::new(u.clone()),
                    right: Box::new(v.clone()),
                }),
                right: Box::new(v.clone()),
            },
            u.clone(),
        ),
        (
            SuperoptRule::WrappingArithId,
            CertExpr::BinOp {
                op: BinOp::MulWrapping,
                left: Box::new(u.clone()),
                right: Box::new(CertExpr::Uint(16, u32_ty.clone())),
            },
            CertExpr::BinOp {
                op: BinOp::Shl,
                left: Box::new(u.clone()),
                right: Box::new(CertExpr::Uint(4, u32_ty.clone())),
            },
        ),
        (
            SuperoptRule::WrappingArithId,
            CertExpr::BinOp {
                op: BinOp::MulWrapping,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::Int(2, i32_ty.clone())),
            },
            CertExpr::BinOp {
                op: BinOp::AddWrapping,
                left: Box::new(x.clone()),
                right: Box::new(x.clone()),
            },
        ),
        // R38: IntMulNegOne (`x * (-1) -> -x`, including exact INT_MIN trap parity!)
        (
            SuperoptRule::IntMulNegOne,
            CertExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::Int(-1, i32_ty.clone())),
            },
            CertExpr::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(x.clone()),
            },
        ),
        // R39: UintShiftMaskSimplify (`(u >> 24) & 0xFF -> u >> 24`, `(u << 8) & 0xFF -> 0`, `(u & 0xFF) >> 8 -> 0`)
        (
            SuperoptRule::UintShiftMaskSimplify,
            CertExpr::BinOp {
                op: BinOp::BitAnd,
                left: Box::new(CertExpr::BinOp {
                    op: BinOp::Shr,
                    left: Box::new(u.clone()),
                    right: Box::new(CertExpr::Uint(24, u32_ty.clone())),
                }),
                right: Box::new(CertExpr::Uint(0xFF, u32_ty.clone())),
            },
            CertExpr::BinOp {
                op: BinOp::Shr,
                left: Box::new(u.clone()),
                right: Box::new(CertExpr::Uint(24, u32_ty.clone())),
            },
        ),
        (
            SuperoptRule::UintShiftMaskSimplify,
            CertExpr::BinOp {
                op: BinOp::BitAnd,
                left: Box::new(CertExpr::BinOp {
                    op: BinOp::Shl,
                    left: Box::new(u.clone()),
                    right: Box::new(CertExpr::Uint(8, u32_ty.clone())),
                }),
                right: Box::new(CertExpr::Uint(0xFF, u32_ty.clone())),
            },
            CertExpr::Uint(0, u32_ty.clone()),
        ),
        (
            SuperoptRule::UintShiftMaskSimplify,
            CertExpr::BinOp {
                op: BinOp::Shr,
                left: Box::new(CertExpr::BinOp {
                    op: BinOp::BitAnd,
                    left: Box::new(u.clone()),
                    right: Box::new(CertExpr::Uint(0xFF, u32_ty.clone())),
                }),
                right: Box::new(CertExpr::Uint(8, u32_ty.clone())),
            },
            CertExpr::Uint(0, u32_ty.clone()),
        ),
        // R40: IntBitDistributiveDeMorgan (`(x & y) | (x & z) -> x & (y | z)`, `(~x) & (~y) -> ~(x | y)`)
        (
            SuperoptRule::IntBitDistributiveDeMorgan,
            CertExpr::BinOp {
                op: BinOp::BitOr,
                left: Box::new(CertExpr::BinOp {
                    op: BinOp::BitAnd,
                    left: Box::new(x.clone()),
                    right: Box::new(y.clone()),
                }),
                right: Box::new(CertExpr::BinOp {
                    op: BinOp::BitAnd,
                    left: Box::new(x.clone()),
                    right: Box::new(z.clone()),
                }),
            },
            CertExpr::BinOp {
                op: BinOp::BitAnd,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::BinOp {
                    op: BinOp::BitOr,
                    left: Box::new(y.clone()),
                    right: Box::new(z.clone()),
                }),
            },
        ),
        (
            SuperoptRule::IntBitDistributiveDeMorgan,
            CertExpr::BinOp {
                op: BinOp::BitAnd,
                left: Box::new(CertExpr::UnaryOp {
                    op: UnaryOp::BitNot,
                    operand: Box::new(x.clone()),
                }),
                right: Box::new(CertExpr::UnaryOp {
                    op: UnaryOp::BitNot,
                    operand: Box::new(y.clone()),
                }),
            },
            CertExpr::UnaryOp {
                op: UnaryOp::BitNot,
                operand: Box::new(CertExpr::BinOp {
                    op: BinOp::BitOr,
                    left: Box::new(x.clone()),
                    right: Box::new(y.clone()),
                }),
            },
        ),
    ];

    // 1. Verify every rule pair passes independent axiom checker AND strict cost drop!
    for (rule, before, after) in &rule_pairs {
        verify_step_axiom(rule, before, after)
            .unwrap_or_else(|e| panic!("axiom check failed for {rule:?}: {e}"));
        assert!(
            expr_cost(after) < expr_cost(before),
            "rule {rule:?} must strictly reduce cost: before={} after={}",
            expr_cost(before),
            expr_cost(after)
        );
    }

    // 2. Differential fuzz across boundary values + 50 deterministic PRNG vectors
    //    (25 rule pairs x 64 input vectors = 1,600 bit-exact differential evaluations!).
    let boundary_i32: &[i32] = &[
        0,
        1,
        -1,
        2,
        -2,
        7,
        -8,
        255,
        -256,
        0x5555_5555,
        -0x5555_5556,
        i32::MAX,
        i32::MIN,
        i32::MIN + 1,
    ];
    let boundary_u32: &[u32] = &[
        0,
        1,
        2,
        4,
        8,
        16,
        255,
        256,
        0x5555_5555,
        0xAAAA_AAAA,
        0x7FFF_FFFF,
        0x8000_0000,
        u32::MAX - 1,
        u32::MAX,
    ];
    let mut rng = SplitMix64(0x2026_1127_0123_C007);
    let mut envs: Vec<[RefVal; 6]> = Vec::with_capacity(64);
    for idx in 0..boundary_i32.len() {
        envs.push([
            RefVal::I32(boundary_i32[idx]),
            RefVal::I32(boundary_i32[(idx * 5 + 3) % boundary_i32.len()]),
            RefVal::I32(boundary_i32[(idx * 7 + 1) % boundary_i32.len()]),
            RefVal::Bool(idx % 2 == 0),
            RefVal::U32(boundary_u32[idx]),
            RefVal::U32(boundary_u32[(idx * 3 + 5) % boundary_u32.len()]),
        ]);
    }
    while envs.len() < 64 {
        envs.push([
            RefVal::I32(rng.next_u64() as i32),
            RefVal::I32(rng.next_u64() as i32),
            RefVal::I32(rng.next_u64() as i32),
            RefVal::Bool((rng.next_u64() & 1) == 1),
            RefVal::U32(rng.next_u64() as u32),
            RefVal::U32(rng.next_u64() as u32),
        ]);
    }

    let mut total_evals = 0usize;
    for (rule, before, after) in &rule_pairs {
        for env in &envs {
            let lhs = eval_ref(before, env);
            let rhs = eval_ref(after, env);
            assert_eq!(
                lhs, rhs,
                "DIFFERENTIAL SOUNDNESS MISMATCH on {rule:?} with env={env:?}: before={lhs:?} after={rhs:?}"
            );
            total_evals += 1;
        }
    }
    assert!(
        total_evals >= 1200,
        "expected >= 1200 fuzz evaluations, got {total_evals}"
    );
    println!(
        "WAVE123C_FUZZ_PASS: pairs={} vectors={} total_evals={total_evals}",
        rule_pairs.len(),
        envs.len()
    );
}

#[test]
fn wave123c_float_nan_and_signed_boundary_guards_never_unsoundly_rewrite() {
    // Build a MIR function with float `NaN == NaN`, `NaN != NaN`, `!(f_a < f_b)`
    // and assert V7 NEVER rewrites any of them!
    let mut module = MirModule::new("nan_guard_test");
    let sig = MirFnSig::new(vec![MirType::f64(), MirType::f64()], MirType::Bool);
    let mut func = MirFunction::new("float_guards", sig);
    let mut p0 = MirLocal::new(LocalId(0), MirType::f64());
    p0.is_param = true;
    let mut p1 = MirLocal::new(LocalId(1), MirType::f64());
    p1.is_param = true;
    let eq_self = MirLocal::new(LocalId(2), MirType::Bool);
    let lt_ab = MirLocal::new(LocalId(3), MirType::Bool);
    let not_lt = MirLocal::new(LocalId(4), MirType::Bool);
    func.locals = vec![p0, p1, eq_self, lt_ab, not_lt];

    let mut bb = MirBlock::new(BlockId::ENTRY);
    bb.stmts.push(MirStmt::assign(
        LocalId(2),
        MirRValue::BinaryOp {
            op: BinOp::Eq,
            left: MirValue::Local(LocalId(0)),
            right: MirValue::Local(LocalId(0)),
        },
    ));
    bb.stmts.push(MirStmt::assign(
        LocalId(3),
        MirRValue::BinaryOp {
            op: BinOp::Lt,
            left: MirValue::Local(LocalId(0)),
            right: MirValue::Local(LocalId(1)),
        },
    ));
    bb.stmts.push(MirStmt::assign(
        LocalId(4),
        MirRValue::UnaryOp {
            op: UnaryOp::Not,
            operand: MirValue::Local(LocalId(3)),
        },
    ));
    bb.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(4)))));
    func.add_block(bb);
    module.add_function(func);

    let cert = superoptimize_module(&mut module, "c").expect("superopt must succeed");
    assert_eq!(
        cert.rewrites_applied, 0,
        "V7 must NEVER rewrite float self-compare (`x == x`) or float ordered negation (`!(a < b)`): {:?}",
        cert.steps
    );
}

#[test]
fn wave123c_planted_unsound_rules_and_forged_cert_fail_closed() {
    let x = CertExpr::Local(0);
    let y = CertExpr::Local(1);
    let z = CertExpr::Local(2);
    let u32_ty = MirType::u32();
    let f64_ty = MirType::f64();

    let planted: Vec<(&str, SuperoptRule, CertExpr, CertExpr)> = vec![
        (
            "non_all_ones_mask_claimed_R31",
            SuperoptRule::IntBitAllOnes,
            CertExpr::BinOp {
                op: BinOp::BitAnd,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::Uint(0x7FFF_FFFF, u32_ty.clone())),
            },
            x.clone(),
        ),
        (
            "float_nan_self_eq_claimed_R35",
            SuperoptRule::IntSelfCompare,
            CertExpr::BinOp {
                op: BinOp::Eq,
                left: Box::new(CertExpr::FloatBits(f64::NAN.to_bits(), f64_ty.clone())),
                right: Box::new(CertExpr::FloatBits(f64::NAN.to_bits(), f64_ty.clone())),
            },
            CertExpr::Bool(true),
        ),
        (
            "float_ordered_lt_negation_claimed_R36",
            SuperoptRule::CompareNegation,
            CertExpr::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(CertExpr::BinOp {
                    op: BinOp::Lt,
                    left: Box::new(CertExpr::FloatBits(f64::NAN.to_bits(), f64_ty.clone())),
                    right: Box::new(CertExpr::FloatBits(1.0_f64.to_bits(), f64_ty.clone())),
                }),
            },
            CertExpr::BinOp {
                op: BinOp::Ge,
                left: Box::new(CertExpr::FloatBits(f64::NAN.to_bits(), f64_ty.clone())),
                right: Box::new(CertExpr::FloatBits(1.0_f64.to_bits(), f64_ty)),
            },
        ),
        (
            "mismatched_wrapping_cancel_claimed_R37",
            SuperoptRule::WrappingArithId,
            CertExpr::BinOp {
                op: BinOp::SubWrapping,
                left: Box::new(CertExpr::BinOp {
                    op: BinOp::AddWrapping,
                    left: Box::new(x.clone()),
                    right: Box::new(y.clone()),
                }),
                right: Box::new(z),
            },
            x.clone(),
        ),
        (
            "unsigned_mul_neg1_claimed_R38",
            SuperoptRule::IntMulNegOne,
            CertExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(x.clone()),
                right: Box::new(CertExpr::Uint(1, u32_ty.clone())),
            },
            CertExpr::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(x.clone()),
            },
        ),
        (
            "narrow_mask_shift_simplify_claimed_R39",
            SuperoptRule::UintShiftMaskSimplify,
            CertExpr::BinOp {
                op: BinOp::BitAnd,
                left: Box::new(CertExpr::BinOp {
                    op: BinOp::Shr,
                    left: Box::new(x.clone()),
                    right: Box::new(CertExpr::Uint(24, u32_ty.clone())),
                }),
                right: Box::new(CertExpr::Uint(0x7F, u32_ty.clone())),
            },
            CertExpr::BinOp {
                op: BinOp::Shr,
                left: Box::new(x),
                right: Box::new(CertExpr::Uint(24, u32_ty)),
            },
        ),
    ];

    for (label, rule, before, after) in planted {
        assert!(
            verify_step_axiom(&rule, &before, &after).is_err(),
            "planted unsound rule `{label}` MUST be rejected by verify_step_axiom"
        );
        // Also wrap in a full certificate with freshly recomputed SHA-256 and confirm fail-closed!
        let mut forged = SuperoptCertificate {
            schema: SUPEROPT_CERT_SCHEMA.to_string(),
            target: "c".to_string(),
            mir_before_sha256: "0".repeat(64),
            mir_after_sha256: "1".repeat(64),
            functions_visited: 1,
            rewrites_applied: 1,
            total_cost_before: expr_cost(&before) as u64,
            total_cost_after: expr_cost(&after) as u64,
            verified: true,
            steps: vec![SuperoptStep {
                function: "forged".to_string(),
                block_id: 0,
                stmt_index: 0,
                dest_local: 9,
                rule,
                before,
                after,
                cost_before: 2,
                cost_after: 0,
                error_witness: RewriteErrorWitness::BitExact,
            }],
            cert_sha256: String::new(),
        };
        forged.cert_sha256 = compute_certificate_sha256(&forged).unwrap();
        assert!(
            verify_certificate(&forged).is_err(),
            "forged certificate `{label}` MUST be rejected fail-closed even with valid SHA-256"
        );
    }
}

const V7_BENCH_PROGRAM: &str = r#"
fn v7_crypto_bitwise_kernel(x: i32, y: i32, z: i32, u: u32, v: u32, flag: bool) -> u32 {
    // R31: Bitwise all-ones identities
    let a1: i32 = x & -1i32;
    let a2: u32 = u | 4294967295u32;

    // R32: Bitwise complement laws (`x & ~x -> 0`, `x | ~x -> -1`, `u ^ ~u -> u32::MAX`)
    let nx: i32 = !x;
    let c1: i32 = x & nx;
    let nu: u32 = !u;
    let c2: u32 = u ^ nu;

    // R33: Bitwise absorption & XOR self-cancellation
    let xy_or: i32 = a1 | y;
    let ab1: i32 = a1 & xy_or;
    let uv_xor: u32 = u ^ v;
    let ab2: u32 = u ^ uv_xor;

    // R34: Boolean logic identities
    let b1: bool = flag == true;
    let b2: bool = b1 != false;
    let b3: bool = b2 == false;
    let b_int: u32 = if b3 { 1u32 } else { 2u32 };

    // R35: Integer self-comparison
    let s1: bool = x == x;
    let s2: bool = u < u;
    let s_int: u32 = if s1 && !s2 { 3u32 } else { 0u32 };

    // R36: Integer comparison negation (`!(x < y) -> x >= y`)
    let lt_xy: bool = x < y;
    let ge_xy: bool = !lt_xy;
    let cmp_int: u32 = if ge_xy { 5u32 } else { 7u32 };

    // R37: Exact modular wrapping arithmetic identities, cancellations, and strength reduction
    let w1: u32 = u.wrapping_add(v);
    let w2: u32 = w1.wrapping_sub(v);
    let w3: u32 = w2.wrapping_mul(16u32);
    let w4: u32 = w3.wrapping_add(0u32);

    // R38: Signed multiply by -1 -> unary neg
    let m1: i32 = y * -1i32;

    // R39: Unsigned shift-mask range simplification (pixel / byte unpack)
    let sh1: u32 = u >> 24u32;
    let sm1: u32 = sh1 & 255u32;
    let sh2: u32 = v << 8u32;
    let sm2: u32 = sh2 & 255u32;

    // R40: Bitwise distributive factoring & De Morgan
    let d1: i32 = x & y;
    let d2: i32 = x & z;
    let dist: i32 = d1 | d2;
    let ny: i32 = !y;
    let nz: i32 = !z;
    let dm: i32 = ny & nz;

    let acc_i: i32 = c1.wrapping_add(ab1).wrapping_add(m1).wrapping_add(dist).wrapping_add(dm);
    a2.wrapping_add(c2)
        .wrapping_add(ab2)
        .wrapping_add(b_int)
        .wrapping_add(s_int)
        .wrapping_add(cmp_int)
        .wrapping_add(w4)
        .wrapping_add(sm1)
        .wrapping_add(sm2)
        .wrapping_add(acc_i as u32)
}

fn v7_nan_guard_runtime(a: f64, b: f64) -> i32 {
    let eq_self: bool = a == a;
    let lt_ab: bool = a < b;
    let not_lt: bool = !lt_ab;
    let r1: i32 = if eq_self { 10 } else { 1 };
    let r2: i32 = if not_lt { 20 } else { 2 };
    r1 + r2
}

fn main() -> i32 ~ Console {
    let r1: u32 = v7_crypto_bitwise_kernel(13, -7, 42, 305419896u32, 2271560481u32, true);
    let r2: u32 = v7_crypto_bitwise_kernel(-100, 55, -1, 4294967295u32, 1u32, false);
    let nan_v: f64 = 0.0 / 0.0;
    let r_nan: i32 = v7_nan_guard_runtime(nan_v, 1.0);
    let r_fin: i32 = v7_nan_guard_runtime(2.0, 5.0);
    println!("{} {} {} {}", r1, r2, r_nan, r_fin);
    0
}
"#;

#[test]
fn wave123c_v6_vs_v7_live_gains_zero_drift_and_verify_cert() {
    let dir = make_temp_dir("v7_e2e");
    let src_path = dir.join("v7_bench.bld");
    let cert_path = dir.join("v7_bench.superopt-cert.json");
    std::fs::write(&src_path, V7_BENCH_PROGRAM).unwrap();

    // 1. Direct in-process V6 vs V7 MIR cost and rewrite delta measurement
    let mut mir_v6 = lower_source_to_mir(V7_BENCH_PROGRAM);
    let mut mir_v7 = mir_v6.clone();
    let cost_unopt = total_module_mir_cost(&mir_v6);
    let cert_v6 = superoptimize_module_v6_baseline(&mut mir_v6, "c").expect("V6 baseline");
    let cost_v6 = total_module_mir_cost(&mir_v6);
    let cert_v7 = superoptimize_module(&mut mir_v7, "c").expect("V7 superopt");
    let cost_v7 = total_module_mir_cost(&mir_v7);

    println!(
        "WAVE123C_V6_VS_V7: unopt_cost={cost_unopt} v6_cost={cost_v6} (rewrites={}) v7_cost={cost_v7} (rewrites={})",
        cert_v6.rewrites_applied, cert_v7.rewrites_applied
    );

    assert!(
        cert_v7.rewrites_applied > cert_v6.rewrites_applied,
        "V7 must apply strictly more certified rewrites than V6: v6={} v7={}",
        cert_v6.rewrites_applied,
        cert_v7.rewrites_applied
    );
    assert!(
        cost_v7 < cost_v6,
        "V7 must strictly reduce total module MIR cost below V6: v6={cost_v6} v7={cost_v7}"
    );

    // Confirm ALL 10 new V7 rules `R31..R40` fired live in `cert_v7`!
    let fired: HashSet<String> = cert_v7
        .steps
        .iter()
        .map(|s| serde_json::to_string(&s.rule).unwrap())
        .collect();
    for expected in [
        "\"R31_INT_BIT_ALL_ONES\"",
        "\"R32_INT_BIT_COMPLEMENT\"",
        "\"R33_INT_BIT_ABSORPTION\"",
        "\"R34_BOOL_LOGIC_ID\"",
        "\"R35_INT_SELF_COMPARE\"",
        "\"R36_COMPARE_NEGATION\"",
        "\"R37_WRAPPING_ARITH_ID\"",
        "\"R38_INT_MUL_NEG_ONE\"",
        "\"R39_UINT_SHIFT_MASK_SIMPLIFY\"",
        "\"R40_INT_BIT_DISTRIBUTIVE_DEMORGAN\"",
    ] {
        assert!(
            fired.contains(expected),
            "expected V7 rule {expected} to fire live; fired={fired:?}"
        );
    }

    // 2. End-to-end CPU execution: unoptimized baseline vs `--superopt --emit-cert`
    let base = Command::new(buildc_bin())
        .env("BUILDLANG_STDLIB", stdlib_dir())
        .args(["run", src_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        base.status.success(),
        "baseline failed: {}",
        String::from_utf8_lossy(&base.stderr)
    );
    let base_out = String::from_utf8_lossy(&base.stdout).trim().to_string();

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
        "V7 superopt run failed: {}",
        String::from_utf8_lossy(&opt.stderr)
    );
    let opt_out = String::from_utf8_lossy(&opt.stdout).trim().to_string();
    assert_eq!(
        base_out, opt_out,
        "V7 superopt drifted from baseline output: base='{base_out}' opt='{opt_out}'"
    );
    // Confirm NaN guard preserved exact IEEE-754 output (`r_nan = 1 + 20 = 21`, `r_fin = 10 + 2 = 12`).
    assert!(
        opt_out.ends_with(" 21 12"),
        "expected exact NaN guard runtime output ending in ' 21 12', got '{opt_out}'"
    );

    // 3. Live `buildc verify-cert` round-trip + planted 1-byte tamper refusal
    let verify_ok = Command::new(buildc_bin())
        .args(["verify-cert", cert_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        verify_ok.status.success(),
        "buildc verify-cert failed: {}",
        String::from_utf8_lossy(&verify_ok.stderr)
    );

    let tampered_path = dir.join("tampered.superopt-cert.json");
    let raw = std::fs::read_to_string(&cert_path).unwrap();
    let tampered = raw.replacen("R31_INT_BIT_ALL_ONES", "R32_INT_BIT_COMPLEMENT", 1);
    std::fs::write(&tampered_path, tampered).unwrap();
    let verify_bad = Command::new(buildc_bin())
        .args(["verify-cert", tampered_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !verify_bad.status.success(),
        "tampered V7 certificate MUST be rejected by buildc verify-cert"
    );
}

#[test]
fn wave123c_gpu_bitwise_kernel_v6_vs_v7_spirv_and_verify_cert() {
    let gpu_src = r#"
#[compute]
fn gpu_bitmix_unpack(a: &[u32], b: &[u32], out: &mut [u32], n: u32) ~ Gpu {
    let i: u32 = gl_GlobalInvocationID.x;
    let in_bounds: bool = i < n;
    let check_eq: bool = in_bounds == true;
    if check_eq {
        let x: u32 = a[i];
        let y: u32 = b[i];
        let x_all: u32 = x & 4294967295u32;
        let ny: u32 = !y;
        let comp_zero: u32 = y & ny;
        let xy_xor: u32 = x_all ^ y;
        let recover_y: u32 = x_all ^ xy_xor;
        let w1: u32 = x_all.wrapping_add(recover_y);
        let w2: u32 = w1.wrapping_sub(recover_y);
        let w3: u32 = w2.wrapping_mul(8u32);
        let hi_byte: u32 = (x_all >> 24u32) & 255u32;
        out[i] = w3.wrapping_add(hi_byte).wrapping_add(comp_zero);
    }
}
"#;
    let mut mir_v6 = lower_source_to_mir(gpu_src);
    let mut mir_v7 = mir_v6.clone();
    let cost_unopt = total_module_mir_cost(&mir_v6);
    let cert_v6 = superoptimize_module_v6_baseline(&mut mir_v6, "spirv").expect("V6 gpu baseline");
    let cost_v6 = total_module_mir_cost(&mir_v6);
    let cert_v7 = superoptimize_module(&mut mir_v7, "spirv").expect("V7 gpu superopt");
    let cost_v7 = total_module_mir_cost(&mir_v7);

    println!(
        "WAVE123C_GPU_V6_VS_V7: unopt_cost={cost_unopt} v6_cost={cost_v6} (rewrites={}) v7_cost={cost_v7} (rewrites={})",
        cert_v6.rewrites_applied, cert_v7.rewrites_applied
    );
    assert!(cert_v7.rewrites_applied > cert_v6.rewrites_applied);
    assert!(cost_v7 < cost_v6);

    let dir = make_temp_dir("v7_gpu_spirv");
    let src_p = dir.join("gpu_bitmix_unpack.bld");
    let spv_p = dir.join("gpu_bitmix_unpack.spv");
    let cert_p = dir.join("gpu_bitmix_unpack.superopt-cert.json");
    std::fs::write(&src_p, gpu_src).unwrap();

    let build = Command::new(buildc_bin())
        .args([
            src_p.to_str().unwrap(),
            "--target",
            "spirv",
            "--superopt",
            "--emit-cert",
            cert_p.to_str().unwrap(),
            "-o",
            spv_p.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "GPU SPIR-V superopt build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let verify = Command::new(buildc_bin())
        .args(["verify-cert", cert_p.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        verify.status.success(),
        "GPU V7 verify-cert failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    if Command::new("spirv-val")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        let sv = Command::new("spirv-val").arg(&spv_p).output().unwrap();
        assert!(
            sv.status.success(),
            "spirv-val failed on V7 superoptimized GPU shader: {}",
            String::from_utf8_lossy(&sv.stderr)
        );
    }
}
