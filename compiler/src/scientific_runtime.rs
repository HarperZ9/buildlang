// ===============================================================================
// BUILDLANG COMPILER - SCIENTIFIC-RUNTIME RECEIPT MODULE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================
//
//! Scientific-runtime receipt (`buildlang-scientific-runtime-receipt/v0`).
//!
//! `buildc run <file> --emit-receipt <path>` compiles and runs a `.bld` program,
//! captures its numeric stdout as a measurement series, checks a stated
//! **invariant** over that series, and emits a sealed, re-checkable JSON receipt.
//!
//! Honest scope (read `docs/superpowers/specs/2026-07-01-scientific-runtime-receipt-design.md`):
//! the receipt witnesses that the compiled program's observed output series
//! satisfies (or, for a negative fixture, expectedly violates) the stated
//! invariant. It does NOT prove the underlying PDE is solved correctly, and it
//! does NOT claim a new physical law. Every receipt carries the label
//! `NOT_A_NEW_PHYSICAL_LAW`. v0 is deliberately one invariant (monotone
//! non-increasing) over one f64 series, sealed and re-derivable.

use std::path::Path;

use super::{source_digest_hex, CheckReceiptSourceDigest};

/// Schema id for the scientific-runtime receipt.
pub const SCIENTIFIC_RUNTIME_SCHEMA: &str = "buildlang-scientific-runtime-receipt/v0";

/// The invariant name emitted for the energy-monotone check.
pub const ENERGY_MONOTONE_INVARIANT: &str = "energy_monotone_nonincreasing";

/// Tolerance used by the monotone-non-increasing check. A step counts as an
/// increase only when `series[k+1] > series[k] + TOLERANCE`, so platform float
/// jitter at the ULP scale does not flip the verdict (design determinism rule).
pub const ENERGY_MONOTONE_TOLERANCE: f64 = 1e-12;

/// Provenance reference to the Telos pass-0009 research probe (reference only;
/// never matched byte-wise, per the determinism decision in the design).
pub const RESEARCH_SOURCE_HASH: &str =
    "b3021c14b0e5dc8adeddadf0d22e2780dbf259c349caf5cbc2ba255b591fd7d5";

// `{algorithm:"sha256", hex}` digest shape. We re-declare a local, owned,
// serde-round-trippable copy so the receipt deserializes back cleanly; it is
// structurally compatible with `super::CheckReceiptSourceDigest` (which is
// serialize-only with a `&'static str` algorithm).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificDigest {
    pub algorithm: String,
    pub hex: String,
}

