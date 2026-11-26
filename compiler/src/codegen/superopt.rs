// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! V5 Certified E-Graph Superoptimizer for BuildLang MIR (`--superopt`).
//!
//! Performs deterministic equality saturation and cost-guided extraction over
//! pure SSA arithmetic expressions in [`MirModule`], emits a SHA-256 sealed
//! `buildlang.superopt-cert/v1` proof certificate recording every rewrite step,
//! and runs an independent fail-closed axiom verifier before any backend sees
//! the superoptimized MIR.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::codegen::ir::{
    BinOp, FloatSize, IntSize, LocalId, MirBlock, MirConst, MirFunction, MirModule, MirRValue,
    MirStmtKind, MirTerminator, MirType, MirValue,
};

/// Canonical schema identifier for BuildLang superoptimization certificates.
pub const SUPEROPT_CERT_SCHEMA: &str = "buildlang.superopt-cert/v1";

/// Certified rewrite rule identifiers checked independently by the verifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuperoptRule {
    /// Exact integer constant folding (`c1 op c2 -> c3`).
    #[serde(rename = "R1_CONST_FOLD_INT")]
    ConstFoldInt,
    /// Exact finite IEEE-754 float constant folding (`c1 op c2 -> c3`).
    #[serde(rename = "R2_CONST_FOLD_FLOAT")]
    ConstFoldFloat,
    /// Integer additive identity (`x + 0 -> x`, `0 + x -> x`).
    #[serde(rename = "R3_INT_ADD_ZERO")]
    IntAddZero,
    /// Integer subtractive identity (`x - 0 -> x`).
    #[serde(rename = "R4_INT_SUB_ZERO")]
    IntSubZero,
    /// Integer self-subtraction (`x - x -> 0`).
    #[serde(rename = "R5_INT_SUB_SELF")]
    IntSubSelf,
    /// Integer multiplicative identity (`x * 1 -> x`, `1 * x -> x`).
    #[serde(rename = "R6_INT_MUL_ONE")]
    IntMulOne,
    /// Integer absorbing multiplication (`x * 0 -> 0`, `0 * x -> 0`).
    #[serde(rename = "R7_INT_MUL_ZERO")]
    IntMulZero,
    /// Integer strength reduction (`x * 2 -> x + x`, `2 * x -> x + x`).
    #[serde(rename = "R8_INT_MUL_TWO_ADD")]
    IntMulTwoAdd,
    /// Integer power-of-two strength reduction (`x * 2^k -> x << k`, `k >= 2`).
    #[serde(rename = "R9_INT_MUL_POW2_SHL")]
    IntMulPow2Shl,
    /// Integer bitwise identity / annihilation (`x | 0`, `x ^ 0`, `x & 0`, `x ^ x`, `x | x`, `x & x`).
    #[serde(rename = "R10_INT_BIT_ID")]
    IntBitId,
    /// Float multiplicative identity (`x * 1.0 -> x`, `1.0 * x -> x`).
    #[serde(rename = "R11_FLOAT_MUL_ONE")]
    FloatMulOne,
    /// IEEE-754 bit-exact float doubling (`x * 2.0 -> x + x`, `2.0 * x -> x + x`).
    #[serde(rename = "R12_FLOAT_MUL_TWO_ADD")]
    FloatMulTwoAdd,
    /// IEEE-754 exact power-of-two float reciprocal (`x / 2^k -> x * 2^-k`).
    #[serde(rename = "R13_FLOAT_DIV_POW2_MUL")]
    FloatDivPow2Mul,
}

/// Canonical pure expression term recorded in a certificate step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertExpr {
    /// Reference to a MIR local variable.
    Local(u32),
    /// Signed integer constant with its MIR type.
    Int(i128, MirType),
    /// Unsigned integer constant with its MIR type.
    Uint(u128, MirType),
    /// Float constant stored by raw IEEE-754 f64 bit pattern for bit-exactness.
    FloatBits(u64, MirType),
    /// Pure binary operation over two child expressions.
    BinOp {
        op: BinOp,
        left: Box<CertExpr>,
        right: Box<CertExpr>,
    },
}

/// A single audited rewrite step inside a superoptimization certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuperoptStep {
    pub function: String,
    pub block_id: u32,
    pub stmt_index: usize,
    pub dest_local: u32,
    pub rule: SuperoptRule,
    pub before: CertExpr,
    pub after: CertExpr,
    pub cost_before: u32,
    pub cost_after: u32,
}

/// Sealed, independently verifiable V5 superoptimization certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuperoptCertificate {
    pub schema: String,
    pub target: String,
    pub mir_before_sha256: String,
    pub mir_after_sha256: String,
    pub functions_visited: usize,
    pub rewrites_applied: usize,
    pub total_cost_before: u64,
    pub total_cost_after: u64,
    pub verified: bool,
    pub steps: Vec<SuperoptStep>,
    pub cert_sha256: String,
}

#[derive(Serialize)]
struct CertDigestPayload<'a> {
    schema: &'a str,
    target: &'a str,
    mir_before_sha256: &'a str,
    mir_after_sha256: &'a str,
    functions_visited: usize,
    rewrites_applied: usize,
    total_cost_before: u64,
    total_cost_after: u64,
    verified: bool,
    steps: &'a [SuperoptStep],
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn mir_digest(module: &MirModule) -> Result<String, String> {
    let bytes =
        serde_json::to_vec(module).map_err(|e| format!("MIR digest serialize error: {e}"))?;
    Ok(sha256_hex(&bytes))
}

