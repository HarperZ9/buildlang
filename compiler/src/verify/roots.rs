// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! A root search for counterexamples (V3, rule M3).
//!
//! For a goal `p == q` or `p != q` over integer polynomials in the
//! parameters, each parameter in turn is solved for with the others fixed at
//! small values and the ends of their domains: the polynomial in that one
//! parameter is scanned across its domain and bisected on each sign change.
//! Every integer root is only a candidate input; the caller runs it on the
//! concrete evaluator and reports nothing else.

use super::poly::{poly, Poly};
use super::smt::stated_bounds;
use super::term::{COp, Term};
use super::ty::Ty;
use std::collections::HashMap;

const SCAN: i128 = 64;

/// The value of `p` with every atom `v:NAME` bound in `env`; `None` on
/// overflow or an atom that is not a parameter.
fn eval(p: &Poly, env: &HashMap<String, i128>) -> Option<i128> {
    let mut sum: i128 = 0;
    for (m, c) in &p.0 {
        let mut v = *c;
        for a in m {
            let x = *env.get(a.strip_prefix("v:")?)?;
            v = v.checked_mul(x)?;
        }
        sum = sum.checked_add(v)?;
    }
    Some(sum)
}

fn sides(goal: &Term) -> Option<(&Term, &Term)> {
    match goal {
        Term::Cmp(COp::Eq | COp::Ne, a, b) => Some((a, b)),
        Term::Not(inner) => sides(inner),
        _ => None,
    }
}

/// Integer roots of `x -> f(x)` in `[lo, hi]` found by scanning and bisection.
fn roots(f: &dyn Fn(i128) -> Option<i128>, lo: i128, hi: i128) -> Vec<i128> {
    let mut out = Vec::new();
    let step = ((hi - lo) / SCAN).max(1);
    let mut pts: Vec<i128> = (0..=SCAN)
        .map(|k| lo + k * step)
        .filter(|x| *x <= hi)
        .collect();
    pts.push(hi);
    for w in pts.windows(2) {
        let (mut a, mut b) = (w[0], w[1]);
        let (Some(fa), Some(fb)) = (f(a), f(b)) else {
            continue;
        };
        if fa == 0 {
            out.push(a);
        }
        if fb == 0 {
            out.push(b);
        }
        if fa.signum() * fb.signum() >= 0 {
            continue;
        }
        let sa = fa.signum();
        while b - a > 1 {
            let mid = a + (b - a) / 2;
            match f(mid) {
                Some(0) => {
                    out.push(mid);
                    break;
                }
                Some(v) if v.signum() == sa => a = mid,
                Some(_) => b = mid,
                None => break,
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Candidate inputs for `goal` under `hyps`, at most `limit` of them.
pub fn candidates(
    hyps: &[Term],
    goal: &Term,
    params: &[(String, Ty)],
    limit: usize,
) -> Vec<Vec<(String, i128)>> {
    let Some((a, b)) = sides(goal) else {
        return Vec::new();
    };
    let Some(f) = poly(a).and_then(|pa| pa.sub(&poly(b)?)) else {
        return Vec::new();
    };
    if params.is_empty() || params.len() > 4 || f.as_const().is_some() {
        return Vec::new();
    }
    let stated = stated_bounds(hyps);
    let mut doms = Vec::new();
    for (n, t) in params {
        let Some((lo, hi)) = t.range() else {
            return Vec::new();
        };
        let (slo, shi) = stated
            .get(&Term::Var(n.clone(), *t))
            .copied()
            .unwrap_or((lo, hi));
        doms.push((lo.max(slo), hi.min(shi)));
    }
    let picks = |(lo, hi): (i128, i128)| -> Vec<i128> {
        let mut v: Vec<i128> = [0, 1, -1, 2, lo, hi, lo + 1, hi - 1]
            .into_iter()
            .filter(|x| *x >= lo && *x <= hi)
            .collect();
        v.sort();
        v.dedup();
        v
    };
    let mut out = Vec::new();
    for (i, (name, _)) in params.iter().enumerate() {
        let others: Vec<usize> = (0..params.len()).filter(|j| *j != i).collect();
        let mut combos: Vec<Vec<i128>> = vec![Vec::new()];
        for j in &others {
            let mut next = Vec::new();
            for c in &combos {
                for v in picks(doms[*j]) {
                    let mut c2 = c.clone();
                    c2.push(v);
                    next.push(c2);
                }
            }
            combos = next;
        }
        for c in combos {
            let mut env: HashMap<String, i128> = HashMap::new();
            for (j, v) in others.iter().zip(&c) {
                env.insert(params[*j].0.clone(), *v);
            }
            let g = |x: i128| {
                let mut e = env.clone();
                e.insert(name.clone(), x);
                eval(&f, &e)
            };
            for r in roots(&g, doms[i].0, doms[i].1) {
                let mut cand: Vec<(String, i128)> =
                    env.iter().map(|(k, v)| (k.clone(), *v)).collect();
                cand.push((name.clone(), r));
                out.push(cand);
                if out.len() >= limit {
                    return out;
                }
            }
        }
    }
    out
}
