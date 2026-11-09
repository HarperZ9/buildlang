// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! An independent checker for the solver's UNSAT answers, written apart from
//! [`super::sat`] and sharing no code with it. It replays the learned clauses
//! in order: each must follow from the clauses before it by unit propagation
//! (reverse unit propagation, RUP), and at the end unit propagation alone must
//! reach a conflict. A wrong learned clause, or a solver that stops early,
//! fails the replay.

use super::sat::Lit;

struct Db {
    clauses: Vec<Vec<Lit>>,
    /// Clause indices by watched literal; unit clauses live in `units`.
    watch: Vec<Vec<usize>>,
    units: Vec<Lit>,
    empty: bool,
    /// 0 unassigned, 1 true, 2 false, by variable.
    val: Vec<u8>,
}

impl Db {
    fn lv(&self, l: Lit) -> u8 {
        match self.val[(l >> 1) as usize] {
            0 => 0,
            v if (l & 1 == 0) == (v == 1) => 1,
            _ => 2,
        }
    }

    fn add(&mut self, c: Vec<Lit>) {
        match c.len() {
            0 => self.empty = true,
            1 => self.units.push(c[0]),
            _ => {
                let i = self.clauses.len();
                self.watch[c[0] as usize].push(i);
                self.watch[c[1] as usize].push(i);
            }
        }
        self.clauses.push(c);
    }

    /// Unit propagation from the assumptions and the unit clauses; `true` on
    /// a conflict. The assignment is undone before returning.
    fn conflicts(&mut self, assumed: &[Lit], work: &mut u64) -> bool {
        if self.empty {
            return true;
        }
        let mut trail: Vec<Lit> = Vec::new();
        let mut conflict = false;
        let units = self.units.clone();
        for &l in assumed.iter().chain(units.iter()) {
            match self.lv(l) {
                2 => {
                    conflict = true;
                    break;
                }
                0 => {
                    self.val[(l >> 1) as usize] = if l & 1 == 0 { 1 } else { 2 };
                    trail.push(l);
                }
                _ => {}
            }
        }
        let mut head = 0;
        while !conflict && head < trail.len() {
            let f = trail[head] ^ 1;
            head += 1;
            let list = std::mem::take(&mut self.watch[f as usize]);
            let mut kept = Vec::with_capacity(list.len());
            let mut k = 0;
            while k < list.len() {
                let ci = list[k];
                k += 1;
                *work += 1;
                if self.clauses[ci][0] == f {
                    self.clauses[ci].swap(0, 1);
                }
                let other = self.clauses[ci][0];
                if self.lv(other) == 1 {
                    kept.push(ci);
                    continue;
                }
                let mut moved = None;
                for j in 2..self.clauses[ci].len() {
                    if self.lv(self.clauses[ci][j]) != 2 {
                        moved = Some(j);
                        break;
                    }
                }
                if let Some(j) = moved {
                    self.clauses[ci].swap(1, j);
                    let nl = self.clauses[ci][1];
                    self.watch[nl as usize].push(ci);
                    continue;
                }
                kept.push(ci);
                match self.lv(other) {
                    2 => {
                        conflict = true;
                        kept.extend_from_slice(&list[k..]);
                        break;
                    }
                    0 => {
                        self.val[(other >> 1) as usize] = if other & 1 == 0 { 1 } else { 2 };
                        trail.push(other);
                    }
                    _ => {}
                }
            }
            self.watch[f as usize] = kept;
        }
        for l in trail {
            self.val[(l >> 1) as usize] = 0;
        }
        conflict
    }
}

/// Whether `learned`, replayed over `original`, proves `original` UNSAT.
/// `max_work` bounds clause visits; past it the replay fails closed.
pub fn check(original: &[Vec<Lit>], learned: &[Vec<Lit>], nvars: usize, max_work: u64) -> bool {
    let mut db = Db {
        clauses: Vec::new(),
        watch: vec![Vec::new(); 2 * nvars],
        units: Vec::new(),
        empty: false,
        val: vec![0; nvars],
    };
    for c in original {
        let mut c = c.clone();
        c.sort_unstable();
        c.dedup();
        if c.windows(2).any(|w| w[0] == w[1] ^ 1) {
            continue;
        }
        db.add(c);
    }
    let mut work = 0u64;
    for c in learned {
        let negated: Vec<Lit> = c.iter().map(|l| l ^ 1).collect();
        if !db.conflicts(&negated, &mut work) || work > max_work {
            return false;
        }
        db.add(c.clone());
    }
    db.conflicts(&[], &mut work) && work <= max_work
}
