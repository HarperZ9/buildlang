// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Symbolic execution of a parsed C function between cut labels.
//!
//! The meaning of each runtime helper is written here as a table, from the
//! helper's C source in the generated prelude: `bl_iadd_T` is `+` that traps
//! outside `T`, `bl_idiv_T` traps on a zero divisor, and so on. A path that
//! traps ends, so each finishing path carries the facts that its steps stayed
//! in range, exactly as the source side does. Arithmetic on a type narrower
//! than `int` is accepted only as the whole right side of an assignment,
//! where C's promotion and the store's truncation give the modelled value.

use super::c_parse::{CExpr, CFunc, CStmt};
use crate::ast::BinOp;
use crate::verify::sym::St;
use crate::verify::sym_ops::apply;
use crate::verify::term::*;
use crate::verify::ty::Ty;
use std::collections::{HashMap, HashSet};

/// Where a C path left the segment.
#[derive(Clone, Debug)]
pub enum CExit {
    Cut(String),
    Ret(Option<Term>),
}

/// The C type name of a scalar, as a subset type.
pub fn c_ty(name: &str) -> Option<Ty> {
    match name {
        "bool" => Some(Ty::Bool),
        _ => {
            let core = name.strip_suffix("_t")?;
            let (signed, bits) = match core.strip_prefix("uint") {
                Some(b) => (false, b),
                None => (true, core.strip_prefix("int")?),
            };
            Ty::from_name(&format!("{}{}", if signed { "i" } else { "u" }, bits))
        }
    }
}

pub struct CExec<'a> {
    pub f: &'a CFunc,
    pub cuts: &'a HashSet<String>,
    /// Return types of the user functions the body may call.
    pub callees: &'a HashMap<String, Ty>,
    labels: HashMap<String, usize>,
    types: HashMap<String, Ty>,
}

type Paths = Vec<(St, CExit)>;

impl<'a> CExec<'a> {
    pub fn new(
        f: &'a CFunc,
        cuts: &'a HashSet<String>,
        callees: &'a HashMap<String, Ty>,
    ) -> Result<Self, String> {
        let mut labels = HashMap::new();
        for (i, s) in f.body.iter().enumerate() {
            if let CStmt::Label(l) = s {
                labels.insert(l.clone(), i);
            }
        }
        let mut types = HashMap::new();
        for (n, t) in f.params.iter().chain(&f.locals) {
            let ty = c_ty(t)
                .ok_or_else(|| format!("C local `{n}` has type `{t}`, outside the subset"))?;
            types.insert(n.clone(), ty);
        }
        Ok(CExec {
            f,
            cuts,
            callees,
            labels,
            types,
        })
    }

    pub fn ty_of(&self, n: &str) -> Option<Ty> {
        self.types.get(n).copied()
    }

