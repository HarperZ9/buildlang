// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Inputs for the concrete side: every input of a small domain, or boundary
//! values and a fixed pseudo-random sample of a large one.

use super::eval::Val;
use super::infer::var_name;
use super::ty::Ty;
use crate::ast::{BinOp, Expr, ExprKind, Literal, UnaryOp};

/// One parameter's values: its type, narrowed by literal bounds in `requires`.
#[derive(Clone, Copy, Debug)]
pub struct Dom {
    pub ty: Ty,
    pub lo: i128,
    pub hi: i128,
}

impl Dom {
    pub fn of(ty: Ty) -> Dom {
        let (lo, hi) = ty.range().unwrap_or((0, 1));
        Dom { ty, lo, hi }
    }
    fn size(&self) -> u128 {
        (self.hi - self.lo + 1).max(0) as u128
    }
}

fn lit(e: &Expr) -> Option<i128> {
    match &e.kind {
        ExprKind::Literal(Literal::Int { value, .. }) => i128::try_from(*value).ok(),
        ExprKind::Unary {
            op: UnaryOp::Neg,
            expr,
        } => lit(expr).map(|v| -v),
        ExprKind::Paren(inner) => lit(inner),
        _ => None,
    }
}

/// Narrow `doms` by the literal bounds of a `requires` body: the conjuncts
/// `x < K`, `x <= K`, `x > K`, `x >= K` (either side). Every input outside
/// the narrowed domain makes that conjunct, and so the clause, false; the
/// clause is still evaluated on every input that is run.
pub fn narrow(doms: &mut [Dom], names: &[String], req: &Expr) {
    match &req.kind {
        ExprKind::Paren(inner) => narrow(doms, names, inner),
        ExprKind::Binary {
            op: BinOp::And,
            left,
            right,
        } => {
            narrow(doms, names, left);
            narrow(doms, names, right);
        }
        ExprKind::Binary { op, left, right } => {
            let (name, k, op) = match (var_name(left), lit(right), var_name(right), lit(left)) {
                (Some(n), Some(k), _, _) => (n, k, *op),
                (_, _, Some(n), Some(k)) => (
                    n,
                    k,
                    match op {
                        BinOp::Lt => BinOp::Gt,
                        BinOp::Le => BinOp::Ge,
                        BinOp::Gt => BinOp::Lt,
                        BinOp::Ge => BinOp::Le,
                        other => *other,
                    },
                ),
                _ => return,
            };
            let Some(i) = names.iter().position(|x| *x == name) else {
                return;
            };
            let d = &mut doms[i];
            match op {
                BinOp::Lt => d.hi = d.hi.min(k - 1),
                BinOp::Le => d.hi = d.hi.min(k),
                BinOp::Gt => d.lo = d.lo.max(k + 1),
                BinOp::Ge => d.lo = d.lo.max(k),
                _ => {}
            }
        }
        _ => {}
    }
}

/// Domains up to this many points are enumerated in full.
pub const EXHAUSTIVE_LIMIT: u128 = 1 << 20;
const BOUNDARY_PRODUCT_LIMIT: usize = 60_000;
const RANDOM_SAMPLES: usize = 4_000;

/// How the inputs cover the domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cover {
    /// Every point of the domain.
    Exhaustive,
    /// Boundary values and a sample.
    Sampled,
}

fn domain_size(doms: &[Dom]) -> Option<u128> {
    doms.iter().try_fold(1u128, |n, d| n.checked_mul(d.size()))
}

fn boundary(d: Dom) -> Vec<i128> {
    let t = d.ty;
    let (lo, hi) = (d.lo, d.hi);
    if t == Ty::Bool {
        return vec![0, 1];
    }
    let mut v: Vec<i128> = vec![
        0,
        1,
        2,
        3,
        7,
        10,
        -1,
        -2,
        -3,
        -7,
        lo,
        lo + 1,
        hi,
        hi - 1,
        hi / 2,
        lo / 2,
    ];
    for k in [8, 16, 31, 32, 63] {
        if k < t.bits() as u32 {
            let p = 1i128 << k;
            v.extend([p - 1, p, p + 1, -p, -p + 1]);
        }
    }
    v.retain(|x| lo <= *x && *x <= hi);
    // Small magnitudes first, so the first counterexample found is a small one.
    v.sort_by_key(|x| (x.unsigned_abs(), *x < 0));
    v.dedup();
    v
}

fn to_val(t: Ty, x: i128) -> Val {
    match t {
        Ty::Bool => Val::Bool(x != 0),
        _ => Val::Int(x, t),
    }
}

/// A splitmix64 stream: fixed seed, same inputs on every run and machine.
struct Mix(u64);

impl Mix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn pick(&mut self, d: Dom) -> i128 {
        let r = self.next();
        match Some((d.lo, d.hi)) {
            None => (r & 1) as i128,
            Some((lo, hi)) => {
                if r & 3 == 0 {
                    // A small value, where most boundary behaviour lives.
                    let s = ((r >> 8) % 129) as i128 - 64;
                    s.clamp(lo, hi)
                } else {
                    let span = (hi - lo) as u128 + 1;
                    lo + ((r as u128 * 0x1_0000_0001 + (r >> 17) as u128) % span) as i128
                }
            }
        }
    }
}

/// The inputs to try for parameters with domains `doms`.
pub fn inputs(doms: &[Dom], seed: u64) -> (Cover, Vec<Vec<Val>>) {
    if let Some(n) = domain_size(doms).filter(|n| *n <= EXHAUSTIVE_LIMIT) {
        let mut out = Vec::with_capacity(n as usize);
        for idx in 0..n {
            let mut rest = idx;
            let mut row = Vec::with_capacity(doms.len());
            for d in doms {
                let size = d.size();
                row.push(to_val(d.ty, d.lo + (rest % size) as i128));
                rest /= size;
            }
            out.push(row);
        }
        return (Cover::Exhaustive, out);
    }
    let tys: Vec<Ty> = doms.iter().map(|d| d.ty).collect();
    let bounds: Vec<Vec<i128>> = doms.iter().map(|d| boundary(*d)).collect();
    let mut out = Vec::new();
    let product = bounds
        .iter()
        .try_fold(1usize, |a, b| a.checked_mul(b.len()));
    if product.map(|p| p <= BOUNDARY_PRODUCT_LIMIT) == Some(true) {
        let total = product.unwrap();
        for idx in 0..total {
            let mut rest = idx;
            let mut row = Vec::new();
            for (t, b) in tys.iter().zip(&bounds) {
                row.push(to_val(*t, b[rest % b.len()]));
                rest /= b.len();
            }
            out.push(row);
        }
    }
    let mut mix = Mix(seed);
    for _ in 0..RANDOM_SAMPLES {
        out.push(doms.iter().map(|d| to_val(d.ty, mix.pick(*d))).collect());
    }
    (Cover::Sampled, out)
}

/// A seed from a name, so each obligation has its own fixed sample.
pub fn seed_of(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
    })
}
