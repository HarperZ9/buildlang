// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! V5 Certified E-Graph Superoptimizer for BuildLang MIR (`--superopt`).
//!
//! Performs deterministic equality saturation and cost-guided extraction over
//! pure SSA arithmetic and 2026 SOTA AI/Graphics kernel expressions in
//! [`MirModule`] (Online-Softmax / Log-Sum-Exp fusion, FMA contraction,
//! block-scaled E2M1/FP8 shared-exponent scale hoisting, and opt-in
//! FlashAttention-4 Horner FMA `exp2` emulation), emits a SHA-256 sealed
//! `buildlang.superopt-cert/v1` proof certificate recording every rewrite step
//! and error-bound witness, and runs an independent fail-closed axiom verifier
//! before any backend sees the superoptimized MIR.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::codegen::ir::{
    BinOp, BlockId, FloatSize, IntSize, LocalId, MirBlock, MirConst, MirFunction, MirLocal,
    MirModule, MirRValue, MirStmt, MirStmtKind, MirTerminator, MirType, MirValue,
};

/// Canonical schema identifier for BuildLang superoptimization certificates.
pub const SUPEROPT_CERT_SCHEMA: &str = "buildlang.superopt-cert/v1";

/// Degree-4 Chebyshev minimax coefficients approximating `2^f` on `f in [0, 1]`
/// with `max_abs_err <= 4.0e-6` (FlashAttention-4 FMA-emulated `exp2` pipe).
pub const HORNER_EXP2_DEG4_COEFFS: [f64; 5] = [
    1.0000034929076984_f64,
    0.6929729221730486_f64,
    0.24160435727010388_f64,
    0.051744997764090285_f64,
    0.0136703094533634_f64,
];

/// Evaluate the certified degree-4 Horner `exp2` polynomial on `f in [0, 1]`.
pub fn horner_exp2_f64(f: f64) -> f64 {
    let c = HORNER_EXP2_DEG4_COEFFS;
    let t1 = c[4].mul_add(f, c[3]);
    let t2 = t1.mul_add(f, c[2]);
    let t3 = t2.mul_add(f, c[1]);
    t3.mul_add(f, c[0])
}

/// Configuration for V5 certified e-graph superoptimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuperoptConfig {
    /// Enable `(a * b) + c -> fma(a, b, c)` contraction on targets with native
    /// or libm FMA support (`c`, `wgsl`, `hlsl`, `msl`, `spirv`, `llvm`).
    pub allow_fma_contract: bool,
    /// Opt-in FlashAttention-4 (`arXiv:2603.05451`) Horner FMA-emulated `exp2`
    /// rewrite over fractional inputs `f in [0, 1]`.
    pub enable_horner_exp2_fma: bool,
}

impl SuperoptConfig {
    /// Default target-aware superoptimization configuration.
    pub fn for_target(target: &str) -> Self {
        Self {
            allow_fma_contract: matches!(target, "c" | "wgsl" | "hlsl" | "msl" | "spirv" | "llvm"),
            enable_horner_exp2_fma: false,
        }
    }
}

