// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The C side's struct definitions, read from the C file and checked against
//! the source's. A C struct is usable when it has the source struct's field
//! names, each with the same type; the order of its fields is read from the C
//! text and mapped to the source order, never assumed.

use super::c_exec::c_ty;
use crate::verify::agg::{intern, Structs};
use crate::verify::ty::Ty;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default)]
pub struct CStructs {
    /// Usable structs: for each C field position, the source field index.
    pub order: HashMap<String, Vec<usize>>,
    /// Source structs whose C definition differs or is missing.
    pub bad: HashSet<String>,
}

/// The backend appends `_` to a field name C reserves (`near`, `far`):
/// `near_` names source field `near` when the source has no `near_`.
fn src_name(fields: &[(String, Ty)], cf: &str) -> Option<usize> {
    let base = cf.strip_suffix('_')?;
    fields.iter().position(|(f, _)| f == base)
}

/// The index and type of the source field that C member `f` names.
pub fn member_field(structs: &Structs, ty: Ty, f: &str) -> Option<(usize, Ty)> {
    structs.field(ty, f).or_else(|| {
        let fields = structs.fields(ty)?;
        let i = src_name(fields, f)?;
        Some((i, fields[i].1))
    })
}

impl CStructs {
    /// Check every source struct against the C definitions.
    pub fn validate(src: &Structs, c: &HashMap<String, Vec<(String, String)>>) -> CStructs {
        let mut out = CStructs::default();
        loop {
            let before = out.order.len();
            for (name, fields) in &src.defs {
                if out.order.contains_key(name) {
                    continue;
                }
                if let Some(order) = out.match_one(fields, c.get(name)) {
                    out.order.insert(name.clone(), order);
                }
            }
            if out.order.len() == before {
                break;
            }
        }
        for name in src.defs.keys() {
            if !out.order.contains_key(name) {
                out.bad.insert(name.clone());
            }
        }
        out
    }

    fn match_one(
        &self,
        fields: &[(String, Ty)],
        c: Option<&Vec<(String, String)>>,
    ) -> Option<Vec<usize>> {
        let c = c?;
        if c.len() != fields.len() {
            return None;
        }
        let mut order = Vec::new();
        for (cf, ct) in c {
            let i = fields
                .iter()
                .position(|(f, _)| f == cf)
                .or_else(|| src_name(fields, cf))?;
            if order.contains(&i) || self.ty(ct) != Some(fields[i].1) {
                return None;
            }
            order.push(i);
        }
        Some(order)
    }

    /// The subset type of a C type name: a scalar or a usable struct.
    pub fn ty(&self, name: &str) -> Option<Ty> {
        c_ty(name).or_else(|| {
            self.order
                .contains_key(name)
                .then(|| Ty::Struct(intern(name)))
        })
    }
}
