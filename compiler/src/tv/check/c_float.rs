// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Float operators and conversions on the C side.

use crate::verify::float;
use crate::verify::term::*;
use crate::verify::ty::Ty;

/// A C binary operator with a float operand: the same opaque IEEE operation
/// or predicate the source side produces. An integer literal operand converts
/// exactly as C converts it.
pub(super) fn float_binary(
    op: &str,
    x: Term,
    xt: Option<Ty>,
    y: Term,
    yt: Option<Ty>,
) -> Result<(Term, Option<Ty>), String> {
    let ty = match (xt, yt) {
        (Some(a), Some(b)) if a.is_float() && b.is_float() && a != b => {
            return Err("C arithmetic on mixed float widths".into())
        }
        (Some(a), _) if a.is_float() => a,
        (_, Some(b)) => b,
        _ => return Err("a float expression of unknown type".into()),
    };
    let lift = |v: Term, t: Option<Ty>| -> Result<Term, String> {
        match t {
            Some(t) if t == ty => Ok(v),
            None if matches!(v, Term::Int(_)) => float::conv(ty, None, v),
            _ => Err("C converts an integer operand inside a float expression".into()),
        }
    };
    let (x, y) = (lift(x, xt)?, lift(y, yt)?);
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
        return Ok((float::fcmp(c, ty, x, y), Some(Ty::Bool)));
    }
    let name = match op {
        "+" => "add",
        "-" => "sub",
        "*" => "mul",
        "/" => "div",
        _ => return Err(format!("C `{op}` on a float")),
    };
    Ok((float::fop(name, ty, vec![x, y]), Some(ty)))
}

/// A conversion C performs (a cast, an assignment, a return). C turns a float
/// into an integer by truncation with undefined behaviour out of range, which
/// is not the source's saturating cast, so that direction is refused; the
/// backend converts through `bl_f2i_T` instead.
pub(super) fn c_conv(to: Ty, from: Option<Ty>, v: Term) -> Result<Term, String> {
    if to.is_int() && from.is_some_and(|f| f.is_float()) {
        return Err("C converts a float to an integer directly".into());
    }
    float::conv(to, from, v)
}
