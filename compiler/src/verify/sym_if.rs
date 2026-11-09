// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! A bool `if` whose branches are single values becomes one path.

use super::sym::{Out, St, Sym};
use super::sym_expr::expr;
use super::term::*;
use super::ty::Ty;
use crate::ast::*;

/// An `if` whose branches each finish on one path, add no facts, assign
/// nothing and yield a bool becomes one path with the value
/// `(c && then) || (!c && else)`, instead of two paths.
pub(super) fn merge_bool_if(
    sym: &mut Sym<'_>,
    s: &St,
    cond: &Term,
    then_branch: &Block,
    else_branch: Option<&Expr>,
    expect: Option<Ty>,
) -> Result<Option<Vec<(St, Out)>>, String> {
    if matches!(cond, Term::Bool(_)) {
        return Ok(None);
    }
    probe(sym, s, cond, then_branch, else_branch, expect)
}

fn probe(
    sym: &mut Sym<'_>,
    s: &St,
    cond: &Term,
    then_branch: &Block,
    else_branch: Option<&Expr>,
    expect: Option<Ty>,
) -> Result<Option<Vec<(St, Out)>>, String> {
    let Some(el) = else_branch else {
        return Ok(None);
    };
    let mut ts = s.clone();
    ts.assume(cond.clone());
    let mut es = s.clone();
    es.assume(not(cond.clone()));
    let tr = sym.block(then_branch, ts.clone(), expect)?;
    let er = expr(sym, el, es.clone(), expect)?;
    let single = |r: &[(St, Out)], start: &St| -> Option<Term> {
        match r {
            [(st, Out::Val(v, Some(Ty::Bool)))]
                if st.vars == start.vars
                    && st.conds.len() == start.conds.len()
                    && st.trace.len() == start.trace.len() =>
            {
                Some(v.clone())
            }
            _ => None,
        }
    };
    match (single(&tr, &ts), single(&er, &es)) {
        (Some(v), Some(w)) => {
            let merged = or(and(cond.clone(), v), and(not(cond.clone()), w));
            Ok(Some(vec![(s.clone(), Out::Val(merged, Some(Ty::Bool)))]))
        }
        _ => {
            let mut out = tr;
            out.extend(er);
            Ok(Some(out))
        }
    }
}
