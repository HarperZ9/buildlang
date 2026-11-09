// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Bit-blasting of verification terms into CNF (Tseitin encoding).
//!
//! Every integer term becomes a `w`-bit two's-complement vector, with `w`
//! chosen by [`super::width`] so that every subterm's exact value fits: then
//! `+ - *` and the comparisons are exact, never modular. The bit operators
//! follow the concrete evaluator's definitions (`eval::bit_op`) at their
//! declared type. Division and remainder terms are fresh vectors bounded by
//! their operands; the hypotheses carry the equations that define them.

use super::sat::{lit, neg, Lit, Solver};
use crate::verify::term::{AOp, BOp, COp, Term};
use crate::verify::ty::Ty;
use std::collections::HashMap;

mod ops;

pub struct Enc {
    pub s: Solver,
    t: Lit,
    pub w: usize,
    /// Test-only fault: every adder drops its carry.
    pub plant_drop_carry: bool,
    ints: HashMap<Term, Vec<Lit>>,
    bools: HashMap<Term, Lit>,
    /// Integer and bool atoms, for reading a model back.
    pub int_atoms: Vec<(Term, Vec<Lit>)>,
    pub bool_atoms: Vec<(Term, Lit)>,
    /// Bounds of atoms, from the hypotheses or their operands.
    pub bounds: HashMap<Term, (i128, i128)>,
}

impl Enc {
    pub fn new(w: usize) -> Enc {
        let mut s = Solver::default();
        let v = s.new_var();
        let t = lit(v, false);
        s.add(vec![t]);
        Enc {
            s,
            t,
            w,
            plant_drop_carry: false,
            ints: HashMap::new(),
            bools: HashMap::new(),
            int_atoms: Vec::new(),
            bool_atoms: Vec::new(),
            bounds: HashMap::new(),
        }
    }

    pub fn tru(&self) -> Lit {
        self.t
    }
    fn fls(&self) -> Lit {
        neg(self.t)
    }
    fn fresh(&mut self) -> Lit {
        lit(self.s.new_var(), false)
    }

    pub fn and(&mut self, a: Lit, b: Lit) -> Lit {
        let (t, f) = (self.t, self.fls());
        if a == f || b == f || a == neg(b) {
            return f;
        }
        if a == t || a == b {
            return b;
        }
        if b == t {
            return a;
        }
        let x = self.fresh();
        self.s.add(vec![neg(x), a]);
        self.s.add(vec![neg(x), b]);
        self.s.add(vec![x, neg(a), neg(b)]);
        x
    }
    pub fn or(&mut self, a: Lit, b: Lit) -> Lit {
        neg(self.and(neg(a), neg(b)))
    }
    pub fn xor(&mut self, a: Lit, b: Lit) -> Lit {
        let (t, f) = (self.t, self.fls());
        match () {
            _ if a == f => return b,
            _ if b == f => return a,
            _ if a == t => return neg(b),
            _ if b == t => return neg(a),
            _ if a == b => return f,
            _ if a == neg(b) => return t,
            _ => {}
        }
        let x = self.fresh();
        self.s.add(vec![neg(x), a, b]);
        self.s.add(vec![neg(x), neg(a), neg(b)]);
        self.s.add(vec![x, neg(a), b]);
        self.s.add(vec![x, a, neg(b)]);
        x
    }
    fn mux(&mut self, c: Lit, a: Lit, b: Lit) -> Lit {
        let x = self.and(c, a);
        let y = self.and(neg(c), b);
        self.or(x, y)
    }

    pub fn konst(&self, v: i128) -> Vec<Lit> {
        (0..self.w)
            .map(|i| {
                let bit = if i < 127 {
                    (v >> i) & 1
                } else {
                    (v < 0) as i128
                };
                if bit == 1 {
                    self.t
                } else {
                    self.fls()
                }
            })
            .collect()
    }

