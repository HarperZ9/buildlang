// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The effect conditions of one segment.
//!
//! Each event in a path's trace carries the number of path conditions in
//! force when it happened, so a path that later traps still contributes
//! the events it wrote before the trap. Two conditions, for every index j:
//!
//! - E1: the source reaches its j-th event on exactly the inputs where the
//!   C reaches its j-th event;
//! - E2: where a source path and a C path both reach their j-th event, the
//!   two events are equal.
//!
//! Each run is deterministic, so on one input at most one path of a side
//! reaches index j; with E1 and E2 both sides write the same events in the
//! same order, on every input, before the segment ends or traps.

use crate::verify::effects::ev_eq;
use crate::verify::prove::{valid, Proof};
use crate::verify::sym::St;
use crate::verify::term::*;
use std::collections::HashSet;

/// An event at index j, under its reach condition.
struct At {
    j: usize,
    reach: Term,
    ev: Term,
}

fn events<E>(paths: &[(St, E)], base: usize) -> Vec<At> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (st, _) in paths {
        for (j, (n, ev)) in st.trace.iter().enumerate() {
            let lo = base.min(*n);
            let reach = st.conds[lo..*n].iter().cloned().fold(Term::Bool(true), and);
            if seen.insert((j, reach.clone(), ev.clone())) {
                out.push(At {
                    j,
                    reach,
                    ev: ev.clone(),
                });
            }
        }
    }
    out
}

fn need(hyps: &[Term], goal: &Term, what: &str) -> Result<(), String> {
    match valid(hyps, goal) {
        Proof::Valid => Ok(()),
        Proof::NotShown(why) => Err(format!("mismatch: {what}: {why}")),
    }
}

/// Check E1 and E2 for one segment. `base` is the number of shared start
/// conditions, `facts` the hypotheses true on every input.
pub fn check<A, B>(
    base: usize,
    facts: &[Term],
    s_paths: &[(St, A)],
    c_paths: &[(St, B)],
    at: &str,
) -> Result<(), String> {
    let s_ev = events(s_paths, base);
    let c_ev = events(c_paths, base);
    let len = s_ev.iter().chain(&c_ev).map(|e| e.j + 1).max().unwrap_or(0);
    for j in 0..len {
        let reach = |evs: &[At]| {
            evs.iter()
                .filter(|e| e.j == j)
                .fold(Term::Bool(false), |a, e| or(a, e.reach.clone()))
        };
        let (rs, rc) = (reach(&s_ev), reach(&c_ev));
        need(
            &[facts.to_vec(), vec![rs.clone()]].concat(),
            &rc,
            &format!("at {at}, the source writes effect {j} where C does not"),
        )?;
        need(
            &[facts.to_vec(), vec![rc]].concat(),
            &rs,
            &format!("at {at}, C writes effect {j} where the source does not"),
        )?;
        for a in s_ev.iter().filter(|e| e.j == j) {
            for b in c_ev.iter().filter(|e| e.j == j) {
                let mut hyps = facts.to_vec();
                hyps.push(a.reach.clone());
                hyps.push(b.reach.clone());
                need(
                    &hyps,
                    &ev_eq(&a.ev, &b.ev),
                    &format!("at {at}, effect {j} differs"),
                )?;
            }
        }
    }
    Ok(())
}
