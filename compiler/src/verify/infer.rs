// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Local type inference for the verification subset.
//!
//! The checker runs on the AST, so it settles the type of every integer
//! expression itself: from declared parameter and `let` types, literal
//! suffixes, casts and callee signatures. An unsuffixed literal takes the type
//! of the other operand; an unannotated `let` bound to one takes the type of
//! the first typed use of its name later in the block. This mirrors the
//! language's inference without sharing code with the type checker or the
//! backend.

use super::ty::Ty;
use crate::ast::*;
use std::collections::HashMap;

/// Signatures of the functions the subset can call.
#[derive(Default)]
pub struct Sigs {
    pub fns: HashMap<String, (Vec<Option<Ty>>, Option<Ty>)>,
}

impl Sigs {
    pub fn from_module(items: &[Item]) -> Sigs {
        let mut fns = HashMap::new();
        for item in items {
            if let ItemKind::Function(f) = &item.kind {
                let params = f.sig.params.iter().map(|p| Ty::from_ast(&p.ty)).collect();
                let ret = f.sig.return_ty.as_deref().and_then(Ty::from_ast);
                fns.insert(f.name.name.to_string(), (params, ret));
            }
        }
        Sigs { fns }
    }
}

/// The name an expression calls, when it is a plain function name.
pub fn callee_name(func: &Expr) -> Option<String> {
    match &func.kind {
        ExprKind::Ident(i) => Some(i.name.to_string()),
        ExprKind::Path(p) if p.segments.len() == 1 => Some(p.segments[0].ident.name.to_string()),
        _ => None,
    }
}

/// The name an identifier-like expression reads.
pub fn var_name(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(i) => Some(i.name.to_string()),
        ExprKind::Path(p) if p.segments.len() == 1 => Some(p.segments[0].ident.name.to_string()),
        ExprKind::Paren(inner) => var_name(inner),
        _ => None,
    }
}

/// `T::MAX` / `T::MIN` for an integer type `T`.
pub fn assoc_const(e: &Expr) -> Option<(Ty, i128)> {
    let ExprKind::Path(p) = &e.kind else {
        return None;
    };
    if p.segments.len() != 2 {
        return None;
    }
    let ty = Ty::from_name(&p.segments[0].ident.name)?;
    let (lo, hi) = ty.range()?;
    match &*p.segments[1].ident.name {
        "MAX" => Some((ty, hi)),
        "MIN" => Some((ty, lo)),
        _ => None,
    }
}

/// The type of `e` where it is settled without context; `None` for an
/// unsuffixed literal or an expression outside the subset.
pub fn static_ty(e: &Expr, env: &dyn Fn(&str) -> Option<Ty>, sigs: &Sigs) -> Option<Ty> {
    match &e.kind {
        ExprKind::Literal(Literal::Int { suffix, .. }) => suffix.and_then(Ty::from_suffix),
        ExprKind::Literal(Literal::Bool(_)) => Some(Ty::Bool),
        ExprKind::Ident(_) | ExprKind::Path(_) => {
            if let Some((ty, _)) = assoc_const(e) {
                return Some(ty);
            }
            env(&var_name(e)?)
        }
        ExprKind::Paren(inner) => static_ty(inner, env, sigs),
        ExprKind::Cast { ty, .. } => Ty::from_ast(ty),
        ExprKind::Unary { op, expr } => match op {
            UnaryOp::Not | UnaryOp::Neg | UnaryOp::BitNot => static_ty(expr, env, sigs),
            _ => None,
        },
        ExprKind::Binary { op, left, right } => match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Some(Ty::Bool),
            BinOp::And | BinOp::Or => Some(Ty::Bool),
            BinOp::Shl | BinOp::Shr => static_ty(left, env, sigs),
            _ => static_ty(left, env, sigs).or_else(|| static_ty(right, env, sigs)),
        },
        ExprKind::Call { func, .. } => sigs.fns.get(&callee_name(func)?)?.1,
        ExprKind::MethodCall {
            receiver, method, ..
        } => match &*method.name {
            "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "abs" | "min" | "max" => {
                static_ty(receiver, env, sigs)
            }
            _ => None,
        },
        ExprKind::If {
            then_branch,
            else_branch,
            ..
        } => block_tail(then_branch)
            .and_then(|t| static_ty(t, env, sigs))
            .or_else(|| else_branch.as_deref().and_then(|e| static_ty(e, env, sigs))),
        ExprKind::Block(b) => block_tail(b).and_then(|t| static_ty(t, env, sigs)),
        _ => None,
    }
}

