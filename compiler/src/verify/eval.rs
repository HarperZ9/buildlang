// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! A concrete evaluator for the subset, with the language's trapping
//! arithmetic. It confirms every counterexample before one is reported, so a
//! refutation never rests on the symbolic engine alone.

use super::infer::{let_ty_from_uses, static_ty, Sigs};
use super::term::BOp;
use super::ty::Ty;
use crate::ast::*;
use std::collections::HashMap;

/// A value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Val {
    Int(i128, Ty),
    Bool(bool),
    Unit,
}

impl Val {
    pub fn int(self) -> Option<i128> {
        match self {
            Val::Int(v, _) => Some(v),
            Val::Bool(b) => Some(b as i128),
            Val::Unit => None,
        }
    }
}

/// Why evaluation stopped without a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The program traps here (overflow, division by zero).
    Trap(String),
    /// A call's `requires` clause is false.
    Requires(String),
    /// The fuel ran out (a long or endless loop or recursion).
    Fuel,
    /// The construct is outside the subset.
    Unsupported(String),
}

pub(super) enum Flow {
    Val(Val),
    Ret(Val),
    Break,
    Continue,
}

pub(super) type R<T> = Result<T, Stop>;

/// Apply a bit-level operator to constants; `None` when undefined.
pub fn bit_op(op: BOp, ty: Ty, x: i128, y: i128) -> Option<i128> {
    let bits = ty.bits() as i128;
    let ux = |v: i128| (v as u128) & ((1u128 << bits) - 1);
    Some(match op {
        BOp::And => ty.wrap(x & y),
        BOp::Or => ty.wrap(x | y),
        BOp::Xor => ty.wrap(x ^ y),
        BOp::Shl => ty.wrap(x << (y & (bits - 1))),
        BOp::Shr => {
            let s = y & (bits - 1);
            match ty {
                Ty::Int { signed: true, .. } => x >> s,
                _ => (ux(x) >> s) as i128,
            }
        }
        BOp::WAdd => ty.wrap(x + y),
        BOp::WMul => ty.wrap(x.wrapping_mul(y)),
    })
}

/// The evaluator.
pub struct Eval<'a> {
    pub fns: &'a HashMap<String, &'a FnDef>,
    pub sigs: &'a Sigs,
    pub fuel: u64,
    /// Check `requires` clauses at every call.
    pub check_requires: bool,
    /// Current call depth; a run deeper than [`MAX_DEPTH`] stops as out of fuel.
    pub depth: u32,
}

/// The deepest call chain a concrete run may make.
pub const MAX_DEPTH: u32 = 2000;

pub(super) struct Frame {
    vars: Vec<(String, Val, Ty)>,
    ret: Option<Ty>,
}

impl Frame {
    pub(super) fn get(&self, n: &str) -> Option<(Val, Ty)> {
        self.vars
            .iter()
            .rev()
            .find(|(k, _, _)| k == n)
            .map(|(_, v, t)| (*v, *t))
    }
    pub(super) fn set(&mut self, n: &str, v: Val) -> R<()> {
        match self.vars.iter_mut().rev().find(|(k, _, _)| k == n) {
            Some(slot) => {
                slot.1 = v;
                Ok(())
            }
            None => Err(Stop::Unsupported(format!("assignment to unknown `{n}`"))),
        }
    }
}

impl<'a> Eval<'a> {
    /// Call function `name` on `args`.
    pub fn call(&mut self, name: &str, args: &[Val]) -> R<Val> {
        if !self.fns.contains_key(name) {
            return Err(Stop::Unsupported(format!(
                "call to `{name}` outside the subset"
            )));
        }
        if self.fuel == 0 || self.depth >= MAX_DEPTH {
            return Err(Stop::Fuel);
        }
        self.fuel -= 1;
        self.depth += 1;
        let out = self.call_checked(name, args);
        self.depth -= 1;
        out
    }

    fn call_checked(&mut self, name: &str, args: &[Val]) -> R<Val> {
        if self.check_requires {
            let req = format!("{}{name}", crate::parser::REQUIRES_PREFIX);
            if self.fns.contains_key(&req) {
                let ok = self.call_raw(&req, args)?;
                if ok == Val::Bool(false) {
                    return Err(Stop::Requires(name.to_string()));
                }
            }
        }
        self.call_raw(name, args)
    }