impl From<&CheckReceiptSourceDigest> for ScientificDigest {
    fn from(value: &CheckReceiptSourceDigest) -> Self {
        ScientificDigest {
            algorithm: value.algorithm.to_string(),
            hex: value.hex.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificBuildState {
    pub target: String,
    pub compiler_status: String,
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificRuntimeState {
    pub os: String,
    pub exit_code: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificProblem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScientificMeasurement {
    pub metric: String,
    pub observed_values: Vec<f64>,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
}

/// The observed outcome of the monotone-non-increasing check over a series.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InvariantObserved {
    /// Number of steps `k` where `series[k+1] > series[k] + tolerance`.
    pub increase_count: usize,
    /// Zero-based index `k` of the first offending step, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_increase_step: Option<usize>,
    /// First series value (if the series is non-empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_value: Option<f64>,
    /// Last series value (if the series is non-empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_value: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScientificInvariant {
    pub name: String,
    pub expectation: String,
    pub tolerance: f64,
    pub observed: InvariantObserved,
    /// `PASS` | `FAIL`.
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScientificProvenance {
    pub research_source_hash: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScientificRuntimeReceipt {
    pub schema: String,
    pub compiler: String,
    pub compiler_version: String,
    pub language_version: String,
    pub source: String,
    pub source_digest: ScientificDigest,
    pub input_graph_digest: ScientificDigest,
    pub build_state: ScientificBuildState,
    pub runtime_state: ScientificRuntimeState,
    pub problem: ScientificProblem,
    pub measurement: ScientificMeasurement,
    pub invariant: ScientificInvariant,
    pub negative_fixture: bool,
    pub labels: Vec<String>,
    pub receipt_status: String,
    pub seal: ScientificDigest,
    pub provenance: ScientificProvenance,
}

/// Energy-monotone-non-increasing invariant over a measured series.
///
/// Counts steps `k` where `series[k + 1] > series[k] + tol`. Records the first
/// offending step (if any) and the initial/final values. The verdict is derived
/// separately in [`invariant_passes`]: PASS requires at least two points AND
/// zero increases. A single-point (or empty) series has nothing to compare, so
/// it cannot witness monotonicity and is not a PASS.
pub fn energy_monotone_nonincreasing(series: &[f64], tol: f64) -> InvariantObserved {
    let mut increase_count = 0usize;
    let mut first_increase_step = None;
    for k in 0..series.len().saturating_sub(1) {
        if series[k + 1] > series[k] + tol {
            increase_count += 1;
            if first_increase_step.is_none() {
                first_increase_step = Some(k);
            }
        }
    }
    InvariantObserved {
        increase_count,
        first_increase_step,
        initial_value: series.first().copied(),
        final_value: series.last().copied(),
    }
}

/// The invariant PASSes iff the series has at least two points and no step
/// increased the energy beyond tolerance.
pub fn invariant_passes(series_len: usize, observed: &InvariantObserved) -> bool {
    series_len >= 2 && observed.increase_count == 0
}

/// Inputs threaded from `cmd_run` into the receipt builder.
pub struct ScientificReceiptInputs<'a> {
    pub source_path: &'a Path,
    pub compiler_version: &'a str,
    pub language_version: String,
    pub source_digest: ScientificDigest,
    pub input_graph_digest: ScientificDigest,
    pub target: &'a str,
    pub os: &'a str,
    pub exit_code: i32,
    pub series: Vec<f64>,
    /// Whether the raw stdout produced at least one parseable numeric token.
    /// When false, the series is empty because nothing parsed -> UNVERIFIABLE.
    pub series_parsed: bool,
    pub metric: String,
    pub units: Option<String>,
    pub problem_label: Option<String>,
    pub negative_fixture: bool,
    pub flags: Vec<String>,
}

/// Build a sealed scientific-runtime receipt from a captured measurement series.
///
/// Status rule (design):
/// - invariant PASS                                  -> `PASS`
/// - invariant FAIL with `--negative-fixture`        -> `FAIL_EXPECTED`
/// - invariant FAIL without `--negative-fixture`     -> `FAIL_UNEXPECTED`
/// - empty / unparseable series                      -> `UNVERIFIABLE`
///
/// Labels always include `NOT_A_NEW_PHYSICAL_LAW`; `NEGATIVE_FIXTURE` is added
/// when the run is a declared negative fixture.
pub fn build_scientific_runtime_receipt(
    inputs: ScientificReceiptInputs<'_>,
) -> ScientificRuntimeReceipt {
    let ScientificReceiptInputs {
        source_path,
        compiler_version,
        language_version,
        source_digest,
        input_graph_digest,
        target,
        os,
        exit_code,
        series,
        series_parsed,
        metric,
        units,
        problem_label,
        negative_fixture,
        flags,
    } = inputs;

    let observed = energy_monotone_nonincreasing(&series, ENERGY_MONOTONE_TOLERANCE);
    let has_series = series_parsed && !series.is_empty();
    let passes = has_series && invariant_passes(series.len(), &observed);

    // Invariant status is PASS/FAIL over the observed series. When there is no
    // series at all the invariant could not be evaluated, so it is FAIL and the
    // receipt_status is UNVERIFIABLE (below).
    let invariant_status = if passes { "PASS" } else { "FAIL" };

    let receipt_status = if !has_series {
        "UNVERIFIABLE"
    } else if passes {
        "PASS"
    } else if negative_fixture {
        "FAIL_EXPECTED"
    } else {
        "FAIL_UNEXPECTED"
    };

    let mut labels = vec!["NOT_A_NEW_PHYSICAL_LAW".to_string()];
    if negative_fixture {
        labels.push("NEGATIVE_FIXTURE".to_string());
    }

    let count = series.len();
    let mut receipt = ScientificRuntimeReceipt {
        schema: SCIENTIFIC_RUNTIME_SCHEMA.to_string(),
        compiler: "buildc".to_string(),
        compiler_version: compiler_version.to_string(),
        language_version,
        source: source_path.to_string_lossy().to_string(),
        source_digest,
        input_graph_digest,
        build_state: ScientificBuildState {
            target: target.to_string(),
            compiler_status: "compiled_and_executed".to_string(),
            flags,
        },
        runtime_state: ScientificRuntimeState {
            os: os.to_string(),
            exit_code,
        },
        problem: ScientificProblem {
            label: problem_label,
        },
        measurement: ScientificMeasurement {
            metric,
            observed_values: series,
            count,
            units,
        },
        invariant: ScientificInvariant {
            name: ENERGY_MONOTONE_INVARIANT.to_string(),
            expectation: "no step increases energy beyond tolerance".to_string(),
            tolerance: ENERGY_MONOTONE_TOLERANCE,
            observed,
            status: invariant_status.to_string(),
        },
        negative_fixture,
        labels,
        receipt_status: receipt_status.to_string(),
        // Placeholder; overwritten by `seal_receipt` below.
        seal: ScientificDigest {
            algorithm: "sha256".to_string(),
            hex: String::new(),
        },
        provenance: ScientificProvenance {
            research_source_hash: RESEARCH_SOURCE_HASH.to_string(),
        },
    };

    seal_receipt(&mut receipt);
    receipt
}

/// Compute and set the receipt seal.
///
/// The seal is `sha256` over the canonical JSON of the receipt with the `seal`
/// field's `hex` blanked (empty string) and `algorithm` fixed to `"sha256"`.
/// We serialize a clone of the receipt whose `seal.hex` is `""`, hash those
/// bytes with the existing [`source_digest_hex`], and store the result in
/// `seal.hex`. This is deterministic: `serde_json::to_vec` preserves struct
/// field order, and the only mutated field is the sealed one. Re-derivation
/// (T3 verify) blanks `seal.hex`, re-serializes, re-hashes, and compares.
pub fn seal_receipt(receipt: &mut ScientificRuntimeReceipt) {
    receipt.seal.algorithm = "sha256".to_string();
    receipt.seal.hex.clear();
    let canonical = serde_json::to_vec(receipt).expect("serialize scientific-runtime receipt");
    receipt.seal.hex = source_digest_hex(&canonical);
}

/// Re-derive the seal from a receipt read back from disk and compare against the
/// stored `seal.hex`. Used by `receipt verify` (T3). Returns the recomputed hex.
// Exercised by unit tests; the external caller (the `receipt verify` schema
// branch) lands in T3, so the bin build sees it as unused for now.
#[allow(dead_code)]
pub fn recompute_seal_hex(receipt: &ScientificRuntimeReceipt) -> String {
    let mut probe = receipt.clone();
    probe.seal.algorithm = "sha256".to_string();
    probe.seal.hex.clear();
    let canonical = serde_json::to_vec(&probe).expect("serialize scientific-runtime receipt");
    source_digest_hex(&canonical)
}

/// Parse whitespace/newline-separated f64 tokens from captured program stdout.
///
/// Accepts BOTH the plain-decimal (`0.530827`) and scientific (`1.59908e+28`,
/// `6.10352e-05`) forms the C `%g` backend emits, via `str::parse::<f64>`.
/// Non-numeric tokens (blank lines, labels) are skipped. Returns the parsed
/// series and whether at least one token parsed (false -> UNVERIFIABLE upstream).
pub fn parse_numeric_series(stdout: &str) -> (Vec<f64>, bool) {
    let mut series = Vec::new();
    let mut any_parsed = false;
    for token in stdout.split_whitespace() {
        if let Ok(value) = token.parse::<f64>() {
            series.push(value);
            any_parsed = true;
        }
    }
    (series, any_parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn base_inputs<'a>(
        path: &'a Path,
        series: Vec<f64>,
        parsed: bool,
        negative_fixture: bool,
    ) -> ScientificReceiptInputs<'a> {
        ScientificReceiptInputs {
            source_path: path,
            compiler_version: "0.0.0",
            language_version: "1.0.0".to_string(),
            source_digest: ScientificDigest {
                algorithm: "sha256".to_string(),
                hex: "a".repeat(64),
            },
            input_graph_digest: ScientificDigest {
                algorithm: "sha256".to_string(),
                hex: "b".repeat(64),
            },
            target: "c",
            os: "test-os",
            exit_code: 0,
            series,
            series_parsed: parsed,
            metric: "series".to_string(),
            units: None,
            problem_label: None,
            negative_fixture,
            flags: Vec::new(),
        }
    }

    #[test]
    fn monotone_series_has_zero_increases() {
        let series = [4.0, 3.0, 3.0, 2.5, 1.0];
        let observed = energy_monotone_nonincreasing(&series, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.increase_count, 0);
        assert_eq!(observed.first_increase_step, None);
        assert_eq!(observed.initial_value, Some(4.0));
        assert_eq!(observed.final_value, Some(1.0));
        assert!(invariant_passes(series.len(), &observed));
    }

    #[test]
    fn one_bump_is_counted_with_first_increase_step() {
        // Increase happens at k = 2 (index 2 -> index 3: 2.0 -> 5.0).
        let series = [4.0, 3.0, 2.0, 5.0, 1.0];
        let observed = energy_monotone_nonincreasing(&series, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.increase_count, 1);
        assert_eq!(observed.first_increase_step, Some(2));
        assert!(!invariant_passes(series.len(), &observed));
    }

    #[test]
    fn tolerance_absorbs_tiny_jitter_but_not_real_growth() {
        // A sub-tolerance wiggle up is NOT an increase.
        let jitter = [1.0, 1.0 + 5e-13, 1.0];
        let observed = energy_monotone_nonincreasing(&jitter, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.increase_count, 0);

        // A supra-tolerance step up IS an increase.
        let growth = [1.0, 1.0 + 1e-9, 1.0 + 2e-9];
        let observed = energy_monotone_nonincreasing(&growth, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.increase_count, 2);
        assert_eq!(observed.first_increase_step, Some(0));
    }

    #[test]
    fn single_point_series_does_not_pass() {
        let series = [1.0];
        let observed = energy_monotone_nonincreasing(&series, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.increase_count, 0);
        // Zero increases but only one point: cannot witness monotonicity.
        assert!(!invariant_passes(series.len(), &observed));
    }

    #[test]
    fn empty_series_does_not_pass() {
        let observed = energy_monotone_nonincreasing(&[], ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.increase_count, 0);
        assert_eq!(observed.initial_value, None);
        assert_eq!(observed.final_value, None);
        assert!(!invariant_passes(0, &observed));
    }

    #[test]
    fn status_rule_pass_for_monotone_series() {
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        assert_eq!(receipt.receipt_status, "PASS");
        assert_eq!(receipt.invariant.status, "PASS");
        assert_eq!(receipt.invariant.observed.increase_count, 0);
        assert!(receipt
            .labels
            .contains(&"NOT_A_NEW_PHYSICAL_LAW".to_string()));
        assert!(!receipt.labels.contains(&"NEGATIVE_FIXTURE".to_string()));
    }

    #[test]
    fn status_rule_fail_expected_when_negative_fixture() {
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![1.0, 2.0, 3.0], true, true));
        assert_eq!(receipt.receipt_status, "FAIL_EXPECTED");
        assert_eq!(receipt.invariant.status, "FAIL");
        assert_eq!(receipt.invariant.observed.increase_count, 2);
        assert_eq!(receipt.invariant.observed.first_increase_step, Some(0));
        assert!(receipt.labels.contains(&"NEGATIVE_FIXTURE".to_string()));
    }

    #[test]
    fn status_rule_fail_unexpected_without_negative_fixture() {
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![1.0, 2.0, 3.0], true, false));
        assert_eq!(receipt.receipt_status, "FAIL_UNEXPECTED");
        assert_eq!(receipt.invariant.status, "FAIL");
        assert!(!receipt.labels.contains(&"NEGATIVE_FIXTURE".to_string()));
    }

