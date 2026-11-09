// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! `buildc verify`: laws and contracts, checked by the compiler.
//!
//! Each obligation gets two independent looks. The symbolic side
//! ([`sym`], [`prove`]) tries to show it for every input; the concrete side
//! ([`eval`], [`search`]) runs it on every input of a small domain or on
//! boundary values and a fixed sample of a large one. A counterexample is a
//! concrete input the evaluator ran, so a refutation never rests on the
//! prover. An obligation neither side settles is reported as unknown.
//!
//! Scope (stage 1): integers up to 64 bits and `bool`; `let`, `if`, `match`,
//! `return`, calls and the integer methods `abs min max wrapping_*`. Loops are
//! run by the concrete side only. Floats, heap values and effects are outside
//! the subset and make an obligation unknown.

pub mod agg;
mod claim;
pub mod effects;
pub mod eval;
mod eval_expr;
mod eval_match;
pub mod float;
mod fm;
pub mod infer;
mod infer_uses;
mod obligation;
pub mod poly;
mod poly_div;
pub mod prove;
pub mod report;
mod roots;
pub mod search;
pub mod smt;
pub mod sym;
mod sym_agg;
mod sym_call;
mod sym_effects;
mod sym_expr;
mod sym_if;
mod sym_match;
pub mod sym_ops;
pub mod term;
pub mod ty;

#[cfg(test)]
mod tests;

use crate::ast::{FnDef, ItemKind, Module};
use crate::parser::{ENSURES_PREFIX, LAW_ATTR, REQUIRES_PREFIX};
use claim::check;
use eval::{Eval, Stop, Val};
use infer::Sigs;
use obligation::{prove_symbolic, run_concrete};
use std::collections::HashMap;
use sym::Sym;

/// What kind of claim an obligation checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A `law` declaration.
    Law,
    /// A function's `ensures` clause, under its `requires`.
    Ensures,
}

/// The verdict on one obligation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Shown for every input; `engine` names how.
    Proved { engine: String },
    /// A concrete input on which the claim is false, confirmed by evaluation.
    Refuted {
        input: Vec<(String, String)>,
        why: String,
    },
    /// Neither side settled it.
    Unknown { reason: String },
    /// The prover and a counterexample disagree: a defect in the checker.
    Inconsistent { input: Vec<(String, String)> },
}

/// Inputs on which the claim's evaluation traps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Traps {
    /// No input of the full domain traps.
    NoneExhaustive,
    /// No tested input trapped; the domain was sampled.
    NoneSampled,
    /// This input traps.
    Witness {
        input: Vec<(String, String)>,
        why: String,
    },
}

/// The result for one obligation.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub kind: Kind,
    pub name: String,
    pub verdict: Verdict,
    pub traps: Traps,
    pub inputs_run: usize,
}

/// Fuel for one concrete run (loop iterations plus calls).
const FUEL: u64 = 200_000;

/// Verify every law and contract in `module`.
pub fn verify_module(module: &Module) -> Vec<Outcome> {
    let mut fns: HashMap<String, &FnDef> = HashMap::new();
    let mut laws = Vec::new();
    for item in &module.items {
        if let ItemKind::Function(f) = &item.kind {
            fns.insert(f.name.name.to_string(), f);
            let is_law = item
                .attrs
                .iter()
                .any(|a| a.path.segments.len() == 1 && &*a.path.segments[0].ident.name == LAW_ATTR);
            if is_law {
                laws.push(f.name.name.to_string());
            }
        }
    }
    let sigs = Sigs::from_module(&module.items);
    let mut out = Vec::new();
    for law in &laws {
        out.push(check(&fns, &sigs, Kind::Law, law));
    }
    let mut contracted: Vec<String> = fns
        .keys()
        .filter_map(|k| k.strip_prefix(ENSURES_PREFIX).map(str::to_string))
        .collect();
    contracted.sort();
    for f in contracted {
        out.push(check(&fns, &sigs, Kind::Ensures, &f));
    }
    out
}