    fn call_raw(&mut self, name: &str, args: &[Val]) -> R<Val> {
        let f = self.fns[name];
        let mut frame = Frame {
            vars: Vec::new(),
            ret: f.sig.return_ty.as_deref().and_then(Ty::from_ast),
        };
        if f.sig.params.len() != args.len() {
            return Err(Stop::Unsupported(format!("arity of `{name}`")));
        }
        for (p, a) in f.sig.params.iter().zip(args) {
            let PatternKind::Ident { name: pn, .. } = &p.pattern.kind else {
                return Err(Stop::Unsupported("pattern parameter".into()));
            };
            let ty = Ty::from_ast(&p.ty)
                .ok_or_else(|| Stop::Unsupported(format!("parameter type of `{name}`")))?;
            frame.vars.push((pn.name.to_string(), *a, ty));
        }
        let body = f
            .body
            .as_deref()
            .ok_or_else(|| Stop::Unsupported(format!("`{name}` has no body")))?;
        let ret = frame_ret(&frame);
        match self.block(body, &mut frame, ret)? {
            Flow::Val(v) | Flow::Ret(v) => Ok(v),
            _ => Err(Stop::Unsupported("break outside a loop".into())),
        }
    }

    fn block(&mut self, b: &Block, fr: &mut Frame, expect: Option<Ty>) -> R<Flow> {
        let depth = fr.vars.len();
        let mut last = Val::Unit;
        for (i, s) in b.stmts.iter().enumerate() {
            let is_tail = i + 1 == b.stmts.len();
            let flow = match &s.kind {
                StmtKind::Local(l) => {
                    self.local(l, &b.stmts[i + 1..], fr)?;
                    Flow::Val(Val::Unit)
                }
                StmtKind::Expr(e) => self.expr(e, fr, if is_tail { expect } else { None })?,
                StmtKind::Semi(e) => match self.expr(e, fr, None)? {
                    Flow::Val(_) => Flow::Val(Val::Unit),
                    other => other,
                },
                StmtKind::Empty => Flow::Val(Val::Unit),
                _ => return Err(Stop::Unsupported("item or macro statement".into())),
            };
            match flow {
                Flow::Val(v) => last = if is_tail { v } else { Val::Unit },
                other => {
                    fr.vars.truncate(depth);
                    return Ok(other);
                }
            }
        }
        if !matches!(b.stmts.last().map(|s| &s.kind), Some(StmtKind::Expr(_))) {
            last = Val::Unit;
        }
        fr.vars.truncate(depth);
        Ok(Flow::Val(last))
    }

    fn local(&mut self, l: &Local, rest: &[Stmt], fr: &mut Frame) -> R<()> {
        let PatternKind::Ident { name, .. } = &l.pattern.kind else {
            return Err(Stop::Unsupported("let pattern".into()));
        };
        let init = l
            .init
            .as_ref()
            .ok_or_else(|| Stop::Unsupported("let without a value".into()))?;
        let env = |n: &str| fr.get(n).map(|(_, t)| t);
        let ty = match l.ty.as_deref() {
            Some(t) => Some(Ty::from_ast(t).ok_or_else(|| Stop::Unsupported("let type".into()))?),
            None => static_ty(&init.expr, &env, self.sigs),
        };
        let ty = ty.unwrap_or_else(|| {
            let_ty_from_uses(&name.name, rest, &env, self.sigs, fr.ret, Ty::I32)
        });
        let v = self.value(&init.expr, fr, Some(ty))?;
        fr.vars.push((name.name.to_string(), v, ty));
        Ok(())
    }

    /// Evaluate to a value, passing control flow out as an error-free Flow.
    fn value(&mut self, e: &Expr, fr: &mut Frame, expect: Option<Ty>) -> R<Val> {
        match self.expr(e, fr, expect)? {
            Flow::Val(v) => Ok(v),
            _ => Err(Stop::Unsupported("control flow inside an operand".into())),
        }
    }

    fn expr(&mut self, e: &Expr, fr: &mut Frame, expect: Option<Ty>) -> R<Flow> {
        super::eval_expr::expr(self, e, fr, expect)
    }
}

fn frame_ret(fr: &Frame) -> Option<Ty> {
    fr.ret
}

impl Frame {
    pub(super) fn push(&mut self, n: &str, v: Val, t: Ty) {
        self.vars.push((n.to_string(), v, t));
    }
    pub(super) fn depth(&self) -> usize {
        self.vars.len()
    }
    pub(super) fn truncate(&mut self, d: usize) {
        self.vars.truncate(d);
    }
    pub(super) fn ret_ty(&self) -> Option<Ty> {
        self.ret
    }
}

impl<'a> Eval<'a> {
    pub(super) fn eval_block(&mut self, b: &Block, fr: &mut Frame, expect: Option<Ty>) -> R<Flow> {
        self.block(b, fr, expect)
    }
    pub(super) fn eval_value(&mut self, e: &Expr, fr: &mut Frame, expect: Option<Ty>) -> R<Val> {
        self.value(e, fr, expect)
    }
}
