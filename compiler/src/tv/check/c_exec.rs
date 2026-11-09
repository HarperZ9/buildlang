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

use super::c_float::c_conv;
use super::c_parse::{CExpr, CFunc, CStmt};
use crate::verify::sym::St;
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
        "double" => Some(Ty::F64),
        "float" => Some(Ty::F32),
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
    /// Return types of the user functions the body may call (`None` for
    /// `void`).
    pub callees: &'a HashMap<String, Option<Ty>>,
    /// The C file's top-level constants.
    pub globals: &'a HashMap<String, (Term, Ty)>,
    /// The C file's string table.
    pub strings: &'a HashMap<String, String>,
    /// Renamed functions: C name to source name.
    pub c_names: &'a HashMap<String, String>,
    /// Source struct definitions and the C structs checked against them.
    pub(super) structs: &'a crate::verify::agg::Structs,
    pub(super) cstructs: &'a super::structs::CStructs,
    labels: HashMap<String, usize>,
    types: HashMap<String, Ty>,
    /// Locals that hold strings.
    pub(super) strs: HashSet<String>,
}

type Paths = Vec<(St, CExit)>;

impl<'a> CExec<'a> {
    pub fn new(
        f: &'a CFunc,
        cuts: &'a HashSet<String>,
        callees: &'a HashMap<String, Option<Ty>>,
        consts: &'a super::relate::Consts,
    ) -> Result<Self, String> {
        let mut labels = HashMap::new();
        for (i, s) in f.body.iter().enumerate() {
            if let CStmt::Label(l) = s {
                labels.insert(l.clone(), i);
            }
        }
        let mut types = HashMap::new();
        let mut strs = HashSet::new();
        for (n, t) in f.params.iter().chain(&f.locals) {
            if super::c_effects::is_string_ty(t) {
                strs.insert(n.clone());
                continue;
            }
            if is_main_arg(f, n, t) {
                continue;
            }
            if consts.cstructs.bad.contains(t) {
                return Err(format!(
                    "mismatch: C struct `{t}` differs from the source struct"
                ));
            }
            let ty = consts
                .cstructs
                .ty(t)
                .ok_or_else(|| format!("C local `{n}` has type `{t}`, outside the subset"))?;
            types.insert(n.clone(), ty);
        }
        Ok(CExec {
            f,
            cuts,
            callees,
            globals: &consts.c,
            strings: &consts.strings,
            c_names: &consts.c_names,
            structs: &consts.structs,
            cstructs: &consts.cstructs,
            labels,
            types,
            strs,
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
        let mut work = vec![(st, pc, 0usize, Vec::<usize>::new())];
        while let Some((mut s, mut pc, mut steps, mut seen)) = work.pop() {
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
                    CStmt::Label(_) => {
                        // A label met twice on one path closes a cycle that
                        // has no cut label: the certificate's loop heads are wrong.
                        if seen.contains(&pc) {
                            return Err("mismatch: a C cycle with no cut label (the certificate's loop heads are wrong)".into());
                        }
                        seen.push(pc);
                        pc += 1
                    }
                    CStmt::Assign(x, e) if self.strs.contains(x) => {
                        let v = self.str_value(e, &mut s)?;
                        match s.strs.iter_mut().rev().find(|v| v.0 == *x) {
                            Some(slot) => slot.1 = v,
                            None => s.strs.push((x.clone(), v)),
                        }
                        pc += 1;
                    }
                    CStmt::Eval(CExpr::Call(name, args)) if self.effect(name, args, &mut s)? => {
                        pc += 1;
                    }
                    CStmt::Assign(x, e) if x.contains('.') => {
                        self.assign_member(x, e, &mut s)?;
                        pc += 1;
                    }
                    CStmt::Assign(x, e) => {
                        let ty = self
                            .ty_of(x)
                            .ok_or(format!("assignment to unknown `{x}`"))?;
                        let (v, from) = self.expr(e, &mut s, Some(ty), true)?;
                        let v = if ty.is_float() || from.is_some_and(|f| f.is_float()) {
                            let c = c_conv(ty, from, v)?;
                            s.fact(in_range(&c, ty));
                            c
                        } else {
                            v
                        };
                        let v = match (from, ty) {
                            (Some(f), _) if f.is_float() || ty.is_float() => v,
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
                                work.push((ns, t, steps, seen.clone()));
                            }
                        }
                        break;
                    }
                    CStmt::Ret(e) => {
                        let rty = self.cstructs.ty(&self.f.ret);
                        let v = match e {
                            None => None,
                            Some(e) => {
                                let (v, from) = self.expr(e, &mut s, rty, true)?;
                                Some(match (from, rty) {
                                    (_, Some(t))
                                        if t.is_float() || from.is_some_and(|f| f.is_float()) =>
                                    {
                                        c_conv(t, from, v)?
                                    }
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
}

/// `argc` and `argv` of a C `main`, which only the start-up call reads.
fn is_main_arg(f: &CFunc, n: &str, t: &str) -> bool {
    f.name == "main" && matches!((n, t), ("argc", "int") | ("argv", "char**"))
}

pub(super) fn set(s: &mut St, n: &str, v: Term, ty: Ty) {
    match s.vars.iter_mut().rev().find(|x| x.0 == n) {
        Some(slot) => slot.1 = v,
        None => s.vars.push((n.to_string(), v, ty)),
    }
}
