//! Differential fuzz target: `i64` expressions through the whole compiler.
//!
//! A seeded generator writes expressions, Rust checked arithmetic gives the
//! reference value, and one batched program is compiled and run per seed. The
//! seeds are fixed and recorded here, so a failure names a seed and an
//! expression index and reproduces on any machine. A mismatch is shrunk to a
//! small expression; that expression belongs in `REGRESSIONS`.
//!
//! Two planted defects show the target can fail: a reference with the wrong
//! remainder sign, and one corrupted output line.

#[path = "fuzz_support/int_expr.rs"]
mod int_expr;

use int_expr::{eval, generate, literal, nodes, render, shrink, Expr, Reference, Rng, POOL};
use std::path::PathBuf;
use std::process::Command;

const SEEDS: [u64; 3] = [0x2026_1109, 0x00B1_D1A6, 7];
const CASES_PER_SEED: usize = 150;
const TRAP_CASES: usize = 8;
const DEPTH: u32 = 4;
const TRUE_REFERENCE: Reference = Reference { euclid_rem: false };
const PLANTED_REFERENCE: Reference = Reference { euclid_rem: true };

/// Minimized expressions from earlier mismatches, with arguments and value.
/// The first rows pin the semantics the reference assumes.
const REGRESSIONS: [(&str, [i64; 4], i64); 4] = [
    ("(a % b)", [-7, 2, 0, 0], -1),
    ("(a / b)", [-7, 2, 0, 0], -3),
    ("(a >> 1)", [-7, 0, 0, 0], -4),
    ("(if a < b { (c - d) } else { (-c) })", [1, 1, 5, 9], -5),
];

struct Case {
    expr: Expr,
    args: [i64; 4],
}

fn arguments(rng: &mut Rng) -> [i64; 4] {
    [0; 4].map(|_| POOL[rng.below(POOL.len() as u64) as usize])
}

/// `count` cases with a defined value and `traps` cases the reference refuses.
fn cases(seed: u64, count: usize, traps: usize) -> (Vec<Case>, Vec<Case>) {
    let mut rng = Rng(seed);
    let (mut defined, mut trapping) = (Vec::new(), Vec::new());
    while defined.len() < count || trapping.len() < traps {
        let expr = generate(&mut rng, DEPTH);
        let args = arguments(&mut rng);
        let value = eval(TRUE_REFERENCE, &expr, &args);
        if value.is_some() && defined.len() < count {
            defined.push(Case { expr, args });
        } else if value.is_none() && trapping.len() < traps {
            trapping.push(Case { expr, args });
        }
    }
    (defined, trapping)
}

fn program(bodies: &[(String, [i64; 4])]) -> String {
    let mut text = String::new();
    for (index, (body, _)) in bodies.iter().enumerate() {
        text += &format!("fn e{index}(a: i64, b: i64, c: i64, d: i64) -> i64 {{ {body} }}\n");
    }
    text += "fn main() ~ Console {\n";
    for (index, (_, args)) in bodies.iter().enumerate() {
        let list: Vec<String> = args.iter().map(|n| literal(*n)).collect();
        text += &format!("    println!(\"{{}}\", e{index}({}));\n", list.join(", "));
    }
    text + "}\n"
}

struct Run {
    code: Option<i32>,
    lines: Vec<String>,
    stderr: String,
}

/// Compile and run one program. `None` when the machine has no C compiler.
fn run(tag: &str, source: &str) -> Option<Run> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = root.parent().expect("the compiler sits in the repository");
    let dir = std::env::temp_dir().join(format!("bl-fuzz-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp directory");
    let file = dir.join("main.bld");
    std::fs::write(&file, source).expect("program written");
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", root.join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if stderr.contains("no C compiler") {
        // Set BUILDLANG_FUZZ_REQUIRE_CC=1 where a skipped run must count as a failure.
        assert!(
            std::env::var("BUILDLANG_FUZZ_REQUIRE_CC").as_deref() != Ok("1"),
            "no C compiler, and BUILDLANG_FUZZ_REQUIRE_CC is 1"
        );
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    Some(Run {
        code: out.status.code(),
        lines: stdout.lines().map(|l| l.trim().to_string()).collect(),
        stderr,
    })
}

/// Index of the first line that differs from the reference values.
fn first_mismatch(lines: &[String], want: &[i64]) -> Option<usize> {
    (0..want.len()).find(|i| lines.get(*i).map(String::as_str) != Some(&want[*i].to_string()))
}

fn compiled_value_differs(case: &Case) -> bool {
    let Some(want) = eval(TRUE_REFERENCE, &case.expr, &case.args) else {
        return false;
    };
    let source = program(&[(render(&case.expr), case.args)]);
    run("shrink", &source).is_some_and(|r| first_mismatch(&r.lines, &[want]).is_some())
}

