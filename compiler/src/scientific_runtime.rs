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
    /// The program arguments the receipt was emitted with. `receipt verify`
    /// re-runs the program with EXACTLY these args, so an argv-parameterized
    /// kernel is faithfully re-derived instead of re-run argless. Sealed like
    /// every other field. REQUIRED: a receipt without this field is malformed
    /// (schema v0 shipped with it; a serde default here would be parse-leniency
    /// only, since the recomputed seal always covers the field and a receipt
    /// sealed without it could never verify anyway).
    pub args: Vec<String>,
    pub problem: ScientificProblem,
    pub measurement: ScientificMeasurement,
    pub invariant: ScientificInvariant,
    pub negative_fixture: bool,
    /// Whether the run diverged (a non-finite value was observed and the series
    /// was truncated to its finite prefix). Load-bearing for verify: for a
    /// diverged run the finite-prefix LENGTH is the index of the first
    /// non-finite value, a function of the exact float trajectory, which the
    /// design declares non-reproducible across toolchains. Verify therefore
    /// gates the prefix-derived checks (count, increase_count) on this field
    /// and instead requires the re-run to reproduce the divergence itself.
    pub diverged: bool,
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
    /// Whether the program emitted a non-finite value (inf / NaN). A diverged
    /// run is UNVERIFIABLE: the invariant could not be honestly evaluated.
    pub diverged: bool,
    /// The program arguments the run was invoked with (recorded so verify can
    /// re-run identically).
    pub args: Vec<String>,
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
        diverged,
        args,
        metric,
        units,
        problem_label,
        negative_fixture,
        flags,
    } = inputs;

    let observed = energy_monotone_nonincreasing(&series, ENERGY_MONOTONE_TOLERANCE);
    let has_series = series_parsed && !series.is_empty();
    // A diverged run (non-finite value observed) cannot witness the invariant,
    // even if its finite prefix looks monotone, so it never PASSes.
    let passes = has_series && !diverged && invariant_passes(series.len(), &observed);

    // Invariant status is PASS/FAIL over the observed series. When there is no
    // series at all, or the run diverged, the invariant could not be evaluated,
    // so it is FAIL and the receipt_status is UNVERIFIABLE (below).
    let invariant_status = if passes { "PASS" } else { "FAIL" };

    let receipt_status = if !has_series || diverged {
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
    if diverged {
        // Records WHY an UNVERIFIABLE receipt is unverifiable: the program
        // produced a non-finite (inf/NaN) value, distinct from "no numeric
        // output at all".
        labels.push("NONFINITE_OBSERVED".to_string());
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
        args,
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
        diverged,
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
/// stored `seal.hex`. Used by `receipt verify` ([`verify_scientific_runtime_receipt`]).
/// Returns the recomputed hex.
pub fn recompute_seal_hex(receipt: &ScientificRuntimeReceipt) -> String {
    let mut probe = receipt.clone();
    probe.seal.algorithm = "sha256".to_string();
    probe.seal.hex.clear();
    let canonical = serde_json::to_vec(&probe).expect("serialize scientific-runtime receipt");
    source_digest_hex(&canonical)
}

/// The outcome of parsing a program's numeric stdout into a measurement series.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedSeries {
    /// The finite f64 values, in order, up to (but not including) the first
    /// non-finite token. Always finite, so it round-trips through JSON cleanly
    /// (a non-finite f64 would serialize to `null` and break re-verification).
    pub series: Vec<f64>,
    /// Whether at least one numeric OR non-finite token was seen. False means
    /// the program emitted no parseable numbers -> UNVERIFIABLE upstream.
    pub any_parsed: bool,
    /// Whether the program emitted a non-finite value (inf / NaN, in any C
    /// runtime spelling). A numerical blow-up means the invariant could not be
    /// honestly evaluated, so the receipt is UNVERIFIABLE regardless of the
    /// finite prefix's shape (which would otherwise look monotone and PASS).
    pub diverged: bool,
}

/// Whether a token that Rust's f64 parser REJECTS is nonetheless a platform
/// C-runtime spelling of a non-finite value: Windows UCRT `nan(ind)`,
/// `-nan(ind)`, `nan(snan)`; legacy MSVCRT `1.#INF`, `-1.#IND`, `1.#QNAN`,
/// `1.#SNAN` (possibly with trailing padding digits under precision
/// formatting). The match is ANCHORED to those exact token shapes, not
/// substring containment: an ordinary stdout label like `step#info:` or
/// `cell#index=3` must NOT flag the run as diverged (substring matching on
/// `#inf`/`#ind` did exactly that). A word like `information` is also not
/// flagged. The Rust-parseable non-finite forms (`inf`, `nan`, `infinity`,
/// ...) are caught at the parse site, not here.
fn is_nonfinite_spelling(token: &str) -> bool {
    let t = token.trim_start_matches(['+', '-']).to_ascii_lowercase();
    t.starts_with("nan(")
        || t.starts_with("1.#inf")
        || t.starts_with("1.#ind")
        || t.starts_with("1.#qnan")
        || t.starts_with("1.#snan")
}

/// Parse whitespace/newline-separated f64 tokens from captured program stdout.
///
/// Accepts BOTH the plain-decimal (`0.530827`) and scientific (`1.59908e+28`,
/// `6.10352e-05`) forms the C `%g` backend emits, via `str::parse::<f64>`.
/// Non-numeric tokens (blank lines, labels) are skipped. A non-finite token
/// (inf / NaN, any spelling) signals a numerical blow-up: parsing STOPS at that
/// point (so `series` holds only the finite prefix and always serializes
/// cleanly) and `diverged` is set, which routes the receipt to UNVERIFIABLE
/// rather than sealing a diverged run as a false PASS.
pub fn parse_numeric_series(stdout: &str) -> ParsedSeries {
    let mut series = Vec::new();
    let mut any_parsed = false;
    let mut diverged = false;
    for token in stdout.split_whitespace() {
        match token.parse::<f64>() {
            Ok(value) if value.is_finite() => {
                series.push(value);
                any_parsed = true;
            }
            Ok(_) => {
                // Parsed as a non-finite float: `inf`, `-inf`, `nan`, `-nan`,
                // `infinity` (glibc / macOS `%g`). Divergence.
                any_parsed = true;
                diverged = true;
                break;
            }
            Err(_) => {
                // Not Rust-parseable. It may still be a platform C-runtime
                // non-finite spelling (UCRT `nan(ind)`, MSVCRT `1.#INF`), which
                // is a divergence signal; otherwise it's a label/blank -> skip.
                if is_nonfinite_spelling(token) {
                    any_parsed = true;
                    diverged = true;
                    break;
                }
            }
        }
    }
    ParsedSeries {
        series,
        any_parsed,
        diverged,
    }
}

/// The re-derived verdict triple used by `receipt verify` to compare against a
/// stored receipt. Deriving it from the SAME rules `build_scientific_runtime_receipt`
/// applies (via [`energy_monotone_nonincreasing`] / [`invariant_passes`]) is what
/// makes verify re-check the invariant instead of trusting the stored values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecomputedVerdict {
    /// `PASS` | `FAIL` over the re-run series.
    pub invariant_status: &'static str,
    /// Number of energy-increasing steps in the re-run series.
    pub increase_count: usize,
    /// `PASS` | `FAIL_EXPECTED` | `FAIL_UNEXPECTED` | `UNVERIFIABLE`.
    pub receipt_status: &'static str,
}

