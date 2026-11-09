// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Calls on the C side: the runtime helper table, the maths functions and
//! user calls, each a call event with a finish atom.

use super::c_exec::CExec;
use super::c_parse::CExpr;
use crate::ast::BinOp;
use crate::verify::effects::{call_event, finish_atom};
use crate::verify::float;
use crate::verify::sym::St;
use crate::verify::sym_ops::apply;
use crate::verify::term::*;
use crate::verify::ty::Ty;

impl CExec<'_> {
    pub(super) fn call(
        &self,
        name: &str,
        args: &[CExpr],
        s: &mut St,
    ) -> Result<(Term, Option<Ty>), String> {
        if let Some((op, ty)) = helper(name) {
            let mut vals = Vec::new();
            for a in args {
                vals.push(self.expr(a, s, Some(ty), false)?.0);
            }
            let r = match (op, vals.as_slice()) {
                (H::Neg, [x]) => {
                    let r = neg(x.clone());
                    s.assume(in_range(&r, ty));
                    r
                }
                (H::Bin(b), [x, y]) => apply(b, ty, x.clone(), y.clone(), s)?,
                (H::Wrap(BOp::WAdd), [x, y]) => {
                    let t = bits(BOp::WAdd, ty, x.clone(), y.clone());
                    s.fact(exact_when_in_range(
                        &t,
                        arith(AOp::Add, x.clone(), y.clone()),
                        ty,
                    ));
                    t
                }
                (H::Wrap(BOp::WMul), [x, y]) => bits(BOp::WMul, ty, x.clone(), y.clone()),
                (H::WSub, [x, y]) => {
                    let t = Term::App(format!("wrapping_sub_{ty}"), vec![x.clone(), y.clone()]);
                    s.fact(in_range(&t, ty));
                    s.fact(exact_when_in_range(
                        &t,
                        arith(AOp::Sub, x.clone(), y.clone()),
                        ty,
                    ));
                    t
                }
                _ => return Err(format!("helper `{name}` arity")),
            };
            return Ok((r, Some(ty)));
        }
        if let Some(to) = name.strip_prefix("bl_f2i_").and_then(Ty::from_name) {
            let [a] = args else {
                return Err(format!("helper `{name}` arity"));
            };
            let (v, from) = self.expr(a, s, Some(Ty::F64), false)?;
            if from != Some(Ty::F64) {
                return Err(format!("`{name}` on a non-double"));
            }
            let r = float::conv(to, from, v)?;
            s.fact(in_range(&r, to));
            return Ok((r, Some(to)));
        }
        if !self.callees.contains_key(name) {
            let math = match name {
                "fmod" => Some(None),
                _ => float::c_math(name).map(|_| Some(name)),
            };
            if let Some(math) = math {
                let mut vals = Vec::new();
                for a in args {
                    let (v, t) = self.expr(a, s, Some(Ty::F64), false)?;
                    vals.push(if t == Some(Ty::F64) {
                        v
                    } else {
                        float::conv(Ty::F64, t, v)?
                    });
                }
                return Ok((
                    match math {
                        None => float::fop("rem", Ty::F64, vals),
                        Some(n) => float::math_call(n, vals),
                    },
                    Some(Ty::F64),
                ));
            }
        }
        let rty = *self
            .callees
            .get(name)
            .ok_or(format!("call to `{name}` outside the model"))?;
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.expr(a, s, None, false)?.0);
        }
        let name = self.c_names.get(name).map_or(name, String::as_str);
        s.event(call_event(name, &vals));
        s.assume(finish_atom(name, &vals));
        let Some(rty) = rty else {
            return Ok((Term::Int(0), None));
        };
        if let Ty::Struct(_) = rty {
            let mut facts = Vec::new();
            let v = self.structs.opaque(name, &vals, rty, &mut facts);
            for f in facts {
                s.fact(f);
            }
            return Ok((v, Some(rty)));
        }
        let app = Term::App(name.to_string(), vals);
        s.fact(in_range(&app, rty));
        Ok((app, Some(rty)))
    }
}

#[derive(Clone, Copy)]
enum H {
    Bin(BinOp),
    Neg,
    Wrap(BOp),
    WSub,
}

/// A wrapping step equals the exact result whenever the exact result fits
/// the type. The C backend emits the wrapping helper for a step it proved
/// cannot overflow (`codegen/backend/c/proved_checks.rs`); with this fact the
/// checker can prove the step equal to the source's trapping one on every
/// path where the source does not trap, and refuse it on any other.
fn exact_when_in_range(t: &Term, exact: Term, ty: Ty) -> Term {
    or(not(in_range(&exact, ty)), cmp(COp::Eq, t.clone(), exact))
}

/// The runtime helpers the model knows, by name: `bl_{i,u}{op}_{T}`.
fn helper(name: &str) -> Option<(H, Ty)> {
    let rest = name.strip_prefix("bl_")?;
    let (stem, suffix) = rest.rsplit_once('_')?;
    let ty = Ty::from_name(suffix)?;
    let signed = matches!(ty, Ty::Int { signed: true, .. });
    let op = match (stem, signed) {
        ("iadd", true) | ("uadd", false) => H::Bin(BinOp::Add),
        ("isub", true) | ("usub", false) => H::Bin(BinOp::Sub),
        ("imul", true) | ("umul", false) => H::Bin(BinOp::Mul),
        ("idiv", true) | ("udiv", false) => H::Bin(BinOp::Div),
        ("irem", true) | ("urem", false) => H::Bin(BinOp::Rem),
        ("ineg", true) => H::Neg,
        ("iadd_wrapping", true) | ("uadd_wrapping", false) => H::Wrap(BOp::WAdd),
        ("imul_wrapping", true) | ("umul_wrapping", false) => H::Wrap(BOp::WMul),
        ("isub_wrapping", true) | ("usub_wrapping", false) => H::WSub,
        _ => return None,
    };
    Some((op, ty))
}
