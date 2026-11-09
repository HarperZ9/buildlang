// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Intervals of the integer subterms of an obligation, which fix the bit
//! width at which nothing wraps.

use crate::verify::term::{AOp, BOp, Term};
use crate::verify::ty::Ty;
use std::collections::HashMap;

pub(super) struct Ranges {
    pub(super) stated: HashMap<Term, (i128, i128)>,
    pub(super) memo: HashMap<Term, (i128, i128)>,
    /// Bounds the encoder must impose on atoms.
    pub(super) atoms: HashMap<Term, (i128, i128)>,
    pub(super) max_ty_bits: u32,
    /// Intervals of products, stated to the solver as facts.
    pub(super) products: HashMap<Term, (i128, i128)>,
}

impl Ranges {
    pub(super) fn int(&mut self, t: &Term) -> Result<(i128, i128), String> {
        if let Some(r) = self.memo.get(t) {
            return Ok(*r);
        }
        let ty_range = |ty: &Ty| ty.range().ok_or("a non-integer operand".to_string());
        let r = match t {
            Term::Int(v) => (*v, *v),
            Term::Var(_, ty) => {
                let r = ty_range(ty)?;
                let r = match self.stated.get(t) {
                    Some(s) => (r.0.max(s.0), r.1.min(s.1)),
                    None => r,
                };
                if r.0 > r.1 {
                    return Err("the hypotheses admit no value".into());
                }
                self.max_ty_bits = self.max_ty_bits.max(ty.bits() as u32);
                self.atoms.insert(t.clone(), r);
                r
            }
            Term::App(..) => {
                let r = *self
                    .stated
                    .get(t)
                    .filter(|r| r.0 > i128::MIN && r.1 < i128::MAX)
                    .ok_or("a call result with no stated bound")?;
                self.atoms.insert(t.clone(), r);
                r
            }
            Term::Arith(op, a, b) => {
                let (x, y) = (self.int(a)?, self.int(b)?);
                let r = match op {
                    AOp::Add => (ck(x.0.checked_add(y.0))?, ck(x.1.checked_add(y.1))?),
                    AOp::Sub => (ck(x.0.checked_sub(y.1))?, ck(x.1.checked_sub(y.0))?),
                    AOp::Mul => {
                        let ps = [
                            ck(x.0.checked_mul(y.0))?,
                            ck(x.0.checked_mul(y.1))?,
                            ck(x.1.checked_mul(y.0))?,
                            ck(x.1.checked_mul(y.1))?,
                        ];
                        let (lo, hi) = (*ps.iter().min().unwrap(), *ps.iter().max().unwrap());
                        // A square is never negative.
                        let lo = if a == b { lo.max(0) } else { lo };
                        // The product's interval is a fact about every run:
                        // the encoder states it, which helps the solver.
                        self.products.insert(t.clone(), (lo, hi));
                        (lo, hi)
                    }
                    AOp::Div => {
                        let m = ck(x.0.checked_abs())?.max(ck(x.1.checked_abs())?);
                        (-m, m)
                    }
                    AOp::Rem => {
                        let ma = ck(x.0.checked_abs())?.max(ck(x.1.checked_abs())?);
                        let mb = ck(y.0.checked_abs())?.max(ck(y.1.checked_abs())?);
                        let m = ma.min(mb.saturating_sub(1)).max(0);
                        (-m, m)
                    }
                };
                if matches!(op, AOp::Div | AOp::Rem) {
                    self.atoms.insert(t.clone(), r);
                }
                r
            }
            Term::Neg(a) => {
                let x = self.int(a)?;
                (ck(x.1.checked_neg())?, ck(x.0.checked_neg())?)
            }
            Term::Cast(ty, a) | Term::BitNot(ty, a) => {
                self.int(a)?;
                self.max_ty_bits = self.max_ty_bits.max(ty.bits() as u32);
                ty_range(ty)?
            }
            Term::Bits(op, ty, a, b) => {
                let x = self.int(a)?;
                self.int(b)?;
                self.max_ty_bits = self.max_ty_bits.max(ty.bits() as u32);
                match (op, ty) {
                    (BOp::Shr, Ty::Int { signed: true, .. }) => (x.0.min(0), x.1.max(0)),
                    _ => ty_range(ty)?,
                }
            }
            other => return Err(format!("not an integer term: {other:?}")),
        };
        self.memo.insert(t.clone(), r);
        Ok(r)
    }

    pub(super) fn boolean(&mut self, t: &Term) -> Result<(), String> {
        match t {
            Term::Bool(_) | Term::Var(_, Ty::Bool) | Term::App(..) => Ok(()),
            Term::Not(a) => self.boolean(a),
            Term::And(a, b) | Term::Or(a, b) | Term::Iff(a, b) => {
                self.boolean(a)?;
                self.boolean(b)
            }
            Term::Cmp(_, a, b) => {
                self.int(a)?;
                self.int(b)?;
                Ok(())
            }
            other => Err(format!("not a boolean term: {other:?}")),
        }
    }
}

fn ck(v: Option<i128>) -> Result<i128, String> {
    v.ok_or_else(|| "an interval too wide for 128 bits".to_string())
}