/// Recompute the invariant + receipt-status verdict from a freshly re-run
/// series, applying the exact same status rule as
/// [`build_scientific_runtime_receipt`]. `negative_fixture` is read back from the
/// stored receipt so the FAIL_EXPECTED / FAIL_UNEXPECTED distinction is
/// reproduced. This checks the VERDICT (monotonicity + increase count), not the
/// exact float values, so it is robust to platform float non-reproducibility.
pub fn recompute_verdict(
    series: &[f64],
    series_parsed: bool,
    diverged: bool,
    negative_fixture: bool,
) -> RecomputedVerdict {
    let observed = energy_monotone_nonincreasing(series, ENERGY_MONOTONE_TOLERANCE);
    let has_series = series_parsed && !series.is_empty();
    let passes = has_series && !diverged && invariant_passes(series.len(), &observed);

    let invariant_status = if passes { "PASS" } else { "FAIL" };
    let receipt_status = if !has_series || diverged {
        "UNVERIFIABLE"
    } else if passes {
        "PASS"
    } else if negative_fixture {
        "FAIL_EXPECTED"
    } else {
        "FAIL_UNEXPECTED"
    };

    RecomputedVerdict {
        invariant_status,
        increase_count: observed.increase_count,
        receipt_status,
    }
}

/// Verify a `buildlang-scientific-runtime-receipt/v0` receipt by RE-DERIVING,
/// never trusting the stored values (mirrors `corpus verify`'s discipline):
///
/// 1. Deserialize the receipt and confirm its schema.
/// 2. Re-derive the source + input-graph digests from the file on disk (via the
///    `rederive_digests` callback, which runs the same check pipeline that
///    produced the stored digests) and compare. A source change fails here.
/// 3. Re-run the program WITH THE STORED ARGS (via `rerun_series`, the shared
///    compile+run+capture path) and re-parse its numeric stdout into a fresh
///    series. The observed-value COUNT is re-checked against the stored
///    `measurement.count` (a deterministic quantity); a mismatch fails.
/// 4. Recompute the invariant + receipt-status verdict from that fresh series
///    and compare the recomputed `invariant.status`, `increase_count`, and
///    `receipt_status` against the stored ones. Any disagreement is a failure.
///    Individual float VALUES are deliberately NOT re-compared: exact floats
///    need not reproduce across platforms, which is precisely why the verdict
///    (monotonicity + count), not the raw series, is the re-checked quantity.
/// 5. Recompute the seal over the stored receipt bytes and confirm it matches
///    the stored `seal.hex` (integrity of the receipt itself).
///
/// A receipt that passes all five checks is FAITHFUL. The exit code additionally
/// reflects the invariant verdict: `Ok(())` for PASS and FAIL_EXPECTED (a
/// negative fixture reproducing its expected failure), and `Err(3)` for
/// FAIL_UNEXPECTED / UNVERIFIABLE. This keeps a bare `receipt verify r.json`
/// safe as a CI pass/fail gate (it does not exit 0 on a recorded invariant
/// violation), distinct from `Err(1)` which means the receipt did NOT reproduce.
///
/// The digest/re-run callbacks are supplied by `main.rs` (which owns `run_check`
/// and `compile_and_capture_run`); the verdict logic and comparisons live here.
///
/// `compiler_version` / `language_version` mismatches WARN (to stderr) rather
/// than hard-fail: a scientific receipt records a numerical verdict that a later
/// compiler build can still legitimately reproduce, so a version bump alone must
/// not be treated as tampering. (The check-receipt verifier pins versions
/// because it replays effect/capability facts that ARE version-sensitive; this
/// numerical receipt is not.)
///
/// `rerun_series` is called with the receipt's recorded args and returns a
/// [`ParsedSeries`].
pub fn verify_scientific_runtime_receipt(
    receipt_json: &serde_json::Value,
    source_override: Option<&Path>,
    json: bool,
    current_compiler_version: &str,
    current_language_version: &str,
    rederive_digests: impl FnOnce(&Path) -> Result<(ScientificDigest, ScientificDigest), i32>,
    rerun_series: impl FnOnce(&Path, &[String]) -> Result<ParsedSeries, i32>,
) -> Result<(), i32> {
    let receipt: ScientificRuntimeReceipt =
        serde_json::from_value(receipt_json.clone()).map_err(|err| {
            eprintln!("Error: scientific-runtime receipt is malformed: {}", err);
            verify_failure_class(json, "MALFORMED", 1)
        })?;

    if receipt.schema != SCIENTIFIC_RUNTIME_SCHEMA {
        eprintln!(
            "Error: unsupported scientific-runtime receipt schema `{}`",
            receipt.schema
        );
        return Err(verify_failure_class(json, "SCHEMA_UNSUPPORTED", 1));
    }
    if receipt.compiler != "buildc" {
        eprintln!(
            "Error: receipt compiler mismatch: expected buildc, got {}",
            receipt.compiler
        );
        return Err(verify_failure_class(json, "COMPILER_MISMATCH", 1));
    }

    // Version drift WARNs, does not fail (see the doc comment above).
    if receipt.compiler_version != current_compiler_version {
        eprintln!(
            "Warning: compiler version differs: receipt {}, current {} (re-checking the verdict anyway)",
            receipt.compiler_version, current_compiler_version
        );
    }
    if receipt.language_version != current_language_version {
        eprintln!(
            "Warning: language version differs: receipt {}, current {} (re-checking the verdict anyway)",
            receipt.language_version, current_language_version
        );
    }

    // Resolve the source path: an explicit --source override, else the embedded
    // `source` field.
    let source_path = match source_override {
        Some(path) => path.to_path_buf(),
        None => Path::new(&receipt.source).to_path_buf(),
    };

    // (2) Re-derive the source + input-graph digests and compare. A source-file
    // change since sealing shows up as a digest mismatch here.
    let (source_digest, input_graph_digest) = rederive_digests(&source_path)
        .map_err(|code| verify_failure_class(json, "REDERIVATION_FAILED", code))?;
    if !digests_match(&source_digest, &receipt.source_digest) {
        eprintln!(
            "Error: source digest mismatch: receipt {}:{}, actual {}:{}",
            receipt.source_digest.algorithm,
            receipt.source_digest.hex,
            source_digest.algorithm,
            source_digest.hex
        );
        return Err(verify_failure_class(json, "SOURCE_DIGEST_MISMATCH", 1));
    }
    if !digests_match(&input_graph_digest, &receipt.input_graph_digest) {
        eprintln!(
            "Error: input graph digest mismatch: receipt {}:{}, actual {}:{}",
            receipt.input_graph_digest.algorithm,
            receipt.input_graph_digest.hex,
            input_graph_digest.algorithm,
            input_graph_digest.hex
        );
        return Err(verify_failure_class(json, "INPUT_GRAPH_DIGEST_MISMATCH", 1));
    }

    // (3) Re-run the program WITH THE STORED ARGS and re-parse its stdout, so an
    // argv-parameterized kernel is reproduced under the same conditions it was
    // emitted under.
    let parsed = rerun_series(&source_path, &receipt.args)
        .map_err(|code| verify_failure_class(json, "RERUN_FAILED", code))?;

    // For a DIVERGED run the finite-prefix length (and hence increase_count
    // over that prefix) is the step index of the first non-finite value: a
    // function of the exact float trajectory, which the design declares
    // non-reproducible across toolchains (a 1-ULP libm difference can shift
    // the divergence step). So when the receipt records divergence AND the
    // re-run also diverges, the prefix-derived checks are skipped and the
    // reproduced divergence itself is the faithfulness signal. A re-run that
    // does NOT diverge when the receipt says it did (or vice versa) falls
    // through to the strict checks and fails as non-reproduction.
    let both_diverged = receipt.diverged && parsed.diverged;

    // (3a) For non-diverged runs the observed-value count IS deterministic
    // (it is the number of values the program prints, independent of float
    // jitter), so a re-run with a different count means the stored measurement
    // was tampered with (or the program is non-deterministic in a way that
    // breaks re-derivation). Element values are NOT re-compared: exact floats
    // need not reproduce across platforms (see the doc comment), so the
    // verdict is the re-checked quantity, with count guarding series length.
    if !both_diverged && parsed.series.len() != receipt.measurement.count {
        eprintln!(
            "Error: measurement count drift: receipt {}, re-run {}",
            receipt.measurement.count,
            parsed.series.len()
        );
        return Err(verify_failure_class(json, "MEASUREMENT_COUNT_DRIFT", 1));
    }

    // (4) Recompute the verdict and compare against the stored one.
    let recomputed = recompute_verdict(
        &parsed.series,
        parsed.any_parsed,
        parsed.diverged,
        receipt.negative_fixture,
    );
    let stored_increase = receipt.invariant.observed.increase_count;

    if recomputed.invariant_status != receipt.invariant.status {
        eprintln!(
            "Error: invariant status drift: receipt {}, re-run {}",
            receipt.invariant.status, recomputed.invariant_status
        );
        return Err(verify_failure_class(json, "INVARIANT_STATUS_DRIFT", 1));
    }
    // Prefix-derived like the count: skipped when both runs diverged (the
    // increase count over a platform-dependent finite prefix is itself
    // platform-dependent).
    if !both_diverged && recomputed.increase_count != stored_increase {
        eprintln!(
            "Error: increase_count drift: receipt {}, re-run {}",
            stored_increase, recomputed.increase_count
        );
        return Err(verify_failure_class(json, "INCREASE_COUNT_DRIFT", 1));
    }
    if recomputed.receipt_status != receipt.receipt_status {
        eprintln!(
            "Error: receipt_status drift: receipt {}, re-run {}",
            receipt.receipt_status, recomputed.receipt_status
        );
        return Err(verify_failure_class(json, "RECEIPT_STATUS_DRIFT", 1));
    }

    // (5) Recompute the seal over the stored receipt and confirm integrity.
    let recomputed_seal = recompute_seal_hex(&receipt);
    if !recomputed_seal.eq_ignore_ascii_case(&receipt.seal.hex) {
        eprintln!(
            "Error: seal mismatch: receipt sha256:{}, recomputed sha256:{}",
            receipt.seal.hex, recomputed_seal
        );
        return Err(verify_failure_class(json, "SEAL_MISMATCH", 1));
    }

    // The receipt is faithful (digests, count, verdict, and seal all re-check).
    // But a faithful receipt that RECORDS a failure is not a pass: an operator
    // running `receipt verify r.json && deploy` must not deploy on a
    // FAIL_UNEXPECTED or UNVERIFIABLE verdict. So the exit code reflects the
    // invariant verdict -- PASS and FAIL_EXPECTED succeed; everything else fails
    // with a distinct code (3, vs 1 for "did not reproduce").
    let invariant_held = matches!(recomputed.receipt_status, "PASS" | "FAIL_EXPECTED");

    if json {
        let mut report = serde_json::json!({
            "schema": SCIENTIFIC_RUNTIME_SCHEMA,
            "status": if invariant_held { "match" } else { "invariant_not_held" },
            "faithful": true,
            "invariant_held": invariant_held,
            "source": source_path.to_string_lossy(),
            "invariant_status": recomputed.invariant_status,
            "increase_count": recomputed.increase_count,
            "receipt_status": recomputed.receipt_status,
            "seal": { "algorithm": "sha256", "hex": receipt.seal.hex },
        });
        if !invariant_held {
            report["failure_class"] = serde_json::Value::String("INVARIANT_NOT_HELD".to_string());
        }
        let text = serde_json::to_string_pretty(&report).map_err(|err| {
            eprintln!(
                "Error serializing scientific-runtime verification report: {}",
                err
            );
            1
        })?;
        println!("{}", text);
    } else if invariant_held {
        println!(
            "MATCH: scientific-runtime receipt re-runs and re-checks clean ({}, increase_count={})",
            recomputed.receipt_status, recomputed.increase_count
        );
    } else {
        eprintln!(
            "FAIL: scientific-runtime receipt faithfully reproduces, but the invariant did not hold ({}, increase_count={}). `receipt verify` exits nonzero so it is safe as a pass/fail gate.",
            recomputed.receipt_status, recomputed.increase_count
        );
    }

    if invariant_held {
        Ok(())
    } else {
        // The class line goes to stderr in both modes (the json report above
        // already carries the field; the human FAIL line is prose).
        eprintln!("failure_class: INVARIANT_NOT_HELD");
        Err(3)
    }
}