/// Compute the canonical SHA-256 digest of a [`SuperoptCertificate`].
pub fn compute_certificate_sha256(cert: &SuperoptCertificate) -> Result<String, String> {
    let payload = CertDigestPayload {
        schema: &cert.schema,
        target: &cert.target,
        mir_before_sha256: &cert.mir_before_sha256,
        mir_after_sha256: &cert.mir_after_sha256,
        functions_visited: cert.functions_visited,
        rewrites_applied: cert.rewrites_applied,
        total_cost_before: cert.total_cost_before,
        total_cost_after: cert.total_cost_after,
        verified: cert.verified,
        steps: &cert.steps,
    };
    let bytes =
        serde_json::to_vec(&payload).map_err(|e| format!("certificate serialize error: {e}"))?;
    Ok(sha256_hex(&bytes))
}

/// Hardware cost model for pure MIR expressions (lower is faster).
pub fn expr_cost(expr: &CertExpr) -> u32 {
    match expr {
        CertExpr::Local(_)
        | CertExpr::Int(_, _)
        | CertExpr::Uint(_, _)
        | CertExpr::FloatBits(_, _) => 0,
        CertExpr::BinOp { op, left, right } => {
            let op_c = match op {
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
            };
            op_c + expr_cost(left) + expr_cost(right)
        }
    }
}

/// Minimal deterministic e-graph for local equality saturation & cost extraction.
struct MiniEGraph {
    nodes: Vec<CertExpr>,
    parent: Vec<usize>,
    rules: Vec<Option<SuperoptRule>>,
}

