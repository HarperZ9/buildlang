// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Expression cases of the concrete evaluator.

use super::eval::{bit_op, Eval, Flow, Frame, Stop, Val, R};
use super::eval_match::{match_expr, method_call};
use super::infer::{assoc_const, callee_name, static_ty, var_name};
use super::term::BOp;
use super::ty::Ty;
use crate::ast::*;

pub(super) fn unsupported<T>(what: &str) -> R<T> {
    Err(Stop::Unsupported(what.to_string()))
}

pub(super) fn trap<T>(what: String) -> R<T> {
    Err(Stop::Trap(what))
}

pub(super) fn int_of(v: Val) -> R<i128> {
    match v {
        Val::Int(x, _) => Ok(x),
        _ => unsupported("an integer operand that is not an integer"),
    }
}

pub(super) fn bool_of(v: Val) -> R<bool> {
    match v {
        Val::Bool(b) => Ok(b),
        _ => unsupported("a condition that is not bool"),
    }
}

/// Checked arithmetic with the language's trap rules.
pub fn arith(op: BinOp, ty: Ty, x: i128, y: i128) -> R<i128> {
    let v = match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x
            .checked_mul(y)
            .ok_or_else(|| Stop::Trap(format!("{ty} multiply overflow")))?,
        BinOp::Div | BinOp::Rem => {
            if y == 0 {
                return trap(format!("{ty} division by zero"));
            }
            if op == BinOp::Div {
                x / y
            } else {
                x % y
            }
        }
        _ => {
            let bop = match op {
                BinOp::BitAnd => BOp::And,
                BinOp::BitOr => BOp::Or,
                BinOp::BitXor => BOp::Xor,
                BinOp::Shl => BOp::Shl,
                BinOp::Shr => BOp::Shr,
                _ => return unsupported("operator"),
            };
            return bit_op(bop, ty, x, y).ok_or_else(|| Stop::Unsupported("bit operator".into()));
        }
    };
    if !ty.holds(v) {
        return trap(format!("{ty} overflow"));
    }
    // `i64::MIN % -1` is representable as 0 but traps in the language.
    if op == BinOp::Rem && y == -1 && ty.range().map(|r| r.0) == Some(x) {
        return trap(format!("{ty} remainder overflow"));
    }
    Ok(v)
}

pub(super) fn expr(ev: &mut Eval<'_>, e: &Expr, fr: &mut Frame, expect: Option<Ty>) -> R<Flow> {
    let env_ty = |fr: &Frame, n: &str| fr.get(n).map(|(_, t)| t);
    let v = match &e.kind {
        ExprKind::Literal(Literal::Int { value, suffix, .. }) => {
            let ty = suffix
                .and_then(Ty::from_suffix)
                .or(expect.filter(|t| t.is_int()))
                .unwrap_or(Ty::I32);
            let v = i128::try_from(*value).map_err(|_| Stop::Unsupported("literal".into()))?;
            if !ty.holds(v) && !ty.holds(-v) {
                return unsupported("literal out of range");
            }
            Val::Int(v, ty)
        }
        ExprKind::Literal(Literal::Bool(b)) => Val::Bool(*b),
        ExprKind::Paren(inner) => return expr(ev, inner, fr, expect),
        ExprKind::Ident(_) | ExprKind::Path(_) => {
            if let Some((ty, v)) = assoc_const(e) {
                Val::Int(v, ty)
            } else {
                let n = var_name(e).ok_or_else(|| Stop::Unsupported("path".into()))?;
                fr.get(&n)
                    .map(|(v, _)| v)
                    .ok_or_else(|| Stop::Unsupported(format!("unknown name `{n}`")))?
            }
        }
        ExprKind::Unary { op, expr: inner } => {
            let v = ev.eval_value(inner, fr, expect)?;
            match (op, v) {
                (UnaryOp::Not, Val::Bool(b)) => Val::Bool(!b),
                (UnaryOp::Not | UnaryOp::BitNot, Val::Int(x, ty)) => Val::Int(ty.wrap(!x), ty),
                (UnaryOp::Neg, Val::Int(x, ty)) => {
                    // A negative literal is the literal's negation, not a trap.
                    if !ty.holds(-x) {
                        return trap(format!("{ty} negation overflow"));
                    }
                    Val::Int(-x, ty)
                }
                _ => return unsupported("unary operator"),
            }
        }
        ExprKind::Binary { op, left, right } => binary(ev, *op, left, right, fr, expect)?,
        ExprKind::Assign { op, target, value } => {
            let n = var_name(target).ok_or_else(|| Stop::Unsupported("assign target".into()))?;
            let ty = env_ty(fr, &n).ok_or_else(|| Stop::Unsupported("assign target".into()))?;
            let rhs = ev.eval_value(value, fr, Some(ty))?;
            let nv = match op.to_bin_op() {
                None => rhs,
                Some(bop) => {
                    let cur = int_of(fr.get(&n).unwrap().0)?;
                    Val::Int(arith(bop, ty, cur, int_of(rhs)?)?, ty)
                }
            };
            fr.set(&n, nv)?;
            Val::Unit
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let c = bool_of(ev.eval_value(condition, fr, Some(Ty::Bool))?)?;
            return if c {
                ev.eval_block(then_branch, fr, expect)
            } else if let Some(el) = else_branch {
                expr(ev, el, fr, expect)
            } else {
                Ok(Flow::Val(Val::Unit))
            };
        }
        ExprKind::Block(b) => return ev.eval_block(b, fr, expect),
        ExprKind::Match { scrutinee, arms } => return match_expr(ev, scrutinee, arms, fr, expect),
        ExprKind::While {
            condition, body, ..
        } => {
            loop {
                if ev.fuel == 0 {
                    return Err(Stop::Fuel);
                }
                ev.fuel -= 1;
                if !bool_of(ev.eval_value(condition, fr, Some(Ty::Bool))?)? {
                    break;
                }
                match ev.eval_block(body, fr, None)? {
                    Flow::Break => break,
                    Flow::Ret(v) => return Ok(Flow::Ret(v)),
                    _ => {}
                }
            }
            Val::Unit
        }
        ExprKind::Loop { body, .. } => {
            loop {
                if ev.fuel == 0 {
                    return Err(Stop::Fuel);
                }
                ev.fuel -= 1;
                match ev.eval_block(body, fr, None)? {
                    Flow::Break => break,
                    Flow::Ret(v) => return Ok(Flow::Ret(v)),
                    _ => {}
                }
            }
            Val::Unit
        }
        ExprKind::Break { value: None, .. } => return Ok(Flow::Break),
        ExprKind::Continue { .. } => return Ok(Flow::Continue),
        ExprKind::Return(v) => {
            let ret = fr.ret_ty();
            let v = match v {
                Some(v) => ev.eval_value(v, fr, ret)?,
                None => Val::Unit,
            };
            return Ok(Flow::Ret(v));
        }
        ExprKind::Cast { expr: inner, ty } => {
            let to = Ty::from_ast(ty).ok_or_else(|| Stop::Unsupported("cast type".into()))?;
            let v = ev.eval_value(inner, fr, None)?;
            match (v, to) {
                (Val::Int(x, _), Ty::Int { .. }) => Val::Int(to.wrap(x), to),
                (Val::Bool(b), Ty::Int { .. }) => Val::Int(b as i128, to),
                _ => return unsupported("cast"),
            }
        }
        ExprKind::Call { func, args } => {
            let name = callee_name(func).ok_or_else(|| Stop::Unsupported("callee".into()))?;
            let ptys = ev
                .sigs
                .fns
                .get(&name)
                .map(|s| s.0.clone())
                .unwrap_or_default();
            let mut vals = Vec::new();
            for (i, a) in args.iter().enumerate() {
                let want = ptys.get(i).copied().flatten();
                vals.push(ev.eval_value(a, fr, want)?);
            }
            ev.call(&name, &vals)?
        }
        ExprKind::MethodCall {
            receiver,
            method,
            args,
            ..
        } => method_call(ev, receiver, &method.name, args, fr, expect)?,
        _ => return unsupported("expression kind"),
    };
    Ok(Flow::Val(v))
}

