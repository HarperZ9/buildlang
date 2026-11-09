// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! C expressions: the term and type each one denotes, integer and bitwise
//! operators included.

use super::c_exec::{c_ty, CExec};
use super::c_float::{c_conv, float_binary};
use super::c_parse::CExpr;
use crate::verify::float;
use crate::verify::sym::St;
use crate::verify::term::*;
use crate::verify::ty::Ty;

impl CExec<'_> {
    /// The term and type of `e`. `top` marks the whole right side of an
    /// assignment, the one place narrow-type arithmetic is accepted.
    pub(super) fn expr(
        &self,
        e: &CExpr,
        s: &mut St,
        want: Option<Ty>,
        top: bool,
    ) -> Result<(Term, Option<Ty>), String> {
        Ok(match e {
            CExpr::Int(v) => (Term::Int(*v), want.filter(|t| t.is_int())),
            CExpr::Float(text) => {
                let (body, f32) = match text.strip_suffix(['f', 'F']) {
                    Some(b) => (b, true),
                    None => (text.as_str(), false),
                };
                if f32 {
                    let v: f32 = body.parse().map_err(|_| format!("float `{text}`"))?;
                    (float::fconst32_bits(v.to_bits()), Some(Ty::F32))
                } else {
                    let v: f64 = body.parse().map_err(|_| format!("float `{text}`"))?;
                    (float::fconst(Ty::F64, v), Some(Ty::F64))
                }
            }
            CExpr::Bool(b) => (Term::Bool(*b), Some(Ty::Bool)),
            CExpr::Var(n) if self.ty_of(n).is_none() && self.globals.contains_key(n) => {
                let (t, ty) = self.globals[n].clone();
                (t, Some(ty))
            }
            CExpr::Var(n) => {
                let ty = self.ty_of(n).ok_or(format!("unknown C name `{n}`"))?;
                let v = s
                    .vars
                    .iter()
                    .rev()
                    .find(|v| v.0 == *n)
                    .map(|v| v.1.clone())
                    .ok_or(format!("unbound `{n}`"))?;
                (v, Some(ty))
            }
            CExpr::Cast(t, inner) => {
                let to = c_ty(t).ok_or("cast type")?;
                let (v, from) = self.expr(inner, s, None, false)?;
                if to.is_float() || from.is_some_and(|f| f.is_float()) {
                    let r = c_conv(to, from, v)?;
                    s.fact(in_range(&r, to));
                    return Ok((r, Some(to)));
                }
                let r = if from == Some(Ty::Bool) && to.is_int() {
                    v
                } else {
                    cast(to, from, v)
                };
                s.fact(in_range(&r, to));
                (r, Some(to))
            }
            CExpr::Un(op, inner) => {
                let (v, ty) = self.expr(inner, s, want, false)?;
                match (op, ty) {
                    ('!', Some(Ty::Bool)) => (not(v), ty),
                    ('-', Some(f @ Ty::Float { .. })) => (float::fop("neg", f, vec![v]), ty),
                    ('~', Some(t @ Ty::Int { bits, .. })) if bits >= 32 || top => {
                        let r = Term::BitNot(t, Box::new(v));
                        s.fact(in_range(&r, t));
                        (r, ty)
                    }
                    _ => return Err(format!("C unary `{op}` outside the model")),
                }
            }
            CExpr::Bin(op, a, b) => self.binary(op, a, b, s, want, top)?,
            CExpr::Call(name, args) => self.call(name, args, s)?,
            CExpr::Member(inner, f) => self.member(inner, f, s)?,
            CExpr::Compound(name, items) => self.compound(name, items, s)?,
            CExpr::Str(_) | CExpr::Cond(..) => {
                return Err("a C string expression outside the model".into())
            }
        })
    }

    pub(super) fn binary(
        &self,
        op: &str,
        a: &CExpr,
        b: &CExpr,
        s: &mut St,
        want: Option<Ty>,
        top: bool,
    ) -> Result<(Term, Option<Ty>), String> {
        if matches!(op, "&&" | "||") {
            let (x, _) = self.expr(a, s, Some(Ty::Bool), false)?;
            let (y, _) = self.expr(b, s, Some(Ty::Bool), false)?;
            return Ok((
                if op == "&&" { and(x, y) } else { or(x, y) },
                Some(Ty::Bool),
            ));
        }
        // An operand's type settles a literal on the other side.
        let hint = |e: &CExpr| match e {
            CExpr::Var(n) => self.ty_of(n),
            CExpr::Cast(t, _) => c_ty(t),
            _ => None,
        };
        let shift = matches!(op, "<<" | ">>");
        let ty = hint(a).or(if shift { None } else { hint(b) }).or(want);
        let (x, xt) = self.expr(a, s, ty, false)?;
        let (y, yt) = self.expr(b, s, if shift { None } else { ty }, false)?;
        if xt.is_some_and(|t| t.is_float()) || yt.is_some_and(|t| t.is_float()) {
            return float_binary(op, x, xt, y, yt);
        }
        let ty = xt.or(ty).ok_or("an expression of unknown type")?;
        let c = match op {
            "==" => Some(COp::Eq),
            "!=" => Some(COp::Ne),
            "<" => Some(COp::Lt),
            "<=" => Some(COp::Le),
            ">" => Some(COp::Gt),
            ">=" => Some(COp::Ge),
            _ => None,
        };
        if let Some(c) = c {
            let t = if ty == Ty::Bool {
                match c {
                    COp::Eq => iff(x, y),
                    COp::Ne => not(iff(x, y)),
                    _ => return Err("an ordering on bool".into()),
                }
            } else {
                cmp(c, x, y)
            };
            return Ok((t, Some(Ty::Bool)));
        }
        if ty == Ty::Bool {
            // C `&`, `|`, `^` on two bools give 0 or 1: the logical result.
            let t = match op {
                "&" => and(x, y),
                "|" => or(x, y),
                "^" => not(iff(x, y)),
                _ => return Err(format!("C `{op}` on bool")),
            };
            return Ok((t, Some(Ty::Bool)));
        }
        if ty.bits() < 32 && !top {
            return Err(format!("C `{op}` on a narrow type inside an expression"));
        }
        let bop = match op {
            "&" => BOp::And,
            "|" => BOp::Or,
            "^" => BOp::Xor,
            "<<" => BOp::Shl,
            ">>" => BOp::Shr,
            _ => return Err(format!("raw C arithmetic `{op}` (overflow is not checked)")),
        };
        let r = bits(bop, ty, x, y);
        s.fact(in_range(&r, ty));
        Ok((r, Some(ty)))
    }
}
