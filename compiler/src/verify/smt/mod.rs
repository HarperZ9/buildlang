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
mod ranges;
pub mod rup;
pub mod sat;

use crate::verify::term::{COp, Term};
use crate::verify::ty::Ty;
use ranges::Ranges;
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
pub(crate) fn stated_bounds(hyps: &[Term]) -> HashMap<Term, (i128, i128)> {
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
