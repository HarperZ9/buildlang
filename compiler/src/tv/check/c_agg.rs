// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Struct values on the C side: member reads, member writes and compound
//! literals, over the same `agg$` values the source side builds. A compound
//! literal fills fields in the C declaration order, mapped to the source
//! order through the checked C definition.

use super::c_exec::CExec;
use super::c_parse::CExpr;
use crate::verify::agg::{agg, replace, select};
use crate::verify::sym::St;
use crate::verify::term::Term;
use crate::verify::ty::Ty;

impl CExec<'_> {
    /// `e.f`.
    pub(super) fn member(
        &self,
        e: &CExpr,
        f: &str,
        s: &mut St,
    ) -> Result<(Term, Option<Ty>), String> {
        let (t, ty) = self.expr(e, s, None, false)?;
        let ty = ty.ok_or("a member of a value of unknown type")?;
        let (i, fty) = super::structs::member_field(self.structs, ty, f)
            .ok_or(format!("C member `{f}` outside the model"))?;
        let v = select(&t, i).ok_or("a member of a value that is not a struct")?;
        Ok((v, Some(fty)))
    }

    /// `(Name){ a, b }`.
    pub(super) fn compound(
        &self,
        name: &str,
        items: &[CExpr],
        s: &mut St,
    ) -> Result<(Term, Option<Ty>), String> {
        let ty = self
            .cstructs
            .ty(name)
            .filter(|t| matches!(t, Ty::Struct(_)))
            .ok_or(format!("C compound literal of `{name}` outside the model"))?;
        let fields = self.structs.fields(ty).ok_or("struct fields")?;
        let order = &self.cstructs.order[name];
        if items.len() != order.len() {
            return Err(format!(
                "mismatch: a literal of `{name}` without every field"
            ));
        }
        let mut comps: Vec<Option<Term>> = vec![None; fields.len()];
        for (item, i) in items.iter().zip(order) {
            let fty = fields[*i].1;
            let (v, from) = self.expr(item, s, Some(fty), false)?;
            if from.is_some() && from != Some(fty) {
                return Err("C converts a value inside a compound literal".into());
            }
            comps[*i] = Some(v);
        }
        let comps = comps
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or("fields")?;
        Ok((agg(name, comps), Some(ty)))
    }

    /// `v.f.g = e`: the variable gets a rebuilt value.
    pub(super) fn assign_member(&self, target: &str, e: &CExpr, s: &mut St) -> Result<(), String> {
        let mut parts = target.split('.');
        let var = parts.next().ok_or("an assignment target")?;
        let vty = self
            .ty_of(var)
            .ok_or(format!("assignment to unknown `{var}`"))?;
        let mut tys = vec![vty];
        let mut idx = Vec::new();
        for f in parts {
            let (i, t) = super::structs::member_field(self.structs, *tys.last().unwrap(), f)
                .ok_or(format!("C member `{f}` outside the model"))?;
            idx.push(i);
            tys.push(t);
        }
        let leaf = *tys.last().unwrap();
        let (v, from) = self.expr(e, s, Some(leaf), true)?;
        if from.is_some() && from != Some(leaf) {
            return Err("C converts a value stored into a member".into());
        }
        let whole = s
            .vars
            .iter()
            .rev()
            .find(|x| x.0 == var)
            .map(|x| x.1.clone())
            .ok_or(format!("unbound `{var}`"))?;
        let mut chain = vec![whole];
        for i in &idx {
            let next = select(chain.last().unwrap(), *i).ok_or("a member of a non-struct")?;
            chain.push(next);
        }
        let mut nv = v;
        for (k, i) in idx.iter().enumerate().rev() {
            nv = replace(&chain[k], *i, nv).ok_or("a member of a non-struct")?;
        }
        super::c_exec::set(s, var, nv, vty);
        Ok(())
    }
}
