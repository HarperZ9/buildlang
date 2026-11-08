// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Polynomial normal form over the integers.
//!
//! An integer term becomes a sum of integer-coefficient monomials over atoms.
//! Atoms are variables and the terms the ring does not interpret (division,
//! remainder, bit operations, casts, uninterpreted calls), each named by a
//! canonical string built from the normal forms of its arguments. Two integer
//! terms with the same normal form are equal for every input: ring identities
//! over Z need no case analysis. Bit operators that are associative and
//! commutative are flattened and sorted, and `x ^ x`, `x & x`, `x | x` are
//! reduced, so their algebraic laws also normalise.

use super::eval::bit_op;
use super::term::{AOp, BOp, Term};
use std::collections::BTreeMap;

/// A monomial: a sorted list of atom names (repeats are powers).
pub type Mono = Vec<String>;

/// A polynomial: monomial to non-zero coefficient. The empty monomial is the
/// constant term.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Poly(pub BTreeMap<Mono, i128>);

impl Poly {
    pub fn constant(c: i128) -> Poly {
        let mut m = BTreeMap::new();
        if c != 0 {
            m.insert(Vec::new(), c);
        }
        Poly(m)
    }

    pub fn atom(name: String) -> Poly {
        let mut m = BTreeMap::new();
        m.insert(vec![name], 1);
        Poly(m)
    }

    /// The constant value, when the polynomial has no atoms.
    pub fn as_const(&self) -> Option<i128> {
        match self.0.len() {
            0 => Some(0),
            1 => self.0.get(&Vec::new()).copied(),
            _ => None,
        }
    }

    pub fn add(&self, o: &Poly) -> Option<Poly> {
        let mut m = self.0.clone();
        for (k, v) in &o.0 {
            let e = m.entry(k.clone()).or_insert(0);
            *e = e.checked_add(*v)?;
            if *e == 0 {
                m.remove(k);
            }
        }
        Some(Poly(m))
    }

    pub fn scale(&self, c: i128) -> Option<Poly> {
        if c == 0 {
            return Some(Poly::default());
        }
        let mut m = BTreeMap::new();
        for (k, v) in &self.0 {
            m.insert(k.clone(), v.checked_mul(c)?);
        }
        Some(Poly(m))
    }

    pub fn sub(&self, o: &Poly) -> Option<Poly> {
        self.add(&o.scale(-1)?)
    }

    pub fn mul(&self, o: &Poly) -> Option<Poly> {
        let mut acc = Poly::default();
        for (ka, va) in &self.0 {
            for (kb, vb) in &o.0 {
                let mut k = ka.clone();
                k.extend(kb.iter().cloned());
                k.sort();
                let mut one = BTreeMap::new();
                one.insert(k, va.checked_mul(*vb)?);
                acc = acc.add(&Poly(one))?;
            }
        }
        Some(acc)
    }

    /// A canonical text form; equal polynomials print equal.
    pub fn canon(&self) -> String {
        if self.0.is_empty() {
            return "0".into();
        }
        let parts: Vec<String> = self
            .0
            .iter()
            .map(|(k, v)| {
                if k.is_empty() {
                    v.to_string()
                } else {
                    format!("{v}*{}", k.join("*"))
                }
            })
            .collect();
        parts.join(" + ")
    }
}