impl MiniEGraph {
    fn new(root: CertExpr) -> Self {
        Self {
            nodes: vec![root],
            parent: vec![0],
            rules: vec![None],
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn add_equiv(&mut self, candidate: CertExpr, rule: SuperoptRule) {
        let id = self.nodes.len();
        self.nodes.push(candidate);
        self.parent.push(0);
        self.rules.push(Some(rule));
        let r0 = self.find(0);
        let ri = self.find(id);
        if ri != r0 {
            self.parent[ri] = r0;
        }
    }

    /// Extract the lowest-cost member of the root e-class that strictly improves
    /// cost (preferring the first rule on ties for determinism).
    fn extract_best(&mut self) -> Option<(CertExpr, SuperoptRule, u32, u32)> {
        let root_class = self.find(0);
        let root_cost = expr_cost(&self.nodes[0]);
        let mut best_idx: Option<usize> = None;
        let mut best_cost = root_cost;
        for i in 1..self.nodes.len() {
            if self.find(i) == root_class {
                let c = expr_cost(&self.nodes[i]);
                if c < best_cost {
                    best_cost = c;
                    best_idx = Some(i);
                }
            }
        }
        best_idx.and_then(|idx| {
            self.rules[idx]
                .clone()
                .map(|rule| (self.nodes[idx].clone(), rule, root_cost, best_cost))
        })
    }
}

fn int_bit_width(ty: &MirType) -> Option<u32> {
    match ty {
        MirType::Int(IntSize::I8, _) => Some(8),
        MirType::Int(IntSize::I16, _) => Some(16),
        MirType::Int(IntSize::I32, _) => Some(32),
        MirType::Int(IntSize::I64, _) => Some(64),
        MirType::Int(IntSize::I128, _) => Some(128),
        MirType::Int(IntSize::ISize, _) => Some(64),
        _ => None,
    }
}

fn int_in_range(val: i128, ty: &MirType) -> bool {
    match ty {
        MirType::Int(IntSize::I8, true) => i8::try_from(val).is_ok(),
        MirType::Int(IntSize::I16, true) => i16::try_from(val).is_ok(),
        MirType::Int(IntSize::I32, true) => i32::try_from(val).is_ok(),
        MirType::Int(IntSize::I64, true) | MirType::Int(IntSize::ISize, true) => {
            i64::try_from(val).is_ok()
        }
        MirType::Int(IntSize::I128, true) => true,
        MirType::Int(IntSize::I8, false) => u8::try_from(val).is_ok(),
        MirType::Int(IntSize::I16, false) => u16::try_from(val).is_ok(),
        MirType::Int(IntSize::I32, false) => u32::try_from(val).is_ok(),
        MirType::Int(IntSize::I64, false) | MirType::Int(IntSize::ISize, false) => {
            u64::try_from(val).is_ok()
        }
        MirType::Int(IntSize::I128, false) => val >= 0,
        _ => false,
    }
}

fn eval_int_binop(op: BinOp, l: i128, r: i128, ty: &MirType) -> Option<i128> {
    let bits = int_bit_width(ty)?;
    let res = match op {
        BinOp::Add => l.checked_add(r)?,
        BinOp::Sub => l.checked_sub(r)?,
        BinOp::Mul => l.checked_mul(r)?,
        BinOp::BitAnd => l & r,
        BinOp::BitOr => l | r,
        BinOp::BitXor => l ^ r,
        BinOp::Shl => {
            if r < 0 || (r as u32) >= bits {
                return None;
            }
            l.checked_shl(r as u32)?
        }
        BinOp::Shr => {
            if r < 0 || (r as u32) >= bits {
                return None;
            }
            l.checked_shr(r as u32)?
        }
        _ => return None,
    };
    if int_in_range(res, ty) {
        Some(res)
    } else {
        None
    }
}

fn eval_uint_binop(op: BinOp, l: u128, r: u128, ty: &MirType) -> Option<u128> {
    let bits = int_bit_width(ty)?;
    let max_val: u128 = if bits >= 128 {
        u128::MAX
    } else {
        (1u128 << bits) - 1
    };
    let res = match op {
        BinOp::Add => l.checked_add(r)?,
        BinOp::Sub => l.checked_sub(r)?,
        BinOp::Mul => l.checked_mul(r)?,
        BinOp::BitAnd => l & r,
        BinOp::BitOr => l | r,
        BinOp::BitXor => l ^ r,
        BinOp::Shl => {
            if r >= bits as u128 {
                return None;
            }
            l.checked_shl(r as u32)?
        }
        BinOp::Shr => {
            if r >= bits as u128 {
                return None;
            }
            l.checked_shr(r as u32)?
        }
        _ => return None,
    };
    if res <= max_val {
        Some(res)
    } else {
        None
    }
}

fn eval_float_binop(op: BinOp, l_bits: u64, r_bits: u64, ty: &MirType) -> Option<u64> {
    let l = f64::from_bits(l_bits);
    let r = f64::from_bits(r_bits);
    if !l.is_finite() || !r.is_finite() {
        return None;
    }
    let res = match ty {
        MirType::Float(FloatSize::F32) => {
            let lf = l as f32;
            let rf = r as f32;
            let out = match op {
                BinOp::Add => lf + rf,
                BinOp::Sub => lf - rf,
                BinOp::Mul => lf * rf,
                BinOp::Div if rf != 0.0 => lf / rf,
                _ => return None,
            };
            if !out.is_finite() {
                return None;
            }
            out as f64
        }
        MirType::Float(FloatSize::F64) => {
            let out = match op {
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div if r != 0.0 => l / r,
                _ => return None,
            };
            if !out.is_finite() {
                return None;
            }
            out
        }
        _ => return None,
    };
    Some(res.to_bits())
}

/// Returns `Some(recip)` iff `v` is an exact finite power of two whose
/// reciprocal is also an exact normal finite power of two (`v * recip == 1.0`).
fn exact_pow2_float_recip(bits: u64, ty: &MirType) -> Option<u64> {
    let v = f64::from_bits(bits);
    if !v.is_finite() || v == 0.0 {
        return None;
    }
    // An IEEE-754 binary64 value is an exact power of two iff its 52-bit
    // significand is zero and its exponent is normal (1..2046).
    let abs_bits = bits & 0x7fff_ffff_ffff_ffff;
    let frac = abs_bits & 0x000f_ffff_ffff_ffff;
    let exp = (abs_bits >> 52) & 0x7ff;
    if frac != 0 || exp == 0 || exp == 0x7ff {
        return None;
    }
    let recip = 1.0 / v;
    if !recip.is_finite() || recip * v != 1.0 {
        return None;
    }
    if matches!(ty, MirType::Float(FloatSize::F32)) {
        let vf = v as f32;
        let rf = recip as f32;
        if !rf.is_finite() || rf == 0.0 || (rf * vf) != 1.0 || (rf as f64) != recip {
            return None;
        }
    }
    // Avoid trivial `x / 1.0 -> x * 1.0` (only worth rewriting when v != 1.0).
    if v == 1.0 {
        return None;
    }
    Some(recip.to_bits())
}

fn saturate_binop(
    op: BinOp,
    left: &CertExpr,
    right: &CertExpr,
    dest_ty: &MirType,
    allow_int_shift: bool,
) -> Option<(CertExpr, SuperoptRule, u32, u32)> {
    let root = CertExpr::BinOp {
        op,
        left: Box::new(left.clone()),
        right: Box::new(right.clone()),
    };
    let mut eg = MiniEGraph::new(root);

    // R1: Exact integer constant folding
    if let (CertExpr::Int(l, lt), CertExpr::Int(r, _)) = (left, right) {
        if let Some(res) = eval_int_binop(op, *l, *r, lt) {
            eg.add_equiv(CertExpr::Int(res, lt.clone()), SuperoptRule::ConstFoldInt);
        }
    }
    if let (CertExpr::Uint(l, lt), CertExpr::Uint(r, _)) = (left, right) {
        if let Some(res) = eval_uint_binop(op, *l, *r, lt) {
            eg.add_equiv(CertExpr::Uint(res, lt.clone()), SuperoptRule::ConstFoldInt);
        }
    }

    // R2: Exact finite IEEE-754 float constant folding
    if let (CertExpr::FloatBits(lb, lt), CertExpr::FloatBits(rb, _)) = (left, right) {
        if let Some(res_bits) = eval_float_binop(op, *lb, *rb, lt) {
            eg.add_equiv(
                CertExpr::FloatBits(res_bits, lt.clone()),
                SuperoptRule::ConstFoldFloat,
            );
        }
    }

    // Integer algebraic identities and strength reductions
    if dest_ty.is_integer() {
        match op {
            BinOp::Add => {
                if matches!(right, CertExpr::Int(0, _) | CertExpr::Uint(0, _)) {
                    eg.add_equiv(left.clone(), SuperoptRule::IntAddZero);
                }
                if matches!(left, CertExpr::Int(0, _) | CertExpr::Uint(0, _)) {
                    eg.add_equiv(right.clone(), SuperoptRule::IntAddZero);
                }
            }
            BinOp::Sub => {
                if matches!(right, CertExpr::Int(0, _) | CertExpr::Uint(0, _)) {
                    eg.add_equiv(left.clone(), SuperoptRule::IntSubZero);
                }
                if left == right {
                    let zero = if dest_ty.is_signed() {
                        CertExpr::Int(0, dest_ty.clone())
                    } else {
                        CertExpr::Uint(0, dest_ty.clone())
                    };
                    eg.add_equiv(zero, SuperoptRule::IntSubSelf);
                }
            }
            BinOp::Mul => {
                if matches!(right, CertExpr::Int(1, _) | CertExpr::Uint(1, _)) {
                    eg.add_equiv(left.clone(), SuperoptRule::IntMulOne);
                }
                if matches!(left, CertExpr::Int(1, _) | CertExpr::Uint(1, _)) {
                    eg.add_equiv(right.clone(), SuperoptRule::IntMulOne);
                }
                if matches!(right, CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                    || matches!(left, CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                {
                    let zero = if dest_ty.is_signed() {
                        CertExpr::Int(0, dest_ty.clone())
                    } else {
                        CertExpr::Uint(0, dest_ty.clone())
                    };
                    eg.add_equiv(zero, SuperoptRule::IntMulZero);
                }
                if matches!(right, CertExpr::Int(2, _) | CertExpr::Uint(2, _)) {
                    eg.add_equiv(
                        CertExpr::BinOp {
                            op: BinOp::Add,
                            left: Box::new(left.clone()),
                            right: Box::new(left.clone()),
                        },
                        SuperoptRule::IntMulTwoAdd,
                    );
                }
                if matches!(left, CertExpr::Int(2, _) | CertExpr::Uint(2, _)) {
                    eg.add_equiv(
                        CertExpr::BinOp {
                            op: BinOp::Add,
                            left: Box::new(right.clone()),
                            right: Box::new(right.clone()),
                        },
                        SuperoptRule::IntMulTwoAdd,
                    );
                }
                if allow_int_shift {
                    let bits = int_bit_width(dest_ty).unwrap_or(32);
                    let try_pow2 = |var: &CertExpr, c: u128| -> Option<CertExpr> {
                        if c > 2 && c.is_power_of_two() {
                            let shift = c.trailing_zeros();
                            if shift < bits.saturating_sub(1) {
                                let shift_const = if dest_ty.is_signed() {
                                    CertExpr::Int(shift as i128, dest_ty.clone())
                                } else {
                                    CertExpr::Uint(shift as u128, dest_ty.clone())
                                };
                                return Some(CertExpr::BinOp {
                                    op: BinOp::Shl,
                                    left: Box::new(var.clone()),
                                    right: Box::new(shift_const),
                                });
                            }
                        }
                        None
                    };
                    match right {
                        CertExpr::Int(c, _) if *c > 2 => {
                            if let Some(shl) = try_pow2(left, *c as u128) {
                                eg.add_equiv(shl, SuperoptRule::IntMulPow2Shl);
                            }
                        }
                        CertExpr::Uint(c, _) if *c > 2 => {
                            if let Some(shl) = try_pow2(left, *c) {
                                eg.add_equiv(shl, SuperoptRule::IntMulPow2Shl);
                            }
                        }
                        _ => {}
                    }
                    match left {
                        CertExpr::Int(c, _) if *c > 2 => {
                            if let Some(shl) = try_pow2(right, *c as u128) {
                                eg.add_equiv(shl, SuperoptRule::IntMulPow2Shl);
                            }
                        }
                        CertExpr::Uint(c, _) if *c > 2 => {
                            if let Some(shl) = try_pow2(right, *c) {
                                eg.add_equiv(shl, SuperoptRule::IntMulPow2Shl);
                            }
                        }
                        _ => {}
                    }
                }
            }
            BinOp::BitOr => {
                if matches!(right, CertExpr::Int(0, _) | CertExpr::Uint(0, _)) {
                    eg.add_equiv(left.clone(), SuperoptRule::IntBitId);
                }
                if matches!(left, CertExpr::Int(0, _) | CertExpr::Uint(0, _)) {
                    eg.add_equiv(right.clone(), SuperoptRule::IntBitId);
                }
                if left == right {
                    eg.add_equiv(left.clone(), SuperoptRule::IntBitId);
                }
            }
            BinOp::BitXor => {
                if matches!(right, CertExpr::Int(0, _) | CertExpr::Uint(0, _)) {
                    eg.add_equiv(left.clone(), SuperoptRule::IntBitId);
                }
                if matches!(left, CertExpr::Int(0, _) | CertExpr::Uint(0, _)) {
                    eg.add_equiv(right.clone(), SuperoptRule::IntBitId);
                }
                if left == right {
                    let zero = if dest_ty.is_signed() {
                        CertExpr::Int(0, dest_ty.clone())
                    } else {
                        CertExpr::Uint(0, dest_ty.clone())
                    };
                    eg.add_equiv(zero, SuperoptRule::IntBitId);
                }
            }
            BinOp::BitAnd => {
                if matches!(right, CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                    || matches!(left, CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                {
                    let zero = if dest_ty.is_signed() {
                        CertExpr::Int(0, dest_ty.clone())
                    } else {
                        CertExpr::Uint(0, dest_ty.clone())
                    };
                    eg.add_equiv(zero, SuperoptRule::IntBitId);
                }
                if left == right {
                    eg.add_equiv(left.clone(), SuperoptRule::IntBitId);
                }
            }
            _ => {}
        }
    }

    // IEEE-754 sound floating-point rules
    if dest_ty.is_float() {
        let one_bits = 1.0_f64.to_bits();
        let two_bits = 2.0_f64.to_bits();
        match op {
            BinOp::Mul => {
                if matches!(right, CertExpr::FloatBits(b, _) if *b == one_bits) {
                    eg.add_equiv(left.clone(), SuperoptRule::FloatMulOne);
                }
                if matches!(left, CertExpr::FloatBits(b, _) if *b == one_bits) {
                    eg.add_equiv(right.clone(), SuperoptRule::FloatMulOne);
                }
                if matches!(right, CertExpr::FloatBits(b, _) if *b == two_bits) {
                    eg.add_equiv(
                        CertExpr::BinOp {
                            op: BinOp::Add,
                            left: Box::new(left.clone()),
                            right: Box::new(left.clone()),
                        },
                        SuperoptRule::FloatMulTwoAdd,
                    );
                }
                if matches!(left, CertExpr::FloatBits(b, _) if *b == two_bits) {
                    eg.add_equiv(
                        CertExpr::BinOp {
                            op: BinOp::Add,
                            left: Box::new(right.clone()),
                            right: Box::new(right.clone()),
                        },
                        SuperoptRule::FloatMulTwoAdd,
                    );
                }
            }
            BinOp::Div => {
                if let CertExpr::FloatBits(b, ft) = right {
                    if let Some(recip_bits) = exact_pow2_float_recip(*b, ft) {
                        eg.add_equiv(
                            CertExpr::BinOp {
                                op: BinOp::Mul,
                                left: Box::new(left.clone()),
                                right: Box::new(CertExpr::FloatBits(recip_bits, ft.clone())),
                            },
                            SuperoptRule::FloatDivPow2Mul,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    eg.extract_best()
}

fn mir_value_to_cert(v: &MirValue, const_env: &HashMap<LocalId, MirConst>) -> Option<CertExpr> {
    match v {
        MirValue::Local(id) => {
            if let Some(c) = const_env.get(id) {
                mir_const_to_cert(c)
            } else {
                Some(CertExpr::Local(id.0))
            }
        }
        MirValue::Const(c) => mir_const_to_cert(c),
        _ => None,
    }
}

fn mir_const_to_cert(c: &MirConst) -> Option<CertExpr> {
    match c {
        MirConst::Int(v, ty) => Some(CertExpr::Int(*v, ty.clone())),
        MirConst::Uint(v, ty) => Some(CertExpr::Uint(*v, ty.clone())),
        MirConst::Float(v, ty) => Some(CertExpr::FloatBits(v.to_bits(), ty.clone())),
        _ => None,
    }
}

fn cert_atom_to_mir_value(expr: &CertExpr) -> Option<MirValue> {
    match expr {
        CertExpr::Local(id) => Some(MirValue::Local(LocalId(*id))),
        CertExpr::Int(v, ty) => Some(MirValue::Const(MirConst::Int(*v, ty.clone()))),
        CertExpr::Uint(v, ty) => Some(MirValue::Const(MirConst::Uint(*v, ty.clone()))),
        CertExpr::FloatBits(bits, ty) => Some(MirValue::Const(MirConst::Float(
            f64::from_bits(*bits),
            ty.clone(),
        ))),
        CertExpr::BinOp { .. } => None,
    }
}

fn cert_to_mir_rvalue(expr: &CertExpr) -> Option<MirRValue> {
    match expr {
        CertExpr::BinOp { op, left, right } => {
            let l = cert_atom_to_mir_value(left)?;
            let r = cert_atom_to_mir_value(right)?;
            Some(MirRValue::BinaryOp {
                op: *op,
                left: l,
                right: r,
            })
        }
        atom => cert_atom_to_mir_value(atom).map(MirRValue::Use),
    }
}

/// Collect locals that are assigned more than once, are parameters, or have
/// their address taken, excluding them from constant propagation so we never
/// fold across loops or aliased mutations.
fn collect_unsafe_locals(func: &MirFunction) -> HashSet<LocalId> {
    let mut unsafe_set = HashSet::new();
    let mut def_counts: HashMap<LocalId, usize> = HashMap::new();
    for local in &func.locals {
        if local.is_param {
            unsafe_set.insert(local.id);
        }
    }
    let Some(blocks) = &func.blocks else {
        return unsafe_set;
    };
    for block in blocks {
        for stmt in &block.stmts {
            match &stmt.kind {
                MirStmtKind::Assign { dest, value } => {
                    *def_counts.entry(*dest).or_insert(0) += 1;
                    if let MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. } = value
                    {
                        unsafe_set.insert(place.local);
                    }
                }
                MirStmtKind::DerefAssign { ptr, .. } => {
                    unsafe_set.insert(*ptr);
                }
                MirStmtKind::FieldAssign { base, .. } => {
                    unsafe_set.insert(*base);
                }
                MirStmtKind::FieldDerefAssign { ptr, .. } => {
                    unsafe_set.insert(*ptr);
                }
                _ => {}
            }
        }
        if let Some(MirTerminator::Call { dest: Some(d), .. }) = &block.terminator {
            *def_counts.entry(*d).or_insert(0) += 1;
        }
    }
    for (id, count) in def_counts {
        if count != 1 {
            unsafe_set.insert(id);
        }
    }
    unsafe_set
}

fn superoptimize_block(
    func_name: &str,
    locals: &[crate::codegen::ir::MirLocal],
    unsafe_locals: &HashSet<LocalId>,
    allow_int_shift: bool,
    block: &mut MirBlock,
    steps: &mut Vec<SuperoptStep>,
) {
    let mut const_env: HashMap<LocalId, MirConst> = HashMap::new();
    for (stmt_idx, stmt) in block.stmts.iter_mut().enumerate() {
        let MirStmtKind::Assign { dest, value } = &mut stmt.kind else {
            continue;
        };
        let dest_id = *dest;
        let dest_ty = locals
            .get(dest_id.0 as usize)
            .map(|l| l.ty.clone())
            .unwrap_or(MirType::Void);

        match value {
            MirRValue::Use(MirValue::Const(c)) => {
                if !unsafe_locals.contains(&dest_id) {
                    const_env.insert(dest_id, c.clone());
                }
            }
            MirRValue::BinaryOp { op, left, right } => {
                let Some(l_expr) = mir_value_to_cert(left, &const_env) else {
                    continue;
                };
                let Some(r_expr) = mir_value_to_cert(right, &const_env) else {
                    continue;
                };
                if let Some((after_expr, rule, cost_before, cost_after)) =
                    saturate_binop(*op, &l_expr, &r_expr, &dest_ty, allow_int_shift)
                {
                    if let Some(new_rval) = cert_to_mir_rvalue(&after_expr) {
                        let before_expr = CertExpr::BinOp {
                            op: *op,
                            left: Box::new(l_expr),
                            right: Box::new(r_expr),
                        };
                        *value = new_rval;
                        if !unsafe_locals.contains(&dest_id) {
                            if let MirRValue::Use(MirValue::Const(c)) = value {
                                const_env.insert(dest_id, c.clone());
                            }
                        }
                        steps.push(SuperoptStep {
                            function: func_name.to_string(),
                            block_id: block.id.0,
                            stmt_index: stmt_idx,
                            dest_local: dest_id.0,
                            rule,
                            before: before_expr,
                            after: after_expr,
                            cost_before,
                            cost_after,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

/// Run V5 certified e-graph superoptimization across every function in `module`,
/// independently verify the generated certificate fail-closed, and return it.
pub fn superoptimize_module(
    module: &mut MirModule,
    target: &str,
) -> Result<SuperoptCertificate, String> {
    let mir_before_sha256 = mir_digest(module)?;
    // WGSL requires unsigned u32 shift RHS (`<< u32(k)`), whereas C/MSL/RISC-V
    // accept matched-type shifts directly. Keep WGSL strictly type-valid.
    let allow_int_shift = target != "wgsl";

    let mut steps = Vec::new();
    let functions_visited = module.functions.len();

    for func in &mut module.functions {
        let unsafe_locals = collect_unsafe_locals(func);
        let func_name = func.name.to_string();
        let locals_snapshot = func.locals.clone();
        if let Some(blocks) = &mut func.blocks {
            for block in blocks.iter_mut() {
                superoptimize_block(
                    &func_name,
                    &locals_snapshot,
                    &unsafe_locals,
                    allow_int_shift,
                    block,
                    &mut steps,
                );
            }
        }
    }

    let mir_after_sha256 = mir_digest(module)?;
    let total_cost_before: u64 = steps.iter().map(|s| s.cost_before as u64).sum();
    let total_cost_after: u64 = steps.iter().map(|s| s.cost_after as u64).sum();

    let mut cert = SuperoptCertificate {
        schema: SUPEROPT_CERT_SCHEMA.to_string(),
        target: target.to_string(),
        mir_before_sha256,
        mir_after_sha256,
        functions_visited,
        rewrites_applied: steps.len(),
        total_cost_before,
        total_cost_after,
        verified: true,
        steps,
        cert_sha256: String::new(),
    };
    cert.cert_sha256 = compute_certificate_sha256(&cert)?;

    // Fail-closed: independently check every step and digest before returning.
    verify_certificate(&cert)?;
    Ok(cert)
}

/// Independently verify a single rewrite step structurally from the axioms.
pub fn verify_step_axiom(
    rule: &SuperoptRule,
    before: &CertExpr,
    after: &CertExpr,
) -> Result<(), String> {
    let CertExpr::BinOp { op, left, right } = before else {
        return Err(format!(
            "rule {:?} requires a binary before-expression",
            rule
        ));
    };
    let ok = match rule {
        SuperoptRule::ConstFoldInt => match (left.as_ref(), right.as_ref(), after) {
            (CertExpr::Int(l, lt), CertExpr::Int(r, _), CertExpr::Int(res, at)) => {
                lt == at && eval_int_binop(*op, *l, *r, lt) == Some(*res)
            }
            (CertExpr::Uint(l, lt), CertExpr::Uint(r, _), CertExpr::Uint(res, at)) => {
                lt == at && eval_uint_binop(*op, *l, *r, lt) == Some(*res)
            }
            _ => false,
        },
        SuperoptRule::ConstFoldFloat => match (left.as_ref(), right.as_ref(), after) {
            (
                CertExpr::FloatBits(lb, lt),
                CertExpr::FloatBits(rb, _),
                CertExpr::FloatBits(ab, at),
            ) => lt == at && eval_float_binop(*op, *lb, *rb, lt) == Some(*ab),
            _ => false,
        },
        SuperoptRule::IntAddZero => {
            *op == BinOp::Add
                && ((matches!(right.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                    && after == left.as_ref())
                    || (matches!(left.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                        && after == right.as_ref()))
        }
        SuperoptRule::IntSubZero => {
            *op == BinOp::Sub
                && matches!(right.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                && after == left.as_ref()
        }
        SuperoptRule::IntSubSelf => {
            *op == BinOp::Sub
                && left == right
                && matches!(after, CertExpr::Int(0, _) | CertExpr::Uint(0, _))
        }
        SuperoptRule::IntMulOne => {
            *op == BinOp::Mul
                && ((matches!(right.as_ref(), CertExpr::Int(1, _) | CertExpr::Uint(1, _))
                    && after == left.as_ref())
                    || (matches!(left.as_ref(), CertExpr::Int(1, _) | CertExpr::Uint(1, _))
                        && after == right.as_ref()))
        }
        SuperoptRule::IntMulZero => {
            *op == BinOp::Mul
                && (matches!(right.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                    || matches!(left.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _)))
                && matches!(after, CertExpr::Int(0, _) | CertExpr::Uint(0, _))
        }
        SuperoptRule::IntMulTwoAdd => {
            *op == BinOp::Mul
                && match after {
                    CertExpr::BinOp {
                        op: BinOp::Add,
                        left: al,
                        right: ar,
                    } if al == ar => {
                        (matches!(right.as_ref(), CertExpr::Int(2, _) | CertExpr::Uint(2, _))
                            && al.as_ref() == left.as_ref())
                            || (matches!(left.as_ref(), CertExpr::Int(2, _) | CertExpr::Uint(2, _))
                                && al.as_ref() == right.as_ref())
                    }
                    _ => false,
                }
        }
        SuperoptRule::IntMulPow2Shl => {
            *op == BinOp::Mul
                && match after {
                    CertExpr::BinOp {
                        op: BinOp::Shl,
                        left: al,
                        right: ar,
                    } => {
                        let check_pair = |var: &CertExpr, c_expr: &CertExpr| -> bool {
                            if al.as_ref() != var {
                                return false;
                            }
                            match (c_expr, ar.as_ref()) {
                                (CertExpr::Int(c, ty), CertExpr::Int(sh, st))
                                    if *c > 2 && ty == st =>
                                {
                                    let cu = *c as u128;
                                    cu.is_power_of_two() && *sh == cu.trailing_zeros() as i128
                                }
                                (CertExpr::Uint(c, ty), CertExpr::Uint(sh, st))
                                    if *c > 2 && ty == st =>
                                {
                                    c.is_power_of_two() && *sh == c.trailing_zeros() as u128
                                }
                                _ => false,
                            }
                        };
                        check_pair(left.as_ref(), right.as_ref())
                            || check_pair(right.as_ref(), left.as_ref())
                    }
                    _ => false,
                }
        }
        SuperoptRule::IntBitId => match op {
            BinOp::BitOr => {
                (matches!(right.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                    && after == left.as_ref())
                    || (matches!(left.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                        && after == right.as_ref())
                    || (left == right && after == left.as_ref())
            }
            BinOp::BitXor => {
                (matches!(right.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                    && after == left.as_ref())
                    || (matches!(left.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                        && after == right.as_ref())
                    || (left == right
                        && matches!(after, CertExpr::Int(0, _) | CertExpr::Uint(0, _)))
            }
            BinOp::BitAnd => {
                ((matches!(right.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _))
                    || matches!(left.as_ref(), CertExpr::Int(0, _) | CertExpr::Uint(0, _)))
                    && matches!(after, CertExpr::Int(0, _) | CertExpr::Uint(0, _)))
                    || (left == right && after == left.as_ref())
            }
            _ => false,
        },
        SuperoptRule::FloatMulOne => {
            let one_bits = 1.0_f64.to_bits();
            *op == BinOp::Mul
                && ((matches!(right.as_ref(), CertExpr::FloatBits(b, _) if *b == one_bits)
                    && after == left.as_ref())
                    || (matches!(left.as_ref(), CertExpr::FloatBits(b, _) if *b == one_bits)
                        && after == right.as_ref()))
        }
        SuperoptRule::FloatMulTwoAdd => {
            let two_bits = 2.0_f64.to_bits();
            *op == BinOp::Mul
                && match after {
                    CertExpr::BinOp {
                        op: BinOp::Add,
                        left: al,
                        right: ar,
                    } if al == ar => {
                        (matches!(right.as_ref(), CertExpr::FloatBits(b, _) if *b == two_bits)
                            && al.as_ref() == left.as_ref())
                            || (matches!(left.as_ref(), CertExpr::FloatBits(b, _) if *b == two_bits)
                                && al.as_ref() == right.as_ref())
                    }
                    _ => false,
                }
        }
        SuperoptRule::FloatDivPow2Mul => {
            *op == BinOp::Div
                && match (right.as_ref(), after) {
                    (
                        CertExpr::FloatBits(div_b, div_ty),
                        CertExpr::BinOp {
                            op: BinOp::Mul,
                            left: al,
                            right: ar,
                        },
                    ) if al.as_ref() == left.as_ref() => match ar.as_ref() {
                        CertExpr::FloatBits(rec_b, rec_ty) if rec_ty == div_ty => {
                            exact_pow2_float_recip(*div_b, div_ty) == Some(*rec_b)
                        }
                        _ => false,
                    },
                    _ => false,
                }
        }
    };

    if ok {
        Ok(())
    } else {
        Err(format!(
            "axiom violation for rule {:?}: before {:?} does not entail after {:?}",
            rule, before, after
        ))
    }
}

/// Independently verify a full [`SuperoptCertificate`] fail-closed.
pub fn verify_certificate(cert: &SuperoptCertificate) -> Result<(), String> {
    if cert.schema != SUPEROPT_CERT_SCHEMA {
        return Err(format!(
            "unsupported certificate schema '{}', expected '{}'",
            cert.schema, SUPEROPT_CERT_SCHEMA
        ));
    }
    if !cert.verified {
        return Err("certificate is not marked verified".to_string());
    }
    if cert.rewrites_applied != cert.steps.len() {
        return Err(format!(
            "rewrites_applied ({}) does not match steps.len() ({})",
            cert.rewrites_applied,
            cert.steps.len()
        ));
    }
    let expected_sha = compute_certificate_sha256(cert)?;
    if cert.cert_sha256 != expected_sha {
        return Err(format!(
            "certificate SHA-256 mismatch: expected {}, found {}",
            expected_sha, cert.cert_sha256
        ));
    }

    let mut sum_before = 0u64;
    let mut sum_after = 0u64;
    for (idx, step) in cert.steps.iter().enumerate() {
        let actual_before = expr_cost(&step.before);
        let actual_after = expr_cost(&step.after);
        if step.cost_before != actual_before || step.cost_after != actual_after {
            return Err(format!(
                "step {idx} cost metadata tampered: recorded ({} -> {}), actual ({} -> {})",
                step.cost_before, step.cost_after, actual_before, actual_after
            ));
        }
        if step.cost_after > step.cost_before {
            return Err(format!(
                "step {idx} increases cost ({} -> {})",
                step.cost_before, step.cost_after
            ));
        }
        verify_step_axiom(&step.rule, &step.before, &step.after)
            .map_err(|e| format!("step {idx} in {} bb{}: {e}", step.function, step.block_id))?;
        sum_before += step.cost_before as u64;
        sum_after += step.cost_after as u64;
    }

    if cert.total_cost_before != sum_before || cert.total_cost_after != sum_after {
        return Err(format!(
            "certificate cost totals mismatch: recorded ({} -> {}), summed ({} -> {})",
            cert.total_cost_before, cert.total_cost_after, sum_before, sum_after
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{BlockId, MirFnSig, MirLocal, MirStmt};

    fn make_test_module() -> MirModule {
        let mut module = MirModule::new("superopt_unit");
        let sig = MirFnSig::new(vec![MirType::i64(), MirType::f64()], MirType::f64());
        let mut func = MirFunction::new("kernel", sig);

        let mut p0 = MirLocal::named(LocalId(0), "x", MirType::i64());
        p0.is_param = true;
        func.add_local(p0);
        let mut p1 = MirLocal::named(LocalId(1), "y", MirType::f64());
        p1.is_param = true;
        func.add_local(p1);
        func.add_local(MirLocal::new(LocalId(2), MirType::i64())); // c_two = 2
        func.add_local(MirLocal::new(LocalId(3), MirType::i64())); // x * 2 -> x + x
        func.add_local(MirLocal::new(LocalId(4), MirType::i64())); // x * 8 -> x << 3
        func.add_local(MirLocal::new(LocalId(5), MirType::i64())); // (x << 3) + 0 -> x << 3
        func.add_local(MirLocal::new(LocalId(6), MirType::f64())); // y / 4.0 -> y * 0.25
        func.add_local(MirLocal::new(LocalId(7), MirType::f64())); // (y * 0.25) * 2.0 -> add

        let mut bb = MirBlock::new(BlockId::ENTRY);
        bb.push_stmt(MirStmt::assign(
            LocalId(2),
            MirRValue::Use(MirValue::Const(MirConst::Int(2, MirType::i64()))),
        ));
        bb.push_stmt(MirStmt::assign(
            LocalId(3),
            MirRValue::BinaryOp {
                op: BinOp::Mul,
                left: MirValue::Local(LocalId(0)),
                right: MirValue::Local(LocalId(2)),
            },
        ));
        bb.push_stmt(MirStmt::assign(
            LocalId(4),
            MirRValue::BinaryOp {
                op: BinOp::Mul,
                left: MirValue::Local(LocalId(0)),
                right: MirValue::Const(MirConst::Int(8, MirType::i64())),
            },
        ));
        bb.push_stmt(MirStmt::assign(
            LocalId(5),
            MirRValue::BinaryOp {
                op: BinOp::Add,
                left: MirValue::Local(LocalId(4)),
                right: MirValue::Const(MirConst::Int(0, MirType::i64())),
            },
        ));
        bb.push_stmt(MirStmt::assign(
            LocalId(6),
            MirRValue::BinaryOp {
                op: BinOp::Div,
                left: MirValue::Local(LocalId(1)),
                right: MirValue::Const(MirConst::Float(4.0, MirType::f64())),
            },
        ));
        bb.push_stmt(MirStmt::assign(
            LocalId(7),
            MirRValue::BinaryOp {
                op: BinOp::Mul,
                left: MirValue::Local(LocalId(6)),
                right: MirValue::Const(MirConst::Float(2.0, MirType::f64())),
            },
        ));
        bb.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(7)))));
        func.add_block(bb);
        module.add_function(func);
        module
    }

    #[test]
    fn superoptimize_reduces_cost_and_verifies_certificate() {
        let mut module = make_test_module();
        let cert = superoptimize_module(&mut module, "c").expect("superopt must succeed");
        assert_eq!(cert.schema, SUPEROPT_CERT_SCHEMA);
        assert_eq!(cert.rewrites_applied, 5);
        assert!(cert.total_cost_after < cert.total_cost_before);
        assert_ne!(cert.mir_before_sha256, cert.mir_after_sha256);
        verify_certificate(&cert).expect("fresh certificate must verify");
    }

    #[test]
    fn verifier_rejects_forged_rewrite_even_with_recomputed_hash() {
        let mut module = make_test_module();
        let mut cert = superoptimize_module(&mut module, "c").expect("superopt must succeed");
        // Forge step 0: claim `x * 2 -> x + 3` under R8_INT_MUL_TWO_ADD.
        cert.steps[0].after = CertExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(CertExpr::Local(0)),
            right: Box::new(CertExpr::Int(3, MirType::i64())),
        };
        cert.cert_sha256 = compute_certificate_sha256(&cert).unwrap();
        let err = verify_certificate(&cert).expect_err("forged rewrite must be rejected");
        assert!(err.contains("axiom violation"), "unexpected err: {err}");
    }

    #[test]
    fn verifier_rejects_tampered_digest() {
        let mut module = make_test_module();
        let mut cert = superoptimize_module(&mut module, "c").expect("superopt must succeed");
        cert.cert_sha256 = "0".repeat(64);
        let err = verify_certificate(&cert).expect_err("tampered digest must fail");
        assert!(err.contains("SHA-256 mismatch"), "unexpected err: {err}");
    }

    #[test]
    fn verifier_rejects_non_power_of_two_float_div_rewrite() {
        let before = CertExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(CertExpr::Local(1)),
            right: Box::new(CertExpr::FloatBits(3.0_f64.to_bits(), MirType::f64())),
        };
        let after = CertExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(CertExpr::Local(1)),
            right: Box::new(CertExpr::FloatBits(
                (1.0_f64 / 3.0_f64).to_bits(),
                MirType::f64(),
            )),
        };
        let err = verify_step_axiom(&SuperoptRule::FloatDivPow2Mul, &before, &after)
            .expect_err("non-pow2 float div rewrite is unsound and must fail");
        assert!(err.contains("axiom violation"), "unexpected err: {err}");
    }
}