/// Two digests match iff their algorithm and (case-insensitive) hex agree.
fn digests_match(actual: &ScientificDigest, expected: &ScientificDigest) -> bool {
    actual.algorithm.eq_ignore_ascii_case(&expected.algorithm)
        && actual.hex.eq_ignore_ascii_case(&expected.hex)
}

/// Report a stable machine-readable `failure_class` for a verify failure and
/// return the exit code to propagate. Emitted on stderr always (a line of the
/// form `failure_class: <CODE>`) and, in `--json` mode, as a JSON failure
/// report on stdout, so negative fixtures and CI consumers can pin
/// (failure_class, exit_code) pairs instead of accepting "anything failed".
///
/// The class vocabulary (stable within schema v0):
/// - `MALFORMED`, `SCHEMA_UNSUPPORTED`, `COMPILER_MISMATCH`: the receipt could
///   not be interpreted.
/// - `REDERIVATION_FAILED`, `RERUN_FAILED`: the source could not be re-checked
///   or re-run (missing file, toolchain failure), distinct from drift.
/// - `SOURCE_DIGEST_MISMATCH`, `INPUT_GRAPH_DIGEST_MISMATCH`: the source
///   changed since sealing.
/// - `MEASUREMENT_COUNT_DRIFT`, `INVARIANT_STATUS_DRIFT`,
///   `INCREASE_COUNT_DRIFT`, `RECEIPT_STATUS_DRIFT`: the re-run disagrees with
///   the stored verdict facts.
/// - `SEAL_MISMATCH`: the stored receipt body does not re-seal.
/// - `INVARIANT_NOT_HELD` (exit 3): the receipt is FAITHFUL but records
///   FAIL_UNEXPECTED / UNVERIFIABLE (emitted at the verdict tail, not here).
fn verify_failure_class(json: bool, failure_class: &str, exit_code: i32) -> i32 {
    eprintln!("failure_class: {failure_class}");
    if json {
        let report = serde_json::json!({
            "schema": SCIENTIFIC_RUNTIME_SCHEMA,
            "status": "failed",
            "failure_class": failure_class,
        });
        if let Ok(text) = serde_json::to_string_pretty(&report) {
            println!("{text}");
        }
    }
    exit_code
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
            diverged: false,
            args: Vec::new(),
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
        let parsed = parse_numeric_series(stdout);
        assert!(parsed.any_parsed);
        assert!(!parsed.diverged);
        assert_eq!(parsed.series.len(), 4);
        assert!((parsed.series[0] - 0.530827).abs() < 1e-9);
        assert!(parsed.series[2] > 1e28);
        assert!(parsed.series[3] > 0.0 && parsed.series[3] < 1e-3);
    }

    #[test]
    fn parse_series_reports_no_parse_for_non_numeric() {
        let parsed = parse_numeric_series("no numbers here\n");
        assert!(!parsed.any_parsed);
        assert!(!parsed.diverged);
        assert!(parsed.series.is_empty());
    }

    #[test]
    fn parse_series_flags_divergence_and_keeps_finite_prefix() {
        // Rust-parseable non-finite forms (glibc/macOS `%g`).
        for tail in ["inf", "-inf", "nan", "-nan", "infinity"] {
            let parsed = parse_numeric_series(&format!("4.0\n3.0\n{tail}\n{tail}\n"));
            assert!(parsed.diverged, "`{tail}` must signal divergence");
            assert!(parsed.any_parsed);
            // Only the finite prefix is kept, so it always serializes cleanly.
            assert_eq!(parsed.series, vec![4.0, 3.0], "tail={tail}");
            assert!(parsed.series.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn parse_series_flags_platform_nonfinite_spellings() {
        // Windows UCRT / legacy MSVCRT spellings Rust's f64 parser rejects.
        for tail in ["-nan(ind)", "nan(ind)", "1.#INF", "-1.#IND", "1.#QNAN"] {
            let parsed = parse_numeric_series(&format!("4.0\n3.0\n{tail}\n"));
            assert!(parsed.diverged, "`{tail}` must signal divergence");
            assert_eq!(parsed.series, vec![4.0, 3.0], "tail={tail}");
        }
    }

    #[test]
    fn parse_series_does_not_flag_ordinary_words() {
        // A label starting with `inf`/`nan` must not be mistaken for a blow-up.
        let parsed = parse_numeric_series("information: 4.0 nanometers 3.0\n");
        assert!(!parsed.diverged);
        assert_eq!(parsed.series, vec![4.0, 3.0]);
    }

    #[test]
    fn parse_series_does_not_flag_hash_labels() {
        // Ordinary labels containing `#inf`/`#ind` substrings must NOT flag
        // divergence: the MSVCRT match is anchored to the full token shapes
        // (`1.#INF` etc.), not substring containment. Regression for the
        // substring version, where `step#info:` diverged a healthy run.
        for label in ["step#info:", "cell#index=3", "grid#index", "x#snapshot"] {
            let parsed = parse_numeric_series(&format!("{label} 4.0\n3.0\n2.0\n"));
            assert!(!parsed.diverged, "`{label}` must not flag divergence");
            assert_eq!(parsed.series, vec![4.0, 3.0, 2.0], "label={label}");
        }
    }

    #[test]
    fn diverged_run_is_unverifiable_not_a_false_pass() {
        // A monotone-looking finite prefix followed by a blow-up is UNVERIFIABLE,
        // never PASS: the invariant could not be honestly evaluated.
        let path = Path::new("k.bld");
        let mut inputs = base_inputs(path, vec![4.0, 3.0], true, false);
        inputs.diverged = true;
        let receipt = build_scientific_runtime_receipt(inputs);
        assert_eq!(receipt.receipt_status, "UNVERIFIABLE");
        assert_eq!(receipt.invariant.status, "FAIL");
        assert!(receipt.diverged, "the diverged flag must be sealed in-band");
        assert!(receipt.labels.contains(&"NONFINITE_OBSERVED".to_string()));
        // The finite prefix is preserved and round-trips (no JSON `null`).
        let json = serde_json::to_string(&receipt).expect("serialize");
        assert!(
            !json.contains("null"),
            "observed_values must be finite: {json}"
        );
        let back: ScientificRuntimeReceipt = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.measurement.observed_values, vec![4.0, 3.0]);
        assert!(back.diverged);
    }

    // --- receipt verify (T3) verdict recomputation ---------------------------

    #[test]
    fn recompute_verdict_matches_builder_for_monotone_series() {
        // A monotone series recomputes PASS/PASS with zero increases, exactly
        // as `build_scientific_runtime_receipt` would have recorded it.
        let verdict = recompute_verdict(&[4.0, 3.0, 2.0], true, false, false);
        assert_eq!(verdict.invariant_status, "PASS");
        assert_eq!(verdict.receipt_status, "PASS");
        assert_eq!(verdict.increase_count, 0);
    }

    #[test]
    fn recompute_verdict_distinguishes_expected_from_unexpected_failure() {
        // The negative-fixture flag (read back from the stored receipt) is what
        // separates FAIL_EXPECTED from FAIL_UNEXPECTED on a re-run.
        let expected = recompute_verdict(&[1.0, 2.0, 3.0], true, false, true);
        assert_eq!(expected.invariant_status, "FAIL");
        assert_eq!(expected.receipt_status, "FAIL_EXPECTED");
        assert_eq!(expected.increase_count, 2);

        let unexpected = recompute_verdict(&[1.0, 2.0, 3.0], true, false, false);
        assert_eq!(unexpected.receipt_status, "FAIL_UNEXPECTED");
    }

    #[test]
    fn recompute_verdict_is_unverifiable_when_nothing_parsed() {
        let verdict = recompute_verdict(&[], false, false, false);
        assert_eq!(verdict.receipt_status, "UNVERIFIABLE");
        assert_eq!(verdict.invariant_status, "FAIL");
    }

    #[test]
    fn recompute_verdict_is_unverifiable_when_diverged() {
        // A monotone finite prefix that diverged is UNVERIFIABLE, not PASS, so a
        // re-run of a diverged program re-derives the same UNVERIFIABLE verdict.
        let verdict = recompute_verdict(&[4.0, 3.0], true, true, false);
        assert_eq!(verdict.receipt_status, "UNVERIFIABLE");
        assert_eq!(verdict.invariant_status, "FAIL");
    }

    /// Build a `ParsedSeries` for a re-run callback in tests.
    fn rerun(series: Vec<f64>) -> ParsedSeries {
        ParsedSeries {
            any_parsed: !series.is_empty(),
            diverged: false,
            series,
        }
    }

    #[test]
    fn verify_matches_a_freshly_built_receipt() {
        // Round trip: build a receipt, serialize it, then verify it with
        // callbacks that reproduce the same digests and series. Verify passes.
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            |_| Ok((src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert!(result.is_ok(), "a faithful re-run must verify");
    }

    #[test]
    fn verify_rejects_a_source_digest_mismatch() {
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        let value = serde_json::to_value(&receipt).expect("to_value");
        let graph_digest = receipt.input_graph_digest.clone();

        // The re-derived source digest disagrees with the stored one.
        let wrong = ScientificDigest {
            algorithm: "sha256".to_string(),
            hex: "c".repeat(64),
        };
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            |_| Ok((wrong.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(result, Err(1), "a source-digest mismatch must fail verify");
    }

    #[test]
    fn verify_rejects_a_verdict_drift() {
        // The stored receipt says PASS, but the re-run produces an increasing
        // series (FAIL). The verdict comparison must reject it.
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            |_| Ok((src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![1.0, 2.0, 3.0])),
        );
        assert_eq!(result, Err(1), "an invariant drift must fail verify");
    }

    #[test]
    fn verify_rejects_measurement_count_drift() {
        // The re-run reproduces a PASSing (monotone) series, but with a DIFFERENT
        // number of points than the stored receipt. The verdict alone would match
        // (PASS/PASS, 0 increases); the count re-check is what catches it.
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        assert_eq!(receipt.measurement.count, 3);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            |_| Ok((src_digest.clone(), graph_digest.clone())),
            // Two points instead of three; still monotone (PASS), so only the
            // count check can reject this.
            |_, _| Ok(rerun(vec![4.0, 3.0])),
        );
        assert_eq!(result, Err(1), "a measurement count drift must fail verify");
    }

    #[test]
    fn verify_fails_a_faithful_but_unexpected_failure() {
        // A receipt that faithfully reproduces but records FAIL_UNEXPECTED must
        // NOT exit 0: `receipt verify && deploy` must not deploy on it. Exit 3
        // distinguishes "faithful, invariant not held" from "did not reproduce".
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![1.0, 2.0, 3.0], true, false));
        assert_eq!(receipt.receipt_status, "FAIL_UNEXPECTED");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            |_| Ok((src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![1.0, 2.0, 3.0])),
        );
        assert_eq!(
            result,
            Err(3),
            "a faithful FAIL_UNEXPECTED receipt must fail verify with exit 3"
        );
    }

    #[test]
    fn verify_passes_a_faithful_negative_fixture() {
        // A negative fixture that reproduces its EXPECTED failure is a pass: the
        // receipt is FAIL_EXPECTED and verify returns Ok.
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![1.0, 2.0, 3.0], true, true));
        assert_eq!(receipt.receipt_status, "FAIL_EXPECTED");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            |_| Ok((src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![1.0, 2.0, 3.0])),
        );
        assert!(
            result.is_ok(),
            "a faithful FAIL_EXPECTED receipt must verify"
        );
    }

    #[test]
    fn verify_fails_a_faithful_diverged_receipt() {
        // A diverged (UNVERIFIABLE) receipt whose re-run reproduces the same
        // divergence is faithful, but UNVERIFIABLE is not a pass -> Err(3).
        // The re-run's finite prefix is deliberately a DIFFERENT length than
        // the stored one: for a diverged run the prefix length is the index of
        // the first non-finite value, a platform-dependent quantity, so the
        // count / increase_count checks must be skipped when both runs
        // diverged (a 1-ULP libm difference can shift the divergence step and
        // must not misclassify an honest receipt as tampering, Err(1)).
        let path = Path::new("k.bld");
        let mut inputs = base_inputs(path, vec![4.0, 3.0], true, false);
        inputs.diverged = true;
        let receipt = build_scientific_runtime_receipt(inputs);
        assert_eq!(receipt.receipt_status, "UNVERIFIABLE");
        assert_eq!(receipt.measurement.count, 2);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            |_| Ok((src_digest.clone(), graph_digest.clone())),
            |_, _| {
                Ok(ParsedSeries {
                    // Three finite values instead of two: the divergence step
                    // shifted by one on the re-run platform.
                    series: vec![4.0, 3.0, 2.5],
                    any_parsed: true,
                    diverged: true,
                })
            },
        );
        assert_eq!(
            result,
            Err(3),
            "a faithful diverged receipt must exit 3 even when the platform-dependent prefix length shifts"
        );
    }

    #[test]
    fn verify_rejects_a_receipt_whose_divergence_does_not_reproduce() {
        // The receipt records divergence, but the re-run completes finite:
        // the recorded blow-up did NOT reproduce, which is genuine
        // non-reproduction -> Err(1), not the faithful-UNVERIFIABLE Err(3).
        let path = Path::new("k.bld");
        let mut inputs = base_inputs(path, vec![4.0, 3.0], true, false);
        inputs.diverged = true;
        let receipt = build_scientific_runtime_receipt(inputs);
        assert_eq!(receipt.receipt_status, "UNVERIFIABLE");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            |_| Ok((src_digest.clone(), graph_digest.clone())),
            // Finite, monotone re-run: no divergence reproduced.
            |_, _| Ok(rerun(vec![4.0, 3.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "a recorded divergence that does not reproduce is non-reproduction"
        );
    }

    #[test]
    fn verify_reruns_with_the_recorded_args() {
        // The re-run must receive the receipt's stored args, so an argv-dependent
        // kernel is reproduced under the same conditions it was emitted with.
        let path = Path::new("k.bld");
        let mut inputs = base_inputs(path, vec![4.0, 3.0, 2.0], true, false);
        inputs.args = vec!["--mode".to_string(), "stable".to_string()];
        let receipt = build_scientific_runtime_receipt(inputs);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            |_| Ok((src_digest.clone(), graph_digest.clone())),
            |_, args| {
                assert_eq!(
                    args,
                    ["--mode".to_string(), "stable".to_string()],
                    "verify must re-run with the receipt's recorded args"
                );
                Ok(rerun(vec![4.0, 3.0, 2.0]))
            },
        );
        assert!(result.is_ok(), "recorded-args re-run must verify");
    }
}
