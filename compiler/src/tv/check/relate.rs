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

use super::c_exec::{CExec, CExit};
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
    /// The C file's string table.
    pub strings: HashMap<String, String>,
    /// The source text, for print macro arguments.
    pub text: String,
    /// C function names the backend gave a renamed source function, to the
    /// source name (`_max` to `max`).
    pub c_names: HashMap<String, String>,
    /// The source's struct values, and the C definitions checked against them.
    pub structs: crate::verify::agg::Structs,
    pub cstructs: super::structs::CStructs,
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
    callees: &HashMap<String, Option<Ty>>,
    consts: &Consts,
) -> Result<usize, String> {
    let body = src.body.as_deref().ok_or("no body")?;
    let ret = match src.sig.return_ty.as_deref() {
        None => None,
        Some(t) => Some(
            consts
                .structs
                .ty_of_ast(t)
                .ok_or("return type outside the subset")?,
        ),
    };
    let mut sym = Sym::new(fns, sigs);
    sym.inline_calls = false;
    sym.floats = true;
    sym.consts = consts.source.clone();
    sym.source = Some(consts.text.as_str());
    sym.structs = consts.structs.clone();
    let cty = |t: &str| consts.cstructs.ty(t);
    let params = sym.params(&src.name.name)?;
    let is_main = src.name.name.as_ref() == "main" && params.is_empty() && ret.is_none();
    let c_params: Vec<_> = if is_main {
        let want = [("argc", "int"), ("argv", "char**")];
        let same = c.params.len() == 2
            && c.params
                .iter()
                .zip(want)
                .all(|(p, w)| p.0 == w.0 && p.1 == w.1);
        if !same {
            return Err("mismatch: C `main` does not take `argc, argv`".into());
        }
        Vec::new()
    } else {
        c.params.clone()
    };
    if c_params.len() != params.len() {
        return Err("mismatch: parameter count differs".into());
    }
    for ((n, t), (cn, ct)) in params.iter().zip(&c_params) {
        if n != cn || cty(ct) != Some(*t) {
            return Err(format!(
                "mismatch: parameter `{n}: {t}` is `{ct} {cn}` in C"
            ));
        }
    }
    let c_ret_ok = match ret {
        Some(r) => cty(&c.ret) == Some(r),
        None if is_main => c.ret == "int32_t",
        None => c.ret == "void",
    };
    if !c_ret_ok {
        return Err(format!(
            "mismatch: returns {} but C returns `{}`",
            ret.map_or("()".to_string(), |r| r.to_string()),
            c.ret
        ));
    }
    if let Some(m) = other_macro(body) {
        return Err(format!("macro `{m}!` outside the subset"));
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
    let cexec = CExec::new(c, &cuts, callees, consts)?;
    let mut sx = SrcExec {
        sym,
        body,
        loops: src_loops,
        ret,
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
        let mut s_st = St::new(Vec::new(), Vec::new(), ret);
        let mut c_st = St::new(Vec::new(), Vec::new(), ret);
        let mut c_bound: HashSet<String> = HashSet::new();
        let st = &consts.structs;
        for (n, t) in &related {
            let v = st.fresh(n, *t);
            base.push(st.in_range(&v, *t));
            s_st.vars.push((n.clone(), v.clone(), *t));
            let cn = hints.vars.get(n).cloned().unwrap_or_else(|| n.clone());
            let ct = cexec
                .ty_of(&cn)
                .ok_or(format!("source `{n}` has no C variable `{cn}`"))?;
            base.push(st.in_range(&v, ct));
            c_st.vars.push((cn.clone(), v, ct));
            c_bound.insert(cn);
        }
        for (n, t) in c_params.iter().chain(&c.locals) {
            if !c_bound.contains(n) && !super::c_effects::is_string_ty(t) {
                let ty = cty(t).ok_or("C local type")?;
                c_st.vars
                    .push((n.clone(), st.fresh(&format!("c${n}"), ty), ty));
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
        super::trace::check(base.len(), &facts, &s_paths, &c_paths, &at)?;
        for (ss, se) in &s_paths {
            if let SExit::Cut(k) = se {
                todo.push(Some(*k));
            }
            for (cs, ce) in &c_paths {
                let mut hyps = ss.hyps();
                hyps.extend(cs.conds[base.len().min(cs.conds.len())..].iter().cloned());
                hyps.extend(cs.facts.iter().cloned());
                let goal = match (se, ce) {
                    _ if ss.trace.len() != cs.trace.len() => Term::Bool(false),
                    (SExit::Ret(v), CExit::Ret(Some(w))) if ret.is_some() => {
                        st.eq(v, w, ret.unwrap())
                    }
                    (SExit::Ret(_), CExit::Ret(Some(w))) if is_main => {
                        cmp(COp::Eq, w.clone(), Term::Int(0))
                    }
                    (SExit::Ret(_), CExit::Ret(None)) if ret.is_none() && !is_main => {
                        Term::Bool(true)
                    }
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
                            g = and(g, st.eq(&sv, &cv, *t));
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

/// The first macro in `body` other than the print macros. Others (`vec!`,
/// `assert!`) may expand to loops and calls the source does not show.
fn other_macro(body: &crate::ast::Block) -> Option<String> {
    use crate::ast::{ExprKind, StmtKind};
    use crate::verify::infer::children;
    fn expr(e: &crate::ast::Expr) -> Option<String> {
        if let ExprKind::Macro { path, .. } = &e.kind {
            let n = path.segments.last()?.ident.name.to_string();
            if !matches!(n.as_str(), "print" | "println" | "eprint" | "eprintln") {
                return Some(n);
            }
        }
        children(e).into_iter().find_map(expr)
    }
    body.stmts.iter().find_map(|s| match &s.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => expr(e),
        StmtKind::Local(l) => l.init.as_ref().and_then(|i| expr(&i.expr)),
        _ => None,
    })
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
