// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Checks of the bit-precise procedure against independent references: brute
//! force for the SAT solver, the concrete bit semantics for the encoder, and
//! forged proofs for the replay checker.

use super::sat::{lit, Answer as SatAnswer, Solver};
use super::*;
use crate::verify::eval::bit_op;
use crate::verify::term::{arith, bits, cast, cmp, neg as tneg};

fn rng(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[test]
fn sat_agrees_with_brute_force_on_random_3sat() {
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    for round in 0..300 {
        let n = 3 + (round % 8) as u32;
        let m = (n as u64 * 4 + rng(&mut seed) % 6) as usize;
        let mut clauses = Vec::new();
        for _ in 0..m {
            let c: Vec<u32> = (0..3)
                .map(|_| lit((rng(&mut seed) % n as u64) as u32, rng(&mut seed) % 2 == 1))
                .collect();
            clauses.push(c);
        }
        let brute = (0..1u32 << n).any(|a| {
            clauses.iter().all(|c| {
                c.iter()
                    .any(|&l| ((a >> (l >> 1)) & 1 == 1) != (l & 1 == 1))
            })
        });
        let mut s = Solver::default();
        for _ in 0..n {
            s.new_var();
        }
        for c in &clauses {
            s.add(c.clone());
        }
        let original = s.clauses.clone();
        match s.solve(1_000_000) {
            SatAnswer::Sat(model) => {
                assert!(brute, "round {round}: SAT on an UNSAT formula");
                for c in &clauses {
                    assert!(c.iter().any(|&l| model[(l >> 1) as usize] != (l & 1 == 1)));
                }
            }
            SatAnswer::Unsat => {
                assert!(!brute, "round {round}: UNSAT on a SAT formula");
                assert!(rup::check(&original, &s.learned, n as usize, u64::MAX));
            }
            SatAnswer::Unknown => panic!("budget"),
        }
    }
}

#[test]
fn the_replay_refuses_a_forged_proof() {
    // (a or b) and (not a or b): satisfiable, with b true.
    let (a, b) = (lit(0, false), lit(1, false));
    let original = vec![vec![a, b], vec![a ^ 1, b]];
    // A forged "learned" unit clause `not b` does not follow by propagation.
    assert!(!rup::check(&original, &[vec![b ^ 1]], 2, u64::MAX));
    // No learned clauses: propagation alone finds no conflict.
    assert!(!rup::check(&original, &[], 2, u64::MAX));
}

/// The value of a term under `env`, by the concrete definitions.
fn ev(t: &Term, env: &HashMap<String, i128>) -> Option<i128> {
    Some(match t {
        Term::Int(v) => *v,
        Term::Var(n, _) => env[n],
        Term::Arith(AOp::Add, a, b) => ev(a, env)? + ev(b, env)?,
        Term::Arith(AOp::Sub, a, b) => ev(a, env)? - ev(b, env)?,
        Term::Arith(AOp::Mul, a, b) => ev(a, env)?.checked_mul(ev(b, env)?)?,
        Term::Neg(a) => -ev(a, env)?,
        Term::Cast(ty, a) => ty.wrap(ev(a, env)?),
        Term::BitNot(ty, a) => ty.wrap(!ev(a, env)?),
        Term::Bits(op, ty, a, b) => bit_op(*op, *ty, ev(a, env)?, ev(b, env)?)?,
        _ => return None,
    })
}

const TYS: [Ty; 4] = [
    Ty::Int {
        bits: 8,
        signed: true,
    },
    Ty::Int {
        bits: 8,
        signed: false,
    },
    Ty::Int {
        bits: 16,
        signed: true,
    },
    Ty::Int {
        bits: 32,
        signed: false,
    },
];

fn random_term(seed: &mut u64, vars: &[(String, Ty)], depth: u32) -> Term {
    let pick = rng(seed) % if depth == 0 { 2 } else { 11 };
    let ty = TYS[(rng(seed) % 4) as usize];
    let sub = |seed: &mut u64| random_term(seed, vars, depth - 1);
    match pick {
        0 => Term::Int((rng(seed) % 300) as i128 - 150),
        1 => {
            let (n, t) = &vars[(rng(seed) % vars.len() as u64) as usize];
            Term::Var(n.clone(), *t)
        }
        2 => arith(AOp::Add, sub(seed), sub(seed)),
        3 => arith(AOp::Sub, sub(seed), sub(seed)),
        4 => arith(AOp::Mul, sub(seed), sub(seed)),
        5 => tneg(sub(seed)),
        6 => cast(ty, None, sub(seed)),
        7 => Term::BitNot(ty, Box::new(sub(seed))),
        8 => bits(BOp::And, ty, sub(seed), sub(seed)),
        9 => {
            let op = [BOp::Or, BOp::Xor, BOp::WAdd, BOp::WMul][(rng(seed) % 4) as usize];
            bits(op, ty, sub(seed), sub(seed))
        }
        _ => {
            let op = if rng(seed) % 2 == 0 {
                BOp::Shl
            } else {
                BOp::Shr
            };
            bits(op, ty, sub(seed), sub(seed))
        }
    }
}

/// Encoder against the concrete semantics: for random terms and inputs, the
/// claim `t == value` must be proved and `t == value + 1` must not be.
fn differential(plant: bool, rounds: usize) -> usize {
    let vars = vec![
        ("x".to_string(), TYS[0]),
        ("y".to_string(), TYS[1]),
        ("z".to_string(), TYS[2]),
    ];
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut wrong = 0;
    let mut done = 0;
    while done < rounds {
        let t = random_term(&mut seed, &vars, 3);
        let env: HashMap<String, i128> = vars
            .iter()
            .map(|(n, ty)| {
                let (lo, hi) = ty.range().unwrap();
                let v = lo + (rng(&mut seed) as i128).rem_euclid(hi - lo + 1);
                (n.clone(), v)
            })
            .collect();
        let Some(v) = ev(&t, &env) else { continue };
        done += 1;
        let hyps: Vec<Term> = vars
            .iter()
            .map(|(n, ty)| cmp(COp::Eq, Term::Var(n.clone(), *ty), Term::Int(env[n])))
            .collect();
        let right = decide(
            &hyps,
            &Term::Cmp(COp::Eq, Box::new(t.clone()), Box::new(Term::Int(v))),
            plant,
        );
        let off = decide(
            &hyps,
            &Term::Cmp(COp::Eq, Box::new(t.clone()), Box::new(Term::Int(v + 1))),
            plant,
        );
        if right != Answer::Valid || off == Answer::Valid {
            wrong += 1;
        }
    }
    wrong
}

#[test]
fn the_encoder_matches_the_concrete_semantics() {
    assert_eq!(differential(false, 400), 0);
}

#[test]
fn a_planted_adder_fault_is_caught_by_the_differential_check() {
    assert!(
        differential(true, 400) > 0,
        "the planted carry fault went unseen"
    );
}

#[test]
fn decides_simple_obligations() {
    let x = Term::Var("x".into(), Ty::I64);
    let y = Term::Var("y".into(), Ty::I64);
    let range = |t: &Term| {
        vec![
            cmp(COp::Ge, t.clone(), Term::Int(i64::MIN as i128)),
            cmp(COp::Le, t.clone(), Term::Int(i64::MAX as i128)),
        ]
    };
    let mut hyps = range(&x);
    hyps.extend(range(&y));
    // x & (x - 1) clears the lowest set bit: never larger than x when x > 0.
    let mut h = hyps.clone();
    h.push(cmp(COp::Gt, x.clone(), Term::Int(0)));
    let g = cmp(
        COp::Le,
        bits(
            BOp::And,
            Ty::I64,
            x.clone(),
            arith(AOp::Sub, x.clone(), Term::Int(1)),
        ),
        x.clone(),
    );
    assert_eq!(decide(&h, &g, false), Answer::Valid);
    // A needle: x != 4611686018427375000 is false at exactly one input.
    let g = cmp(COp::Ne, x.clone(), Term::Int(4_611_686_018_427_375_000));
    match decide(&hyps, &g, false) {
        Answer::Candidate(m) => assert!(m.contains(&("x".to_string(), 4_611_686_018_427_375_000))),
        other => panic!("{other:?}"),
    }
}
