//! V2, the bit-precise solver, and V3, the word-level rules (each preregistered
//! in docs/VERIFY.md before the run). The refusal set: no false claim is
//! proved. The acceptance set: no true claim is refuted, and the claims the
//! linear prover could not close are proved. A planted wrong division rule is
//! caught by the refusal set.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn verify(file: &str, plant: bool) -> Value {
    verify_with(file, plant, false)
}

fn verify_with(file: &str, plant: bool, plant_div: bool) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/verify")
        .join(file);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.args(["verify", "--json"]).arg(path);
    if plant {
        cmd.env("BUILDLANG_PLANT_SMT", "drop-carry");
    } else {
        cmd.env_remove("BUILDLANG_PLANT_SMT");
    }
    if plant_div {
        cmd.env("BUILDLANG_PLANT_POLY", "div-inexact");
    } else {
        cmd.env_remove("BUILDLANG_PLANT_POLY");
    }
    let o = cmd.output().expect("buildc runs");
    serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {}", String::from_utf8_lossy(&o.stderr)))
}

fn verdicts(doc: &Value) -> Vec<(String, String)> {
    doc["obligations"]
        .as_array()
        .or_else(|| doc["results"].as_array())
        .expect("obligations")
        .iter()
        .map(|o| {
            (
                o["name"].as_str().unwrap().to_string(),
                o["verdict"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn no_false_claim_is_proved() {
    let doc = verify("smt_false.bld", false);
    assert_eq!(doc["summary"]["proved"], 0, "{doc}");
    assert_eq!(doc["summary"]["inconsistent"], 0, "{doc}");
    let refuted = doc["summary"]["refuted"].as_u64().unwrap();
    // V3: every claim of the extended set is refuted, `sum_sq` included.
    assert_eq!(refuted, 29, "{doc}");
}

const BY_SAT: [&str; 7] = [
    "lowbit_lt",
    "shl_is_mul",
    "square_nonneg",
    "mask_low",
    "or_ge",
    "and_le",
    "avg_no_overflow",
];

#[test]
fn no_true_claim_is_refuted_and_the_bit_level_ones_are_proved() {
    let doc = verify("smt_true.bld", false);
    assert_eq!(doc["summary"]["refuted"], 0, "{doc}");
    assert_eq!(doc["summary"]["inconsistent"], 0, "{doc}");
    for (name, verdict) in verdicts(&doc) {
        if BY_SAT.contains(&name.as_str()) {
            assert_eq!(verdict, "proved", "{name}: {doc}");
        }
    }
}

/// Closed in V3 by the word-level rules (exact division, wrapping identity).
const BY_WORD: [&str; 4] = [
    "mul_div",
    "inverse_round_trip",
    "div_exact_scaled",
    "rem_exact",
];

#[test]
fn the_word_level_claims_are_proved() {
    let doc = verify("smt_true.bld", false);
    assert_eq!(doc["summary"]["proved"], 16, "{doc}");
    for (name, verdict) in verdicts(&doc) {
        if BY_WORD.contains(&name.as_str()) {
            assert_eq!(verdict, "proved", "{name}: {doc}");
        }
    }
}

#[test]
fn a_planted_inexact_division_rule_is_caught() {
    let doc = verify_with("smt_false.bld", false, true);
    let proved = doc["summary"]["proved"].as_u64().unwrap();
    let inconsistent = doc["summary"]["inconsistent"].as_u64().unwrap();
    assert!(proved + inconsistent > 0, "{doc}");
}