    fn add_c(&mut self, x: &[Lit], y: &[Lit], mut c: Lit) -> Vec<Lit> {
        let mut out = Vec::with_capacity(x.len());
        for i in 0..x.len() {
            let p = self.xor(x[i], y[i]);
            out.push(self.xor(p, c));
            let g = self.and(x[i], y[i]);
            let q = self.and(c, p);
            c = if self.plant_drop_carry {
                self.fls()
            } else {
                self.or(g, q)
            };
        }
        out
    }
    pub fn add(&mut self, x: &[Lit], y: &[Lit]) -> Vec<Lit> {
        let f = self.fls();
        self.add_c(x, y, f)
    }
    pub fn sub(&mut self, x: &[Lit], y: &[Lit]) -> Vec<Lit> {
        let ny: Vec<Lit> = y.iter().map(|&l| neg(l)).collect();
        let t = self.t;
        self.add_c(x, &ny, t)
    }
    pub fn neg_bv(&mut self, x: &[Lit]) -> Vec<Lit> {
        let z = self.konst(0);
        self.sub(&z, x)
    }
    pub fn mul(&mut self, x: &[Lit], y: &[Lit]) -> Vec<Lit> {
        let n = x.len();
        let mut acc = self.konst(0);
        acc.truncate(n);
        for (i, &yi) in y.iter().enumerate().take(n) {
            if yi == self.fls() {
                continue;
            }
            let mut row = vec![self.fls(); n];
            for j in 0..n - i {
                row[i + j] = self.and(x[j], yi);
            }
            acc = self.add(&acc, &row);
        }
        acc
    }
    pub fn eq(&mut self, x: &[Lit], y: &[Lit]) -> Lit {
        let mut r = self.t;
        for i in 0..x.len() {
            let d = self.xor(x[i], y[i]);
            r = self.and(r, neg(d));
        }
        r
    }
    /// Signed `x < y`: the sign of `x - y` computed one bit wider.
    pub fn slt(&mut self, x: &[Lit], y: &[Lit]) -> Lit {
        let mut a = x.to_vec();
        a.push(*x.last().unwrap());
        let mut b = y.to_vec();
        b.push(*y.last().unwrap());
        let d = self.sub(&a, &b);
        *d.last().unwrap()
    }

    fn atom(&mut self, t: &Term) -> Vec<Lit> {
        let v: Vec<Lit> = (0..self.w).map(|_| self.fresh()).collect();
        if let Some(&(lo, hi)) = self.bounds.get(t) {
            let (l, h) = (self.konst(lo), self.konst(hi));
            let below = self.slt(&v, &l);
            let above = self.slt(&h, &v);
            self.s.add(vec![neg(below)]);
            self.s.add(vec![neg(above)]);
        }
        self.int_atoms.push((t.clone(), v.clone()));
        v
    }

    /// The bit vector of an integer term.
    pub fn int(&mut self, t: &Term) -> Result<Vec<Lit>, String> {
        if let Some(v) = self.ints.get(t) {
            return Ok(v.clone());
        }
        let v = match t {
            Term::Int(v) => self.konst(*v),
            Term::Var(_, ty) if ty.is_int() => self.atom(t),
            Term::App(..) | Term::Arith(AOp::Div | AOp::Rem, ..) => self.atom(t),
            Term::Arith(op, a, b) => {
                let (x, y) = (self.int(a)?, self.int(b)?);
                match op {
                    AOp::Add => self.add(&x, &y),
                    AOp::Sub => self.sub(&x, &y),
                    _ => self.mul(&x, &y),
                }
            }
            Term::Neg(a) => {
                let x = self.int(a)?;
                self.neg_bv(&x)
            }
            Term::Cast(ty, a) => {
                let x = self.int(a)?;
                self.wrap(&x, *ty)
            }
            Term::BitNot(ty, a) => {
                let x: Vec<Lit> = self.int(a)?.iter().map(|&l| neg(l)).collect();
                self.wrap(&x, *ty)
            }
            Term::Bits(op, ty, a, b) => self.bits(*op, *ty, a, b)?,
            other => return Err(format!("not an integer term: {other:?}")),
        };
        self.ints.insert(t.clone(), v.clone());
        Ok(v)
    }

    /// The literal of a boolean term.
    pub fn boolean(&mut self, t: &Term) -> Result<Lit, String> {
        if let Some(&l) = self.bools.get(t) {
            return Ok(l);
        }
        let l = match t {
            Term::Bool(b) => {
                if *b {
                    self.t
                } else {
                    self.fls()
                }
            }
            Term::Var(_, Ty::Bool) | Term::App(..) => {
                let l = self.fresh();
                self.bool_atoms.push((t.clone(), l));
                l
            }
            Term::Not(a) => neg(self.boolean(a)?),
            Term::And(a, b) => {
                let (x, y) = (self.boolean(a)?, self.boolean(b)?);
                self.and(x, y)
            }
            Term::Or(a, b) => {
                let (x, y) = (self.boolean(a)?, self.boolean(b)?);
                self.or(x, y)
            }
            Term::Iff(a, b) => {
                let (x, y) = (self.boolean(a)?, self.boolean(b)?);
                neg(self.xor(x, y))
            }
            Term::Cmp(op, a, b) => {
                let (x, y) = (self.int(a)?, self.int(b)?);
                match op {
                    COp::Eq => self.eq(&x, &y),
                    COp::Ne => neg(self.eq(&x, &y)),
                    COp::Lt => self.slt(&x, &y),
                    COp::Gt => self.slt(&y, &x),
                    COp::Le => neg(self.slt(&y, &x)),
                    COp::Ge => neg(self.slt(&x, &y)),
                }
            }
            other => return Err(format!("not a boolean term: {other:?}")),
        };
        self.bools.insert(t.clone(), l);
        Ok(l)
    }
}
