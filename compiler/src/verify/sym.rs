// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Symbolic execution of the loop-free subset.
//!
//! Every path through a function becomes a list of path conditions and a
//! result term. A plain arithmetic step adds the fact that its exact result
//! lies in its type's range: on a path that finishes, the step did not trap.
//! Calls are inlined; a recursive call is replaced by an uninterpreted term,
//! constrained by the callee's `ensures` when it has one (induction over the
//! call depth of finishing runs). Each call to a function with `requires`
//! records a side obligation that the clause holds at that call.

use super::infer::{is_float_lit, let_ty_from_uses, static_ty, Sigs};
use super::term::*;
use super::ty::Ty;
use crate::ast::*;
use crate::parser::{ENSURES_PREFIX, REQUIRES_PREFIX};
use std::collections::HashMap;

/// An obligation raised at a call site.
#[derive(Clone, Debug)]
pub struct SideOb {
    pub label: String,
    pub hyps: Vec<Term>,
    pub goal: Term,
}

/// A path state.
#[derive(Clone, Debug)]
pub struct St {
    pub conds: Vec<Term>,
    /// Facts true on every input (the range of a bit operation, a cast or a
    /// call's result). They never decide whether a path traps.
    pub facts: Vec<Term>,
    pub vars: Vec<(String, Term, Ty)>,
    pub ret: Option<Ty>,
}

impl St {
    pub fn ty_of(&self, n: &str) -> Option<Ty> {
        self.vars.iter().rev().find(|v| v.0 == n).map(|v| v.2)
    }
    pub(super) fn get(&self, n: &str) -> Option<(Term, Ty)> {
        self.vars
            .iter()
            .rev()
            .find(|v| v.0 == n)
            .map(|v| (v.1.clone(), v.2))
    }
    pub(super) fn set(&mut self, n: &str, t: Term) -> bool {
        match self.vars.iter_mut().rev().find(|v| v.0 == n) {
            Some(v) => {
                v.1 = t;
                true
            }
            None => false,
        }
    }
    /// Record a fact that holds for every input.
    pub fn fact(&mut self, t: Term) {
        if t != Term::Bool(true) && !self.facts.contains(&t) {
            self.facts.push(t);
        }
    }

    /// The path conditions and the facts, as one hypothesis list.
    pub fn hyps(&self) -> Vec<Term> {
        let mut h = self.conds.clone();
        h.extend(self.facts.iter().cloned());
        h
    }

    pub fn assume(&mut self, t: Term) {
        if t != Term::Bool(true) && !self.conds.contains(&t) {
            self.conds.push(t);
        }
    }
}

/// What a path produced.
#[derive(Clone, Debug)]
pub enum Out {
    /// A value (`None` type for unit).
    Val(Term, Option<Ty>),
    /// A `return`.
    Ret(Term, Option<Ty>),
    /// A `break` out of the innermost loop (translation validation only).
    Break,
    /// A `continue` of the innermost loop (translation validation only).
    Continue,
}

pub type Res = Result<Vec<(St, Out)>, String>;

/// The symbolic executor.
pub struct Sym<'a> {
    pub fns: &'a HashMap<String, &'a FnDef>,
    pub sigs: &'a Sigs,
    pub stack: Vec<String>,
    pub side: Vec<SideOb>,
    pub max_paths: usize,
    /// Inline calls (verification) or keep each call as an uninterpreted
    /// term constrained only to its return type's range (translation
    /// validation, where both sides call the same function).
    pub inline_calls: bool,
    /// Admit `f32` and `f64` as opaque IEEE values (translation validation
    /// only; `buildc verify` keeps floats outside its subset).
    pub floats: bool,
    /// Named constants by value (translation validation).
    pub consts: HashMap<String, (Term, Ty)>,
}

impl<'a> Sym<'a> {
    pub fn new(fns: &'a HashMap<String, &'a FnDef>, sigs: &'a Sigs) -> Self {
        Sym {
            fns,
            sigs,
            stack: Vec::new(),
            side: Vec::new(),
            max_paths: 2048,
            inline_calls: true,
            floats: false,
            consts: HashMap::new(),
        }
    }

