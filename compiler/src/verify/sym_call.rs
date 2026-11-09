// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Calls in the symbolic executor: inlined, or uninterpreted under the
//! callee's contracts for recursion, or opaque in translation validation.

use super::sym::{Out, Res, SideOb, St, Sym};
use super::term::*;
use crate::parser::{ENSURES_PREFIX, REQUIRES_PREFIX};

impl Sym<'_> {
    /// Run `name` on `args` from state `st`; results come back as values in
    /// states that keep the caller's variables.
    pub fn call(&mut self, name: &str, args: Vec<Term>, st: St) -> Res {
        let f = *self
            .fns
            .get(name)
            .ok_or(format!("call to `{name}` outside the subset"))?;
        let params = self.params(name)?;
        let ret = f.sig.return_ty.as_deref().and_then(|t| self.ty_of_ast(t));
        if f.sig.return_ty.is_some() && ret.is_none() {
            return Err(format!("`{name}` returns a type outside the subset"));
        }
        let body = f.body.as_deref().ok_or(format!("`{name}` has no body"))?;
        let mut inner = St {
            conds: st.conds.clone(),
            facts: st.facts.clone(),
            vars: params
                .iter()
                .zip(args)
                .map(|((n, t), a)| (n.clone(), a, *t))
                .collect(),
            ret,
            trace: st.trace.clone(),
            strs: st.strs.clone(),
        };
        for (n, t) in &params {
            let (v, _) = inner.get(n).unwrap();
            inner.fact(in_range(&v, *t));
        }
        self.stack.push(name.to_string());
        let res = self.block(body, inner, ret);
        self.stack.pop();
        let mut out = Vec::new();
        for (s, o) in res? {
            let (Out::Val(v, t) | Out::Ret(v, t)) = o else {
                return Err(format!("`break` or `continue` escapes `{name}`"));
            };
            let back = St {
                conds: s.conds,
                facts: s.facts,
                vars: st.vars.clone(),
                ret: st.ret,
                trace: s.trace,
                strs: st.strs.clone(),
            };
            out.push((back, Out::Val(v, t)));
        }
        Ok(out)
    }

    /// The paths of a call, honouring contracts and recursion.
    pub(super) fn user_call(&mut self, name: &str, args: Vec<Term>, st: St) -> Res {
        let req = format!("{REQUIRES_PREFIX}{name}");
        if self.inline_calls && self.fns.contains_key(&req) && !self.stack.contains(&req) {
            for (s, o) in self.call(&req, args.clone(), st.clone())? {
                let Out::Val(goal, _) = o else { continue };
                self.side.push(SideOb {
                    label: format!("call to `{name}` meets its requires"),
                    hyps: s.hyps(),
                    goal,
                });
            }
        }
        if !self.inline_calls {
            return super::sym_effects::opaque_call(self, name, args, st);
        }
        let recursive = self.stack.iter().any(|s| s == name) || self.stack.len() > 24;
        if !recursive {
            return self.call(name, args, st);
        }
        let ret = self.sigs.fns.get(name).and_then(|s| s.1);
        let Some(rty) = ret else {
            return Err(format!("recursive `{name}` without a subset return type"));
        };
        let app = Term::App(name.to_string(), args.clone());
        let mut st = st;
        st.fact(in_range(&app, rty));
        let ens = format!("{ENSURES_PREFIX}{name}");
        if !self.fns.contains_key(&ens) || self.stack.contains(&ens) {
            return Ok(vec![(st, Out::Val(app, Some(rty)))]);
        }
        let mut eargs = args;
        eargs.push(app.clone());
        let mut out = Vec::new();
        for (mut s, o) in self.call(&ens, eargs, st)? {
            let Out::Val(fact, _) = o else { continue };
            s.assume(fact);
            out.push((s, Out::Val(app.clone(), Some(rty))));
        }
        Ok(out)
    }
}
