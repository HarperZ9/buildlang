// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The variable-activity heap of the solver, activity bumps and
//! backtracking.

use super::*;

impl Solver {
    pub(super) fn pick(&mut self) -> Option<usize> {
        while let Some(v) = self.heap_pop() {
            if self.value[v] == -1 {
                return Some(v);
            }
        }
        None
    }

    pub(super) fn heap_insert(&mut self, v: usize) {
        self.pos[v] = Some(self.heap.len());
        self.heap.push(v);
        self.sift_up(self.heap.len() - 1);
    }

    pub(super) fn heap_pop(&mut self) -> Option<usize> {
        let top = *self.heap.first()?;
        let last = self.heap.pop().unwrap();
        self.pos[top] = None;
        if !self.heap.is_empty() {
            self.heap[0] = last;
            self.pos[last] = Some(0);
            self.sift_down(0);
        }
        Some(top)
    }

    pub(super) fn sift_up(&mut self, mut i: usize) {
        while i > 0 {
            let p = (i - 1) / 2;
            if self.act[self.heap[p]] >= self.act[self.heap[i]] {
                break;
            }
            self.swap_heap(i, p);
            i = p;
        }
    }

    pub(super) fn sift_down(&mut self, mut i: usize) {
        loop {
            let (l, r) = (2 * i + 1, 2 * i + 2);
            let mut m = i;
            if l < self.heap.len() && self.act[self.heap[l]] > self.act[self.heap[m]] {
                m = l;
            }
            if r < self.heap.len() && self.act[self.heap[r]] > self.act[self.heap[m]] {
                m = r;
            }
            if m == i {
                break;
            }
            self.swap_heap(i, m);
            i = m;
        }
    }

    pub(super) fn swap_heap(&mut self, a: usize, b: usize) {
        self.heap.swap(a, b);
        self.pos[self.heap[a]] = Some(a);
        self.pos[self.heap[b]] = Some(b);
    }
}

/// The Luby sequence 1 1 2 1 1 2 4 ...
pub(super) fn luby(mut i: u64) -> u64 {
    let mut size = 1;
    let mut seq = 0;
    while size < i + 1 {
        seq += 1;
        size = 2 * size + 1;
    }
    while size - 1 != i {
        size = (size - 1) >> 1;
        seq -= 1;
        i %= size;
    }
    1 << seq
}

impl Solver {
    pub(super) fn bump(&mut self, v: usize) {
        self.act[v] += self.inc;
        if self.act[v] > 1e100 {
            for a in &mut self.act {
                *a *= 1e-100;
            }
            self.inc *= 1e-100;
        }
        if let Some(p) = self.pos[v] {
            self.sift_up(p);
        }
    }

    pub(super) fn backtrack(&mut self, level: usize) {
        if self.lim.len() <= level {
            return;
        }
        let to = self.lim[level];
        while self.trail.len() > to {
            let l = self.trail.pop().unwrap();
            let v = var(l);
            self.phase[v] = l & 1 == 0;
            self.value[v] = -1;
            self.reason[v] = None;
            if self.pos[v].is_none() {
                self.heap_insert(v);
            }
        }
        self.lim.truncate(level);
        self.head = self.trail.len();
    }
}