    /// Run from `start` (a cut label, or the entry when `None`).
    pub fn run(&self, start: Option<&str>, st: St) -> Result<Paths, String> {
        let pc = match start {
            None => 0,
            Some(l) => *self.labels.get(l).ok_or(format!("no label `{l}`"))? + 1,
        };
        let mut out = Vec::new();
        let mut work = vec![(st, pc, 0usize)];
        while let Some((mut s, mut pc, mut steps)) = work.pop() {
            loop {
                steps += 1;
                if steps > 20_000 || out.len() + work.len() > 4096 {
                    return Err("a cycle with no cut label, or too many paths".into());
                }
                let Some(stmt) = self.f.body.get(pc) else {
                    return Err("control reaches the end of the function".into());
                };
                match stmt {
                    CStmt::Label(l) if self.cuts.contains(l) => {
                        out.push((s, CExit::Cut(l.clone())));
                        break;
                    }
                    CStmt::Label(_) => pc += 1,
                    CStmt::Assign(x, e) => {
                        let ty = self
                            .ty_of(x)
                            .ok_or(format!("assignment to unknown `{x}`"))?;
                        let (v, from) = self.expr(e, &mut s, Some(ty), true)?;
                        let v = match (from, ty) {
                            (Some(f), t) if f != t && t != Ty::Bool => {
                                let c = cast(t, Some(f), v);
                                s.fact(in_range(&c, t));
                                c
                            }
                            _ => v,
                        };
                        set(&mut s, x, v, ty);
                        pc += 1;
                    }
                    CStmt::Eval(e) => {
                        self.expr(e, &mut s, None, true)?;
                        pc += 1;
                    }
                    CStmt::Goto(l) => {
                        if self.cuts.contains(l) {
                            out.push((s, CExit::Cut(l.clone())));
                            break;
                        }
                        pc = *self.labels.get(l).ok_or(format!("no label `{l}`"))?;
                    }
                    CStmt::If(c, a, b) => {
                        let (cv, cty) = self.expr(c, &mut s, None, false)?;
                        let cond = if cty == Some(Ty::Bool) {
                            cv
                        } else {
                            cmp(COp::Ne, cv, Term::Int(0))
                        };
                        for (target, truth) in [(b, not(cond.clone())), (a, cond.clone())] {
                            if truth == Term::Bool(false) {
                                continue;
                            }
                            let mut ns = s.clone();
                            ns.assume(truth);
                            if self.cuts.contains(target) {
                                out.push((ns, CExit::Cut(target.clone())));
                            } else {
                                let t = *self
                                    .labels
                                    .get(target)
                                    .ok_or(format!("no label `{target}`"))?;
                                work.push((ns, t, steps));
                            }
                        }
                        break;
                    }
                    CStmt::Ret(e) => {
                        let rty = c_ty(&self.f.ret);
                        let v = match e {
                            None => None,
                            Some(e) => {
                                let (v, from) = self.expr(e, &mut s, rty, true)?;
                                Some(match (from, rty) {
                                    (Some(f), Some(t)) if f != t && t != Ty::Bool => {
                                        cast(t, Some(f), v)
                                    }
                                    _ => v,
                                })
                            }
                        };
                        out.push((s, CExit::Ret(v)));
                        break;
                    }
                }
            }
        }
        Ok(out)
    }

    /// The term and type of `e`. `top` marks the whole right side of an
    /// assignment, the one place narrow-type arithmetic is accepted.
    fn expr(
        &self,
        e: &CExpr,
        s: &mut St,
        want: Option<Ty>,
        top: bool,
    ) -> Result<(Term, Option<Ty>), String> {
        Ok(match e {
            CExpr::Int(v) => (Term::Int(*v), want.filter(|t| t.is_int())),
            CExpr::Bool(b) => (Term::Bool(*b), Some(Ty::Bool)),
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
        })
    }

    fn binary(
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
        let (y, _) = self.expr(b, s, if shift { None } else { ty }, false)?;
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

    fn call(&self, name: &str, args: &[CExpr], s: &mut St) -> Result<(Term, Option<Ty>), String> {
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
                (H::Wrap(BOp::WAdd), [x, y]) => bits(BOp::WAdd, ty, x.clone(), y.clone()),
                (H::Wrap(BOp::WMul), [x, y]) => bits(BOp::WMul, ty, x.clone(), y.clone()),
                (H::WSub, [x, y]) => {
                    let t = Term::App(format!("wrapping_sub_{ty}"), vec![x.clone(), y.clone()]);
                    s.fact(in_range(&t, ty));
                    t
                }
                _ => return Err(format!("helper `{name}` arity")),
            };
            return Ok((r, Some(ty)));
        }
        let rty = *self
            .callees
            .get(name)
            .ok_or(format!("call to `{name}` outside the model"))?;
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.expr(a, s, None, false)?.0);
        }
        let app = Term::App(name.to_string(), vals);
        s.fact(in_range(&app, rty));
        Ok((app, Some(rty)))
    }
}

fn set(s: &mut St, n: &str, v: Term, ty: Ty) {
    match s.vars.iter_mut().rev().find(|x| x.0 == n) {
        Some(slot) => slot.1 = v,
        None => s.vars.push((n.to_string(), v, ty)),
    }
}

#[derive(Clone, Copy)]
enum H {
    Bin(BinOp),
    Neg,
    Wrap(BOp),
    WSub,
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
