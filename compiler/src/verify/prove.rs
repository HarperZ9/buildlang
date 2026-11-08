// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The validity check: `hyps => goal` holds for every integer assignment.
//!
//! The check searches for a model of `hyps && !goal` by case splitting on
//! atoms (a small DPLL over a three-valued evaluation). Boolean variables and
//! uninterpreted boolean terms are atoms; each integer comparison becomes one
//! canonical linear literal (`p >= 0` or `p == 0`) over the polynomial normal
//! form, with nonlinear monomials as free unknowns. Each branch's assigned
//! literals go to Fourier-Motzkin elimination with integer tightening. A model
//! is never claimed; the result is either "valid" (every branch closed) or
//! "not shown". Treating a monomial as free and tightening over the integers
//! are both sound, so "valid" is sound; the procedure is incomplete.

use super::poly::{canon_any, poly, Poly};
use super::term::{COp, Term};
use std::collections::HashMap;

const MAX_ROWS: usize = 3000;
const MAX_NODES: usize = 200_000;
/// The most `!=` literals a branch splits on; past this the rest are dropped,
/// which only weakens the branch (sound, less complete).
const MAX_NE_SPLIT: usize = 8;

/// The outcome of a validity check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proof {
    Valid,
    NotShown(String),
}

/// A linear literal: `p >= 0` (`eq == false`) or `p == 0`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Lin {
    p: Poly,
    eq: bool,
}

enum Prop {
    Const(bool),
    Atom(usize),
    Not(Box<Prop>),
    And(Vec<Prop>),
    Or(Vec<Prop>),
    Iff(Box<Prop>, Box<Prop>),
}

#[derive(Default)]
struct Atoms {
    index: HashMap<String, usize>,
    lin: Vec<Option<Lin>>,
}

impl Atoms {
    fn intern(&mut self, key: String, lin: Option<Lin>) -> usize {
        if let Some(i) = self.index.get(&key) {
            return *i;
        }
        let i = self.lin.len();
        self.index.insert(key, i);
        self.lin.push(lin);
        i
    }
}

/// `p >= 0` and `-p - 1 >= 0` are negations of each other; both map to the
/// smaller of the two, with a polarity.
fn canon_ge(p: Poly) -> Option<(Lin, bool)> {
    let other = p.scale(-1)?.sub(&Poly::constant(1))?;
    Some(if p <= other {
        (Lin { p, eq: false }, true)
    } else {
        (
            Lin {
                p: other,
                eq: false,
            },
            false,
        )
    })
}

fn canon_eq(p: Poly) -> Option<Lin> {
    let flip = p.0.values().next().map(|c| *c < 0) == Some(true);
    Some(Lin {
        p: if flip { p.scale(-1)? } else { p },
        eq: true,
    })
}

fn lower(t: &Term, atoms: &mut Atoms) -> Option<Prop> {
    Some(match t {
        Term::Bool(b) => Prop::Const(*b),
        Term::Not(x) => Prop::Not(Box::new(lower(x, atoms)?)),
        Term::And(a, b) => Prop::And(vec![lower(a, atoms)?, lower(b, atoms)?]),
        Term::Or(a, b) => Prop::Or(vec![lower(a, atoms)?, lower(b, atoms)?]),
        Term::Iff(a, b) => Prop::Iff(Box::new(lower(a, atoms)?), Box::new(lower(b, atoms)?)),
        Term::Cmp(op, a, b) => {
            let (Some(pa), Some(pb)) = (poly(a), poly(b)) else {
                let key = canon_any(t)?;
                return Some(Prop::Atom(atoms.intern(key, None)));
            };
            let d = pa.sub(&pb)?;
            let one = Poly::constant(1);
            let (lin, pos) = match op {
                COp::Eq => (canon_eq(d)?, true),
                COp::Ne => (canon_eq(d)?, false),
                COp::Ge => canon_ge(d)?,
                COp::Gt => canon_ge(d.sub(&one)?)?,
                COp::Le => canon_ge(d.scale(-1)?)?,
                COp::Lt => canon_ge(d.scale(-1)?.sub(&one)?)?,
            };
            if let Some(c) = lin.p.as_const() {
                let truth = if lin.eq { c == 0 } else { c >= 0 };
                return Some(Prop::Const(truth == pos));
            }
            let key = format!("{}{}", if lin.eq { "eq:" } else { "ge:" }, lin.p.canon());
            let atom = Prop::Atom(atoms.intern(key, Some(lin)));
            if pos {
                atom
            } else {
                Prop::Not(Box::new(atom))
            }
        }
        other => Prop::Atom(atoms.intern(canon_any(other)?, None)),
    })
}