/// The tail expression of a block, if it ends in one.
pub fn block_tail(b: &Block) -> Option<&Expr> {
    match b.stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Expr(e)) => Some(e),
        _ => None,
    }
}

/// The type of an unannotated `let name = <literal>`, read from the first
/// typed use of `name` in `rest`; `fallback` when no use settles it.
pub fn let_ty_from_uses(
    name: &str,
    rest: &[Stmt],
    env: &dyn Fn(&str) -> Option<Ty>,
    sigs: &Sigs,
    fn_ret: Option<Ty>,
    fallback: Ty,
) -> Ty {
    let mut found = None;
    for s in rest {
        let e = match &s.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => e,
            StmtKind::Local(l) => match &l.init {
                Some(init) => &init.expr,
                None => continue,
            },
            _ => continue,
        };
        if matches!(&s.kind, StmtKind::Expr(_)) && var_name(e).as_deref() == Some(name) {
            if let Some(t) = fn_ret {
                return t;
            }
        }
        visit_uses(e, name, env, sigs, fn_ret, &mut found);
        if let Some(t) = found {
            return t;
        }
    }
    fallback
}

fn visit_uses(
    e: &Expr,
    name: &str,
    env: &dyn Fn(&str) -> Option<Ty>,
    sigs: &Sigs,
    fn_ret: Option<Ty>,
    found: &mut Option<Ty>,
) {
    if found.is_some() {
        return;
    }
    let is_name = |x: &Expr| var_name(x).as_deref() == Some(name);
    match &e.kind {
        ExprKind::Binary { op, left, right } if !matches!(op, BinOp::And | BinOp::Or) => {
            if is_name(left) && !matches!(op, BinOp::Shl | BinOp::Shr) {
                *found = static_ty(right, env, sigs);
            } else if is_name(right) && !matches!(op, BinOp::Shl | BinOp::Shr) {
                *found = static_ty(left, env, sigs);
            }
        }
        ExprKind::Assign { target, value, .. } => {
            if is_name(target) {
                *found = static_ty(value, env, sigs);
            } else if is_name(value) {
                *found = static_ty(target, env, sigs);
            }
        }
        ExprKind::Call { func, args } => {
            if let Some((ptys, _)) = callee_name(func).and_then(|n| sigs.fns.get(&n)) {
                for (a, t) in args.iter().zip(ptys) {
                    if is_name(a) && t.is_some() {
                        *found = *t;
                        return;
                    }
                }
            }
        }
        ExprKind::Return(Some(v)) if is_name(v) => *found = fn_ret,
        _ => {}
    }
    if found.is_some() {
        return;
    }
    for child in children(e) {
        visit_uses(child, name, env, sigs, fn_ret, found);
    }
}

/// The direct sub-expressions of `e` (statements of nested blocks included).
pub fn children(e: &Expr) -> Vec<&Expr> {
    let mut out: Vec<&Expr> = Vec::new();
    fn push_block<'b>(b: &'b Block, out: &mut Vec<&'b Expr>) {
        for s in &b.stmts {
            match &s.kind {
                StmtKind::Expr(x) | StmtKind::Semi(x) => out.push(x),
                StmtKind::Local(l) => {
                    if let Some(init) = &l.init {
                        out.push(&init.expr);
                    }
                }
                _ => {}
            }
        }
    }
    match &e.kind {
        ExprKind::Unary { expr, .. } | ExprKind::Paren(expr) | ExprKind::Cast { expr, .. } => {
            out.push(expr)
        }
        ExprKind::Binary { left, right, .. } => {
            out.push(left);
            out.push(right);
        }
        ExprKind::Assign { target, value, .. } => {
            out.push(target);
            out.push(value);
        }
        ExprKind::Call { args, .. } => out.extend(args.iter()),
        ExprKind::MethodCall { receiver, args, .. } => {
            out.push(receiver);
            out.extend(args.iter());
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            out.push(condition);
            push_block(then_branch, &mut out);
            if let Some(e) = else_branch {
                out.push(e);
            }
        }
        ExprKind::While {
            condition, body, ..
        } => {
            out.push(condition);
            push_block(body, &mut out);
        }
        ExprKind::Loop { body, .. } | ExprKind::Block(body) => push_block(body, &mut out),
        ExprKind::Match { scrutinee, arms } => {
            out.push(scrutinee);
            for a in arms {
                out.push(&a.body);
            }
        }
        ExprKind::Return(Some(v)) => out.push(v),
        ExprKind::Break { value: Some(v), .. } => out.push(v),
        _ => {}
    }
    out
}
