// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The two looks at one obligation: a concrete run and a symbolic proof.

use super::eval::{Eval, Stop, Val};
use super::sym::{Out, St, Sym};
use super::term::{self, Term};
use super::ty::Ty;
use super::{prove, Kind};
use crate::parser::{ENSURES_PREFIX, REQUIRES_PREFIX};

/// `Ok(None)`: outside the `requires`. `Ok(Some(b))`: the claim's value.
pub(super) fn run_concrete(
    ev: &mut Eval<'_>,
    kind: Kind,
    name: &str,
    input: &[Val],
) -> Result<Option<bool>, Stop> {
    let req = format!("{REQUIRES_PREFIX}{name}");
    if ev.fns.contains_key(&req) {
        match ev.call(&req, input) {
            Ok(Val::Bool(true)) => {}
            Ok(_) | Err(Stop::Trap(_)) => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    let v = match kind {
        Kind::Law => ev.call(name, input)?,
        Kind::Ensures => {
            let r = ev.call(name, input)?;
            let mut args = input.to_vec();
            args.push(r);
            ev.call(&format!("{ENSURES_PREFIX}{name}"), &args)?
        }
    };
    match v {
        Val::Bool(b) => Ok(Some(b)),
        _ => Err(Stop::Unsupported("a claim that is not bool".into())),
    }
}

/// `Ok(engine)` when every path's goal and every side obligation is valid.
pub(super) fn prove_symbolic(
    sym: &mut Sym<'_>,
    kind: Kind,
    name: &str,
    params: &[(String, Ty)],
) -> Result<String, String> {
    let args: Vec<Term> = params
        .iter()
        .map(|(n, t)| Term::Var(n.clone(), *t))
        .collect();
    let mut st0 = St {
        conds: Vec::new(),
        facts: Vec::new(),
        vars: Vec::new(),
        ret: None,
    };
    for (a, (_, t)) in args.iter().zip(params) {
        st0.conds.push(term::in_range(a, *t));
    }
    let req = format!("{REQUIRES_PREFIX}{name}");
    let mut starts = Vec::new();
    if sym.fns.contains_key(&req) {
        for (mut s, o) in sym.call(&req, args.clone(), st0)? {
            if let Out::Val(b, _) = o {
                s.conds.push(b);
                starts.push(s);
            }
        }
    } else {
        starts.push(st0);
    }
    let mut goals = Vec::new();
    for s in starts {
        for (s2, o) in sym.call(name, args.clone(), s)? {
            let Out::Val(v, _) = o else { continue };
            match kind {
                Kind::Law => goals.push((s2.hyps(), v)),
                Kind::Ensures => {
                    let mut eargs = args.clone();
                    eargs.push(v);
                    for (s3, o3) in sym.call(&format!("{ENSURES_PREFIX}{name}"), eargs, s2)? {
                        if let Out::Val(g, _) = o3 {
                            goals.push((s3.hyps(), g));
                        }
                    }
                }
            }
        }
    }
    let side = std::mem::take(&mut sym.side);
    for ob in &side {
        if let prove::Proof::NotShown(why) = prove::valid(&ob.hyps, &ob.goal) {
            return Err(format!("{} not shown: {why}", ob.label));
        }
    }
    for (hyps, goal) in &goals {
        if let prove::Proof::NotShown(why) = prove::valid(hyps, goal) {
            return Err(format!("symbolic: {why}"));
        }
    }
    Ok(format!(
        "symbolic: {} path(s), {} call-site requires, linear arithmetic over Z",
        goals.len(),
        side.len()
    ))
}
