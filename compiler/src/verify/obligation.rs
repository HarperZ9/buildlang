// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The two looks at one obligation: a concrete run and a symbolic proof.

use super::eval::{Eval, Stop, Val};
use super::sym::{Out, St, Sym};
use super::term::{self, Term};
use super::ty::Ty;
use super::{prove, smt, Kind};
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

/// The symbolic look: `proved` is `Ok(engine)` when every path's goal and
/// every side obligation is valid; `candidates` are inputs the bit-precise
/// solver found for obligations it could not close, for the concrete side
/// to run.
pub(super) struct Symbolic {
    pub proved: Result<String, String>,
    pub candidates: Vec<Vec<(String, i128)>>,
}

/// The test-only encoder fault, from `BUILDLANG_PLANT_SMT=drop-carry`.
fn planted() -> bool {
    std::env::var("BUILDLANG_PLANT_SMT").as_deref() == Ok("drop-carry")
}

pub(super) fn prove_symbolic(
    sym: &mut Sym<'_>,
    kind: Kind,
    name: &str,
    params: &[(String, Ty)],
) -> Symbolic {
    let obligations = match collect(sym, kind, name, params) {
        Ok(o) => o,
        Err(why) => {
            return Symbolic {
                proved: Err(why),
                candidates: Vec::new(),
            }
        }
    };
    let (side, goals, list) = obligations;
    let mut by_smt = 0;
    let mut failed: Option<String> = None;
    let mut candidates = Vec::new();
    for (label, hyps, goal) in &list {
        let why = match prove::valid(hyps, goal) {
            prove::Proof::Valid => continue,
            prove::Proof::NotShown(why) => why,
        };
        let decided = smt::decide(hyps, goal, planted());
        if decided != smt::Answer::Valid {
            // Roots of a polynomial (dis)equality: candidates only, run below.
            candidates.extend(super::roots::candidates(hyps, goal, params, 16));
        }
        match decided {
            smt::Answer::Valid => by_smt += 1,
            smt::Answer::Candidate(m) => {
                candidates.push(m);
                failed.get_or_insert(format!(
                    "{label}: {why}; the bit-precise solver found a candidate input"
                ));
            }
            smt::Answer::Unknown(u) => {
                failed.get_or_insert(format!("{label}: {why}; bit-precise solver: {u}"));
            }
        }
    }
    let proved = match failed {
        Some(f) => Err(f),
        None if by_smt == 0 => Ok(format!(
            "symbolic: {goals} path(s), {side} call-site requires, linear arithmetic over Z"
        )),
        None => Ok(format!(
            "symbolic: {goals} path(s), {side} call-site requires; {by_smt} obligation(s) by \
             bit-blasting and SAT, the UNSAT proof replayed"
        )),
    };
    Symbolic { proved, candidates }
}

type Obligations = (usize, usize, Vec<(String, Vec<Term>, Term)>);

/// Every obligation of one claim: the side obligations, then each path's goal.
fn collect(
    sym: &mut Sym<'_>,
    kind: Kind,
    name: &str,
    params: &[(String, Ty)],
) -> Result<Obligations, String> {
    let args: Vec<Term> = params
        .iter()
        .map(|(n, t)| Term::Var(n.clone(), *t))
        .collect();
    let mut st0 = St::new(Vec::new(), Vec::new(), None);
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
    let (n_side, n_goals) = (side.len(), goals.len());
    let mut list: Vec<(String, Vec<Term>, Term)> = side
        .into_iter()
        .map(|ob| (format!("{} not shown", ob.label), ob.hyps, ob.goal))
        .collect();
    list.extend(
        goals
            .into_iter()
            .map(|(h, g)| ("symbolic".to_string(), h, g)),
    );
    Ok((n_side, n_goals, list))
}
