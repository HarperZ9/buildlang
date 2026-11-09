// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Exact division in the polynomial normal form (V3, rule M1).

use super::poly::Poly;
use std::collections::BTreeMap;

/// The test-only fault `BUILDLANG_PLANT_POLY=div-inexact`: exact division
/// ignores the monomials and coefficients the divisor does not divide.
fn plant_div_inexact() -> bool {
    static P: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *P.get_or_init(|| std::env::var("BUILDLANG_PLANT_POLY").as_deref() == Ok("div-inexact"))
}

/// `q` with `n = d * q` exactly, when `d` is a single monomial `c * m` that
/// divides every monomial of `n`, coefficient included.
pub fn exact_div(n: &Poly, d: &Poly) -> Option<Poly> {
    let (dm, dc) = match d.0.iter().collect::<Vec<_>>().as_slice() {
        [(m, c)] => ((*m).clone(), **c),
        _ => return None,
    };
    if n.0.is_empty() {
        return None;
    }
    let plant = plant_div_inexact();
    let mut q = BTreeMap::new();
    for (m, c) in &n.0 {
        let mut rest = m.clone();
        let mut divides = c % dc == 0;
        for a in &dm {
            match rest.iter().position(|x| x == a) {
                Some(i) => {
                    rest.remove(i);
                }
                None => divides = false,
            }
        }
        if !divides {
            if plant {
                continue;
            }
            return None;
        }
        q.insert(rest, c / dc);
    }
    Some(Poly(q))
}

#[cfg(test)]
mod tests {
    use super::exact_div;
    use crate::verify::poly::Poly;

    fn v(n: &str) -> Poly {
        Poly::atom(format!("v:{n}"))
    }

    #[test]
    fn only_exact_quotients_are_taken() {
        let ab = v("a").mul(&v("b")).unwrap();
        assert_eq!(exact_div(&ab, &v("b")), Some(v("a")));
        let ab1 = ab.add(&Poly::constant(1)).unwrap();
        assert_eq!(exact_div(&ab1, &v("b")), None);
        let six_x = v("x").scale(6).unwrap();
        assert_eq!(exact_div(&six_x, &Poly::constant(3)), v("x").scale(2));
        let seven_x = v("x").scale(7).unwrap();
        assert_eq!(exact_div(&seven_x, &Poly::constant(3)), None);
        let b_plus_c = v("b").add(&v("c")).unwrap();
        assert_eq!(exact_div(&ab, &b_plus_c), None);
    }
}
