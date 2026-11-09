// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! A CDCL SAT solver: two watched literals, first-UIP learning, VSIDS
//! activity, phase saving and Luby restarts. Every learned clause is logged,
//! so an UNSAT answer can be replayed by the independent checker in
//! [`super::rup`]. A conflict budget bounds the work; past it the answer is
//! `Unknown`.

mod heap;
use heap::luby;

/// A literal: `var * 2`, plus 1 when negated.
pub type Lit = u32;

pub fn lit(var: u32, neg: bool) -> Lit {
    var * 2 + neg as u32
}

pub fn neg(l: Lit) -> Lit {
    l ^ 1
}

fn var(l: Lit) -> usize {
    (l >> 1) as usize
}

/// A solver answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A satisfying assignment, indexed by variable.
    Sat(Vec<bool>),
    Unsat,
    Unknown,
}

#[derive(Default)]
pub struct Solver {
    pub clauses: Vec<Vec<Lit>>,
    /// Learned clauses, in the order they were derived.
    pub learned: Vec<Vec<Lit>>,
    nvars: usize,
    watches: Vec<Vec<usize>>,
    value: Vec<i8>,
    level: Vec<u32>,
    reason: Vec<Option<usize>>,
    trail: Vec<Lit>,
    lim: Vec<usize>,
    head: usize,
    act: Vec<f64>,
    inc: f64,
    phase: Vec<bool>,
    heap: Vec<usize>,
    pos: Vec<Option<usize>>,
    empty: bool,
}

impl Solver {
    pub fn new_var(&mut self) -> u32 {
        let v = self.nvars;
        self.nvars += 1;
        self.watches.push(Vec::new());
        self.watches.push(Vec::new());
        self.value.push(-1);
        self.level.push(0);
        self.reason.push(None);
        self.act.push(0.0);
        self.phase.push(false);
        self.pos.push(None);
        self.heap_insert(v);
        v as u32
    }

    pub fn num_vars(&self) -> usize {
        self.nvars
    }

    fn val(&self, l: Lit) -> i8 {
        match self.value[var(l)] {
            -1 => -1,
            v => (v as u32 ^ (l & 1)) as i8,
        }
    }

    /// Add a clause (before solving).
    pub fn add(&mut self, mut c: Vec<Lit>) {
        c.sort_unstable();
        c.dedup();
        if c.windows(2).any(|w| w[0] == neg(w[1])) {
            return;
        }
        let i = self.clauses.len();
        match c.len() {
            0 => self.empty = true,
            1 => {}
            _ => {
                self.watches[c[0] as usize].push(i);
                self.watches[c[1] as usize].push(i);
            }
        }
        self.clauses.push(c);
    }

    fn assign(&mut self, l: Lit, reason: Option<usize>) {
        let v = var(l);
        self.value[v] = (l & 1 == 0) as i8;
        self.level[v] = self.lim.len() as u32;
        self.reason[v] = reason;
        self.trail.push(l);
    }

    /// Unit propagation; the index of a conflicting clause, if any.
    fn propagate(&mut self) -> Option<usize> {
        while self.head < self.trail.len() {
            let falsified = neg(self.trail[self.head]);
            self.head += 1;
            let ws = std::mem::take(&mut self.watches[falsified as usize]);
            let mut keep = Vec::with_capacity(ws.len());
            let mut conflict = None;
            for (k, &ci) in ws.iter().enumerate() {
                if conflict.is_some() {
                    keep.extend_from_slice(&ws[k..]);
                    break;
                }
                let c = &mut self.clauses[ci];
                if c[0] == falsified {
                    c.swap(0, 1);
                }
                let first = c[0];
                if self.val(first) == 1 {
                    keep.push(ci);
                    continue;
                }
                let c = &self.clauses[ci];
                let mut moved = false;
                for j in 2..c.len() {
                    if self.val(c[j]) != 0 {
                        let l = c[j];
                        let c = &mut self.clauses[ci];
                        c.swap(1, j);
                        self.watches[l as usize].push(ci);
                        moved = true;
                        break;
                    }
                }
                if moved {
                    continue;
                }
                keep.push(ci);
                match self.val(first) {
                    0 => conflict = Some(ci),
                    -1 => self.assign(first, Some(ci)),
                    _ => {}
                }
            }
            self.watches[falsified as usize] = keep;
            if conflict.is_some() {
                return conflict;
            }
        }
        None
    }

