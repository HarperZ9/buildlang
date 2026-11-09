// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The per-function simulation check.
//!
//! Both sides start each segment from the same state: every related variable
//! is the same symbolic value on both sides. The certificate supplies the
//! relation (loop head labels and the variable map); this module proves it:
//!
//! 1. the two sides finish the segment on the same inputs (they trap alike):
//!    the disjunction of the source path conditions is equivalent to the
//!    disjunction of the C path conditions;
//! 2. for every source path and C path that can both run, they reach the
//!    same exit, and there the related variables, or the returned values,
//!    are equal.
//!
//! Induction over the sequence of cuts then gives equal results, equal
//! traps and equal non-termination for every input.

use super::c_exec::{c_ty, CExec, CExit};
use super::c_parse::CFunc;
use super::src_exec::{loops, SExit, SrcExec};
use crate::ast::FnDef;
use crate::verify::infer::Sigs;
use crate::verify::prove::{valid, Proof};
use crate::verify::sym::{St, Sym};
use crate::verify::term::*;
use crate::verify::ty::Ty;
use std::collections::{BTreeSet, HashMap, HashSet};

/// Named constants: the source's, from its `const` items and the built-ins,
/// and the C file's, from its top-level `const` definitions.
#[derive(Default)]
pub struct Consts {
    pub source: HashMap<String, (Term, Ty)>,
    pub c: HashMap<String, (Term, Ty)>,
}

/// The relation a certificate claims for one function.
pub struct Hints {
    /// C label of the head of source loop `k`, in source pre-order.
    pub loop_heads: Vec<String>,
    /// Source variable name to C variable name.
    pub vars: HashMap<String, String>,
}

fn conj(ts: &[Term]) -> Term {
    ts.iter().cloned().fold(Term::Bool(true), and)
}

fn eq(a: &Term, b: &Term, ty: Option<Ty>) -> Term {
    if ty == Some(Ty::Bool) || matches!(a, Term::Bool(_)) {
        iff(a.clone(), b.clone())
    } else {
        cmp(COp::Eq, a.clone(), b.clone())
    }
}

fn need(hyps: &[Term], goal: &Term, what: &str) -> Result<(), String> {
    match valid(hyps, goal) {
        Proof::Valid => Ok(()),
        Proof::NotShown(why) => Err(format!("mismatch: {what}: {why}")),
    }
}

