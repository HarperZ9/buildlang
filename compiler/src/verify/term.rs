// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Symbolic terms. Integer terms denote exact mathematical integers: plain
//! BuildLang arithmetic traps instead of wrapping, so every run that finishes
//! computed the exact value. Range facts are carried as separate hypotheses.

use super::ty::Ty;

/// An arithmetic operator with exact integer meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AOp {
    Add,
    Sub,
    Mul,
    /// Division truncating toward zero.
    Div,
    /// Remainder with the sign of the dividend.
    Rem,
}

/// A bit-level operator, interpreted only on constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BOp {
    And,
    Or,
    Xor,
    Shl,
    Shr,
    /// `wrapping_add`, commutative and associative modulo 2^bits.
    WAdd,
    /// `wrapping_mul`, commutative and associative modulo 2^bits.
    WMul,
}

impl BOp {
    /// Whether the operator is associative and commutative.
    pub fn is_ac(self) -> bool {
        matches!(self, BOp::And | BOp::Or | BOp::Xor | BOp::WAdd | BOp::WMul)
    }
}

/// A comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum COp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A symbolic term.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Term {
    Int(i128),
    Bool(bool),
    Var(String, Ty),
    Arith(AOp, Box<Term>, Box<Term>),
    Neg(Box<Term>),
    Bits(BOp, Ty, Box<Term>, Box<Term>),
    BitNot(Ty, Box<Term>),
    /// `e as ty`: wraps into `ty`.
    Cast(Ty, Box<Term>),
    Cmp(COp, Box<Term>, Box<Term>),
    /// Equality of two booleans.
    Iff(Box<Term>, Box<Term>),
    Not(Box<Term>),
    And(Box<Term>, Box<Term>),
    Or(Box<Term>, Box<Term>),
    /// A call left uninterpreted (recursion, or a callee outside the subset).
    App(String, Vec<Term>),
}

pub fn arith(op: AOp, a: Term, b: Term) -> Term {
    if let (Term::Int(x), Term::Int(y)) = (&a, &b) {
        let v = match op {
            AOp::Add => x.checked_add(*y),
            AOp::Sub => x.checked_sub(*y),
            AOp::Mul => x.checked_mul(*y),
            AOp::Div => (*y != 0).then(|| x / y),
            AOp::Rem => (*y != 0).then(|| x % y),
        };
        if let Some(v) = v {
            return Term::Int(v);
        }
    }
    Term::Arith(op, Box::new(a), Box::new(b))
}

pub fn neg(a: Term) -> Term {
    match a {
        Term::Int(x) => Term::Int(-x),
        other => Term::Neg(Box::new(other)),
    }
}

pub fn bits(op: BOp, ty: Ty, a: Term, b: Term) -> Term {
    if let (Term::Int(x), Term::Int(y)) = (&a, &b) {
        if let Some(v) = super::eval::bit_op(op, ty, *x, *y) {
            return Term::Int(v);
        }
    }
    Term::Bits(op, ty, Box::new(a), Box::new(b))
}

pub fn cast(ty: Ty, from: Option<Ty>, a: Term) -> Term {
    if let Term::Int(x) = a {
        return Term::Int(ty.wrap(x));
    }
    // A widening cast keeps the value.
    if let (Some((flo, fhi)), Some((tlo, thi))) = (from.and_then(Ty::range), ty.range()) {
        if tlo <= flo && fhi <= thi {
            return a;
        }
    }
    Term::Cast(ty, Box::new(a))
}

pub fn cmp(op: COp, a: Term, b: Term) -> Term {
    if let (Term::Int(x), Term::Int(y)) = (&a, &b) {
        return Term::Bool(match op {
            COp::Eq => x == y,
            COp::Ne => x != y,
            COp::Lt => x < y,
            COp::Le => x <= y,
            COp::Gt => x > y,
            COp::Ge => x >= y,
        });
    }
    if a == b {
        return Term::Bool(matches!(op, COp::Eq | COp::Le | COp::Ge));
    }
    Term::Cmp(op, Box::new(a), Box::new(b))
}

pub fn iff(a: Term, b: Term) -> Term {
    match (&a, &b) {
        (Term::Bool(x), Term::Bool(y)) => Term::Bool(x == y),
        (Term::Bool(true), _) => b,
        (_, Term::Bool(true)) => a,
        (Term::Bool(false), _) => not(b),
        (_, Term::Bool(false)) => not(a),
        _ if a == b => Term::Bool(true),
        _ => Term::Iff(Box::new(a), Box::new(b)),
    }
}

pub fn not(a: Term) -> Term {
    match a {
        Term::Bool(x) => Term::Bool(!x),
        Term::Not(inner) => *inner,
        other => Term::Not(Box::new(other)),
    }
}

pub fn and(a: Term, b: Term) -> Term {
    match (&a, &b) {
        (Term::Bool(false), _) | (_, Term::Bool(false)) => Term::Bool(false),
        (Term::Bool(true), _) => b,
        (_, Term::Bool(true)) => a,
        _ => Term::And(Box::new(a), Box::new(b)),
    }
}

pub fn or(a: Term, b: Term) -> Term {
    match (&a, &b) {
        (Term::Bool(true), _) | (_, Term::Bool(true)) => Term::Bool(true),
        (Term::Bool(false), _) => b,
        (_, Term::Bool(false)) => a,
        _ => Term::Or(Box::new(a), Box::new(b)),
    }
}

/// `lo <= t <= hi` for an integer type, or `true` for bool.
pub fn in_range(t: &Term, ty: Ty) -> Term {
    match ty.range() {
        Some((lo, hi)) => and(
            cmp(COp::Ge, t.clone(), Term::Int(lo)),
            cmp(COp::Le, t.clone(), Term::Int(hi)),
        ),
        None => Term::Bool(true),
    }
}