/// Three-valued evaluation under a partial assignment.
fn eval3(p: &Prop, a: &[Option<bool>]) -> Option<bool> {
    match p {
        Prop::Const(b) => Some(*b),
        Prop::Atom(i) => a[*i],
        Prop::Not(x) => eval3(x, a).map(|b| !b),
        Prop::And(xs) => {
            let mut all = true;
            for x in xs {
                match eval3(x, a) {
                    Some(false) => return Some(false),
                    None => all = false,
                    _ => {}
                }
            }
            all.then_some(true)
        }
        Prop::Or(xs) => {
            let mut none = true;
            for x in xs {
                match eval3(x, a) {
                    Some(true) => return Some(true),
                    None => none = false,
                    _ => {}
                }
            }
            none.then_some(false)
        }
        Prop::Iff(x, y) => Some(eval3(x, a)? == eval3(y, a)?),
    }
}

/// An unassigned atom the value of `p` still depends on.
fn pick(p: &Prop, a: &[Option<bool>]) -> Option<usize> {
    if eval3(p, a).is_some() {
        return None;
    }
    match p {
        Prop::Atom(i) => Some(*i),
        Prop::Not(x) => pick(x, a),
        Prop::And(xs) | Prop::Or(xs) => xs.iter().find_map(|x| pick(x, a)),
        Prop::Iff(x, y) => pick(x, a).or_else(|| pick(y, a)),
        Prop::Const(_) => None,
    }
}

/// `Some(true)` when the assigned linear literals have no integer solution.
fn theory_closed(atoms: &Atoms, a: &[Option<bool>]) -> Option<bool> {
    let (mut eqs, mut ges, mut nes) = (Vec::new(), Vec::new(), Vec::new());
    for (i, v) in a.iter().enumerate() {
        let (Some(v), Some(lin)) = (v, &atoms.lin[i]) else {
            continue;
        };
        match (lin.eq, *v) {
            (true, true) => eqs.push(lin.p.clone()),
            (true, false) => nes.push(lin.p.clone()),
            (false, true) => ges.push(lin.p.clone()),
            (false, false) => ges.push(lin.p.scale(-1)?.sub(&Poly::constant(1))?),
        }
    }
    nes.truncate(MAX_NE_SPLIT);
    let one = Poly::constant(1);
    for mask in 0u32..(1u32 << nes.len()) {
        let mut g = ges.clone();
        for (i, p) in nes.iter().enumerate() {
            let side = if mask & (1 << i) != 0 {
                p.clone()
            } else {
                p.scale(-1)?
            };
            g.push(side.sub(&one)?);
        }
        if !super::fm::eliminate(eqs.clone(), g, MAX_ROWS)? {
            return Some(false);
        }
    }
    Some(true)
}

enum Search {
    Closed,
    Open(String),
}

fn search(f: &Prop, atoms: &Atoms, a: &mut Vec<Option<bool>>, nodes: &mut usize) -> Search {
    *nodes += 1;
    if *nodes > MAX_NODES {
        return Search::Open("case analysis exceeded its bound".into());
    }
    match eval3(f, a) {
        Some(false) => return Search::Closed,
        Some(true) => {
            return match theory_closed(atoms, a) {
                Some(true) => Search::Closed,
                Some(false) => Search::Open("a case of the negated goal is not refuted".into()),
                None => Search::Open("linear elimination exceeded its bounds".into()),
            }
        }
        None => {}
    }
    let has_lin = a
        .iter()
        .enumerate()
        .any(|(i, v)| v.is_some() && atoms.lin[i].is_some());
    if has_lin && theory_closed(atoms, a) == Some(true) {
        return Search::Closed;
    }
    let Some(i) = pick(f, a) else {
        return Search::Open("no atom to split on".into());
    };
    for v in [true, false] {
        a[i] = Some(v);
        if let Search::Open(why) = search(f, atoms, a, nodes) {
            a[i] = None;
            return Search::Open(why);
        }
    }
    a[i] = None;
    Search::Closed
}

/// Whether `hyps => goal` is valid.
pub fn valid(hyps: &[Term], goal: &Term) -> Proof {
    let mut atoms = Atoms::default();
    let mut parts = Vec::new();
    for h in hyps {
        match lower(h, &mut atoms) {
            Some(p) => parts.push(p),
            None => return Proof::NotShown("a hypothesis outside the prover".into()),
        }
    }
    match lower(goal, &mut atoms) {
        Some(g) => parts.push(Prop::Not(Box::new(g))),
        None => return Proof::NotShown("a goal outside the prover".into()),
    }
    let f = Prop::And(parts);
    let mut assign = vec![None; atoms.lin.len()];
    match search(&f, &atoms, &mut assign, &mut 0) {
        Search::Closed => Proof::Valid,
        Search::Open(why) => Proof::NotShown(why),
    }
}