    /// First-UIP conflict analysis: the learned clause (asserting literal
    /// first) and the level to jump back to.
    fn analyze(&mut self, mut ci: usize) -> (Vec<Lit>, usize) {
        let cur = self.lim.len() as u32;
        let mut seen = vec![false; self.nvars];
        let mut out = vec![0];
        let mut count = 0;
        let mut idx = self.trail.len();
        let mut p: Option<Lit> = None;
        loop {
            let c = self.clauses[ci].clone();
            for &q in c.iter() {
                if Some(q) == p {
                    continue;
                }
                let v = var(q);
                if !seen[v] && self.level[v] > 0 {
                    seen[v] = true;
                    self.bump(v);
                    if self.level[v] == cur {
                        count += 1;
                    } else {
                        out.push(q);
                    }
                }
            }
            loop {
                idx -= 1;
                if seen[var(self.trail[idx])] {
                    break;
                }
            }
            let l = self.trail[idx];
            seen[var(l)] = false;
            count -= 1;
            if count == 0 {
                out[0] = neg(l);
                break;
            }
            p = Some(l);
            ci = self.reason[var(l)].expect("a propagated literal has a reason");
        }
        let back = out[1..]
            .iter()
            .map(|&l| self.level[var(l)] as usize)
            .max()
            .unwrap_or(0);
        if out.len() > 1 {
            let k = (1..out.len())
                .max_by_key(|&k| self.level[var(out[k])])
                .unwrap();
            out.swap(1, k);
        }
        self.inc *= 1.0 / 0.95;
        (out, back)
    }

    /// Solve with a conflict budget.
    pub fn solve(&mut self, budget: u64) -> Answer {
        if self.empty {
            return Answer::Unsat;
        }
        self.inc = 1.0;
        let units: Vec<Lit> = self
            .clauses
            .iter()
            .filter(|c| c.len() == 1)
            .map(|c| c[0])
            .collect();
        for (i, u) in units.into_iter().enumerate() {
            match self.val(u) {
                0 => return Answer::Unsat,
                -1 => {
                    let ci = self.clauses.iter().position(|c| c == &vec![u]).unwrap_or(i);
                    self.assign(u, Some(ci));
                }
                _ => {}
            }
        }
        let mut conflicts = 0u64;
        let mut restart_at = 100 * luby(1);
        let mut restarts = 1;
        loop {
            if let Some(ci) = self.propagate() {
                conflicts += 1;
                if self.lim.is_empty() {
                    return Answer::Unsat;
                }
                if conflicts > budget {
                    return Answer::Unknown;
                }
                let (learnt, back) = self.analyze(ci);
                self.backtrack(back);
                self.learned.push(learnt.clone());
                let i = self.clauses.len();
                if learnt.len() == 1 {
                    self.clauses.push(learnt.clone());
                    self.assign(learnt[0], Some(i));
                } else {
                    self.watches[learnt[0] as usize].push(i);
                    self.watches[learnt[1] as usize].push(i);
                    self.clauses.push(learnt.clone());
                    self.assign(learnt[0], Some(i));
                }
                if conflicts >= restart_at {
                    restarts += 1;
                    restart_at = conflicts + 100 * luby(restarts);
                    self.backtrack(0);
                }
                continue;
            }
            let Some(v) = self.pick() else {
                let model = self.value.iter().map(|&x| x == 1).collect();
                return Answer::Sat(model);
            };
            self.lim.push(self.trail.len());
            let l = lit(v as u32, !self.phase[v]);
            self.assign(l, None);
        }
    }
}
