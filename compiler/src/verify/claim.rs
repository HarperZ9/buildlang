// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! One claim checked both ways: concrete runs and the symbolic proof.

use super::*;

pub(super) fn show(name: &str, v: &Val) -> (String, String) {
    let s = match v {
        Val::Int(x, _) => x.to_string(),
        Val::Bool(b) => b.to_string(),
        Val::Unit => "()".into(),
    };
    (name.to_string(), s)
}

pub(super) fn check(fns: &HashMap<String, &FnDef>, sigs: &Sigs, kind: Kind, name: &str) -> Outcome {
    let mut sym = Sym::new(fns, sigs);
    let params = match sym.params(name) {
        Ok(p) => p,
        Err(reason) => {
            return Outcome {
                kind,
                name: name.into(),
                verdict: Verdict::Unknown { reason },
                traps: Traps::NoneSampled,
                inputs_run: 0,
            }
        }
    };
    let sym_look = prove_symbolic(&mut sym, kind, name, &params);
    let symbolic = sym_look.proved;
    let mut doms: Vec<search::Dom> = params.iter().map(|p| search::Dom::of(p.1)).collect();
    let req = format!("{REQUIRES_PREFIX}{name}");
    if let Some(body) = fns.get(&req).and_then(|f| f.body.as_deref()) {
        if let Some(e) = infer::block_tail(body) {
            let names: Vec<String> = params.iter().map(|p| p.0.clone()).collect();
            search::narrow(&mut doms, &names, e);
        }
    }
    if doms.iter().any(|d| d.lo > d.hi) {
        return Outcome {
            kind,
            name: name.into(),
            verdict: Verdict::Unknown {
                reason: "the requires admits no input".into(),
            },
            traps: Traps::NoneSampled,
            inputs_run: 0,
        };
    }
    let (cover, inputs) = search::inputs(&doms, search::seed_of(name));
    let mut fuel_outs = 0;
    let mut ev = Eval {
        fns,
        sigs,
        fuel: FUEL,
        check_requires: true,
        depth: 0,
    };
    let mut cex = None;
    let mut trap = None;
    let mut incomplete: Option<String> = None;
    let mut run = 0;
    for input in &inputs {
        ev.fuel = FUEL;
        ev.depth = 0;
        let named = || {
            params
                .iter()
                .zip(input)
                .map(|(p, v)| show(&p.0, v))
                .collect::<Vec<_>>()
        };
        match run_concrete(&mut ev, kind, name, input) {
            Ok(None) => {}
            Ok(Some(true)) => run += 1,
            Ok(Some(false)) => {
                let what = match kind {
                    Kind::Law => format!("law `{name}` is false here"),
                    Kind::Ensures => format!("the ensures of `{name}` is false here"),
                };
                cex = Some((named(), what));
                break;
            }
            Err(Stop::Trap(why)) => {
                run += 1;
                trap.get_or_insert((named(), why));
            }
            Err(Stop::Requires(callee)) => {
                cex = Some((
                    named(),
                    format!("the call to `{callee}` breaks its requires"),
                ));
                break;
            }
            Err(Stop::Fuel) => {
                incomplete = Some("a run ran out of fuel".into());
                fuel_outs += 1;
                if fuel_outs >= 32 {
                    break;
                }
            }
            Err(Stop::Unsupported(what)) => {
                incomplete = Some(format!("the evaluator does not run {what}"));
                break;
            }
        }
    }
    // Inputs the bit-precise solver proposed: run each; only a run that makes
    // the claim false (or breaks a callee's requires) is a counterexample.
    if cex.is_none() {
        for cand in &sym_look.candidates {
            // A value outside its parameter's type is no input at all.
            let in_types = params.iter().all(|(n, t)| {
                let v = cand.iter().find(|c| c.0 == *n).map_or(0, |c| c.1);
                match t {
                    ty::Ty::Bool => v == 0 || v == 1,
                    _ => t.holds(v),
                }
            });
            if !in_types {
                continue;
            }
            let input: Vec<Val> = params
                .iter()
                .map(|(n, t)| {
                    let v = cand.iter().find(|c| c.0 == *n).map_or(0, |c| c.1);
                    match t {
                        ty::Ty::Bool => Val::Bool(v != 0),
                        _ => Val::Int(v, *t),
                    }
                })
                .collect();
            ev.fuel = FUEL;
            ev.depth = 0;
            let named = || {
                params
                    .iter()
                    .zip(&input)
                    .map(|(p, v)| show(&p.0, v))
                    .collect::<Vec<_>>()
            };
            match run_concrete(&mut ev, kind, name, &input) {
                Ok(Some(false)) => {
                    let what = match kind {
                        Kind::Law => format!("law `{name}` is false here (input from the solver)"),
                        Kind::Ensures => {
                            format!("the ensures of `{name}` is false here (input from the solver)")
                        }
                    };
                    cex = Some((named(), what));
                    break;
                }
                Err(Stop::Requires(callee)) => {
                    cex = Some((
                        named(),
                        format!(
                            "the call to `{callee}` breaks its requires (input from the solver)"
                        ),
                    ));
                    break;
                }
                Ok(Some(true)) => run += 1,
                _ => {}
            }
        }
    }
    let exhaustive = cover == search::Cover::Exhaustive && incomplete.is_none();
    let traps = match (&trap, exhaustive) {
        (Some((input, why)), _) => Traps::Witness {
            input: input.clone(),
            why: why.clone(),
        },
        (None, true) => Traps::NoneExhaustive,
        (None, false) => Traps::NoneSampled,
    };
    let verdict = match (cex, &symbolic) {
        (Some((input, _)), Ok(_)) => Verdict::Inconsistent { input },
        (Some((input, why)), Err(_)) => Verdict::Refuted { input, why },
        (None, _) if exhaustive => Verdict::Proved {
            engine: format!("exhaustive: every input, {run} in the domain"),
        },
        (None, Ok(engine)) => Verdict::Proved {
            engine: engine.clone(),
        },
        (None, Err(reason)) => Verdict::Unknown {
            reason: match &incomplete {
                Some(i) => format!("{reason}; {i}"),
                None => format!("{reason}; no counterexample in {run} inputs"),
            },
        },
    };
    Outcome {
        kind,
        name: name.into(),
        verdict,
        traps,
        inputs_run: run,
    }
}