    /// The subset type of an AST type under this executor's mode.
    pub fn ty_of_ast(&self, t: &Type) -> Option<Ty> {
        if self.floats {
            Ty::from_ast_tv(t)
        } else {
            Ty::from_ast(t)
        }
    }

    /// The parameters of `name` as typed variables.
    pub fn params(&self, name: &str) -> Result<Vec<(String, Ty)>, String> {
        let f = self.fns.get(name).ok_or(format!("no function `{name}`"))?;
        f.sig
            .params
            .iter()
            .map(|p| {
                let PatternKind::Ident { name: n, .. } = &p.pattern.kind else {
                    return Err("a pattern parameter".to_string());
                };
                let ty = self.ty_of_ast(&p.ty).ok_or(format!(
                    "parameter `{}` has a type outside the subset",
                    n.name
                ))?;
                Ok((n.name.to_string(), ty))
            })
            .collect()
    }

    /// Run `name` on `args` from state `st`; results come back as values in
    /// states that keep the caller's variables.
    pub fn call(&mut self, name: &str, args: Vec<Term>, st: St) -> Res {
        let f = *self
            .fns
            .get(name)
            .ok_or(format!("call to `{name}` outside the subset"))?;
        let params = self.params(name)?;
        let ret = f.sig.return_ty.as_deref().and_then(|t| self.ty_of_ast(t));
        if f.sig.return_ty.is_some() && ret.is_none() {
            return Err(format!("`{name}` returns a type outside the subset"));
        }
        let body = f.body.as_deref().ok_or(format!("`{name}` has no body"))?;
        let mut inner = St {
            conds: st.conds.clone(),
            facts: st.facts.clone(),
            vars: params
                .iter()
                .zip(args)
                .map(|((n, t), a)| (n.clone(), a, *t))
                .collect(),
            ret,
        };
        for (n, t) in &params {
            let (v, _) = inner.get(n).unwrap();
            inner.fact(in_range(&v, *t));
        }
        self.stack.push(name.to_string());
        let res = self.block(body, inner, ret);
        self.stack.pop();
        let mut out = Vec::new();
        for (s, o) in res? {
            let (Out::Val(v, t) | Out::Ret(v, t)) = o else {
                return Err(format!("`break` or `continue` escapes `{name}`"));
            };
            let back = St {
                conds: s.conds,
                facts: s.facts,
                vars: st.vars.clone(),
                ret: st.ret,
            };
            out.push((back, Out::Val(v, t)));
        }
        Ok(out)
    }

    /// The paths of a call, honouring contracts and recursion.
    pub(super) fn user_call(&mut self, name: &str, args: Vec<Term>, st: St) -> Res {
        let req = format!("{REQUIRES_PREFIX}{name}");
        if self.inline_calls && self.fns.contains_key(&req) && !self.stack.contains(&req) {
            for (s, o) in self.call(&req, args.clone(), st.clone())? {
                let Out::Val(goal, _) = o else { continue };
                self.side.push(SideOb {
                    label: format!("call to `{name}` meets its requires"),
                    hyps: s.hyps(),
                    goal,
                });
            }
        }
        let recursive =
            !self.inline_calls || self.stack.iter().any(|s| s == name) || self.stack.len() > 24;
        if !recursive {
            return self.call(name, args, st);
        }
        let ret = self.sigs.fns.get(name).and_then(|s| s.1);
        let Some(rty) = ret else {
            return Err(format!("recursive `{name}` without a subset return type"));
        };
        let app = Term::App(name.to_string(), args.clone());
        let mut st = st;
        st.fact(in_range(&app, rty));
        let ens = format!("{ENSURES_PREFIX}{name}");
        if !self.inline_calls || !self.fns.contains_key(&ens) || self.stack.contains(&ens) {
            return Ok(vec![(st, Out::Val(app, Some(rty)))]);
        }
        let mut eargs = args;
        eargs.push(app.clone());
        let mut out = Vec::new();
        for (mut s, o) in self.call(&ens, eargs, st)? {
            let Out::Val(fact, _) = o else { continue };
            s.assume(fact);
            out.push((s, Out::Val(app.clone(), Some(rty))));
        }
        Ok(out)
    }

