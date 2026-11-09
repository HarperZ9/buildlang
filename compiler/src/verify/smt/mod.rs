// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! A bit-precise decision procedure for verification obligations: the
//! obligation is bit-blasted ([`enc`]) and handed to a CDCL SAT solver
//! ([`sat`]); every UNSAT answer is replayed by an independent RUP checker
//! ([`rup`]) before it counts as a proof. It ships inside `buildc` with no
//! outside solver.
//!
//! Exactness: each integer subterm gets an interval from its type, its
//! operands, or bounds the hypotheses state; the bit width is chosen so every
//! interval fits with room to spare, so no arithmetic wraps. A term with no
//! bound (a call result the hypotheses do not bound) makes the answer
//! unknown. A satisfying assignment is only a candidate: the caller re-runs it
//! on the concrete evaluator before reporting a counterexample.

pub mod enc;
pub mod rup;
pub mod sat;

use crate::verify::term::{AOp, BOp, COp, Term};
use crate::verify::ty::Ty;
use std::collections::HashMap;

/// The answer for one obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// `hyps => goal` holds for every input; the UNSAT proof was replayed.
    Valid,
    /// An assignment of the parameters under which the goal may be false.
    Candidate(Vec<(String, i128)>),
    Unknown(String),
}

/// Widest bit vector the encoder builds.
pub const MAX_WIDTH: usize = 192;
/// Conflicts the solver may spend on one obligation.
pub const BUDGET: u64 = 30_000;

fn conjuncts<'t>(t: &'t Term, out: &mut Vec<&'t Term>) {
    match t {
        Term::And(a, b) => {
            conjuncts(a, out);
            conjuncts(b, out);
        }
        other => out.push(other),
    }
}

/// Bounds the hypotheses state for parameters and call results: `t >= c`,
/// `t <= c` as top-level conjuncts.
fn stated_bounds(hyps: &[Term]) -> HashMap<Term, (i128, i128)> {
    let mut out: HashMap<Term, (i128, i128)> = HashMap::new();
    let mut cs = Vec::new();
    for h in hyps {
        conjuncts(h, &mut cs);
    }
    for c in cs {
        let Term::Cmp(op, a, b) = c else { continue };
        let (t, k, op) = match (&**a, &**b) {
            (t @ (Term::App(..) | Term::Var(..)), Term::Int(k)) => (t, *k, *op),
            (Term::Int(k), t @ (Term::App(..) | Term::Var(..))) => (
                t,
                *k,
                match op {
                    COp::Lt => COp::Gt,
                    COp::Le => COp::Ge,
                    COp::Gt => COp::Lt,
                    COp::Ge => COp::Le,
                    o => *o,
                },
            ),
            _ => continue,
        };
        let e = out.entry(t.clone()).or_insert((i128::MIN, i128::MAX));
        match op {
            COp::Ge => e.0 = e.0.max(k),
            COp::Gt => e.0 = e.0.max(k.saturating_add(1)),
            COp::Le => e.1 = e.1.min(k),
            COp::Lt => e.1 = e.1.min(k.saturating_sub(1)),
            COp::Eq => {
                e.0 = e.0.max(k);
                e.1 = e.1.min(k);
            }
            COp::Ne => {}
        }
    }
    out
}

struct Ranges {
    stated: HashMap<Term, (i128, i128)>,
    memo: HashMap<Term, (i128, i128)>,
    /// Bounds the encoder must impose on atoms.
    atoms: HashMap<Term, (i128, i128)>,
    max_ty_bits: u32,
    /// Intervals of products, stated to the solver as facts.
    products: HashMap<Term, (i128, i128)>,
}

impl Ranges {
    fn int(&mut self, t: &Term) -> Result<(i128, i128), String> {
        if let Some(r) = self.memo.get(t) {
            return Ok(*r);
        }
        let ty_range = |ty: &Ty| ty.range().ok_or("a non-integer operand".to_string());
        let r = match t {
            Term::Int(v) => (*v, *v),
            Term::Var(_, ty) => {
                let r = ty_range(ty)?;
                let r = match self.stated.get(t) {
                    Some(s) => (r.0.max(s.0), r.1.min(s.1)),
                    None => r,
                };
                if r.0 > r.1 {
                    return Err("the hypotheses admit no value".into());
                }
                self.max_ty_bits = self.max_ty_bits.max(ty.bits() as u32);
                self.atoms.insert(t.clone(), r);
                r
            }
            Term::App(..) => {
                let r = *self
                    .stated
                    .get(t)
                    .filter(|r| r.0 > i128::MIN && r.1 < i128::MAX)
                    .ok_or("a call result with no stated bound")?;
                self.atoms.insert(t.clone(), r);
                r
            }
            Term::Arith(op, a, b) => {
                let (x, y) = (self.int(a)?, self.int(b)?);
                let r = match op {
                    AOp::Add => (ck(x.0.checked_add(y.0))?, ck(x.1.checked_add(y.1))?),
                    AOp::Sub => (ck(x.0.checked_sub(y.1))?, ck(x.1.checked_sub(y.0))?),
                    AOp::Mul => {
                        let ps = [
                            ck(x.0.checked_mul(y.0))?,
                            ck(x.0.checked_mul(y.1))?,
                            ck(x.1.checked_mul(y.0))?,
                            ck(x.1.checked_mul(y.1))?,
                        ];
                        let (lo, hi) = (*ps.iter().min().unwrap(), *ps.iter().max().unwrap());
                        // A square is never negative.
                        let lo = if a == b { lo.max(0) } else { lo };
                        // The product's interval is a fact about every run:
                        // the encoder states it, which helps the solver.
                        self.products.insert(t.clone(), (lo, hi));
                        (lo, hi)
                    }
                    AOp::Div => {
                        let m = ck(x.0.checked_abs())?.max(ck(x.1.checked_abs())?);
                        (-m, m)
                    }
                    AOp::Rem => {
                        let ma = ck(x.0.checked_abs())?.max(ck(x.1.checked_abs())?);
                        let mb = ck(y.0.checked_abs())?.max(ck(y.1.checked_abs())?);
                        let m = ma.min(mb.saturating_sub(1)).max(0);
                        (-m, m)
                    }
                };
                if matches!(op, AOp::Div | AOp::Rem) {
                    self.atoms.insert(t.clone(), r);
                }
                r
            }
            Term::Neg(a) => {
                let x = self.int(a)?;
                (ck(x.1.checked_neg())?, ck(x.0.checked_neg())?)
            }
            Term::Cast(ty, a) | Term::BitNot(ty, a) => {
                self.int(a)?;
                self.max_ty_bits = self.max_ty_bits.max(ty.bits() as u32);
                ty_range(ty)?
            }
            Term::Bits(op, ty, a, b) => {
                let x = self.int(a)?;
                self.int(b)?;
                self.max_ty_bits = self.max_ty_bits.max(ty.bits() as u32);
                match (op, ty) {
                    (BOp::Shr, Ty::Int { signed: true, .. }) => (x.0.min(0), x.1.max(0)),
                    _ => ty_range(ty)?,
                }
            }
            other => return Err(format!("not an integer term: {other:?}")),
        };
        self.memo.insert(t.clone(), r);
        Ok(r)
    }

