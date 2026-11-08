//! `buildc verify` and `buildc check` on the law files in `tests/verify`.
//!
//! The expected verdict of every obligation is written here, before the run
//! it governs: the ported Bend laws, BuildLang's own laws and the planted
//! checks. A planted false law must be refuted, a true one proved, and a law
//! beyond the engine reported unknown. `check` fails (BL0701) only on a
//! refutation.

use serde_json::Value;
use std::{path::PathBuf, process::Command};

fn law_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/verify")
        .join(name)
}

fn verify_json(name: &str) -> (Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .args(["verify", "--json"])
        .arg(law_file(name))
        .output()
        .expect("buildc runs");
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{name}: bad JSON ({e}): {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (doc, out.status.code().unwrap_or(-1))
}

fn assert_verdicts(name: &str, expected: &[(&str, &str)]) -> i32 {
    let (doc, code) = verify_json(name);
    assert_eq!(doc["schema"], "buildlang-verify/v1");
    let obligations = doc["obligations"].as_array().unwrap();
    assert_eq!(
        obligations.len(),
        expected.len(),
        "{name}: obligation count"
    );
    for (law, want) in expected {
        let got = obligations
            .iter()
            .find(|o| o["name"] == *law)
            .unwrap_or_else(|| panic!("{name}: no obligation {law}"));
        assert_eq!(got["verdict"], *want, "{name}: {law}: {got}");
        if *want == "refuted" {
            assert!(got["detail"]["counterexample"].is_object(), "{law}: {got}");
        }
    }
    code
}

#[test]
fn planted_checks_hold() {
    let code = assert_verdicts(
        "planted.bld",
        &[
            ("bad_sub_comm", "refuted"),
            ("add_comm", "proved"),
            ("nat_add_comm", "unknown"),
            ("bad_abs", "refuted"),
            ("max2", "proved"),
        ],
    );
    assert_eq!(code, 1, "a refuted law makes verify exit 1");
}

#[test]
fn bend_laws_ported() {
    let code = assert_verdicts(
        "bend_laws.bld",
        &[
            ("add_comm_rec", "proved"),
            ("add_assoc_rec", "proved"),
            ("mul_comm_rec", "proved"),
            ("mul_dist_rec", "proved"),
            ("divmod_ok_rec", "proved"),
            ("add_comm_rec_u32", "unknown"),
            ("add_comm", "proved"),
            ("add_assoc", "proved"),
            ("mul_comm", "proved"),
            ("mul_dist", "proved"),
            ("divmod_ok", "proved"),
            ("tree_is_seq", "proved"),
            ("last_press", "proved"),
        ],
    );
    assert_eq!(code, 0);
}

#[test]
fn own_laws() {
    let code = assert_verdicts(
        "own_laws.bld",
        &[
            ("imul_comm", "proved"),
            ("imul_assoc", "proved"),
            ("xor_self", "proved"),
            ("xor_assoc", "proved"),
            ("xor_cancel", "proved"),
            ("i8_u8_roundtrip", "proved"),
            ("planted_wadd_is_add", "refuted"),
            ("zigzag_roundtrip", "proved"),
            ("pack_roundtrip", "proved"),
            ("planted_bad_unpack", "refuted"),
            ("reaction_atoms", "proved"),
            ("planted_leaky_transfer", "refuted"),
            ("damp_twice", "proved"),
            ("planted_damp_negative", "refuted"),
            ("damp", "proved"),
            ("smooth", "proved"),
        ],
    );
    assert_eq!(code, 1);
}

#[test]
fn check_fails_on_a_refuted_law_and_passes_otherwise() {
    let run = |name: &str| {
        Command::new(env!("CARGO_BIN_EXE_buildc"))
            .args(["check", "--error-format", "json"])
            .arg(law_file(name))
            .output()
            .expect("buildc runs")
    };
    let bad = run("planted.bld");
    assert!(!bad.status.success());
    let text = String::from_utf8_lossy(&bad.stdout);
    assert!(text.contains("BL0701"), "{text}");
    assert!(text.contains("bad_sub_comm"), "{text}");
    let good = run("bend_laws.bld");
    assert!(
        good.status.success(),
        "{}{}",
        String::from_utf8_lossy(&good.stdout),
        String::from_utf8_lossy(&good.stderr)
    );
}
