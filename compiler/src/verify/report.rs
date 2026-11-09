// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Text and JSON reports for `buildc verify`.

use super::{Kind, Outcome, Traps, Verdict};
use serde_json::{json, Value};

fn inputs(i: &[(String, String)]) -> String {
    i.iter()
        .map(|(n, v)| format!("{n} = {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn kind_word(k: Kind) -> &'static str {
    match k {
        Kind::Law => "law",
        Kind::Ensures => "ensures",
    }
}

/// One line per obligation, then a count line.
pub fn human(outcomes: &[Outcome]) -> String {
    let mut s = String::new();
    for o in outcomes {
        let head = format!("{} {}", kind_word(o.kind), o.name);
        let body = match &o.verdict {
            Verdict::Proved { engine } => format!("proved ({engine})"),
            Verdict::Refuted { input, why } => format!("REFUTED at {}: {why}", inputs(input)),
            Verdict::Unknown { reason } => format!("unknown: {reason}"),
            Verdict::Inconsistent { input } => format!(
                "INCONSISTENT: the prover claimed it but {} is a counterexample; not proved",
                inputs(input)
            ),
        };
        s.push_str(&format!("{head}: {body}\n"));
        match &o.traps {
            Traps::Witness { input, why } => {
                s.push_str(&format!("  traps at {}: {why}\n", inputs(input)))
            }
            Traps::NoneExhaustive => s.push_str("  never traps (every input run)\n"),
            Traps::NoneSampled => {}
        }
    }
    let (p, r, u, x) = counts(outcomes);
    s.push_str(&format!(
        "{} obligations: {p} proved, {r} refuted, {u} unknown, {x} inconsistent\n",
        outcomes.len()
    ));
    s
}

/// Counts of proved, refuted, unknown and inconsistent verdicts.
pub fn counts(outcomes: &[Outcome]) -> (usize, usize, usize, usize) {
    let mut c = (0, 0, 0, 0);
    for o in outcomes {
        match o.verdict {
            Verdict::Proved { .. } => c.0 += 1,
            Verdict::Refuted { .. } => c.1 += 1,
            Verdict::Unknown { .. } => c.2 += 1,
            Verdict::Inconsistent { .. } => c.3 += 1,
        }
    }
    c
}

fn input_json(i: &[(String, String)]) -> Value {
    Value::Object(
        i.iter()
            .map(|(n, v)| (n.clone(), Value::String(v.clone())))
            .collect(),
    )
}

/// The `buildlang-verify/v1` report.
pub fn to_json(file: &str, outcomes: &[Outcome]) -> Value {
    let items: Vec<Value> = outcomes
        .iter()
        .map(|o| {
            let (verdict, detail) = match &o.verdict {
                Verdict::Proved { engine } => ("proved", json!({ "engine": engine })),
                Verdict::Refuted { input, why } => (
                    "refuted",
                    json!({ "counterexample": input_json(input), "why": why }),
                ),
                Verdict::Unknown { reason } => ("unknown", json!({ "reason": reason })),
                Verdict::Inconsistent { input } => (
                    "inconsistent",
                    json!({ "counterexample": input_json(input) }),
                ),
            };
            let traps = match &o.traps {
                Traps::NoneExhaustive => json!({ "status": "none", "cover": "exhaustive" }),
                Traps::NoneSampled => json!({ "status": "none-found", "cover": "sampled" }),
                Traps::Witness { input, why } => {
                    json!({ "status": "traps", "input": input_json(input), "why": why })
                }
            };
            json!({
                "kind": kind_word(o.kind),
                "name": o.name,
                "verdict": verdict,
                "detail": detail,
                "traps": traps,
                "inputs_run": o.inputs_run,
            })
        })
        .collect();
    let (p, r, u, x) = counts(outcomes);
    json!({
        "schema": "buildlang-verify/v1",
        "file": file,
        "meaning": "proved: every evaluation that finishes, for every input in the declared types and requires, makes the claim true; trapping inputs are listed under traps",
        "obligations": items,
        "summary": { "proved": p, "refuted": r, "unknown": u, "inconsistent": x },
    })
}