    fn boolean(&mut self, t: &Term) -> Result<(), String> {
        match t {
            Term::Bool(_) | Term::Var(_, Ty::Bool) | Term::App(..) => Ok(()),
            Term::Not(a) => self.boolean(a),
            Term::And(a, b) | Term::Or(a, b) | Term::Iff(a, b) => {
                self.boolean(a)?;
                self.boolean(b)
            }
            Term::Cmp(_, a, b) => {
                self.int(a)?;
                self.int(b)?;
                Ok(())
            }
            other => Err(format!("not a boolean term: {other:?}")),
        }
    }
}

fn ck(v: Option<i128>) -> Result<i128, String> {
    v.ok_or_else(|| "an interval too wide for 128 bits".to_string())
}

/// Two's-complement bits needed for `v`.
fn bits_for(v: i128) -> usize {
    let m = if v < 0 { !v } else { v };
    (128 - m.leading_zeros() as usize) + 1
}

/// Decide `hyps => goal`. `plant_drop_carry` is a test-only fault.
pub fn decide(hyps: &[Term], goal: &Term, plant_drop_carry: bool) -> Answer {
    let mut ranges = Ranges {
        stated: stated_bounds(hyps),
        memo: HashMap::new(),
        atoms: HashMap::new(),
        max_ty_bits: 1,
        products: HashMap::new(),
    };
    for t in hyps.iter().chain(std::iter::once(goal)) {
        if let Err(why) = ranges.boolean(t) {
            return Answer::Unknown(why);
        }
    }
    let need = ranges
        .memo
        .values()
        .map(|r| bits_for(r.0).max(bits_for(r.1)))
        .max()
        .unwrap_or(1)
        .max(ranges.max_ty_bits as usize + 1);
    let w = need + 2;
    if w > MAX_WIDTH {
        return Answer::Unknown(format!("needs {w}-bit vectors (limit {MAX_WIDTH})"));
    }
    let mut e = enc::Enc::new(w);
    e.plant_drop_carry = plant_drop_carry;
    if !e.self_check() {
        return Answer::Unknown("the encoder failed its self-check".into());
    }
    e.bounds = std::mem::take(&mut ranges.atoms);
    for (t, (lo, hi)) in &ranges.products {
        let b = Term::And(
            Box::new(Term::Cmp(
                COp::Ge,
                Box::new(t.clone()),
                Box::new(Term::Int(*lo)),
            )),
            Box::new(Term::Cmp(
                COp::Le,
                Box::new(t.clone()),
                Box::new(Term::Int(*hi)),
            )),
        );
        match e.boolean(&b) {
            Ok(l) => e.s.add(vec![l]),
            Err(why) => return Answer::Unknown(why),
        }
    }
    for h in hyps {
        match e.boolean(h) {
            Ok(l) => e.s.add(vec![l]),
            Err(why) => return Answer::Unknown(why),
        }
    }
    match e.boolean(goal) {
        Ok(l) => e.s.add(vec![sat::neg(l)]),
        Err(why) => return Answer::Unknown(why),
    }
    let original = e.s.clauses.clone();
    let nvars = e.s.num_vars();
    match e.s.solve(BUDGET) {
        sat::Answer::Unsat => {
            if rup::check(&original, &e.s.learned, nvars, 400_000_000) {
                Answer::Valid
            } else {
                Answer::Unknown("the UNSAT answer did not replay".into())
            }
        }
        sat::Answer::Sat(model) => {
            let mut out = Vec::new();
            for (t, v) in &e.int_atoms {
                if let Term::Var(n, _) = t {
                    out.push((n.clone(), e.read(v, &model)));
                }
            }
            for (t, l) in &e.bool_atoms {
                if let Term::Var(n, Ty::Bool) = t {
                    out.push((n.clone(), e.bit(*l, &model) as i128));
                }
            }
            Answer::Candidate(out)
        }
        sat::Answer::Unknown => Answer::Unknown(format!("no answer in {BUDGET} conflicts")),
    }
}

#[cfg(test)]
mod tests;
