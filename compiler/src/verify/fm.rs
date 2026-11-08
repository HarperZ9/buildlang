// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Fourier-Motzkin elimination with integer tightening.
//!
//! Rows are linear: each non-constant monomial is one unknown. Equalities are
//! used to eliminate an unknown from every other row, then inequalities are
//! eliminated pairwise with positive multipliers. Each inequality is divided
//! by the gcd of its unknowns' coefficients with the constant rounded down,
//! which is exact over the integers. Reaching a negative constant shows the
//! rows have no integer solution.

use super::poly::{Mono, Poly};

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn coef(p: &Poly, m: &Mono) -> i128 {
    p.0.get(m).copied().unwrap_or(0)
}

fn konst(p: &Poly) -> i128 {
    coef(p, &Vec::new())
}

fn unknowns(p: &Poly) -> impl Iterator<Item = (&Mono, &i128)> {
    p.0.iter().filter(|(k, _)| !k.is_empty())
}

/// `q` with unknown `x` removed using `p` (where `p` is `== 0` and has a
/// non-zero coefficient `c` on `x`). Multiplies `q` by `|c|`, which keeps the
/// direction of an inequality.
fn cancel(q: &Poly, p: &Poly, x: &Mono) -> Option<Poly> {
    let c = coef(p, x);
    let d = coef(q, x);
    if d == 0 {
        return Some(q.clone());
    }
    q.scale(c.abs())?.sub(&p.scale(c.signum() * d)?)
}

/// Divide an inequality by the gcd of its unknowns, rounding the constant
/// down. Returns `None` on an empty (constant) row.
fn tighten(p: &Poly) -> Option<Poly> {
    let g = unknowns(p).fold(0, |g, (_, c)| gcd(g, *c));
    if g == 0 {
        return None;
    }
    let mut out = Poly::default();
    for (k, v) in &p.0 {
        let nv = if k.is_empty() { v.div_euclid(g) } else { v / g };
        if nv != 0 {
            out.0.insert(k.clone(), nv);
        }
    }
    Some(out)
}

/// `Some(true)`: no integer solution. `Some(false)`: no contradiction found.
/// `None`: the row bound or a coefficient bound was exceeded.
pub fn eliminate(mut eqs: Vec<Poly>, mut ges: Vec<Poly>, max_rows: usize) -> Option<bool> {
    while let Some(p) = eqs.pop() {
        let g = unknowns(&p).fold(0, |g, (_, c)| gcd(g, *c));
        if g == 0 {
            if konst(&p) != 0 {
                return Some(true);
            }
            continue;
        }
        if konst(&p) % g != 0 {
            return Some(true);
        }
        let x = unknowns(&p).next().map(|(k, _)| k.clone())?;
        for q in eqs.iter_mut().chain(ges.iter_mut()) {
            *q = cancel(q, &p, &x)?;
        }
    }
    loop {
        let mut rows: Vec<Poly> = Vec::new();
        for p in ges.drain(..) {
            match tighten(&p) {
                None => {
                    if konst(&p) < 0 {
                        return Some(true);
                    }
                }
                Some(t) => rows.push(t),
            }
        }
        rows.sort();
        rows.dedup();
        if rows.is_empty() {
            return Some(false);
        }
        if rows.len() > max_rows {
            return None;
        }
        // Pick the unknown whose elimination makes the fewest new rows.
        let mut best: Option<(usize, Mono)> = None;
        for r in &rows {
            for (k, _) in unknowns(r) {
                let pos = rows.iter().filter(|q| coef(q, k) > 0).count();
                let neg = rows.iter().filter(|q| coef(q, k) < 0).count();
                let cost = pos * neg;
                if best.as_ref().map(|b| cost < b.0) != Some(false) {
                    best = Some((cost, k.clone()));
                }
            }
        }
        let x = best?.1;
        let (mut pos, mut neg) = (Vec::new(), Vec::new());
        for r in rows {
            match coef(&r, &x) {
                0 => ges.push(r),
                c if c > 0 => pos.push(r),
                _ => neg.push(r),
            }
        }
        for p in &pos {
            for n in &neg {
                let a = coef(p, &x);
                let b = coef(n, &x);
                ges.push(p.scale(-b)?.add(&n.scale(a)?)?);
            }
        }
        if ges.len() > max_rows {
            return None;
        }
    }
}