    pub fn block(&mut self, b: &Block, st: St, expect: Option<Ty>) -> Res {
        let depth = st.vars.len();
        let mut live: Vec<(St, Out)> = vec![(st, Out::Val(Term::Int(0), None))];
        let mut done = Vec::new();
        for (i, s) in b.stmts.iter().enumerate() {
            let tail = i + 1 == b.stmts.len();
            let mut next = Vec::new();
            for (state, _) in live {
                let res = self.stmt(s, &b.stmts[i + 1..], state, tail, expect)?;
                for (s2, o) in res {
                    match o {
                        Out::Ret(..) | Out::Break | Out::Continue => done.push((s2, o)),
                        v => next.push((s2, v)),
                    }
                }
            }
            if next.len() + done.len() > self.max_paths {
                return Err("too many paths".into());
            }
            live = next;
        }
        let ends_in_expr = matches!(b.stmts.last().map(|s| &s.kind), Some(StmtKind::Expr(_)));
        for (mut s, o) in live {
            s.vars.truncate(depth);
            let o = if ends_in_expr {
                o
            } else {
                Out::Val(Term::Int(0), None)
            };
            done.push((s, o));
        }
        for (s, _) in done.iter_mut() {
            s.vars.truncate(depth.min(s.vars.len()));
        }
        Ok(done)
    }

    /// Run one statement; `rest` is what follows it in its block (for the
    /// types of unannotated `let`s), `tail` whether it is the block's value.
    pub fn stmt(
        &mut self,
        s: &Stmt,
        rest: &[Stmt],
        state: St,
        tail: bool,
        expect: Option<Ty>,
    ) -> Res {
        Ok(match &s.kind {
            StmtKind::Local(l) => self.local(l, rest, state)?,
            StmtKind::Expr(e) => self.expr(e, state, if tail { expect } else { None })?,
            StmtKind::Semi(e) => self
                .expr(e, state, None)?
                .into_iter()
                .map(|(s, o)| match o {
                    Out::Val(..) => (s, Out::Val(Term::Int(0), None)),
                    r => (s, r),
                })
                .collect(),
            StmtKind::Empty => vec![(state, Out::Val(Term::Int(0), None))],
            _ => return Err("an item or macro statement".into()),
        })
    }

    fn local(&mut self, l: &Local, rest: &[Stmt], st: St) -> Res {
        let PatternKind::Ident { name, .. } = &l.pattern.kind else {
            return Err("a let pattern".into());
        };
        let init = l.init.as_ref().ok_or("a let without a value")?;
        let ty = {
            let env = |n: &str| st.ty_of(n).or_else(|| self.consts.get(n).map(|c| c.1));
            match l.ty.as_deref() {
                Some(t) => Some(self.ty_of_ast(t).ok_or("a let type outside the subset")?),
                None => static_ty(&init.expr, &env, self.sigs),
            }
            .unwrap_or_else(|| {
                let fallback = if self.floats && is_float_lit(&init.expr) {
                    Ty::F64
                } else {
                    Ty::I32
                };
                let_ty_from_uses(&name.name, rest, &env, self.sigs, st.ret, fallback)
            })
        };
        let mut out = Vec::new();
        for (mut s, o) in self.expr(&init.expr, st, Some(ty))? {
            match o {
                Out::Val(v, _) => {
                    s.vars.push((name.name.to_string(), v, ty));
                    out.push((s, Out::Val(Term::Int(0), None)));
                }
                r => out.push((s, r)),
            }
        }
        Ok(out)
    }

    pub fn expr(&mut self, e: &Expr, st: St, expect: Option<Ty>) -> Res {
        super::sym_expr::expr(self, e, st, expect)
    }
}