#[test]
fn compiled_values_match_the_reference() {
    for seed in SEEDS {
        let (defined, _) = cases(seed, CASES_PER_SEED, 0);
        let bodies: Vec<_> = defined.iter().map(|c| (render(&c.expr), c.args)).collect();
        let want: Vec<i64> = defined
            .iter()
            .map(|c| eval(TRUE_REFERENCE, &c.expr, &c.args).expect("defined case"))
            .collect();
        let Some(got) = run(&format!("s{seed}"), &program(&bodies)) else {
            return;
        };
        assert_eq!(got.code, Some(0), "seed {seed:#x}: {}", got.stderr);
        if let Some(index) = first_mismatch(&got.lines, &want) {
            let args = defined[index].args;
            let small = shrink(&defined[index].expr, |e| {
                compiled_value_differs(&Case {
                    expr: e.clone(),
                    args,
                })
            });
            panic!(
                "seed {seed:#x} case {index}: compiled {:?}, reference {}. Minimized: {} with {:?}. Add it to REGRESSIONS.",
                got.lines.get(index),
                want[index],
                render(&small),
                args
            );
        }
        assert_eq!(got.lines.len(), CASES_PER_SEED);
    }
}

#[test]
fn reference_traps_are_compiled_traps() {
    let (_, trapping) = cases(SEEDS[0], 0, TRAP_CASES);
    for (index, case) in trapping.iter().enumerate() {
        let source = program(&[(render(&case.expr), case.args)]);
        let Some(got) = run(&format!("t{index}"), &source) else {
            return;
        };
        assert_eq!(
            got.code,
            Some(101),
            "trap case {index} `{}` with {:?} printed {:?}",
            render(&case.expr),
            case.args,
            got.lines
        );
        assert!(got.lines.is_empty());
    }
}

#[test]
fn recorded_regressions_hold() {
    let bodies: Vec<_> = REGRESSIONS
        .iter()
        .map(|(text, args, _)| (text.to_string(), *args))
        .collect();
    let want: Vec<i64> = REGRESSIONS.iter().map(|(_, _, value)| *value).collect();
    let Some(got) = run("reg", &program(&bodies)) else {
        return;
    };
    assert_eq!(got.code, Some(0), "{}", got.stderr);
    assert_eq!(
        first_mismatch(&got.lines, &want),
        None,
        "got {:?}",
        got.lines
    );
}

/// Planted defect 1: a reference with the wrong remainder sign must disagree
/// with the true reference on the recorded seed, and the disagreement must
/// shrink to a remainder with few nodes.
#[test]
fn a_planted_remainder_defect_is_caught_and_minimized() {
    let (defined, _) = cases(SEEDS[0], CASES_PER_SEED, 0);
    let differs = |e: &Expr, args: &[i64; 4]| {
        let truth = eval(TRUE_REFERENCE, e, args);
        truth.is_some() && truth != eval(PLANTED_REFERENCE, e, args)
    };
    let caught: Vec<&Case> = defined
        .iter()
        .filter(|c| differs(&c.expr, &c.args))
        .collect();
    assert!(
        !caught.is_empty(),
        "the seed has no case that separates the two remainders"
    );
    let small = shrink(&caught[0].expr, |e| differs(e, &caught[0].args));
    assert!(differs(&small, &caught[0].args));
    assert!(
        render(&small).contains('%'),
        "minimized to {}",
        render(&small)
    );
    assert!(
        nodes(&small) <= 5,
        "minimized to {} nodes: {}",
        nodes(&small),
        render(&small)
    );
    assert!(nodes(&small) <= nodes(&caught[0].expr));
}

/// Planted defect 2: one wrong output line is found at its index.
#[test]
fn a_corrupted_output_line_is_located() {
    let want = [3, -4, 12];
    let good: Vec<String> = want.iter().map(|n| n.to_string()).collect();
    assert_eq!(first_mismatch(&good, &want), None);
    let mut bad = good.clone();
    bad[1] = "-5".to_string();
    assert_eq!(first_mismatch(&bad, &want), Some(1));
    assert_eq!(first_mismatch(&good[..2], &want), Some(2));
}

#[test]
fn the_generator_is_a_pure_function_of_the_seed() {
    let (first, _) = cases(SEEDS[1], 20, 0);
    let (second, _) = cases(SEEDS[1], 20, 0);
    let text = |cs: &[Case]| cs.iter().map(|c| render(&c.expr)).collect::<Vec<_>>();
    assert_eq!(text(&first), text(&second));
    assert!(text(&first).iter().any(|t| t.contains("if ")));
}
