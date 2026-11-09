// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Planted checks for `buildc verify`, written before the first run: a false
//! law is refuted with a counterexample, a true law is proved, a law beyond
//! the engine is unknown and never proved. A false-accept set holds the
//! symbolic prover to false laws whose counterexamples sampling is unlikely
//! to hit.

use super::{verify_module, Outcome, Verdict};
use crate::lexer::{Lexer, SourceFile};
use crate::parser::Parser;

fn run(src: &str) -> Vec<Outcome> {
    let src = src.to_string();
    // Concrete runs recurse; give them a stack like the one `buildc` runs on.
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let sf = SourceFile::new("t.bld", src);
            let tokens = Lexer::new(&sf).tokenize().expect("lex");
            let mut parser = Parser::new(&sf, tokens);
            let module = parser.parse().expect("parse");
            assert!(parser.errors().is_empty(), "{:?}", parser.errors());
            verify_module(&module)
        })
        .unwrap()
        .join()
        .unwrap()
}

fn verdict<'a>(out: &'a [Outcome], name: &str) -> &'a Verdict {
    &out.iter().find(|o| o.name == name).expect(name).verdict
}

fn proved(v: &Verdict) -> bool {
    matches!(v, Verdict::Proved { .. })
}

#[test]
fn planted_false_law_is_refuted_with_a_confirmed_counterexample() {
    let out = run("law bad(a: i64, b: i64) { a - b == b - a }\nfn main() {}");
    match verdict(&out, "bad") {
        Verdict::Refuted { input, .. } => {
            let a: i128 = input[0].1.parse().unwrap();
            let b: i128 = input[1].1.parse().unwrap();
            assert_ne!(a - b, b - a, "the reported input must refute the law");
        }
        other => panic!("expected refuted, got {other:?}"),
    }
}

#[test]
fn planted_true_law_is_proved() {
    let out = run("law comm(a: i64, b: i64) { a + b == b + a }\nfn main() {}");
    assert!(proved(verdict(&out, "comm")), "{out:?}");
}

#[test]
fn planted_unprovable_law_is_unknown_never_proved() {
    let src =
        "fn nat_add(a: u32, b: u32) -> u32 { if a == 0 { b } else { nat_add(a - 1, b) + 1 } }\n\
               law nat_comm(a: u32, b: u32) { nat_add(a, b) == nat_add(b, a) }\nfn main() {}";
    let out = run(src);
    assert!(
        matches!(verdict(&out, "nat_comm"), Verdict::Unknown { .. }),
        "{out:?}"
    );
}

#[test]
fn ensures_false_is_refuted_and_true_is_proved() {
    let src = "fn bad_abs(x: i64) -> i64 ensures result >= 0 { if x > 0 { x } else { x } }\n\
               fn max2(a: i64, b: i64) -> i64 ensures result >= a && result >= b { if a > b { a } else { b } }\n\
               fn main() {}";
    let out = run(src);
    assert!(
        matches!(verdict(&out, "bad_abs"), Verdict::Refuted { .. }),
        "{out:?}"
    );
    assert!(proved(verdict(&out, "max2")), "{out:?}");
}

#[test]
fn a_call_that_breaks_requires_is_refuted() {
    let src = "fn half(x: i64) -> i64 requires x >= 0 ensures result >= 0 { x / 2 }\n\
               law ok(x: i64) requires x >= 0 { half(x) <= x }\n\
               law bad(x: i64) { half(x) <= x + 1 }\nfn main() {}";
    let out = run(src);
    assert!(proved(verdict(&out, "ok")), "{out:?}");
    assert!(proved(verdict(&out, "half")), "{out:?}");
    match verdict(&out, "bad") {
        Verdict::Refuted { why, .. } => assert!(why.contains("requires"), "{why}"),
        other => panic!("expected refuted, got {other:?}"),
    }
}

#[test]
fn small_domains_are_proved_by_exhaustion() {
    let out = run("law rt(x: i8) { ((x as u8) as i8) == x }\nfn main() {}");
    match verdict(&out, "rt") {
        Verdict::Proved { engine } => assert!(engine.starts_with("exhaustive"), "{engine}"),
        other => panic!("expected proved, got {other:?}"),
    }
}

/// False laws whose only counterexamples are rare. The search may or may not
/// find them; the prover must never claim them.
#[test]
fn the_prover_never_accepts_a_false_law() {
    let false_laws = [
        "law f1(a: i64) { a != 123456789 }",
        "law f2(a: i64) { (a / 3) * 3 <= a }",
        "law f3(a: i64, b: i64) requires a > 0 { a * b != 7 * b + 1 }",
        "law f4(a: i64, b: i64) { a <= b || a > b + 1 }",
        "law f5(a: u64) { a % 1000003 != 999999 }",
        "law f6(a: i32, b: i32) { a.wrapping_add(b) != 77 || a + b == 77 }",
        "law f7(a: i64) { a * a != 4 }",
        "law f8(x: i64) { (x ^ 5) != 0 }",
        "law f9(a: i64, b: i64) requires a >= 0 && b >= 0 { a + b >= 1 }",
        "fn g(x: i64) -> i64 { if x == 31337 { 0 } else { 1 } }\nlaw f10(x: i64) { g(x) == 1 }",
    ];
    for src in false_laws {
        let out = run(&format!("{src}\nfn main() {{}}"));
        for o in &out {
            assert!(
                !proved(&o.verdict) && !matches!(o.verdict, Verdict::Inconsistent { .. }),
                "false law accepted: {src}: {:?}",
                o.verdict
            );
        }
    }
}

#[test]
fn true_laws_over_large_domains_are_proved_symbolically() {
    let src = "law l1(a: u64) { (a / 3) * 3 <= a }\n\
               law l2(a: i64, b: i64) { (a <= b) == !(a > b) }\n\
               law l3(a: i32, b: i32, c: i32) { (a ^ b) ^ c == a ^ (b ^ c) }\n\
               law l4(n: u64, m: u64) requires m > 0 { n % m < m }\n\
               fn main() {}";
    let out = run(src);
    for n in ["l1", "l2", "l3", "l4"] {
        assert!(proved(verdict(&out, n)), "{n}: {out:?}");
    }
}