/// The normal form of an integer term; `None` when a coefficient overflows or
/// the term is boolean.
pub fn poly(t: &Term) -> Option<Poly> {
    Some(match t {
        Term::Int(c) => Poly::constant(*c),
        Term::Var(n, _) => Poly::atom(format!("v:{n}")),
        Term::Arith(op, a, b) => {
            let (pa, pb) = (poly(a)?, poly(b)?);
            match op {
                AOp::Add => pa.add(&pb)?,
                AOp::Sub => pa.sub(&pb)?,
                AOp::Mul => pa.mul(&pb)?,
                AOp::Div | AOp::Rem => {
                    if let (Some(x), Some(y)) = (pa.as_const(), pb.as_const()) {
                        if y != 0 {
                            return Some(Poly::constant(if *op == AOp::Div {
                                x / y
                            } else {
                                x % y
                            }));
                        }
                    }
                    let tag = if *op == AOp::Div { "div" } else { "rem" };
                    Poly::atom(format!("{tag}({},{})", pa.canon(), pb.canon()))
                }
            }
        }
        Term::Neg(a) => poly(a)?.scale(-1)?,
        Term::Bits(op, ty, a, b) => return bits_poly(*op, *ty, a, b),
        Term::BitNot(ty, a) => Poly::atom(format!("not{ty}({})", poly(a)?.canon())),
        Term::Cast(ty, a) => {
            let pa = poly(a)?;
            match pa.as_const() {
                Some(c) => Poly::constant(ty.wrap(c)),
                None => Poly::atom(format!("as{ty}({})", pa.canon())),
            }
        }
        Term::App(f, args) => {
            let mut parts = Vec::new();
            for a in args {
                parts.push(canon_any(a)?);
            }
            Poly::atom(format!("{f}({})", parts.join(",")))
        }
        _ => return None,
    })
}

/// A canonical name for any term, boolean or integer.
pub fn canon_any(t: &Term) -> Option<String> {
    match poly(t) {
        Some(p) => Some(p.canon()),
        None => Some(format!("b:{t:?}")),
    }
}

fn flatten<'t>(op: BOp, ty: super::ty::Ty, t: &'t Term, out: &mut Vec<&'t Term>) {
    match t {
        Term::Bits(o, tt, a, b) if *o == op && *tt == ty => {
            flatten(op, ty, a, out);
            flatten(op, ty, b, out);
        }
        other => out.push(other),
    }
}

fn bits_poly(op: BOp, ty: super::ty::Ty, a: &Term, b: &Term) -> Option<Poly> {
    if !op.is_ac() {
        let (pa, pb) = (poly(a)?, poly(b)?);
        if let (Some(x), Some(y)) = (pa.as_const(), pb.as_const()) {
            return Some(Poly::constant(bit_op(op, ty, x, y)?));
        }
        return Some(Poly::atom(format!(
            "{op:?}{ty}({},{})",
            pa.canon(),
            pb.canon()
        )));
    }
    let mut leaves = Vec::new();
    flatten(op, ty, a, &mut leaves);
    flatten(op, ty, b, &mut leaves);
    let mut konst: Option<i128> = None;
    let mut names: Vec<(String, Poly)> = Vec::new();
    for leaf in leaves {
        let p = poly(leaf)?;
        match p.as_const() {
            Some(c) => {
                konst = Some(match konst {
                    None => ty.wrap(c),
                    Some(k) => bit_op(op, ty, k, c)?,
                })
            }
            None => names.push((p.canon(), p)),
        }
    }
    names.sort_by(|x, y| x.0.cmp(&y.0));
    match op {
        BOp::Xor => {
            let mut kept: Vec<(String, Poly)> = Vec::new();
            for n in names {
                if kept.last().map(|k| k.0 == n.0) == Some(true) {
                    kept.pop();
                } else {
                    kept.push(n);
                }
            }
            names = kept;
            if konst == Some(0) {
                konst = None;
            }
        }
        BOp::And | BOp::Or => {
            names.dedup_by(|x, y| x.0 == y.0);
            let identity = if op == BOp::And { ty.wrap(-1) } else { 0 };
            if konst == Some(identity) {
                konst = None;
            }
        }
        BOp::WAdd => {
            if konst == Some(0) {
                konst = None;
            }
        }
        _ => {}
    }
    if names.is_empty() {
        return Some(Poly::constant(konst.unwrap_or(0)));
    }
    if names.len() == 1 && konst.is_none() {
        // A single operand: the value itself, wrapped to the type.
        let only = names.pop().unwrap().1;
        return Some(only);
    }
    let mut parts: Vec<String> = names.into_iter().map(|n| n.0).collect();
    if let Some(k) = konst {
        parts.push(k.to_string());
    }
    Some(Poly::atom(format!("{op:?}{ty}[{}]", parts.join(";"))))
}
