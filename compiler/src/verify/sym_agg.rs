// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Struct values on the source side of translation validation: literals,
//! field reads and field writes. See `super::agg`.

use super::agg::{agg, replace, select};
use super::infer::var_name;
use super::sym::{Out, Res, St, Sym};
use super::sym_expr::{then, val};
use super::sym_ops::apply;
use super::term::Term;
use super::ty::Ty;
use crate::ast::{AssignOp, Expr, ExprKind, FieldExpr, Ident, Path};

/// The struct type of `e` when it is evidently a struct value: a literal, a
/// struct variable, a call of a function returning a struct, or a field
/// of struct type.
pub(super) fn agg_ty(sym: &Sym<'_>, e: &Expr, st: &St) -> Option<Ty> {
    let t = match &e.kind {
        ExprKind::Paren(inner) => return agg_ty(sym, inner, st),
        ExprKind::Struct { path, .. } => {
            let n = path.segments.last()?.ident.name.to_string();
            sym.structs
                .defs
                .contains_key(&n)
                .then(|| Ty::Struct(super::agg::intern(&n)))?
        }
        ExprKind::Ident(_) | ExprKind::Path(_) => st.ty_of(&var_name(e)?)?,
        ExprKind::Call { func, .. } => {
            let n = super::infer::callee_name(func)?;
            let f = sym.fns.get(&n)?;
            sym.ty_of_ast(f.sig.return_ty.as_deref()?)?
        }
        ExprKind::Field { expr, field } => {
            let base = agg_ty(sym, expr, st)?;
            sym.structs.field(base, &field.name)?.1
        }
        _ => return None,
    };
    matches!(t, Ty::Struct(_)).then_some(t)
}

/// A struct literal: fields evaluated in the order written, the value built
/// in declaration order.
pub(super) fn literal(
    sym: &mut Sym<'_>,
    path: &Path,
    fields: &[FieldExpr],
    rest: Option<&Expr>,
    st: St,
) -> Res {
    if rest.is_some() {
        return Err("a struct update with `..` (outside the subset)".into());
    }
    let name = path
        .segments
        .last()
        .map(|s| s.ident.name.to_string())
        .ok_or("a struct path")?;
    let ty = Ty::Struct(super::agg::intern(&name));
    let defs = sym
        .structs
        .fields(ty)
        .cloned()
        .ok_or(format!("struct `{name}` outside the subset"))?;
    if fields.len() != defs.len() {
        return Err(format!("a literal of `{name}` without every field"));
    }
    let mut res: Vec<(St, Vec<(String, Term)>)> = vec![(st, Vec::new())];
    for f in fields {
        let fname = f.name.name.to_string();
        let fty = defs
            .iter()
            .find(|d| d.0 == fname)
            .map(|d| d.1)
            .ok_or(format!("`{name}` has no field `{fname}`"))?;
        let mut next = Vec::new();
        for (s, acc) in res {
            let outs = match &f.value {
                Some(v) => sym.expr(v, s, Some(fty))?,
                None => {
                    let (t, _) = s.get(&fname).ok_or(format!("unknown name `{fname}`"))?;
                    vec![(s, Out::Val(t, Some(fty)))]
                }
            };
            for (s2, o) in outs {
                let Out::Val(t, _) = o else {
                    return Err("control flow in a struct field".into());
                };
                let mut a = acc.clone();
                a.push((fname.clone(), t));
                next.push((s2, a));
            }
        }
        res = next;
    }
    let mut out = Vec::new();
    for (s, vals) in res {
        let comps = defs
            .iter()
            .map(|(n, _)| vals.iter().find(|v| v.0 == *n).map(|v| v.1.clone()))
            .collect::<Option<Vec<_>>>()
            .ok_or(format!("a literal of `{name}` without every field"))?;
        out.push((s, Out::Val(agg(&name, comps), Some(ty))));
    }
    Ok(out)
}

/// A field read.
pub(super) fn field(sym: &mut Sym<'_>, base: &Expr, f: &Ident, st: St) -> Res {
    let r = sym.expr(base, st, None)?;
    then(sym, r, |sym, s, t, ty| {
        let ty = ty.ok_or("a field of a value of unknown type")?;
        let (i, fty) = sym
            .structs
            .field(ty, &f.name)
            .ok_or(format!("field `{}` outside the subset", f.name))?;
        let v = select(&t, i).ok_or("a field of a value that is not a struct")?;
        val(s, v, Some(fty))
    })
}

/// The variable and field path of an assignment target `v.f.g`.
fn target_path(e: &Expr) -> Option<(String, Vec<String>)> {
    match &e.kind {
        ExprKind::Field { expr, field } => {
            let (v, mut p) = target_path(expr)?;
            p.push(field.name.to_string());
            Some((v, p))
        }
        ExprKind::Paren(inner) => target_path(inner),
        _ => Some((var_name(e)?, Vec::new())),
    }
}

/// `v.f.g = e` and `v.f.g op= e`: the variable gets a rebuilt value.
pub(super) fn assign_field(
    sym: &mut Sym<'_>,
    op: AssignOp,
    target: &Expr,
    value: &Expr,
    st: St,
) -> Res {
    let (var, path) = target_path(target).ok_or("an assignment target")?;
    let (_, vty) = st.get(&var).ok_or("an assignment target")?;
    let mut tys = vec![vty];
    let mut idx = Vec::new();
    for f in &path {
        let (i, t) = sym
            .structs
            .field(*tys.last().unwrap(), f)
            .ok_or(format!("field `{f}` outside the subset"))?;
        idx.push(i);
        tys.push(t);
    }
    let leaf = *tys.last().unwrap();
    let bop = op.to_bin_op();
    if leaf.is_float() && bop.is_some() {
        return Err("a compound float assignment".into());
    }
    let r = sym.expr(value, st, Some(leaf))?;
    then(sym, r, |_, mut s, t, _| {
        let (whole, _) = s.get(&var).ok_or("an assignment target")?;
        // The chain of nested values from the variable down to the leaf.
        let mut chain = vec![whole];
        for i in &idx {
            let next = select(chain.last().unwrap(), *i).ok_or("a field of a non-struct")?;
            chain.push(next);
        }
        let mut nv = match bop {
            None => t,
            Some(b) => {
                let cur = chain.last().unwrap().clone();
                apply(b, leaf, cur, t, &mut s)?
            }
        };
        for (k, i) in idx.iter().enumerate().rev() {
            nv = replace(&chain[k], *i, nv).ok_or("a field of a non-struct")?;
        }
        s.set(&var, nv);
        val(s, Term::Int(0), None)
    })
}
