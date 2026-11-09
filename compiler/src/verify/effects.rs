// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Effect events, shared by the two sides of translation validation.
//!
//! An event is a term. Output is `out$1` (stdout) or `out$2` (stderr) over a
//! list of chunks: literal text (`lit$TEXT`, adjacent text merged), the
//! decimal text of an integer (`dec`), `true` or `false` (`btxt`), or a
//! named float rendering (`fmt$KIND`). A call of a user function is
//! `call$NAME` over its arguments. Both sides build events only through
//! these constructors, and [`ev_eq`] states when two events are the same.

use super::term::*;

/// Literal output text.
pub fn lit(text: &str) -> Term {
    Term::App(format!("lit${text}"), Vec::new())
}

/// The decimal text of an integer value.
pub fn dec(v: Term) -> Term {
    Term::App("dec".into(), vec![v])
}

/// `true` or `false` for a bool value.
pub fn btxt(b: Term) -> Term {
    Term::App("btxt".into(), vec![b])
}

/// A float rendered by the named rule (`f64`, `f64dbg`, `f32`, `f32dbg`,
/// or `.N` for a fixed number of decimals).
pub fn fmt(kind: &str, v: Term) -> Term {
    Term::App(format!("fmt${kind}"), vec![v])
}

fn lit_text(t: &Term) -> Option<&str> {
    match t {
        Term::App(n, a) if a.is_empty() => n.strip_prefix("lit$"),
        _ => None,
    }
}

/// An output event on `stream` (1 stdout, 2 stderr): adjacent literal text
/// is merged and empty text dropped, so the chunking of the format string
/// does not matter.
pub fn out_event(stream: u8, chunks: Vec<Term>) -> Term {
    let mut merged: Vec<Term> = Vec::new();
    for c in chunks {
        match (lit_text(&c), merged.last().and_then(lit_text)) {
            (Some(""), _) => {}
            (Some(b), Some(a)) => {
                let joined = lit(&format!("{a}{b}"));
                *merged.last_mut().unwrap() = joined;
            }
            _ => merged.push(c),
        }
    }
    Term::App(format!("out${stream}"), merged)
}

/// A call of user function `name`.
pub fn call_event(name: &str, args: &[Term]) -> Term {
    Term::App(format!("call${name}"), args.to_vec())
}

/// The path condition after a call: the callee finished on these
/// arguments. The same uninterpreted atom on both sides.
pub fn finish_atom(name: &str, args: &[Term]) -> Term {
    cmp(
        COp::Eq,
        Term::App(format!("fin${name}"), args.to_vec()),
        Term::Int(1),
    )
}

/// Whether a term is a bool by its form.
pub fn is_bool_term(t: &Term) -> bool {
    matches!(
        t,
        Term::Bool(_)
            | Term::Cmp(..)
            | Term::Iff(..)
            | Term::Not(_)
            | Term::And(..)
            | Term::Or(..)
            | Term::Var(_, super::ty::Ty::Bool)
    )
}

fn value_eq(a: &Term, b: &Term) -> Term {
    if let (Term::App(na, xa), Term::App(nb, xb)) = (a, b) {
        if na.starts_with("agg$") || nb.starts_with("agg$") {
            if na != nb || xa.len() != xb.len() {
                return Term::Bool(false);
            }
            return xa
                .iter()
                .zip(xb)
                .fold(Term::Bool(true), |g, (x, y)| and(g, value_eq(x, y)));
        }
    }
    match (is_bool_term(a), is_bool_term(b)) {
        (true, true) => iff(a.clone(), b.clone()),
        (false, false) => cmp(COp::Eq, a.clone(), b.clone()),
        _ => Term::Bool(false),
    }
}

/// The condition under which two events are the same event: the same kind,
/// the same literal text, and equal values in the same places.
pub fn ev_eq(a: &Term, b: &Term) -> Term {
    let (Term::App(na, xa), Term::App(nb, xb)) = (a, b) else {
        return Term::Bool(false);
    };
    if na != nb || xa.len() != xb.len() {
        return Term::Bool(false);
    }
    let pairs = xa.iter().zip(xb);
    if na.starts_with("out$") {
        pairs.fold(Term::Bool(true), |g, (x, y)| and(g, ev_eq(x, y)))
    } else if na == "btxt" {
        iff(xa[0].clone(), xb[0].clone())
    } else {
        // `lit$` (no arguments), `dec`, `fmt$`, `call$`: equal values.
        pairs.fold(Term::Bool(true), |g, (x, y)| and(g, value_eq(x, y)))
    }
}
