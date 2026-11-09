// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The type of an unannotated `let` from the uses that follow it, and the
//! child expressions of an expression.

use super::infer::{callee_name, static_ty, var_name, Sigs};
use super::ty::Ty;
use crate::ast::*;

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
