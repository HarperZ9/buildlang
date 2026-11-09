// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Bit operators, shifts, the encoder's self-check, and reading a model.

use super::*;

impl Enc {
    /// Keep the low `ty.bits` bits and extend by sign (signed) or zero.
    pub(super) fn wrap(&self, x: &[Lit], ty: Ty) -> Vec<Lit> {
        let (bits, signed) = match ty {
            Ty::Int { bits, signed } => (bits as usize, signed),
            _ => return x.to_vec(),
        };
        let fill = if signed { x[bits - 1] } else { self.fls() };
        (0..self.w)
            .map(|i| if i < bits { x[i] } else { fill })
            .collect()
    }

    pub(super) fn shift(&mut self, x: &[Lit], amount: &[Lit], left: bool, fill: Lit) -> Vec<Lit> {
        let mut cur = x.to_vec();
        for (k, &a) in amount.iter().enumerate() {
            let by = 1usize << k;
            let moved: Vec<Lit> = (0..self.w)
                .map(|i| {
                    if left {
                        if i >= by {
                            cur[i - by]
                        } else {
                            self.fls()
                        }
                    } else if i + by < self.w {
                        cur[i + by]
                    } else {
                        fill
                    }
                })
                .collect();
            cur = (0..self.w).map(|i| self.mux(a, moved[i], cur[i])).collect();
        }
        cur
    }

    pub(super) fn bits(&mut self, op: BOp, ty: Ty, a: &Term, b: &Term) -> Result<Vec<Lit>, String> {
        let (x, y) = (self.int(a)?, self.int(b)?);
        let bits = ty.bits() as usize;
        let r = match op {
            BOp::And | BOp::Or | BOp::Xor => {
                let z: Vec<Lit> = (0..self.w)
                    .map(|i| match op {
                        BOp::And => self.and(x[i], y[i]),
                        BOp::Or => self.or(x[i], y[i]),
                        _ => self.xor(x[i], y[i]),
                    })
                    .collect();
                self.wrap(&z, ty)
            }
            BOp::WAdd => {
                let z = self.add(&x, &y);
                self.wrap(&z, ty)
            }
            BOp::WMul => {
                let z = self.mul(&x, &y);
                self.wrap(&z, ty)
            }
            BOp::Shl | BOp::Shr => {
                let k = bits.trailing_zeros() as usize;
                let amount = y[..k].to_vec();
                match (op, ty) {
                    (BOp::Shl, _) => {
                        let f = self.fls();
                        let z = self.shift(&x, &amount, true, f);
                        self.wrap(&z, ty)
                    }
                    (_, Ty::Int { signed: true, .. }) => {
                        let fill = x[self.w - 1];
                        self.shift(&x, &amount, false, fill)
                    }
                    _ => {
                        let u = self.wrap(
                            &x,
                            Ty::Int {
                                bits: bits as u8,
                                signed: false,
                            },
                        );
                        let f = self.fls();
                        self.shift(&u, &amount, false, f)
                    }
                }
            }
        };
        Ok(r)
    }

    /// A self-check before any use: constant operations, which the gates fold
    /// to constants, must give the values the concrete arithmetic gives. A
    /// fault in an adder, multiplier, shifter or comparison shows up here.
    pub fn self_check(&mut self) -> bool {
        let lim = 1i128 << (self.w.min(120) - 2) / 2;
        let cases: [(i128, i128); 6] = [
            (1, 1),
            (-1, 1),
            (lim - 1, 1),
            (-lim, lim - 1),
            (lim / 3, -(lim / 5)),
            (-(lim / 7), -1),
        ];
        for (a, b) in cases {
            let (x, y) = (self.konst(a), self.konst(b));
            let sum = self.add(&x, &y);
            let diff = self.sub(&x, &y);
            let prod = self.mul(&x, &y);
            let lt = self.slt(&x, &y);
            let checks = [(sum, a + b), (diff, a - b), (prod, a * b)];
            for (v, want) in checks {
                let k = self.konst(want);
                if self.eq(&v, &k) != self.t {
                    return false;
                }
            }
            if (lt == self.t) != (a < b) || (lt != self.t && lt != self.fls()) {
                return false;
            }
        }
        true
    }
}

impl Enc {
    /// The value of an integer vector in a model.
    pub fn read(&self, v: &[Lit], model: &[bool]) -> i128 {
        let bit = |l: Lit| model[(l >> 1) as usize] ^ (l & 1 == 1);
        let mut x: i128 = 0;
        for (i, &l) in v.iter().enumerate().take(127) {
            if bit(l) {
                x |= 1 << i;
            }
        }
        if v.len() < 127 && bit(v[v.len() - 1]) {
            x -= 1i128 << v.len();
        }
        x
    }

    pub fn bit(&self, l: Lit, model: &[bool]) -> bool {
        model[(l >> 1) as usize] ^ (l & 1 == 1)
    }
}
