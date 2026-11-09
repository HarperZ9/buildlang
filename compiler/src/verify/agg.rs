// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Struct values for translation validation.
//!
//! A struct whose fields are subset scalars or such structs is a value: the
//! term `agg$Name` over its fields in source declaration order. Two struct
//! values are equal when every field is. Struct names are interned so that
//! `Ty` stays `Copy`; the definitions themselves live in a [`Structs`]
//! table built per source file.

use super::term::*;
use super::ty::Ty;
use crate::ast::{Item, ItemKind, StructFields, Type, TypeKind};
use std::collections::HashMap;
use std::sync::Mutex;

static NAMES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The interned id of a struct name.
pub fn intern(name: &str) -> u16 {
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(i) = names.iter().position(|n| n == name) {
        return i as u16;
    }
    names.push(name.to_string());
    (names.len() - 1) as u16
}

/// The name behind an interned id.
pub fn name_of(id: u16) -> String {
    let names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    names.get(id as usize).cloned().unwrap_or_default()
}

/// A struct value.
pub fn agg(name: &str, comps: Vec<Term>) -> Term {
    Term::App(format!("agg${name}"), comps)
}

/// Struct definitions: name to fields (name and type) in declaration order.
#[derive(Clone, Debug, Default)]
pub struct Structs {
    pub defs: HashMap<String, Vec<(String, Ty)>>,
}

fn path_name(t: &Type) -> Option<&str> {
    match &t.kind {
        TypeKind::Path(p) if p.segments.len() == 1 && p.segments[0].generics.is_empty() => {
            Some(&p.segments[0].ident.name)
        }
        _ => None,
    }
}

impl Structs {
    /// The structs of a module whose fields are all subset values.
    pub fn from_items(items: &[Item]) -> Structs {
        let mut pending: Vec<(String, Vec<(String, &Type)>)> = Vec::new();
        for item in items {
            let ItemKind::Struct(sd) = &item.kind else {
                continue;
            };
            let StructFields::Named(fields) = &sd.fields else {
                continue;
            };
            if !sd.generics.params.is_empty() || fields.is_empty() {
                continue;
            }
            let fs = fields
                .iter()
                .map(|f| (f.name.name.to_string(), &*f.ty))
                .collect();
            pending.push((sd.name.name.to_string(), fs));
        }
        let mut out = Structs::default();
        // A struct is admitted once every field type is admitted.
        loop {
            let before = out.defs.len();
            for (name, fs) in &pending {
                if out.defs.contains_key(name) {
                    continue;
                }
                let tys: Option<Vec<(String, Ty)>> = fs
                    .iter()
                    .map(|(n, t)| out.ty_of_ast(t).map(|ty| (n.clone(), ty)))
                    .collect();
                if let Some(tys) = tys {
                    out.defs.insert(name.clone(), tys);
                }
            }
            if out.defs.len() == before {
                return out;
            }
        }
    }

    /// The subset type of an AST type: a scalar or an admitted struct.
    pub fn ty_of_ast(&self, t: &Type) -> Option<Ty> {
        Ty::from_ast_tv(t).or_else(|| {
            let n = path_name(t)?;
            self.defs.contains_key(n).then(|| Ty::Struct(intern(n)))
        })
    }

    /// The fields of a struct type.
    pub fn fields(&self, ty: Ty) -> Option<&Vec<(String, Ty)>> {
        match ty {
            Ty::Struct(id) => self.defs.get(&name_of(id)),
            _ => None,
        }
    }

    /// A value of fresh variables named `name`, `name.f`, `name.f.g`.
    pub fn fresh(&self, name: &str, ty: Ty) -> Term {
        match self.fields(ty) {
            Some(fs) => agg(
                &ty.to_string(),
                fs.iter()
                    .map(|(f, t)| self.fresh(&format!("{name}.{f}"), *t))
                    .collect(),
            ),
            None => Term::Var(name.to_string(), ty),
        }
    }

    /// Every scalar field of `t` lies in its type's range.
    pub fn in_range(&self, t: &Term, ty: Ty) -> Term {
        match (self.fields(ty), t) {
            (Some(fs), Term::App(_, comps)) if comps.len() == fs.len() => fs
                .iter()
                .zip(comps)
                .fold(Term::Bool(true), |g, ((_, ft), c)| {
                    and(g, self.in_range(c, *ft))
                }),
            (Some(_), _) => Term::Bool(true),
            (None, _) => in_range(t, ty),
        }
    }

    /// Equality of two values of type `ty`, field by field.
    pub fn eq(&self, a: &Term, b: &Term, ty: Ty) -> Term {
        match (self.fields(ty), a, b) {
            (Some(fs), Term::App(na, xa), Term::App(nb, xb))
                if na == nb && xa.len() == fs.len() && xb.len() == fs.len() =>
            {
                fs.iter()
                    .zip(xa.iter().zip(xb))
                    .fold(Term::Bool(true), |g, ((_, ft), (x, y))| {
                        and(g, self.eq(x, y, *ft))
                    })
            }
            (Some(_), _, _) => Term::Bool(false),
            (None, _, _) if ty == Ty::Bool => iff(a.clone(), b.clone()),
            (None, _, _) => cmp(COp::Eq, a.clone(), b.clone()),
        }
    }

    /// The result of an uninterpreted call `name(args)` of type `ty`: one
    /// term per scalar field, each with its range fact.
    pub fn opaque(&self, name: &str, args: &[Term], ty: Ty, facts: &mut Vec<Term>) -> Term {
        self.opaque_at(name, "", args, ty, facts)
    }

    fn opaque_at(
        &self,
        name: &str,
        at: &str,
        args: &[Term],
        ty: Ty,
        facts: &mut Vec<Term>,
    ) -> Term {
        match self.fields(ty) {
            Some(fs) => agg(
                &ty.to_string(),
                fs.iter()
                    .map(|(f, t)| self.opaque_at(name, &format!("{at}.{f}"), args, *t, facts))
                    .collect(),
            ),
            None => {
                let app = Term::App(format!("{name}{at}"), args.to_vec());
                facts.push(in_range(&app, ty));
                app
            }
        }
    }

    /// The index of field `f` of struct type `ty`, and its type.
    pub fn field(&self, ty: Ty, f: &str) -> Option<(usize, Ty)> {
        let fs = self.fields(ty)?;
        let i = fs.iter().position(|(n, _)| n == f)?;
        Some((i, fs[i].1))
    }
}

/// Component `i` of a struct value.
pub fn select(t: &Term, i: usize) -> Option<Term> {
    match t {
        Term::App(n, comps) if n.starts_with("agg$") => comps.get(i).cloned(),
        _ => None,
    }
}

/// `t` with component `i` replaced by `v`.
pub fn replace(t: &Term, i: usize, v: Term) -> Option<Term> {
    match t {
        Term::App(n, comps) if n.starts_with("agg$") && i < comps.len() => {
            let mut c = comps.clone();
            c[i] = v;
            Some(Term::App(n.clone(), c))
        }
        _ => None,
    }
}