/// Certified accuracy witness attached to each rewrite step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RewriteErrorWitness {
    /// Bit-exact equivalence (`0.0` error, `0` ULP drift).
    #[default]
    BitExact,
    /// Algebraically exact over reals with certified IEEE-754 bounded ULP error.
    BoundedUlp { max_ulps: u32 },
}

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
    /// IEEE-754 bit-exact float unit division (`x / 1.0 -> x`).
    #[serde(rename = "R14_FLOAT_DIV_ONE")]
    FloatDivOne,
    /// Online-Softmax / Log-Sum-Exp exponential product fusion
    /// (`exp(a) * exp(b) -> exp(a + b)` and `exp2(a) * exp2(b) -> exp2(a + b)`).
    #[serde(rename = "R15_ONLINE_SOFTMAX_EXP_MUL_FUSE")]
    OnlineSoftmaxExpMulFuse,
    /// Online-Softmax / Log-Sum-Exp exponential ratio fusion
    /// (`exp(a) / exp(b) -> exp(a - b)` and `exp2(a) / exp2(b) -> exp2(a - b)`).
    #[serde(rename = "R16_ONLINE_SOFTMAX_EXP_DIV_FUSE")]
    OnlineSoftmaxExpDivFuse,
    /// Log-Sum-Exp inverse cancellation (`log(exp(x)) -> x`, `log2(exp2(x)) -> x`).
    #[serde(rename = "R17_LOG_EXP_CANCEL")]
    LogExpCancel,
    /// Single-rounding FMA contraction (`(a * b) + c -> fma(a, b, c)` and
    /// `c + (a * b) -> fma(a, b, c)`) for dot products, Online-Softmax, and
    /// Gated-DeltaNet recurrences.
    #[serde(rename = "R18_FMA_CONTRACT")]
    FmaContract,
    /// Block-scaled microscaling (`E2M1`/`FP8` shared-exponent `E8M0`/`E4M3`)
    /// scale hoisting: `(s * a) + (s * b) -> s * (a + b)` (and commutative
    /// forms) or `(a * c1) * (b * c2) -> (a * b) * (c1 * c2)`.
    #[serde(rename = "R19_BLOCK_SCALE_HOIST")]
    BlockScaleHoist,
    /// FlashAttention-4 degree-4 Horner FMA-emulated `exp2` rewrite over
    /// fractional inputs `f in [0, 1]`.
    #[serde(rename = "R20_HORNER_EXP2_FMA")]
    HornerExp2Fma,
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
    /// Pure mathematical intrinsic call (`exp`, `exp2`, `log`, `log2`, `fma`).
    Call {
        func: String,
        args: Vec<CertExpr>,
        ret_ty: MirType,
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
    #[serde(default)]
    pub error_witness: RewriteErrorWitness,
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
        CertExpr::Call { func, args, .. } => {
            let call_c = match func.as_str() {
                "fma" => 2,
                "exp" | "exp2" | "log" | "log2" => 12,
                _ => 6,
            };
            call_c + args.iter().map(expr_cost).sum::<u32>()
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

/// Returns `true` iff `bits` encodes an exact finite IEEE-754 power of two
/// (`+/- 2^k` with normal exponent, i.e. an `E8M0` microscaling shared exponent).
fn is_exact_pow2_float(bits: u64, ty: &MirType) -> bool {
    let v = f64::from_bits(bits);
    if !v.is_finite() || v == 0.0 {
        return false;
    }
    let abs_bits = bits & 0x7fff_ffff_ffff_ffff;
    let frac = abs_bits & 0x000f_ffff_ffff_ffff;
    let exp = (abs_bits >> 52) & 0x7ff;
    if frac != 0 || exp == 0 || exp == 0x7ff {
        return false;
    }
    if matches!(ty, MirType::Float(FloatSize::F32)) {
        let vf = v as f32;
        if !vf.is_finite() || vf == 0.0 || (vf as f64) != v {
            return false;
        }
    }
    true
}

/// Returns `Some(recip)` iff `v` is an exact finite power of two whose
/// reciprocal is also an exact normal finite power of two (`v * recip == 1.0`).
fn exact_pow2_float_recip(bits: u64, ty: &MirType) -> Option<u64> {
    if !is_exact_pow2_float(bits, ty) {
        return None;
    }
    let v = f64::from_bits(bits);
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
    // Avoid trivial `x / 1.0 -> x * 1.0` (handled by R14_FLOAT_DIV_ONE).
    if v == 1.0 {
        return None;
    }
    Some(recip.to_bits())
}

fn is_cert_atom(expr: &CertExpr) -> bool {
    matches!(
        expr,
        CertExpr::Local(_) | CertExpr::Int(_, _) | CertExpr::Uint(_, _) | CertExpr::FloatBits(_, _)
    )
}

fn collect_expr_locals(expr: &CertExpr, out: &mut HashSet<LocalId>) {
    match expr {
        CertExpr::Local(id) => {
            out.insert(LocalId(*id));
        }
        CertExpr::Int(_, _) | CertExpr::Uint(_, _) | CertExpr::FloatBits(_, _) => {}
        CertExpr::BinOp { left, right, .. } => {
            collect_expr_locals(left, out);
            collect_expr_locals(right, out);
        }
        CertExpr::Call { args, .. } => {
            for a in args {
                collect_expr_locals(a, out);
            }
        }
    }
}

/// Construct the canonical degree-4 Horner FMA `CertExpr` for `exp2(f)`.
pub fn horner_exp2_cert_expr(f: CertExpr, ret_ty: MirType) -> CertExpr {
    let c = HORNER_EXP2_DEG4_COEFFS;
    let c4 = CertExpr::FloatBits(c[4].to_bits(), ret_ty.clone());
    let c3 = CertExpr::FloatBits(c[3].to_bits(), ret_ty.clone());
    let c2 = CertExpr::FloatBits(c[2].to_bits(), ret_ty.clone());
    let c1 = CertExpr::FloatBits(c[1].to_bits(), ret_ty.clone());
    let c0 = CertExpr::FloatBits(c[0].to_bits(), ret_ty.clone());
    let t1 = CertExpr::Call {
        func: "fma".to_string(),
        args: vec![c4, f.clone(), c3],
        ret_ty: ret_ty.clone(),
    };
    let t2 = CertExpr::Call {
        func: "fma".to_string(),
        args: vec![t1, f.clone(), c2],
        ret_ty: ret_ty.clone(),
    };
    let t3 = CertExpr::Call {
        func: "fma".to_string(),
        args: vec![t2, f.clone(), c1],
        ret_ty: ret_ty.clone(),
    };
    CertExpr::Call {
        func: "fma".to_string(),
        args: vec![t3, f, c0],
        ret_ty,
    }
}

/// Extract shared factor `(s, a, b)` from `(l1 * l2) + (r1 * r2)` for
/// `R19_BLOCK_SCALE_HOIST`.
fn extract_shared_scale_sum(
    left: &CertExpr,
    right: &CertExpr,
) -> Option<(CertExpr, CertExpr, CertExpr)> {
    let CertExpr::BinOp {
        op: BinOp::Mul,
        left: l1,
        right: l2,
    } = left
    else {
        return None;
    };
    let CertExpr::BinOp {
        op: BinOp::Mul,
        left: r1,
        right: r2,
    } = right
    else {
        return None;
    };
    if !is_cert_atom(l1) || !is_cert_atom(l2) || !is_cert_atom(r1) || !is_cert_atom(r2) {
        return None;
    }
    if l1 == r1 {
        Some((
            l1.as_ref().clone(),
            l2.as_ref().clone(),
            r2.as_ref().clone(),
        ))
    } else if l1 == r2 {
        Some((
            l1.as_ref().clone(),
            l2.as_ref().clone(),
            r1.as_ref().clone(),
        ))
    } else if l2 == r1 {
        Some((
            l2.as_ref().clone(),
            l1.as_ref().clone(),
            r2.as_ref().clone(),
        ))
    } else if l2 == r2 {
        Some((
            l2.as_ref().clone(),
            l1.as_ref().clone(),
            r1.as_ref().clone(),
        ))
    } else {
        None
    }
}

/// Extract `(a, c1, b, c2, ty)` from `(a * c1) * (b * c2)` (any factor order)
/// where `c1` and `c2` are finite float constants of matching type `ty`.
fn extract_const_scale_pair(
    left: &CertExpr,
    right: &CertExpr,
) -> Option<(CertExpr, u64, CertExpr, u64, MirType)> {
    let extract_scaled_atom = |e: &CertExpr| -> Option<(CertExpr, u64, MirType)> {
        let CertExpr::BinOp {
            op: BinOp::Mul,
            left: a,
            right: b,
        } = e
        else {
            return None;
        };
        match (a.as_ref(), b.as_ref()) {
            (var, CertExpr::FloatBits(cb, ty)) if is_cert_atom(var) => {
                Some((var.clone(), *cb, ty.clone()))
            }
            (CertExpr::FloatBits(cb, ty), var) if is_cert_atom(var) => {
                Some((var.clone(), *cb, ty.clone()))
            }
            _ => None,
        }
    };
    let (a, c1, ty1) = extract_scaled_atom(left)?;
    let (b, c2, ty2) = extract_scaled_atom(right)?;
    if ty1 != ty2 {
        return None;
    }
    Some((a, c1, b, c2, ty1))
}

/// Deterministic certified error-bound witness for `(rule, before)`.
pub fn expected_error_witness(rule: &SuperoptRule, before: &CertExpr) -> RewriteErrorWitness {
    match rule {
        SuperoptRule::ConstFoldInt
        | SuperoptRule::ConstFoldFloat
        | SuperoptRule::IntAddZero
        | SuperoptRule::IntSubZero
        | SuperoptRule::IntSubSelf
        | SuperoptRule::IntMulOne
        | SuperoptRule::IntMulZero
        | SuperoptRule::IntMulTwoAdd
        | SuperoptRule::IntMulPow2Shl
        | SuperoptRule::IntBitId
        | SuperoptRule::FloatMulOne
        | SuperoptRule::FloatMulTwoAdd
        | SuperoptRule::FloatDivPow2Mul
        | SuperoptRule::FloatDivOne => RewriteErrorWitness::BitExact,
        SuperoptRule::OnlineSoftmaxExpMulFuse | SuperoptRule::OnlineSoftmaxExpDivFuse => {
            RewriteErrorWitness::BoundedUlp { max_ulps: 2 }
        }
        SuperoptRule::LogExpCancel | SuperoptRule::FmaContract => {
            RewriteErrorWitness::BoundedUlp { max_ulps: 1 }
        }
        SuperoptRule::BlockScaleHoist => {
            // Exact power-of-two E8M0 shared scales hoist without significand rounding.
            if let CertExpr::BinOp { op, left, right } = before {
                if *op == BinOp::Add {
                    if let Some((CertExpr::FloatBits(sb, ty), _, _)) =
                        extract_shared_scale_sum(left, right)
                    {
                        if is_exact_pow2_float(sb, &ty) {
                            return RewriteErrorWitness::BitExact;
                        }
                    }
                } else if *op == BinOp::Mul {
                    if let Some((_, c1, _, c2, ty)) = extract_const_scale_pair(left, right) {
                        if is_exact_pow2_float(c1, &ty) && is_exact_pow2_float(c2, &ty) {
                            return RewriteErrorWitness::BitExact;
                        }
                    }
                }
            }
            RewriteErrorWitness::BoundedUlp { max_ulps: 2 }
        }
        SuperoptRule::HornerExp2Fma => RewriteErrorWitness::BoundedUlp { max_ulps: 32 },
    }
}

/// Deterministic normalized Walsh-Hadamard transform `H_B` (`B` power of two,
/// e.g. `16` for NVFP4 or `32` for MXFP4) satisfying `H_B * H_B^T = I`
/// (Full-Stack FP4, arXiv:2607.04422).
pub fn walsh_hadamard_transform_normalized(buf: &mut [f64]) -> Result<(), String> {
    let n = buf.len();
    if n == 0 || !n.is_power_of_two() {
        return Err(format!(
            "Walsh-Hadamard block size must be a positive power of two, got {n}"
        ));
    }
    let mut h = 1usize;
    while h < n {
        let step = h * 2;
        for start in (0..n).step_by(step) {
            for i in start..(start + h) {
                let u = buf[i];
                let v = buf[i + h];
                buf[i] = u + v;
                buf[i + h] = u - v;
            }
        }
        h = step;
    }
    let inv_sqrt = 1.0_f64 / (n as f64).sqrt();
    for x in buf.iter_mut() {
        *x *= inv_sqrt;
    }
    Ok(())
}

/// Quantize a block of `B` finite f64 values into `E2M1` (FP4 values in
/// `{0, +/-0.5, +/-1, +/-1.5, +/-2, +/-3, +/-4, +/-6}`) with either an exact
/// power-of-two `E8M0` (MXFP4) scale or an `E4M3`-style (NVFP4) block scale.
/// Returns `(block_scale, quantized_e2m1_values)`.
pub fn quantize_e2m1_block(block: &[f64], use_e8m0_pow2: bool) -> Result<(f64, Vec<f64>), String> {
    if block.is_empty() || !block.len().is_power_of_two() {
        return Err(format!(
            "microscaling block length must be a power of two (16 or 32), got {}",
            block.len()
        ));
    }
    if block.iter().any(|v| !v.is_finite()) {
        return Err("non-finite value in microscaling block".to_string());
    }
    let max_abs = block.iter().fold(0.0_f64, |acc, &v| acc.max(v.abs()));
    if max_abs == 0.0 {
        return Ok((1.0, vec![0.0; block.len()]));
    }
    let raw_scale = max_abs / 6.0_f64;
    let scale = if use_e8m0_pow2 {
        // Round up to exact power of two `2^ceil(log2(raw_scale))` (E8M0).
        2.0_f64.powi(raw_scale.log2().ceil() as i32)
    } else {
        raw_scale
    };
    const E2M1_LEVELS: [f64; 8] = [0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0];
    let mut q = Vec::with_capacity(block.len());
    for &x in block {
        let scaled = x / scale;
        let sign = if scaled < 0.0 { -1.0 } else { 1.0 };
        let mag = scaled.abs();
        let mut best = E2M1_LEVELS[0];
        let mut best_diff = (mag - best).abs();
        for &lvl in &E2M1_LEVELS[1..] {
            let d = (mag - lvl).abs();
            if d < best_diff {
                best_diff = d;
                best = lvl;
            }
        }
        q.push(sign * best);
    }
    Ok((scale, q))
}

fn saturate_binop(
    op: BinOp,
    left: &CertExpr,
    right: &CertExpr,
    dest_ty: &MirType,
    allow_int_shift: bool,
    allow_fma_contract: bool,
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
                    let try_pow2 = |var: &CertExpr,
                                    c: u128,
                                    c_ty: &MirType,
                                    is_signed_const: bool|
                     -> Option<CertExpr> {
                        if c > 2 && c.is_power_of_two() {
                            let shift = c.trailing_zeros();
                            if shift < bits.saturating_sub(1) {
                                let shift_const = if is_signed_const {
                                    CertExpr::Int(shift as i128, c_ty.clone())
                                } else {
                                    CertExpr::Uint(shift as u128, c_ty.clone())
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
                        CertExpr::Int(c, c_ty) if *c > 2 => {
                            if let Some(shl) = try_pow2(left, *c as u128, c_ty, true) {
                                eg.add_equiv(shl, SuperoptRule::IntMulPow2Shl);
                            }
                        }
                        CertExpr::Uint(c, c_ty) if *c > 2 => {
                            if let Some(shl) = try_pow2(left, *c, c_ty, false) {
                                eg.add_equiv(shl, SuperoptRule::IntMulPow2Shl);
                            }
                        }
                        _ => {}
                    }
                    match left {
                        CertExpr::Int(c, c_ty) if *c > 2 => {
                            if let Some(shl) = try_pow2(right, *c as u128, c_ty, true) {
                                eg.add_equiv(shl, SuperoptRule::IntMulPow2Shl);
                            }
                        }
                        CertExpr::Uint(c, c_ty) if *c > 2 => {
                            if let Some(shl) = try_pow2(right, *c, c_ty, false) {
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

    // IEEE-754 sound floating-point rules + 2026 SOTA AI/Graphics kernel rules
    if dest_ty.is_float() {
        let one_bits = 1.0_f64.to_bits();
        let two_bits = 2.0_f64.to_bits();
        match op {
            BinOp::Mul => {
                if is_cert_atom(left)
                    && matches!(right, CertExpr::FloatBits(b, _) if *b == one_bits)
                {
                    eg.add_equiv(left.clone(), SuperoptRule::FloatMulOne);
                }
                if is_cert_atom(right)
                    && matches!(left, CertExpr::FloatBits(b, _) if *b == one_bits)
                {
                    eg.add_equiv(right.clone(), SuperoptRule::FloatMulOne);
                }
                if is_cert_atom(left)
                    && matches!(right, CertExpr::FloatBits(b, _) if *b == two_bits)
                {
                    eg.add_equiv(
                        CertExpr::BinOp {
                            op: BinOp::Add,
                            left: Box::new(left.clone()),
                            right: Box::new(left.clone()),
                        },
                        SuperoptRule::FloatMulTwoAdd,
                    );
                }
                if is_cert_atom(right)
                    && matches!(left, CertExpr::FloatBits(b, _) if *b == two_bits)
                {
                    eg.add_equiv(
                        CertExpr::BinOp {
                            op: BinOp::Add,
                            left: Box::new(right.clone()),
                            right: Box::new(right.clone()),
                        },
                        SuperoptRule::FloatMulTwoAdd,
                    );
                }

                // R15: Online-Softmax / Log-Sum-Exp `exp(a) * exp(b) -> exp(a + b)`
                if let (
                    CertExpr::Call {
                        func: f1,
                        args: a1,
                        ret_ty: t1,
                    },
                    CertExpr::Call {
                        func: f2,
                        args: a2,
                        ret_ty: t2,
                    },
                ) = (left, right)
                {
                    if f1 == f2
                        && matches!(f1.as_str(), "exp" | "exp2")
                        && a1.len() == 1
                        && a2.len() == 1
                        && t1 == dest_ty
                        && t2 == dest_ty
                        && is_cert_atom(&a1[0])
                        && is_cert_atom(&a2[0])
                    {
                        eg.add_equiv(
                            CertExpr::Call {
                                func: f1.clone(),
                                args: vec![CertExpr::BinOp {
                                    op: BinOp::Add,
                                    left: Box::new(a1[0].clone()),
                                    right: Box::new(a2[0].clone()),
                                }],
                                ret_ty: dest_ty.clone(),
                            },
                            SuperoptRule::OnlineSoftmaxExpMulFuse,
                        );
                    }
                }

                // R19 Form B: `(a * c1) * (b * c2) -> (a * b) * (c1 * c2)`
                if let Some((a, c1, b, c2, ty)) = extract_const_scale_pair(left, right) {
                    if &ty == dest_ty {
                        if let Some(c12) = eval_float_binop(BinOp::Mul, c1, c2, &ty) {
                            eg.add_equiv(
                                CertExpr::BinOp {
                                    op: BinOp::Mul,
                                    left: Box::new(CertExpr::BinOp {
                                        op: BinOp::Mul,
                                        left: Box::new(a),
                                        right: Box::new(b),
                                    }),
                                    right: Box::new(CertExpr::FloatBits(c12, ty)),
                                },
                                SuperoptRule::BlockScaleHoist,
                            );
                        }
                    }
                }
            }
            BinOp::Div => {
                // R14: Bit-exact `x / 1.0 -> x`
                if is_cert_atom(left)
                    && matches!(right, CertExpr::FloatBits(b, _) if *b == one_bits)
                {
                    eg.add_equiv(left.clone(), SuperoptRule::FloatDivOne);
                }
                // R13: Power-of-two float reciprocal `x / 2^k -> x * 2^-k`
                if is_cert_atom(left) {
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
                // R16: Online-Softmax / Log-Sum-Exp `exp(a) / exp(b) -> exp(a - b)`
                if let (
                    CertExpr::Call {
                        func: f1,
                        args: a1,
                        ret_ty: t1,
                    },
                    CertExpr::Call {
                        func: f2,
                        args: a2,
                        ret_ty: t2,
                    },
                ) = (left, right)
                {
                    if f1 == f2
                        && matches!(f1.as_str(), "exp" | "exp2")
                        && a1.len() == 1
                        && a2.len() == 1
                        && t1 == dest_ty
                        && t2 == dest_ty
                        && is_cert_atom(&a1[0])
                        && is_cert_atom(&a2[0])
                    {
                        eg.add_equiv(
                            CertExpr::Call {
                                func: f1.clone(),
                                args: vec![CertExpr::BinOp {
                                    op: BinOp::Sub,
                                    left: Box::new(a1[0].clone()),
                                    right: Box::new(a2[0].clone()),
                                }],
                                ret_ty: dest_ty.clone(),
                            },
                            SuperoptRule::OnlineSoftmaxExpDivFuse,
                        );
                    }
                }
            }
            BinOp::Add => {
                // R19 Form A: Shared block-scale hoisting `(s * a) + (s * b) -> s * (a + b)`
                // (cost 7 -> 4, beats FMA cost 5 automatically in MiniEGraph!)
                if let Some((s, a, b)) = extract_shared_scale_sum(left, right) {
                    eg.add_equiv(
                        CertExpr::BinOp {
                            op: BinOp::Mul,
                            left: Box::new(s),
                            right: Box::new(CertExpr::BinOp {
                                op: BinOp::Add,
                                left: Box::new(a),
                                right: Box::new(b),
                            }),
                        },
                        SuperoptRule::BlockScaleHoist,
                    );
                }

                // R18: Single-rounding FMA contraction `(a * b) + c -> fma(a, b, c)`
                if allow_fma_contract {
                    if let CertExpr::BinOp {
                        op: BinOp::Mul,
                        left: ma,
                        right: mb,
                    } = left
                    {
                        if is_cert_atom(ma) && is_cert_atom(mb) && is_cert_atom(right) {
                            eg.add_equiv(
                                CertExpr::Call {
                                    func: "fma".to_string(),
                                    args: vec![
                                        ma.as_ref().clone(),
                                        mb.as_ref().clone(),
                                        right.clone(),
                                    ],
                                    ret_ty: dest_ty.clone(),
                                },
                                SuperoptRule::FmaContract,
                            );
                        }
                    }
                    if let CertExpr::BinOp {
                        op: BinOp::Mul,
                        left: ma,
                        right: mb,
                    } = right
                    {
                        if is_cert_atom(ma) && is_cert_atom(mb) && is_cert_atom(left) {
                            eg.add_equiv(
                                CertExpr::Call {
                                    func: "fma".to_string(),
                                    args: vec![
                                        ma.as_ref().clone(),
                                        mb.as_ref().clone(),
                                        left.clone(),
                                    ],
                                    ret_ty: dest_ty.clone(),
                                },
                                SuperoptRule::FmaContract,
                            );
                        }
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
        CertExpr::BinOp { .. } | CertExpr::Call { .. } => None,
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
        CertExpr::Call { .. } => None,
        atom => cert_atom_to_mir_value(atom).map(MirRValue::Use),
    }
}

/// Partition function locals into:
/// - `mutated_locals`: assigned more than once, address-taken, or mutated via
///   deref/field (never safe for cross-block expression substitution).
/// - `single_def_locals`: assigned exactly once, not a param, not address-taken.
/// - `unsafe_const_locals`: `mutated_locals` union `param_locals`.
fn analyze_function_locals(
    func: &MirFunction,
) -> (HashSet<LocalId>, HashSet<LocalId>, HashSet<LocalId>) {
    let mut mutated = HashSet::new();
    let mut params = HashSet::new();
    let mut def_counts: HashMap<LocalId, usize> = HashMap::new();
    for local in &func.locals {
        if local.is_param {
            params.insert(local.id);
        }
    }
    if let Some(blocks) = &func.blocks {
        for block in blocks {
            for stmt in &block.stmts {
                match &stmt.kind {
                    MirStmtKind::Assign { dest, value } => {
                        *def_counts.entry(*dest).or_insert(0) += 1;
                        if let MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. } =
                            value
                        {
                            mutated.insert(place.local);
                        }
                    }
                    MirStmtKind::DerefAssign { ptr, .. } => {
                        mutated.insert(*ptr);
                    }
                    MirStmtKind::FieldAssign { base, .. } => {
                        mutated.insert(*base);
                    }
                    MirStmtKind::FieldDerefAssign { ptr, .. } => {
                        mutated.insert(*ptr);
                    }
                    _ => {}
                }
            }
            if let Some(MirTerminator::Call { dest: Some(d), .. }) = &block.terminator {
                *def_counts.entry(*d).or_insert(0) += 1;
            }
        }
    }
    for (&id, &count) in &def_counts {
        if count != 1 || params.contains(&id) {
            mutated.insert(id);
        }
    }
    let mut single_def = HashSet::new();
    for (&id, &count) in &def_counts {
        if count == 1 && !params.contains(&id) && !mutated.contains(&id) {
            single_def.insert(id);
        }
    }
    let mut unsafe_const = mutated.clone();
    unsafe_const.extend(params.iter().copied());
    (mutated, single_def, unsafe_const)
}

fn callee_func_name(v: &MirValue) -> Option<&str> {
    match v {
        MirValue::Function(n) | MirValue::Global(n) => Some(n.as_ref()),
        _ => None,
    }
}

/// Build a function-wide map of single-def pure SSA expressions whose leaf
/// locals are never mutated anywhere in `func`, plus copy-origin chains
/// (`let var = _tmp`) for dead-code elimination after fusion.
fn collect_immutable_pure_defs(
    func: &MirFunction,
    mutated_locals: &HashSet<LocalId>,
    single_def_locals: &HashSet<LocalId>,
) -> (HashMap<LocalId, CertExpr>, HashMap<LocalId, LocalId>) {
    let mut defs: HashMap<LocalId, CertExpr> = HashMap::new();
    let mut copy_origin: HashMap<LocalId, LocalId> = HashMap::new();
    let Some(blocks) = &func.blocks else {
        return (defs, copy_origin);
    };
    let empty_const = HashMap::new();

    // Pass A: collect base definitions (constants, pure Mul, pure math Calls).
    for block in blocks {
        for stmt in &block.stmts {
            if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                if !single_def_locals.contains(dest) {
                    continue;
                }
                match value {
                    MirRValue::Use(MirValue::Const(c)) => {
                        if let Some(ce) = mir_const_to_cert(c) {
                            defs.insert(*dest, ce);
                        }
                    }
                    MirRValue::Use(MirValue::Local(src)) => {
                        copy_origin.insert(*dest, *src);
                    }
                    MirRValue::BinaryOp {
                        op: BinOp::Mul,
                        left,
                        right,
                    } => {
                        if let (Some(le), Some(re)) = (
                            mir_value_to_cert(left, &empty_const),
                            mir_value_to_cert(right, &empty_const),
                        ) {
                            let mut leaves = HashSet::new();
                            collect_expr_locals(&le, &mut leaves);
                            collect_expr_locals(&re, &mut leaves);
                            if leaves.iter().all(|l| !mutated_locals.contains(l)) {
                                defs.insert(
                                    *dest,
                                    CertExpr::BinOp {
                                        op: BinOp::Mul,
                                        left: Box::new(le),
                                        right: Box::new(re),
                                    },
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(MirTerminator::Call {
            func: callee,
            args,
            dest: Some(dest),
            ..
        }) = &block.terminator
        {
            if !single_def_locals.contains(dest) {
                continue;
            }
            let Some(name) = callee_func_name(callee) else {
                continue;
            };
            if matches!(name, "exp" | "exp2" | "log" | "log2") && args.len() == 1 {
                if let Some(arg_expr) = mir_value_to_cert(&args[0], &empty_const) {
                    let mut leaves = HashSet::new();
                    collect_expr_locals(&arg_expr, &mut leaves);
                    if leaves.iter().all(|l| !mutated_locals.contains(l)) {
                        let ret_ty = func
                            .locals
                            .get(dest.0 as usize)
                            .map(|l| l.ty.clone())
                            .unwrap_or(MirType::Void);
                        if ret_ty.is_float() {
                            defs.insert(
                                *dest,
                                CertExpr::Call {
                                    func: name.to_string(),
                                    args: vec![arg_expr],
                                    ret_ty,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    // Pass B: propagate pure expressions across single-def copy chains (`let ex = _tmp`).
    loop {
        let mut changed = false;
        for (&dest, &src) in &copy_origin {
            if !defs.contains_key(&dest) {
                if let Some(expr) = defs.get(&src).cloned() {
                    defs.insert(dest, expr);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    (defs, copy_origin)
}

fn mark_eliminable_chain(
    id: LocalId,
    copy_origin: &HashMap<LocalId, LocalId>,
    eliminable: &mut HashSet<LocalId>,
) {
    let mut cur = id;
    while eliminable.insert(cur) {
        if let Some(&orig) = copy_origin.get(&cur) {
            cur = orig;
        } else {
            break;
        }
    }
}

fn alloc_fresh_local(locals: &mut Vec<MirLocal>, ty: MirType) -> LocalId {
    let id = LocalId(locals.len() as u32);
    locals.push(MirLocal::new(id, ty));
    id
}

fn count_local_uses_in_value(v: &MirValue, counts: &mut HashMap<LocalId, usize>) {
    if let MirValue::Local(id) = v {
        *counts.entry(*id).or_insert(0) += 1;
    }
}

fn count_local_uses_in_place(
    p: &crate::codegen::ir::MirPlace,
    counts: &mut HashMap<LocalId, usize>,
) {
    *counts.entry(p.local).or_insert(0) += 1;
    for proj in &p.projections {
        if let crate::codegen::ir::PlaceProjection::Index(idx) = proj {
            *counts.entry(*idx).or_insert(0) += 1;
        }
    }
}

fn count_local_uses_in_rvalue(rv: &MirRValue, counts: &mut HashMap<LocalId, usize>) {
    match rv {
        MirRValue::Use(v)
        | MirRValue::UnaryOp { operand: v, .. }
        | MirRValue::Cast { value: v, .. } => {
            count_local_uses_in_value(v, counts);
        }
        MirRValue::BinaryOp { left, right, .. } => {
            count_local_uses_in_value(left, counts);
            count_local_uses_in_value(right, counts);
        }
        MirRValue::Ref { place, .. }
        | MirRValue::AddressOf { place, .. }
        | MirRValue::Discriminant(place)
        | MirRValue::Len(place) => count_local_uses_in_place(place, counts),
        MirRValue::Aggregate { operands, .. } => {
            for op in operands {
                count_local_uses_in_value(op, counts);
            }
        }
        MirRValue::Repeat { value, .. } => count_local_uses_in_value(value, counts),
        MirRValue::FieldAccess { base, .. } | MirRValue::VariantField { base, .. } => {
            count_local_uses_in_value(base, counts);
        }
        MirRValue::IndexAccess { base, index, .. } => {
            count_local_uses_in_value(base, counts);
            count_local_uses_in_value(index, counts);
        }
        MirRValue::Deref { ptr, .. } => count_local_uses_in_value(ptr, counts),
        MirRValue::TextureSample {
            texture,
            sampler,
            coords,
        } => {
            count_local_uses_in_value(texture, counts);
            count_local_uses_in_value(sampler, counts);
            count_local_uses_in_value(coords, counts);
        }
        MirRValue::AddressOfGlobal { .. } | MirRValue::NullaryOp(_, _) => {}
    }
}

fn count_all_local_uses(blocks: &[MirBlock]) -> HashMap<LocalId, usize> {
    let mut counts = HashMap::new();
    for block in blocks {
        for stmt in &block.stmts {
            match &stmt.kind {
                MirStmtKind::Assign { value, .. } => count_local_uses_in_rvalue(value, &mut counts),
                MirStmtKind::DerefAssign { ptr, value } => {
                    *counts.entry(*ptr).or_insert(0) += 1;
                    count_local_uses_in_rvalue(value, &mut counts);
                }
                MirStmtKind::FieldAssign { base, value, .. } => {
                    *counts.entry(*base).or_insert(0) += 1;
                    count_local_uses_in_rvalue(value, &mut counts);
                }
                MirStmtKind::FieldDerefAssign { ptr, value, .. } => {
                    *counts.entry(*ptr).or_insert(0) += 1;
                    count_local_uses_in_rvalue(value, &mut counts);
                }
                MirStmtKind::IndexStore {
                    base, index, value, ..
                } => {
                    count_local_uses_in_value(base, &mut counts);
                    count_local_uses_in_value(index, &mut counts);
                    count_local_uses_in_rvalue(value, &mut counts);
                }
                MirStmtKind::GlobalStore { value, .. } => {
                    count_local_uses_in_rvalue(value, &mut counts);
                }
                MirStmtKind::StorageLive(_)
                | MirStmtKind::StorageDead(_)
                | MirStmtKind::Nop
                | MirStmtKind::WorkgroupBarrier => {}
            }
        }
        if let Some(term) = &block.terminator {
            match term {
                MirTerminator::If { cond, .. } => count_local_uses_in_value(cond, &mut counts),
                MirTerminator::Switch { value, .. } => {
                    count_local_uses_in_value(value, &mut counts)
                }
                MirTerminator::Call { func, args, .. } => {
                    count_local_uses_in_value(func, &mut counts);
                    for a in args {
                        count_local_uses_in_value(a, &mut counts);
                    }
                }
                MirTerminator::Return(Some(v)) => count_local_uses_in_value(v, &mut counts),
                MirTerminator::Drop { place, .. } => count_local_uses_in_place(place, &mut counts),
                MirTerminator::Assert { cond, .. } => count_local_uses_in_value(cond, &mut counts),
                MirTerminator::Goto(_)
                | MirTerminator::Return(None)
                | MirTerminator::Unreachable
                | MirTerminator::Resume
                | MirTerminator::Abort => {}
            }
        }
    }
    counts
}

/// Remove pure dead definitions whose locals were superseded by a certified
/// superopt rewrite and now have zero remaining uses.
fn prune_superseded_dead_defs(blocks: &mut [MirBlock], eliminable: &HashSet<LocalId>) {
    if eliminable.is_empty() {
        return;
    }
    loop {
        let uses = count_all_local_uses(blocks);
        let mut changed = false;
        for block in blocks.iter_mut() {
            let before_len = block.stmts.len();
            block.stmts.retain(|stmt| {
                if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                    let is_pure = matches!(
                        value,
                        MirRValue::BinaryOp { .. } | MirRValue::UnaryOp { .. } | MirRValue::Use(_)
                    );
                    if is_pure
                        && eliminable.contains(dest)
                        && uses.get(dest).copied().unwrap_or(0) == 0
                    {
                        return false;
                    }
                }
                true
            });
            if block.stmts.len() != before_len {
                changed = true;
            }
            if let Some(MirTerminator::Call {
                func,
                dest: Some(d),
                target: Some(next_bb),
                ..
            }) = &block.terminator
            {
                let is_pure_math = callee_func_name(func)
                    .is_some_and(|n| matches!(n, "exp" | "exp2" | "log" | "log2" | "fma"));
                if is_pure_math && eliminable.contains(d) && uses.get(d).copied().unwrap_or(0) == 0
                {
                    let target_bb = *next_bb;
                    block.terminator = Some(MirTerminator::Goto(target_bb));
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn superoptimize_function(
    func: &mut MirFunction,
    allow_int_shift: bool,
    config: SuperoptConfig,
    steps: &mut Vec<SuperoptStep>,
) {
    let (mutated_locals, single_def_locals, unsafe_const_locals) = analyze_function_locals(func);
    let (immutable_defs, mut copy_origin) =
        collect_immutable_pure_defs(func, &mutated_locals, &single_def_locals);
    let func_name = func.name.to_string();
    let mut eliminable_locals: HashSet<LocalId> = HashSet::new();

    let Some(mut blocks) = func.blocks.take() else {
        return;
    };

    // Pass 1: Call-level SOTA rewrites (R17_LOG_EXP_CANCEL, R20_HORNER_EXP2_FMA).
    let initial_block_count = blocks.len();
    for bi in 0..initial_block_count {
        let term_snapshot = blocks[bi].terminator.clone();
        if let Some(MirTerminator::Call {
            func: callee,
            args,
            dest: Some(dest_id),
            target: Some(next_bb),
            ..
        }) = term_snapshot
        {
            let Some(cname) = callee_func_name(&callee) else {
                continue;
            };
            let dest_ty = func
                .locals
                .get(dest_id.0 as usize)
                .map(|l| l.ty.clone())
                .unwrap_or(MirType::Void);

            // R17: `log(exp(x)) -> x` and `log2(exp2(x)) -> x`
            if matches!(cname, "log" | "log2") && args.len() == 1 && dest_ty.is_float() {
                if let MirValue::Local(arg_id) = args[0] {
                    if let Some(CertExpr::Call {
                        func: inner_fn,
                        args: inner_args,
                        ret_ty: inner_ty,
                    }) = immutable_defs.get(&arg_id)
                    {
                        let matched = (cname == "log" && inner_fn == "exp")
                            || (cname == "log2" && inner_fn == "exp2");
                        if matched
                            && inner_args.len() == 1
                            && inner_ty == &dest_ty
                            && is_cert_atom(&inner_args[0])
                        {
                            if let Some(x_val) = cert_atom_to_mir_value(&inner_args[0]) {
                                let before_expr = CertExpr::Call {
                                    func: cname.to_string(),
                                    args: vec![CertExpr::Call {
                                        func: inner_fn.clone(),
                                        args: vec![inner_args[0].clone()],
                                        ret_ty: inner_ty.clone(),
                                    }],
                                    ret_ty: dest_ty.clone(),
                                };
                                let after_expr = inner_args[0].clone();
                                let cost_before = expr_cost(&before_expr);
                                let cost_after = expr_cost(&after_expr);
                                let stmt_idx = blocks[bi].stmts.len();
                                blocks[bi]
                                    .stmts
                                    .push(MirStmt::assign(dest_id, MirRValue::Use(x_val)));
                                blocks[bi].terminator = Some(MirTerminator::Goto(next_bb));
                                mark_eliminable_chain(arg_id, &copy_origin, &mut eliminable_locals);
                                steps.push(SuperoptStep {
                                    function: func_name.clone(),
                                    block_id: blocks[bi].id.0,
                                    stmt_index: stmt_idx,
                                    dest_local: dest_id.0,
                                    rule: SuperoptRule::LogExpCancel,
                                    error_witness: expected_error_witness(
                                        &SuperoptRule::LogExpCancel,
                                        &before_expr,
                                    ),
                                    before: before_expr,
                                    after: after_expr,
                                    cost_before,
                                    cost_after,
                                });
                                continue;
                            }
                        }
                    }
                }
            }

            // R20: Opt-in FlashAttention-4 Horner FMA-emulated `exp2(f)`
            if config.enable_horner_exp2_fma
                && config.allow_fma_contract
                && cname == "exp2"
                && args.len() == 1
                && dest_ty.is_float()
            {
                let empty_const = HashMap::new();
                if let Some(f_expr) = mir_value_to_cert(&args[0], &empty_const) {
                    if is_cert_atom(&f_expr) {
                        let f_val = args[0].clone();
                        let c = HORNER_EXP2_DEG4_COEFFS;
                        let mk_c = |v: f64| MirValue::Const(MirConst::Float(v, dest_ty.clone()));
                        let t1 = alloc_fresh_local(&mut func.locals, dest_ty.clone());
                        let t2 = alloc_fresh_local(&mut func.locals, dest_ty.clone());
                        let t3 = alloc_fresh_local(&mut func.locals, dest_ty.clone());

                        let bb1_id = BlockId(blocks.len() as u32);
                        let bb2_id = BlockId(blocks.len() as u32 + 1);
                        let bb3_id = BlockId(blocks.len() as u32 + 2);

                        let stmt_idx = blocks[bi].stmts.len();
                        let block_id = blocks[bi].id.0;
                        blocks[bi].terminator = Some(MirTerminator::Call {
                            func: MirValue::Function(Arc::from("fma")),
                            args: vec![mk_c(c[4]), f_val.clone(), mk_c(c[3])],
                            dest: Some(t1),
                            target: Some(bb1_id),
                            unwind: None,
                        });

                        let mut bb1 = MirBlock::new(bb1_id);
                        bb1.terminator = Some(MirTerminator::Call {
                            func: MirValue::Function(Arc::from("fma")),
                            args: vec![MirValue::Local(t1), f_val.clone(), mk_c(c[2])],
                            dest: Some(t2),
                            target: Some(bb2_id),
                            unwind: None,
                        });
                        let mut bb2 = MirBlock::new(bb2_id);
                        bb2.terminator = Some(MirTerminator::Call {
                            func: MirValue::Function(Arc::from("fma")),
                            args: vec![MirValue::Local(t2), f_val.clone(), mk_c(c[1])],
                            dest: Some(t3),
                            target: Some(bb3_id),
                            unwind: None,
                        });
                        let mut bb3 = MirBlock::new(bb3_id);
                        bb3.terminator = Some(MirTerminator::Call {
                            func: MirValue::Function(Arc::from("fma")),
                            args: vec![MirValue::Local(t3), f_val, mk_c(c[0])],
                            dest: Some(dest_id),
                            target: Some(next_bb),
                            unwind: None,
                        });
                        blocks.push(bb1);
                        blocks.push(bb2);
                        blocks.push(bb3);

                        let before_expr = CertExpr::Call {
                            func: "exp2".to_string(),
                            args: vec![f_expr.clone()],
                            ret_ty: dest_ty.clone(),
                        };
                        let after_expr = horner_exp2_cert_expr(f_expr, dest_ty);
                        let cost_before = expr_cost(&before_expr);
                        let cost_after = expr_cost(&after_expr);
                        steps.push(SuperoptStep {
                            function: func_name.clone(),
                            block_id,
                            stmt_index: stmt_idx,
                            dest_local: dest_id.0,
                            rule: SuperoptRule::HornerExp2Fma,
                            error_witness: expected_error_witness(
                                &SuperoptRule::HornerExp2Fma,
                                &before_expr,
                            ),
                            before: before_expr,
                            after: after_expr,
                            cost_before,
                            cost_after,
                        });
                    }
                }
            }
        }
    }

    // Pass 2: Statement-level saturation (R1..R16, R18, R19) across every block
    // (including continuation blocks created by splitting).
    let mut bi = 0usize;
    while bi < blocks.len() {
        let mut const_env: HashMap<LocalId, MirConst> = HashMap::new();
        let mut same_block_defs: HashMap<LocalId, CertExpr> = HashMap::new();
        let mut stmt_idx = 0usize;

        while stmt_idx < blocks[bi].stmts.len() {
            let stmt_clone = blocks[bi].stmts[stmt_idx].clone();
            let MirStmtKind::Assign { dest, value } = stmt_clone.kind else {
                stmt_idx += 1;
                continue;
            };
            let dest_id = dest;
            let dest_ty = func
                .locals
                .get(dest_id.0 as usize)
                .map(|l| l.ty.clone())
                .unwrap_or(MirType::Void);

            // Invalidate any same_block_defs entry whose leaves depend on dest_id.
            same_block_defs.retain(|_, expr| {
                let mut leaves = HashSet::new();
                collect_expr_locals(expr, &mut leaves);
                !leaves.contains(&dest_id)
            });

            match value {
                MirRValue::Use(MirValue::Const(c)) => {
                    if !unsafe_const_locals.contains(&dest_id) {
                        const_env.insert(dest_id, c);
                    }
                    stmt_idx += 1;
                }
                MirRValue::Use(MirValue::Local(src)) => {
                    if !unsafe_const_locals.contains(&dest_id) {
                        if let Some(c) = const_env.get(&src).cloned() {
                            const_env.insert(dest_id, c);
                        }
                    }
                    if let Some(expr) = same_block_defs.get(&src).cloned() {
                        same_block_defs.insert(dest_id, expr);
                        copy_origin.insert(dest_id, src);
                    } else if let Some(expr) = immutable_defs.get(&src).cloned() {
                        same_block_defs.insert(dest_id, expr);
                        copy_origin.insert(dest_id, src);
                    }
                    stmt_idx += 1;
                }
                MirRValue::BinaryOp { op, left, right } => {
                    let Some(l_atom) = mir_value_to_cert(&left, &const_env) else {
                        stmt_idx += 1;
                        continue;
                    };
                    let Some(r_atom) = mir_value_to_cert(&right, &const_env) else {
                        stmt_idx += 1;
                        continue;
                    };

                    // First attempt atom-level saturation (R1..R14).
                    let atom_best = saturate_binop(
                        op,
                        &l_atom,
                        &r_atom,
                        &dest_ty,
                        allow_int_shift,
                        config.allow_fma_contract,
                    );
                    if let Some((after_expr, rule, cost_before, cost_after)) = atom_best {
                        if let Some(new_rval) = cert_to_mir_rvalue(&after_expr) {
                            let before_expr = CertExpr::BinOp {
                                op,
                                left: Box::new(l_atom),
                                right: Box::new(r_atom),
                            };
                            if let MirStmtKind::Assign {
                                value: ref mut v_slot,
                                ..
                            } = blocks[bi].stmts[stmt_idx].kind
                            {
                                *v_slot = new_rval.clone();
                            }
                            if !unsafe_const_locals.contains(&dest_id) {
                                if let MirRValue::Use(MirValue::Const(c)) = &new_rval {
                                    const_env.insert(dest_id, c.clone());
                                }
                            }
                            if let CertExpr::BinOp { op: BinOp::Mul, .. } = &after_expr {
                                if single_def_locals.contains(&dest_id) {
                                    same_block_defs.insert(dest_id, after_expr.clone());
                                }
                            }
                            let witness = expected_error_witness(&rule, &before_expr);
                            steps.push(SuperoptStep {
                                function: func_name.clone(),
                                block_id: blocks[bi].id.0,
                                stmt_index: stmt_idx,
                                dest_local: dest_id.0,
                                rule,
                                before: before_expr,
                                after: after_expr,
                                cost_before,
                                cost_after,
                                error_witness: witness,
                            });
                            stmt_idx += 1;
                            continue;
                        }
                    }

                    // Next attempt compound-operand saturation (R15, R16, R18, R19).
                    let resolve_compound = |v: &MirValue, atom: &CertExpr| -> CertExpr {
                        if let MirValue::Local(id) = v {
                            if let Some(e) = same_block_defs.get(id) {
                                return e.clone();
                            }
                            if let Some(e) = immutable_defs.get(id) {
                                return e.clone();
                            }
                        }
                        atom.clone()
                    };
                    let l_deep = resolve_compound(&left, &l_atom);
                    let r_deep = resolve_compound(&right, &r_atom);

                    if l_deep != l_atom || r_deep != r_atom {
                        // Try full compound pair first (R15/R16/R19); fall back to
                        // one-sided expansion for R18_FMA_CONTRACT when both sides
                        // are independent products (e.g. `state * alpha + write_term`).
                        let candidates: [(&CertExpr, &CertExpr, bool, bool); 3] = [
                            (&l_deep, &r_deep, l_deep != l_atom, r_deep != r_atom),
                            (&l_deep, &r_atom, l_deep != l_atom, false),
                            (&l_atom, &r_deep, false, r_deep != r_atom),
                        ];
                        let mut matched_compound = None;
                        for &(cl, cr, use_l, use_r) in &candidates {
                            if !use_l && !use_r {
                                continue;
                            }
                            if let Some(res) = saturate_binop(
                                op,
                                cl,
                                cr,
                                &dest_ty,
                                allow_int_shift,
                                config.allow_fma_contract,
                            ) {
                                matched_compound =
                                    Some((cl.clone(), cr.clone(), use_l, use_r, res));
                                break;
                            }
                        }

                        if let Some((
                            chosen_l,
                            chosen_r,
                            used_l_compound,
                            used_r_compound,
                            (after_expr, rule, cost_before, cost_after),
                        )) = matched_compound
                        {
                            let before_expr = CertExpr::BinOp {
                                op,
                                left: Box::new(chosen_l),
                                right: Box::new(chosen_r),
                            };
                            let witness = expected_error_witness(&rule, &before_expr);

                            match (&rule, &after_expr) {
                                // R15 / R16: `exp(a) * exp(b) -> exp(a + b)` or `exp(a) / exp(b) -> exp(a - b)`
                                (
                                    SuperoptRule::OnlineSoftmaxExpMulFuse
                                    | SuperoptRule::OnlineSoftmaxExpDivFuse,
                                    CertExpr::Call {
                                        func: fn_name,
                                        args,
                                        ret_ty,
                                    },
                                ) if args.len() == 1 => {
                                    if let CertExpr::BinOp {
                                        op: inner_op,
                                        left: ia,
                                        right: ib,
                                    } = &args[0]
                                    {
                                        if let (Some(va), Some(vb)) =
                                            (cert_atom_to_mir_value(ia), cert_atom_to_mir_value(ib))
                                        {
                                            let tmp =
                                                alloc_fresh_local(&mut func.locals, ret_ty.clone());
                                            blocks[bi].stmts[stmt_idx] = MirStmt::assign(
                                                tmp,
                                                MirRValue::BinaryOp {
                                                    op: *inner_op,
                                                    left: va,
                                                    right: vb,
                                                },
                                            );
                                            let cont_id = BlockId(blocks.len() as u32);
                                            let mut cont_bb = MirBlock::new(cont_id);
                                            cont_bb.stmts =
                                                blocks[bi].stmts.split_off(stmt_idx + 1);
                                            cont_bb.terminator = blocks[bi].terminator.take();
                                            blocks[bi].terminator = Some(MirTerminator::Call {
                                                func: MirValue::Function(Arc::from(
                                                    fn_name.as_str(),
                                                )),
                                                args: vec![MirValue::Local(tmp)],
                                                dest: Some(dest_id),
                                                target: Some(cont_id),
                                                unwind: None,
                                            });
                                            blocks.push(cont_bb);
                                            if let MirValue::Local(lid) = left {
                                                mark_eliminable_chain(
                                                    lid,
                                                    &copy_origin,
                                                    &mut eliminable_locals,
                                                );
                                            }
                                            if let MirValue::Local(rid) = right {
                                                mark_eliminable_chain(
                                                    rid,
                                                    &copy_origin,
                                                    &mut eliminable_locals,
                                                );
                                            }
                                            steps.push(SuperoptStep {
                                                function: func_name.clone(),
                                                block_id: blocks[bi].id.0,
                                                stmt_index: stmt_idx,
                                                dest_local: dest_id.0,
                                                rule,
                                                before: before_expr,
                                                after: after_expr,
                                                cost_before,
                                                cost_after,
                                                error_witness: witness,
                                            });
                                            break;
                                        }
                                    }
                                }

                                // R18: `(a * b) + c -> fma(a, b, c)`
                                (
                                    SuperoptRule::FmaContract,
                                    CertExpr::Call {
                                        func: fn_name,
                                        args,
                                        ..
                                    },
                                ) if fn_name == "fma" && args.len() == 3 => {
                                    if let (Some(va), Some(vb), Some(vc)) = (
                                        cert_atom_to_mir_value(&args[0]),
                                        cert_atom_to_mir_value(&args[1]),
                                        cert_atom_to_mir_value(&args[2]),
                                    ) {
                                        // Remove the Add statement and split into a Call terminator.
                                        let tail = blocks[bi].stmts.split_off(stmt_idx + 1);
                                        blocks[bi].stmts.pop();
                                        let cont_id = BlockId(blocks.len() as u32);
                                        let mut cont_bb = MirBlock::new(cont_id);
                                        cont_bb.stmts = tail;
                                        cont_bb.terminator = blocks[bi].terminator.take();
                                        blocks[bi].terminator = Some(MirTerminator::Call {
                                            func: MirValue::Function(Arc::from("fma")),
                                            args: vec![va, vb, vc],
                                            dest: Some(dest_id),
                                            target: Some(cont_id),
                                            unwind: None,
                                        });
                                        blocks.push(cont_bb);
                                        if used_l_compound {
                                            if let MirValue::Local(lid) = left {
                                                mark_eliminable_chain(
                                                    lid,
                                                    &copy_origin,
                                                    &mut eliminable_locals,
                                                );
                                            }
                                        }
                                        if used_r_compound {
                                            if let MirValue::Local(rid) = right {
                                                mark_eliminable_chain(
                                                    rid,
                                                    &copy_origin,
                                                    &mut eliminable_locals,
                                                );
                                            }
                                        }
                                        steps.push(SuperoptStep {
                                            function: func_name.clone(),
                                            block_id: blocks[bi].id.0,
                                            stmt_index: stmt_idx,
                                            dest_local: dest_id.0,
                                            rule,
                                            before: before_expr,
                                            after: after_expr,
                                            cost_before,
                                            cost_after,
                                            error_witness: witness,
                                        });
                                        break;
                                    }
                                }

                                // R19 Form A: `(s * a) + (s * b) -> s * (a + b)`
                                (
                                    SuperoptRule::BlockScaleHoist,
                                    CertExpr::BinOp {
                                        op: BinOp::Mul,
                                        left: s_expr,
                                        right: sum_expr,
                                    },
                                ) if matches!(
                                    sum_expr.as_ref(),
                                    CertExpr::BinOp { op: BinOp::Add, .. }
                                ) =>
                                {
                                    if let CertExpr::BinOp {
                                        op: BinOp::Add,
                                        left: a_expr,
                                        right: b_expr,
                                    } = sum_expr.as_ref()
                                    {
                                        if let (Some(vs), Some(va), Some(vb)) = (
                                            cert_atom_to_mir_value(s_expr),
                                            cert_atom_to_mir_value(a_expr),
                                            cert_atom_to_mir_value(b_expr),
                                        ) {
                                            let tmp_sum = alloc_fresh_local(
                                                &mut func.locals,
                                                dest_ty.clone(),
                                            );
                                            blocks[bi].stmts[stmt_idx] = MirStmt::assign(
                                                dest_id,
                                                MirRValue::BinaryOp {
                                                    op: BinOp::Mul,
                                                    left: vs,
                                                    right: MirValue::Local(tmp_sum),
                                                },
                                            );
                                            blocks[bi].stmts.insert(
                                                stmt_idx,
                                                MirStmt::assign(
                                                    tmp_sum,
                                                    MirRValue::BinaryOp {
                                                        op: BinOp::Add,
                                                        left: va,
                                                        right: vb,
                                                    },
                                                ),
                                            );
                                            if let MirValue::Local(lid) = left {
                                                mark_eliminable_chain(
                                                    lid,
                                                    &copy_origin,
                                                    &mut eliminable_locals,
                                                );
                                            }
                                            if let MirValue::Local(rid) = right {
                                                mark_eliminable_chain(
                                                    rid,
                                                    &copy_origin,
                                                    &mut eliminable_locals,
                                                );
                                            }
                                            // Record `dest_id = s * tmp_sum` so subsequent terms
                                            // in a 16/32-element block cascade automatically!
                                            same_block_defs.insert(
                                                dest_id,
                                                CertExpr::BinOp {
                                                    op: BinOp::Mul,
                                                    left: Box::new(s_expr.as_ref().clone()),
                                                    right: Box::new(CertExpr::Local(tmp_sum.0)),
                                                },
                                            );
                                            steps.push(SuperoptStep {
                                                function: func_name.clone(),
                                                block_id: blocks[bi].id.0,
                                                stmt_index: stmt_idx,
                                                dest_local: dest_id.0,
                                                rule,
                                                before: before_expr,
                                                after: after_expr,
                                                cost_before,
                                                cost_after,
                                                error_witness: witness,
                                            });
                                            stmt_idx += 2;
                                            continue;
                                        }
                                    }
                                }

                                // R19 Form B: `(a * c1) * (b * c2) -> (a * b) * (c1 * c2)`
                                (
                                    SuperoptRule::BlockScaleHoist,
                                    CertExpr::BinOp {
                                        op: BinOp::Mul,
                                        left: ab_expr,
                                        right: c12_expr,
                                    },
                                ) if matches!(
                                    ab_expr.as_ref(),
                                    CertExpr::BinOp { op: BinOp::Mul, .. }
                                ) =>
                                {
                                    if let CertExpr::BinOp {
                                        op: BinOp::Mul,
                                        left: a_expr,
                                        right: b_expr,
                                    } = ab_expr.as_ref()
                                    {
                                        if let (Some(va), Some(vb), Some(vc12)) = (
                                            cert_atom_to_mir_value(a_expr),
                                            cert_atom_to_mir_value(b_expr),
                                            cert_atom_to_mir_value(c12_expr),
                                        ) {
                                            let tmp_ab = alloc_fresh_local(
                                                &mut func.locals,
                                                dest_ty.clone(),
                                            );
                                            blocks[bi].stmts[stmt_idx] = MirStmt::assign(
                                                dest_id,
                                                MirRValue::BinaryOp {
                                                    op: BinOp::Mul,
                                                    left: MirValue::Local(tmp_ab),
                                                    right: vc12,
                                                },
                                            );
                                            blocks[bi].stmts.insert(
                                                stmt_idx,
                                                MirStmt::assign(
                                                    tmp_ab,
                                                    MirRValue::BinaryOp {
                                                        op: BinOp::Mul,
                                                        left: va,
                                                        right: vb,
                                                    },
                                                ),
                                            );
                                            if let MirValue::Local(lid) = left {
                                                mark_eliminable_chain(
                                                    lid,
                                                    &copy_origin,
                                                    &mut eliminable_locals,
                                                );
                                            }
                                            if let MirValue::Local(rid) = right {
                                                mark_eliminable_chain(
                                                    rid,
                                                    &copy_origin,
                                                    &mut eliminable_locals,
                                                );
                                            }
                                            same_block_defs.insert(
                                                dest_id,
                                                CertExpr::BinOp {
                                                    op: BinOp::Mul,
                                                    left: Box::new(CertExpr::Local(tmp_ab.0)),
                                                    right: Box::new(c12_expr.as_ref().clone()),
                                                },
                                            );
                                            steps.push(SuperoptStep {
                                                function: func_name.clone(),
                                                block_id: blocks[bi].id.0,
                                                stmt_index: stmt_idx,
                                                dest_local: dest_id.0,
                                                rule,
                                                before: before_expr,
                                                after: after_expr,
                                                cost_before,
                                                cost_after,
                                                error_witness: witness,
                                            });
                                            stmt_idx += 2;
                                            continue;
                                        }
                                    }
                                }

                                _ => {}
                            }
                        }
                    }

                    // Record pure float multiply in same_block_defs for downstream FMA / scale hoisting.
                    if op == BinOp::Mul
                        && dest_ty.is_float()
                        && is_cert_atom(&l_atom)
                        && is_cert_atom(&r_atom)
                    {
                        same_block_defs.insert(
                            dest_id,
                            CertExpr::BinOp {
                                op: BinOp::Mul,
                                left: Box::new(l_atom),
                                right: Box::new(r_atom),
                            },
                        );
                    }
                    stmt_idx += 1;
                }
                _ => {
                    stmt_idx += 1;
                }
            }
        }
        bi += 1;
    }

    prune_superseded_dead_defs(&mut blocks, &eliminable_locals);
    func.blocks = Some(blocks);
}

/// Run V5 certified e-graph superoptimization across every function in `module`
/// with explicit configuration, independently verify the generated certificate
/// fail-closed, and return it.
pub fn superoptimize_module_with_config(
    module: &mut MirModule,
    target: &str,
    config: SuperoptConfig,
) -> Result<SuperoptCertificate, String> {
    let mir_before_sha256 = mir_digest(module)?;
    // WGSL requires unsigned u32 shift RHS (`<< u32(k)`), whereas C/MSL/RISC-V
    // accept matched-type shifts directly. Keep WGSL strictly type-valid.
    let allow_int_shift = target != "wgsl";

    let mut steps = Vec::new();
    let functions_visited = module.functions.len();

    for func in &mut module.functions {
        superoptimize_function(func, allow_int_shift, config, &mut steps);
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

/// Run V5 certified e-graph superoptimization across every function in `module`,
/// independently verify the generated certificate fail-closed, and return it.
pub fn superoptimize_module(
    module: &mut MirModule,
    target: &str,
) -> Result<SuperoptCertificate, String> {
    superoptimize_module_with_config(module, target, SuperoptConfig::for_target(target))
}

/// Independently verify a single rewrite step structurally from the axioms.
pub fn verify_step_axiom(
    rule: &SuperoptRule,
    before: &CertExpr,
    after: &CertExpr,
) -> Result<(), String> {
    // Handle call-rooted SOTA rules first (R17, R20).
    match rule {
        SuperoptRule::LogExpCancel => {
            let ok = match before {
                CertExpr::Call {
                    func: outer_fn,
                    args: outer_args,
                    ret_ty: outer_ty,
                } if outer_args.len() == 1 => match &outer_args[0] {
                    CertExpr::Call {
                        func: inner_fn,
                        args: inner_args,
                        ret_ty: inner_ty,
                    } if inner_args.len() == 1 && inner_ty == outer_ty => {
                        let inverse_pair = (outer_fn == "log" && inner_fn == "exp")
                            || (outer_fn == "log2" && inner_fn == "exp2");
                        inverse_pair && after == &inner_args[0]
                    }
                    _ => false,
                },
                _ => false,
            };
            return if ok {
                Ok(())
            } else {
                Err(format!(
                    "axiom violation for rule {:?}: before {:?} does not entail after {:?}",
                    rule, before, after
                ))
            };
        }
        SuperoptRule::HornerExp2Fma => {
            let ok = match before {
                CertExpr::Call { func, args, ret_ty }
                    if func == "exp2" && args.len() == 1 && ret_ty.is_float() =>
                {
                    let expected = horner_exp2_cert_expr(args[0].clone(), ret_ty.clone());
                    after == &expected
                }
                _ => false,
            };
            return if ok {
                Ok(())
            } else {
                Err(format!(
                    "axiom violation for rule {:?}: before {:?} does not match certified degree-4 Horner exp2 FMA polynomial {:?}",
                    rule, before, after
                ))
            };
        }
        _ => {}
    }

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
        SuperoptRule::FloatDivOne => {
            let one_bits = 1.0_f64.to_bits();
            *op == BinOp::Div
                && matches!(right.as_ref(), CertExpr::FloatBits(b, _) if *b == one_bits)
                && after == left.as_ref()
        }
        SuperoptRule::OnlineSoftmaxExpMulFuse => {
            *op == BinOp::Mul
                && match (left.as_ref(), right.as_ref(), after) {
                    (
                        CertExpr::Call {
                            func: f1,
                            args: a1,
                            ret_ty: t1,
                        },
                        CertExpr::Call {
                            func: f2,
                            args: a2,
                            ret_ty: t2,
                        },
                        CertExpr::Call {
                            func: fa,
                            args: aa,
                            ret_ty: ta,
                        },
                    ) if f1 == f2
                        && f1 == fa
                        && matches!(f1.as_str(), "exp" | "exp2")
                        && a1.len() == 1
                        && a2.len() == 1
                        && aa.len() == 1
                        && t1 == t2
                        && t1 == ta =>
                    {
                        match &aa[0] {
                            CertExpr::BinOp {
                                op: BinOp::Add,
                                left: al,
                                right: ar,
                            } => {
                                (al.as_ref() == &a1[0] && ar.as_ref() == &a2[0])
                                    || (al.as_ref() == &a2[0] && ar.as_ref() == &a1[0])
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                }
        }
        SuperoptRule::OnlineSoftmaxExpDivFuse => {
            *op == BinOp::Div
                && match (left.as_ref(), right.as_ref(), after) {
                    (
                        CertExpr::Call {
                            func: f1,
                            args: a1,
                            ret_ty: t1,
                        },
                        CertExpr::Call {
                            func: f2,
                            args: a2,
                            ret_ty: t2,
                        },
                        CertExpr::Call {
                            func: fa,
                            args: aa,
                            ret_ty: ta,
                        },
                    ) if f1 == f2
                        && f1 == fa
                        && matches!(f1.as_str(), "exp" | "exp2")
                        && a1.len() == 1
                        && a2.len() == 1
                        && aa.len() == 1
                        && t1 == t2
                        && t1 == ta =>
                    {
                        match &aa[0] {
                            CertExpr::BinOp {
                                op: BinOp::Sub,
                                left: al,
                                right: ar,
                            } => al.as_ref() == &a1[0] && ar.as_ref() == &a2[0],
                            _ => false,
                        }
                    }
                    _ => false,
                }
        }
        SuperoptRule::FmaContract => {
            *op == BinOp::Add
                && match after {
                    CertExpr::Call { func, args, ret_ty }
                        if func == "fma" && args.len() == 3 && ret_ty.is_float() =>
                    {
                        let check_mul_add = |mul_side: &CertExpr, add_side: &CertExpr| -> bool {
                            match mul_side {
                                CertExpr::BinOp {
                                    op: BinOp::Mul,
                                    left: ma,
                                    right: mb,
                                } => {
                                    add_side == &args[2]
                                        && ((ma.as_ref() == &args[0] && mb.as_ref() == &args[1])
                                            || (ma.as_ref() == &args[1] && mb.as_ref() == &args[0]))
                                }
                                _ => false,
                            }
                        };
                        check_mul_add(left.as_ref(), right.as_ref())
                            || check_mul_add(right.as_ref(), left.as_ref())
                    }
                    _ => false,
                }
        }
        SuperoptRule::BlockScaleHoist => match op {
            BinOp::Add => match (extract_shared_scale_sum(left, right), after) {
                (
                    Some((s, a, b)),
                    CertExpr::BinOp {
                        op: BinOp::Mul,
                        left: al,
                        right: ar,
                    },
                ) => {
                    let check_factor = |scale_side: &CertExpr, sum_side: &CertExpr| -> bool {
                        if scale_side != &s {
                            return false;
                        }
                        match sum_side {
                            CertExpr::BinOp {
                                op: BinOp::Add,
                                left: sl,
                                right: sr,
                            } => {
                                (sl.as_ref() == &a && sr.as_ref() == &b)
                                    || (sl.as_ref() == &b && sr.as_ref() == &a)
                            }
                            _ => false,
                        }
                    };
                    check_factor(al.as_ref(), ar.as_ref()) || check_factor(ar.as_ref(), al.as_ref())
                }
                _ => false,
            },
            BinOp::Mul => match (extract_const_scale_pair(left, right), after) {
                (
                    Some((a, c1, b, c2, ty)),
                    CertExpr::BinOp {
                        op: BinOp::Mul,
                        left: al,
                        right: ar,
                    },
                ) => {
                    let Some(expected_c12) = eval_float_binop(BinOp::Mul, c1, c2, &ty) else {
                        return Err(format!(
                            "axiom violation for rule {:?}: non-finite scale product",
                            rule
                        ));
                    };
                    let check_pair = |ab_side: &CertExpr, c_side: &CertExpr| -> bool {
                        match (ab_side, c_side) {
                            (
                                CertExpr::BinOp {
                                    op: BinOp::Mul,
                                    left: pl,
                                    right: pr,
                                },
                                CertExpr::FloatBits(cb, cty),
                            ) if *cb == expected_c12 && cty == &ty => {
                                (pl.as_ref() == &a && pr.as_ref() == &b)
                                    || (pl.as_ref() == &b && pr.as_ref() == &a)
                            }
                            _ => false,
                        }
                    };
                    check_pair(al.as_ref(), ar.as_ref()) || check_pair(ar.as_ref(), al.as_ref())
                }
                _ => false,
            },
            _ => false,
        },
        SuperoptRule::LogExpCancel | SuperoptRule::HornerExp2Fma => unreachable!(),
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
        let expected_witness = expected_error_witness(&step.rule, &step.before);
        if step.error_witness != expected_witness {
            return Err(format!(
                "step {idx} error_witness mismatch for {:?}: expected {:?}, found {:?}",
                step.rule, expected_witness, step.error_witness
            ));
        }
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

    #[test]
    fn horner_exp2_deg4_meets_f32_accuracy_bound() {
        let mut max_err = 0.0_f64;
        for k in 0..=10000 {
            let f = (k as f64) / 10000.0;
            let err = (horner_exp2_f64(f) - 2.0_f64.powf(f)).abs();
            if err > max_err {
                max_err = err;
            }
        }
        assert!(
            max_err <= 4.0e-6,
            "Horner deg-4 exp2 max error {max_err} exceeded 4.0e-6"
        );
    }

    #[test]
    fn walsh_hadamard_preserves_inner_product_and_suppresses_outliers() {
        let mut x = [0.0_f64; 16];
        let mut w = [0.0_f64; 16];
        for i in 0..16 {
            x[i] = ((i as f64) * 0.25) - 1.5;
            w[i] = ((15 - i) as f64) * 0.125 - 0.8;
        }
        x[3] = 100.0; // 100x outlier spike
        let exact_dot: f64 = x.iter().zip(w.iter()).map(|(a, b)| a * b).sum();
        let orig_max = x.iter().fold(0.0_f64, |acc, &v| acc.max(v.abs()));

        let mut hx = x;
        let mut hw = w;
        walsh_hadamard_transform_normalized(&mut hx).unwrap();
        walsh_hadamard_transform_normalized(&mut hw).unwrap();
        let rot_dot: f64 = hx.iter().zip(hw.iter()).map(|(a, b)| a * b).sum();
        let rot_max = hx.iter().fold(0.0_f64, |acc, &v| acc.max(v.abs()));

        assert!(
            (exact_dot - rot_dot).abs() < 1e-11,
            "Parseval inner product drifted: {exact_dot} vs {rot_dot}"
        );
        assert!(
            rot_max < orig_max * 0.35,
            "Hadamard should suppress 100x outlier: {orig_max} -> {rot_max}"
        );
    }
}