/// Check one function. `Ok(segments)` when certified.
#[allow(clippy::too_many_arguments)]
pub fn check(
    src: &FnDef,
    fns: &HashMap<String, &FnDef>,
    sigs: &Sigs,
    c: &CFunc,
    hints: &Hints,
    callees: &HashMap<String, Ty>,
    consts: &Consts,
) -> Result<usize, String> {
    let body = src.body.as_deref().ok_or("no body")?;
    let ret = src
        .sig
        .return_ty
        .as_deref()
        .and_then(Ty::from_ast_tv)
        .ok_or("return type outside the subset")?;
    let mut sym = Sym::new(fns, sigs);
    sym.inline_calls = false;
    sym.floats = true;
    sym.consts = consts.source.clone();
    let params = sym.params(&src.name.name)?;
    if c.params.len() != params.len() {
        return Err("mismatch: parameter count differs".into());
    }
    for ((n, t), (cn, ct)) in params.iter().zip(&c.params) {
        if n != cn || c_ty(ct) != Some(*t) {
            return Err(format!(
                "mismatch: parameter `{n}: {t}` is `{ct} {cn}` in C"
            ));
        }
    }
    if c_ty(&c.ret) != Some(ret) {
        return Err(format!("mismatch: returns {ret} but C returns `{}`", c.ret));
    }
    let src_loops = loops(body);
    let all_loops = count_loops(body);
    if all_loops != src_loops.len() {
        return Err("a loop nested inside an expression (outside the checker's subset)".into());
    }
    if src_loops.len() != hints.loop_heads.len() {
        return Err(format!(
            "mismatch: {} source loops but {} loop heads in the certificate",
            src_loops.len(),
            hints.loop_heads.len()
        ));
    }
    let cuts: HashSet<String> = hints.loop_heads.iter().cloned().collect();
    let label_of: HashMap<&str, usize> = hints
        .loop_heads
        .iter()
        .enumerate()
        .map(|(k, l)| (l.as_str(), k))
        .collect();
    let cexec = CExec::new(c, &cuts, callees, &consts.c)?;
    let mut sx = SrcExec {
        sym,
        body,
        loops: src_loops,
        ret: Some(ret),
        arrivals: HashMap::new(),
    };
    let mut todo: Vec<Option<usize>> = vec![None];
    let mut done: BTreeSet<Option<usize>> = BTreeSet::new();
    let mut c_seen: BTreeSet<usize> = BTreeSet::new();
    while let Some(cut) = todo.pop() {
        if !done.insert(cut) {
            continue;
        }
        // The shared start state.
        let related: Vec<(String, Ty)> = match cut {
            None => params.clone(),
            Some(k) => sx.arrivals[&k].vars.clone(),
        };
        let mut base = Vec::new();
        let mut s_st = St {
            conds: Vec::new(),
            facts: Vec::new(),
            vars: Vec::new(),
            ret: Some(ret),
        };
        let mut c_st = St {
            conds: Vec::new(),
            facts: Vec::new(),
            vars: Vec::new(),
            ret: Some(ret),
        };
        let mut c_bound: HashSet<String> = HashSet::new();
        for (n, t) in &related {
            let v = Term::Var(n.clone(), *t);
            base.push(in_range(&v, *t));
            s_st.vars.push((n.clone(), v.clone(), *t));
            let cn = hints.vars.get(n).cloned().unwrap_or_else(|| n.clone());
            let ct = cexec
                .ty_of(&cn)
                .ok_or(format!("source `{n}` has no C variable `{cn}`"))?;
            base.push(in_range(&v, ct));
            c_st.vars.push((cn.clone(), v, ct));
            c_bound.insert(cn);
        }
        for (n, t) in c.params.iter().chain(&c.locals) {
            if !c_bound.contains(n) {
                let ty = c_ty(t).ok_or("C local type")?;
                c_st.vars
                    .push((n.clone(), Term::Var(format!("c${n}"), ty), ty));
            }
        }
        s_st.conds = base.clone();
        c_st.conds = base.clone();
        let label = cut.map(|k| hints.loop_heads[k].as_str());
        let s_paths = sx.run(cut, s_st, params.len())?;
        let c_paths = cexec.run(label, c_st)?;
        let tail = |st: &St| conj(&st.conds[base.len().min(st.conds.len())..]);
        let s_any = s_paths
            .iter()
            .fold(Term::Bool(false), |a, (st, _)| or(a, tail(st)));
        let c_any = c_paths
            .iter()
            .fold(Term::Bool(false), |a, (st, _)| or(a, tail(st)));
        let at = match cut {
            None => "entry".to_string(),
            Some(k) => format!("loop {k} ({})", hints.loop_heads[k]),
        };
        let mut facts = base.clone();
        for st in s_paths
            .iter()
            .map(|p| &p.0)
            .chain(c_paths.iter().map(|p| &p.0))
        {
            facts.extend(st.facts.iter().cloned());
        }
        need(
            &[facts.clone(), vec![s_any.clone()]].concat(),
            &c_any,
            &format!("at {at}, C traps where the source does not"),
        )?;
        need(
            &[facts.clone(), vec![c_any]].concat(),
            &s_any,
            &format!("at {at}, the source traps where C does not"),
        )?;
        for (ss, se) in &s_paths {
            if let SExit::Cut(k) = se {
                todo.push(Some(*k));
            }
            for (cs, ce) in &c_paths {
                let mut hyps = ss.hyps();
                hyps.extend(cs.conds[base.len().min(cs.conds.len())..].iter().cloned());
                hyps.extend(cs.facts.iter().cloned());
                let goal = match (se, ce) {
                    (SExit::Ret(v), CExit::Ret(Some(w))) => eq(v, w, Some(ret)),
                    (SExit::Cut(k), CExit::Cut(l)) if label_of.get(l.as_str()) == Some(k) => {
                        c_seen.insert(*k);
                        let vars = &sx.arrivals[k].vars;
                        let mut g = Term::Bool(true);
                        for (n, t) in vars {
                            let sv = ss
                                .vars
                                .iter()
                                .rev()
                                .find(|v| v.0 == *n)
                                .map(|v| v.1.clone())
                                .ok_or("scope")?;
                            let cn = hints.vars.get(n).cloned().unwrap_or_else(|| n.clone());
                            let cv = cs
                                .vars
                                .iter()
                                .rev()
                                .find(|v| v.0 == cn)
                                .map(|v| v.1.clone())
                                .ok_or("C scope")?;
                            g = and(g, eq(&sv, &cv, Some(*t)));
                        }
                        g
                    }
                    _ => Term::Bool(false),
                };
                need(
                    &hyps,
                    &goal,
                    &format!("at {at}, a source path and a C path disagree"),
                )?;
            }
        }
    }
    if let Some(k) = c_seen.iter().find(|k| !done.contains(&Some(**k))) {
        return Err(format!(
            "mismatch: C reaches loop head {k} that the source never does"
        ));
    }
    Ok(done.len())
}

/// Every `while` and `loop` in `body`, at any depth.
fn count_loops(body: &crate::ast::Block) -> usize {
    use crate::ast::{ExprKind, StmtKind};
    use crate::verify::infer::children;
    fn expr(e: &crate::ast::Expr) -> usize {
        let here = matches!(
            e.kind,
            ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::For { .. }
        ) as usize;
        here + children(e).into_iter().map(expr).sum::<usize>()
    }
    body.stmts
        .iter()
        .map(|s| match &s.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => expr(e),
            StmtKind::Local(l) => l.init.as_ref().map_or(0, |i| expr(&i.expr)),
            _ => 0,
        })
        .sum()
}