fn operand_ty(ev: &Eval<'_>, l: &Expr, r: &Expr, fr: &Frame, expect: Option<Ty>) -> Ty {
    let env = |n: &str| fr.get(n).map(|(_, t)| t);
    static_ty(l, &env, ev.sigs)
        .or_else(|| static_ty(r, &env, ev.sigs))
        .or(expect)
        .unwrap_or(Ty::I32)
}

fn binary(
    ev: &mut Eval<'_>,
    op: BinOp,
    l: &Expr,
    r: &Expr,
    fr: &mut Frame,
    expect: Option<Ty>,
) -> R<Val> {
    match op {
        BinOp::And | BinOp::Or => {
            let a = bool_of(ev.eval_value(l, fr, Some(Ty::Bool))?)?;
            if (op == BinOp::And) != a {
                return Ok(Val::Bool(a));
            }
            return Ok(Val::Bool(bool_of(ev.eval_value(r, fr, Some(Ty::Bool))?)?));
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ty = operand_ty(ev, l, r, fr, None);
            let a = ev.eval_value(l, fr, Some(ty))?;
            let b = ev.eval_value(r, fr, Some(ty))?;
            let (x, y) = match (a, b) {
                (Val::Int(x, _), Val::Int(y, _)) => (x, y),
                (Val::Bool(x), Val::Bool(y)) => (x as i128, y as i128),
                _ => return unsupported("comparison operands"),
            };
            return Ok(Val::Bool(match op {
                BinOp::Eq => x == y,
                BinOp::Ne => x != y,
                BinOp::Lt => x < y,
                BinOp::Le => x <= y,
                BinOp::Gt => x > y,
                _ => x >= y,
            }));
        }
        _ => {}
    }
    let shift = matches!(op, BinOp::Shl | BinOp::Shr);
    let ty = if shift {
        let env = |n: &str| fr.get(n).map(|(_, t)| t);
        static_ty(l, &env, ev.sigs).or(expect).unwrap_or(Ty::I32)
    } else {
        operand_ty(ev, l, r, fr, expect)
    };
    let a = ev.eval_value(l, fr, Some(ty))?;
    if let (Val::Bool(x), BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor) = (a, op) {
        let y = bool_of(ev.eval_value(r, fr, Some(Ty::Bool))?)?;
        return Ok(Val::Bool(match op {
            BinOp::BitAnd => x & y,
            BinOp::BitOr => x | y,
            _ => x ^ y,
        }));
    }
    let b = ev.eval_value(r, fr, if shift { None } else { Some(ty) })?;
    Ok(Val::Int(arith(op, ty, int_of(a)?, int_of(b)?)?, ty))
}
