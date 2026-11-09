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
    /// Effects in order (translation validation): each is the number of
    /// path conditions in force when it happened, and the event term.
    pub trace: Vec<(usize, Term)>,
    /// Opaque string values by variable name (the C side's string locals).
    pub strs: Vec<(String, Term)>,
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

    /// A fresh state with these conditions and variables and no effects.
    pub fn new(conds: Vec<Term>, vars: Vec<(String, Term, Ty)>, ret: Option<Ty>) -> St {
        St {
            conds,
            facts: Vec::new(),
            vars,
            ret,
            trace: Vec::new(),
            strs: Vec::new(),
        }
    }

    /// Record an effect under the conditions in force now.
    pub fn event(&mut self, ev: Term) {
        self.trace.push((self.conds.len(), ev));
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
    /// The source text, for the arguments of print macros (translation
    /// validation, where prints are effects).
    pub source: Option<&'a str>,
    /// Structs that are values (translation validation).
    pub structs: super::agg::Structs,
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
            source: None,
            structs: super::agg::Structs::default(),
        }
    }

    /// The subset type of an AST type under this executor's mode.
    pub fn ty_of_ast(&self, t: &Type) -> Option<Ty> {
        if self.floats {
            self.structs.ty_of_ast(t)
        } else {
            Ty::from_ast(t)
        }
    }

    /// The type of a variable, a named constant, or a dotted field path of a
    /// struct variable.
    pub fn env_ty(&self, st: &St, n: &str) -> Option<Ty> {
        let mut parts = n.split('.');
        let head = parts.next()?;
        let mut ty = st
            .ty_of(head)
            .or_else(|| self.consts.get(head).map(|c| c.1))?;
        for f in parts {
            ty = self.structs.field(ty, f)?.1;
        }
        Some(ty)
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
            let env = |n: &str| self.env_ty(&st, n);
            match l.ty.as_deref() {
                Some(t) => Some(self.ty_of_ast(t).ok_or("a let type outside the subset")?),
                None => static_ty(&init.expr, &env, self.sigs)
                    .or_else(|| super::sym_agg::agg_ty(self, &init.expr, &st)),
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