    #[test]
    fn status_rule_unverifiable_when_series_unparseable() {
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(base_inputs(path, vec![], false, false));
        assert_eq!(receipt.receipt_status, "UNVERIFIABLE");
        assert_eq!(receipt.measurement.count, 0);
    }

    #[test]
    fn always_labels_not_a_new_physical_law() {
        let path = Path::new("k.bld");
        for (series, parsed, neg) in [
            (vec![4.0, 3.0], true, false),
            (vec![1.0, 2.0], true, true),
            (vec![], false, false),
        ] {
            let receipt = build_scientific_runtime_receipt(base_inputs(path, series, parsed, neg));
            assert!(
                receipt
                    .labels
                    .contains(&"NOT_A_NEW_PHYSICAL_LAW".to_string()),
                "every receipt must carry NOT_A_NEW_PHYSICAL_LAW"
            );
        }
    }

    #[test]
    fn seal_is_64_hex_and_stable_and_reproducible() {
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        assert_eq!(receipt.seal.algorithm, "sha256");
        assert_eq!(receipt.seal.hex.len(), 64);
        assert!(receipt
            .seal
            .hex
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()));

        // Re-deriving the seal from the sealed receipt reproduces the stored hex.
        assert_eq!(recompute_seal_hex(&receipt), receipt.seal.hex);
    }

    #[test]
    fn seal_detects_tampering() {
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        let original = receipt.seal.hex.clone();
        // Tamper with a witnessed field; the recomputed seal must diverge.
        receipt.invariant.observed.increase_count = 99;
        assert_ne!(recompute_seal_hex(&receipt), original);
    }

    #[test]
    fn receipt_round_trips_as_json() {
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        let json = serde_json::to_string(&receipt).expect("serialize");
        let back: ScientificRuntimeReceipt = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.schema, SCIENTIFIC_RUNTIME_SCHEMA);
        assert_eq!(back.receipt_status, "PASS");
        assert_eq!(back.seal.hex, receipt.seal.hex);
        // The seal read back re-verifies against its own body.
        assert_eq!(recompute_seal_hex(&back), back.seal.hex);
    }

    #[test]
    fn parse_series_accepts_plain_and_scientific() {
        let stdout = "0.530827\n0.530404\n1.59908e+28\n6.10352e-05\n";
        let (series, parsed) = parse_numeric_series(stdout);
        assert!(parsed);
        assert_eq!(series.len(), 4);
        assert!((series[0] - 0.530827).abs() < 1e-9);
        assert!(series[2] > 1e28);
        assert!(series[3] > 0.0 && series[3] < 1e-3);
    }

    #[test]
    fn parse_series_reports_no_parse_for_non_numeric() {
        let (series, parsed) = parse_numeric_series("no numbers here\n");
        assert!(!parsed);
        assert!(series.is_empty());
    }
}
