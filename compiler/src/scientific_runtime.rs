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

/// The invariant name emitted for the conserved-quantity check: the measured
/// scalar (a conserved quantity such as total mass or a Hamiltonian) must stay
/// within tolerance of its initial value at every step.
pub const CONSERVATION_INVARIANT: &str = "conserved_quantity_constant";

/// Tolerance used by the monotone-non-increasing check. A step counts as an
/// increase only when `series[k+1] > series[k] + TOLERANCE`, so platform float
/// jitter at the ULP scale does not flip the verdict (design determinism rule).
pub const ENERGY_MONOTONE_TOLERANCE: f64 = 1e-12;

/// Tolerance used by the conserved-quantity check. Looser than the monotone
/// tolerance because a genuinely conserved discrete quantity still accumulates
/// floating-point roundoff over many steps, while a real leak (a non-conserving
/// scheme) drifts by an O(1) amount that this bound still catches decisively.
pub const CONSERVATION_TOLERANCE: f64 = 1e-9;

/// The invariant name emitted for the discrete maximum-principle check: the
/// measured scalar must never RISE above its initial value (a one-sided upper
/// bound). Distinct from monotone (which forbids any step-wise increase) and
/// from conservation (which forbids any deviation): a bounded quantity may
/// oscillate and decay freely, it just may never exceed where it started.
pub const BOUNDED_INVARIANT: &str = "bounded_by_initial_maximum";

/// Tolerance used by the maximum-principle check. Same scale as conservation:
/// a genuinely bounded quantity sits at or below its initial value to roundoff,
/// while a real overshoot (an unstable, energy-injecting scheme) exceeds it by
/// an O(1) amount this bound catches decisively.
pub const BOUNDED_TOLERANCE: f64 = 1e-9;

/// The invariant name emitted for the APPROXIMATE-conservation (bounded-drift)
/// check: the measured scalar must stay within a fixed error BUDGET of its
/// initial value, forever. It reuses conservation's two-sided evaluator but
/// with a looser, calibrated tolerance, so it accepts a quantity that is only
/// APPROXIMATELY conserved (a symplectic integrator's energy oscillates in an
/// O(dt^2) band and never drifts secularly) while still rejecting a scheme
/// whose energy drifts away (explicit Euler). Distinct from `conservation`
/// (roundoff-exact), `bounded` (one-sided, and too tight for a band that rises
/// slightly above the start), and `energy-monotone`.
pub const CONSERVED_BAND_INVARIANT: &str = "conserved_within_band";

/// Tolerance (error budget) for the bounded-drift check. Calibrated so a
/// well-resolved symplectic scheme passes and a drifting one fails: the
/// flagship leapfrog oscillator (dt = 0.1) holds its energy within a measured
/// ~1.25e-3 band forever, so `5e-3` clears it ~4x, while explicit Euler leaves
/// the band within two steps and grows without bound. Absolute, like every
/// family tolerance: a kernel must be resolved (and scaled) to fit the budget.
pub const CONSERVED_BAND_TOLERANCE: f64 = 5e-3;

/// The invariant name emitted for the discrete energy-identity check: the
/// measured scalar is a per-step energy-BALANCE residual that must stay within
/// tolerance of ZERO (an absolute bound, unlike conservation/bounded which
/// reference the initial value). This is the family's first QUANTITATIVE
/// invariant: it checks a computed numerical identity holds, not just a
/// monotonicity or bound pattern.
pub const ENERGY_IDENTITY_INVARIANT: &str = "energy_identity_residual";

/// Tolerance used by the energy-identity check. The exact discrete energy
/// balance holds to roundoff (measured max residual ~2e-14 for the reference
/// FTCS kernel), so `1e-9` clears it by ~5 orders (absorbing cross-platform
/// variation in a residual formed by cancelling O(1) quantities) while a real
/// dropped-term error leaves an O(r^2) residual ~1e-5 that this bound catches
/// by ~4 orders. Reference is 0, so every step (including step 0) is checked.
pub const ENERGY_IDENTITY_TOLERANCE: f64 = 1e-9;

/// The invariant name emitted for the cross-column RELATION check: each row of
/// a multi-column series must AGREE (all columns within tolerance of the row's
/// first column). This is the family's first invariant the VERIFIER computes
/// from raw captured columns rather than trusting a residual the kernel printed:
/// a kernel that emits two independent computations of a quantity cannot hide a
/// disagreement, because the check happens at verify, not in the program.
pub const RELATION_INVARIANT: &str = "relation_columns_agree";

/// Tolerance used by the relation check. Two faithful computations of the same
/// quantity agree to roundoff, while a genuine divergence (a dropped factor, a
/// wrong formula) differs by an O(1) amount this bound catches decisively.
pub const RELATION_TOLERANCE: f64 = 1e-9;

/// Provenance reference to the Telos pass-0009 research probe (reference only;
/// never matched byte-wise, per the determinism decision in the design).
pub const RESEARCH_SOURCE_HASH: &str =
    "b3021c14b0e5dc8adeddadf0d22e2780dbf259c349caf5cbc2ba255b591fd7d5";

/// The machine-readable claims boundary every scientific-runtime receipt
/// carries (the pass-0122 `non_promotion_boundary` field as sealed data): what
/// a verdict does NOT witness. Verify rejects a receipt whose boundary omits
/// "physical_law" (an emitter or hand-builder that dropped the boundary is
/// overclaiming by omission).
pub const NOT_CLAIMED_BOUNDARY: &[&str] = &[
    "numerical_correctness",
    "convergence",
    "pde_accuracy",
    "physical_law",
];

/// The versioned series-extraction policy: how raw stdout bytes become the
/// numeric series. Sealed in the receipt so a policy change is visible as a
/// different receipt, and byte-level drift stays distinguishable from
/// extraction-policy tolerance.
pub const SERIES_EXTRACTION_POLICY: &str =
    "whitespace-f64/v1: whitespace-split tokens parsed as finite f64; a non-finite token truncates the series and marks the run diverged; other tokens are skipped";

/// The criterion the verdict is measured against (the pass-0122 `oracle`
/// field). v0 supports one kind, `declared_invariant`: the named invariant IS
/// the criterion, declared rather than derived from an executed reference.
/// Future kinds (`reference_implementation`, `exact_proof`) get their own
/// status vocabulary when an oracle actually executes; the kind/status split
/// keeps "what the criterion is" separate from "how it was established".
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificOracle {
    /// `declared_invariant` (v0).
    pub kind: String,
    /// The criterion's name; for `declared_invariant` this MUST equal
    /// `invariant.name` (verify checks the binding).
    pub name: String,
    /// `DECLARED` (v0): the criterion was stated, not independently executed.
    pub status: String,
}

/// The pass-0122 `compiler_branch` contract: the toolchain that produced (and
/// re-produces) the run. Sealed at emit; verify re-probes the local toolchain
/// and treats a mismatch as environmental CONTEXT (a receipt may legitimately
/// re-verify under a different toolchain because the re-checked quantity is
/// the verdict, not bytes), and a missing toolchain as its own
/// `TOOL_UNAVAILABLE` verdict.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificToolchain {
    /// The resolved C compiler command (e.g. `cl.exe`, `gcc`).
    pub c_compiler: String,
    /// First line of the compiler's version banner (human triage).
    pub c_compiler_version: String,
    /// sha256 over the compiler's full version-probe output bytes.
    pub version_output_digest: ScientificDigest,
    /// The os/arch the emitting buildc binary was BUILT FOR (compile-time
    /// constants, e.g. `windows/x86_64`); equals the host in every supported
    /// configuration, but is not a runtime host probe.
    pub target: String,
    /// sha256 of the buildc binary that emitted the receipt.
    pub buildc_binary_digest: ScientificDigest,
    /// sha256 of the compiled program executable BEFORE it ran. REPORTED at
    /// verify (`executable_reproduced`), never required: C compiler output is
    /// not byte-stable across compiler versions, and requiring it would
    /// contradict the verdict-level determinism rule.
    pub program_executable_digest: ScientificDigest,
}

/// The master plan's "type/effect policy" receipt field, genuinely
/// witnessed: a digest over the canonical effect/capability facts the type
/// checker derived from source at emit, plus the observed capability union.
/// Verify RE-DERIVES both through the same check pipeline and fails with
/// `EFFECT_POLICY_DRIFT` on any disagreement, so the sealed policy facts can
/// neither be edited nor go stale against the source.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificEffectPolicy {
    /// sha256 over the canonical rendering of every function's declared
    /// effects and observed capabilities (sorted, so the digest is stable).
    pub facts_digest: ScientificDigest,
    /// Sorted union of capability names observed across the program.
    pub observed_capabilities: Vec<String>,
    /// Whether any function reads stdin. `Console` covers BOTH stdout writes
    /// (safe) and stdin reads (an external input that breaks the dataset and
    /// determinism absences), so the capability NAME alone cannot decide
    /// those fields; this flag disambiguates. Re-derived at verify, so a
    /// tampered flag is caught by `EFFECT_POLICY_DRIFT`.
    pub reads_stdin: bool,
}

/// A field whose VALUE is an honest statement about evidence: either a
/// witnessed absence (the effect system proves the thing cannot have
/// happened) or an explicit fence (buildc cannot witness it either way).
/// Used for `input_dataset` and `seed`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificWitnessedField {
    /// `NONE_WITNESSED` (proven absent via capabilities),
    /// `POSSIBLE_UNWITNESSED` (capabilities permit it; buildc does not track
    /// which resources were touched), or `NOT_APPLICABLE` (the language
    /// cannot express it at all).
    pub status: String,
    /// The derivation grounds, human-readable and re-derivable.
    pub grounds: String,
}

/// The determinism statement DERIVED from the observed capability set: a
/// program whose capabilities exclude every nondeterminism source the
/// language exposes is deterministic modulo its sealed args.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificDeterminism {
    pub deterministic_modulo_args: bool,
    pub grounds: String,
}

/// The master plan's "numerical method" field: author-DECLARED (buildc
/// cannot derive scheme semantics from source and must not pretend to).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificNumericalMethod {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `DECLARED` when a description was supplied, `UNDECLARED` otherwise;
    /// verify rejects an inconsistent pair (FIELD_CONTRACT_VIOLATION).
    pub status: String,
}

/// Derive the witnessed-absence fields from the observed capability union
/// (PURE, unit-tested): the typed-effect system doing receipt work.
///
/// FAIL CLOSED. Each capability is treated as a hazard for a claim UNLESS it
/// is provably incapable of undermining it. Two claims are derived:
///
/// * `input_dataset` (NONE_WITNESSED only if the program provably read no
///   external data): every capability that opens a data channel
///   (FileSystem, Network, Environment, Foreign, Gpu, and `Console` when it
///   reads stdin) is a hazard. `Process` (exit) and `Clock` are not: exiting
///   reads nothing, and the wall clock is a scalar nondeterminism source, not
///   a dataset channel.
/// * `determinism` (deterministic modulo sealed args only if nothing varies
///   between runs): every capability above PLUS `Clock` is a hazard;
///   `Process` alone is not.
///
/// `Console` is the one capability whose character depends on HOW it is used
/// (stdout writes are safe, stdin reads are not), which `reads_stdin`
/// disambiguates. Every capability not explicitly recognised as safe counts
/// as a hazard for BOTH claims, so a capability added to the type checker
/// later cannot silently widen a witnessed-absence claim. `seed` is
/// NOT_APPLICABLE in v0 because the language has no RNG builtin at all.
pub fn witnessed_fields_from_capabilities(
    observed_capabilities: &[String],
    reads_stdin: bool,
) -> (
    ScientificWitnessedField,
    ScientificWitnessedField,
    ScientificDeterminism,
) {
    let mut dataset_hazards: Vec<String> = Vec::new();
    let mut determinism_hazards: Vec<String> = Vec::new();
    for cap in observed_capabilities {
        let (feeds_dataset, varies) = match cap.as_str() {
            "Console" => (reads_stdin, reads_stdin),
            "Process" => (false, false),
            "Clock" => (false, true),
            // FileSystem, Network, Environment, Foreign, Gpu, and any
            // capability this build does not recognise: assume it can do both.
            _ => (true, true),
        };
        let label = if cap == "Console" && reads_stdin {
            "stdin".to_string()
        } else {
            cap.clone()
        };
        if feeds_dataset {
            dataset_hazards.push(label.clone());
        }
        if varies {
            determinism_hazards.push(label);
        }
    }

    let input_dataset = if dataset_hazards.is_empty() {
        ScientificWitnessedField {
            status: "NONE_WITNESSED".to_string(),
            grounds: "no observed capability can feed an external dataset (no FileSystem, Network, stdin, Environment, Foreign, or GPU access), so the program provably consumed none".to_string(),
        }
    } else {
        ScientificWitnessedField {
            status: "POSSIBLE_UNWITNESSED".to_string(),
            grounds: format!(
                "observed capabilities include {}, which can feed an external dataset; buildc does not track which resources were touched",
                dataset_hazards.join(", ")
            ),
        }
    };

    let seed = ScientificWitnessedField {
        status: "NOT_APPLICABLE".to_string(),
        grounds: "the language has no RNG builtin; there is no seed to record".to_string(),
    };

    let determinism = if determinism_hazards.is_empty() {
        ScientificDeterminism {
            deterministic_modulo_args: true,
            grounds: "no observed capability varies between runs (no Clock, Environment, FileSystem, Network, stdin, Foreign, or GPU access)".to_string(),
        }
    } else {
        ScientificDeterminism {
            deterministic_modulo_args: false,
            grounds: format!(
                "observed capabilities include {}, which can vary between runs",
                determinism_hazards.join(", ")
            ),
        }
    };

    (input_dataset, seed, determinism)
}

/// An explicitly fenced receipt branch (the corpus's UNAVAILABLE_FENCED
/// discipline): the pass-0122 contract names `telemetry_branch` and
/// `lineage_branch`; buildc does not produce either, and says so in-band
/// rather than omitting the fields (absence of evidence is witnessed, not
/// implied).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScientificFencedBranch {
    /// `UNAVAILABLE_FENCED`.
    pub status: String,
}

impl ScientificFencedBranch {
    pub fn fenced() -> Self {
        ScientificFencedBranch {
            status: "UNAVAILABLE_FENCED".to_string(),
        }
    }
}

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
    /// The pass-0122 `compiler_branch` block (see [`ScientificToolchain`]).
    pub toolchain: ScientificToolchain,
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
    /// How many columns each row of `observed_values` holds (row-major). `1`
    /// for the single-scalar-per-step invariants; `>= 2` for a relation
    /// invariant, whose verifier de-interleaves the flat series into columns
    /// and checks a relation ACROSS them. Sealed (a tamper is SEAL_MISMATCH);
    /// `count` remains the total token count, so a re-run's token-count drift
    /// is still caught independently of the column structure.
    ///
    /// REQUIRED (no serde default): the recomputed seal always covers this
    /// field, so a receipt sealed without it could never re-verify anyway. A
    /// default would only convert an honest `MALFORMED: missing column_count`
    /// into a misleading `SEAL_MISMATCH` (the same reasoning the `args` field
    /// records for rejecting a default on a sealed field).
    pub column_count: usize,
    /// sha256 over the EXACT raw stdout bytes captured at emit. The parse into
    /// `observed_values` is a lossy transform; sealing the raw payload keeps
    /// byte drift distinguishable from semantic drift. Verify recomputes this
    /// over the re-run's bytes and REPORTS whether it reproduced
    /// (`raw_stdout_reproduced`); a raw mismatch with a matching verdict is
    /// still faithful (exact bytes are platform-dependent by design).
    pub raw_stdout_digest: ScientificDigest,
    /// The versioned extraction policy that produced `observed_values` from
    /// the raw bytes (see [`SERIES_EXTRACTION_POLICY`]). Hard-checked at
    /// verify: a receipt extracted under a different policy cannot be
    /// faithfully re-checked by this build's parser.
    pub series_extraction_policy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
}

/// The observed outcome of the monotone-non-increasing check over a series.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InvariantObserved {
    /// Number of steps `k` where `series[k+1] > series[k] + tolerance`.
    pub violation_count: usize,
    /// Zero-based index `k` of the first offending step, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_violation_step: Option<usize>,
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
    /// The criterion the verdict is measured against (pass-0122 `oracle`).
    pub oracle: ScientificOracle,
    /// The witnessed type/effect policy facts (master plan field; verify
    /// re-derives them from source: EFFECT_POLICY_DRIFT on disagreement).
    pub effect_policy: ScientificEffectPolicy,
    /// Witnessed absence or explicit fence, derived from the capability
    /// facts (see `witnessed_fields_from_capabilities`).
    pub input_dataset: ScientificWitnessedField,
    pub seed: ScientificWitnessedField,
    pub determinism: ScientificDeterminism,
    /// Author-declared numerical method (buildc cannot derive scheme
    /// semantics and does not pretend to).
    pub numerical_method: ScientificNumericalMethod,
    pub measurement: ScientificMeasurement,
    pub invariant: ScientificInvariant,
    /// Explicitly fenced pass-0122 branches buildc does not produce: absence
    /// is witnessed in-band, never implied by omission.
    pub telemetry_branch: ScientificFencedBranch,
    pub lineage_branch: ScientificFencedBranch,
    pub negative_fixture: bool,
    /// Whether the run diverged (a non-finite value was observed and the series
    /// was truncated to its finite prefix). Load-bearing for verify: for a
    /// diverged run the finite-prefix LENGTH is the index of the first
    /// non-finite value, a function of the exact float trajectory, which the
    /// design declares non-reproducible across toolchains. Verify therefore
    /// gates the prefix-derived checks (count, violation_count) on this field
    /// and instead requires the re-run to reproduce the divergence itself.
    pub diverged: bool,
    /// The machine-readable claims boundary (pass-0122 `non_promotion_boundary`
    /// as sealed data; see [`NOT_CLAIMED_BOUNDARY`]). Verify rejects a receipt
    /// whose boundary omits "physical_law".
    pub not_claimed: Vec<String>,
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
    let mut violation_count = 0usize;
    let mut first_violation_step = None;
    for k in 0..series.len().saturating_sub(1) {
        if series[k + 1] > series[k] + tol {
            violation_count += 1;
            if first_violation_step.is_none() {
                first_violation_step = Some(k);
            }
        }
    }
    InvariantObserved {
        violation_count,
        first_violation_step,
        initial_value: series.first().copied(),
        final_value: series.last().copied(),
    }
}

/// Conserved-quantity invariant over a measured series: the scalar must stay
/// within `tol` of its INITIAL value at every step. A "violation" is a step
/// whose value deviates from `series[0]` by more than `tol`.
///
/// The reference is the FIRST value, not the mean: the mean depends on the
/// whole series, so a re-run that reproduces a different-length prefix (the
/// diverged case) would shift the reference and could flip the verdict, whereas
/// `series[0]` is stable under prefixing. Step 0 always deviates by exactly 0,
/// so it is never a violation. Records the first offending step and the
/// initial/final values; the verdict is derived in [`invariant_passes`] (PASS
/// requires at least two points AND zero violations).
pub fn conserved_quantity_constant(series: &[f64], tol: f64) -> InvariantObserved {
    let reference = series.first().copied();
    let mut violation_count = 0usize;
    let mut first_violation_step = None;
    if let Some(ref0) = reference {
        for (k, &value) in series.iter().enumerate() {
            if (value - ref0).abs() > tol {
                violation_count += 1;
                if first_violation_step.is_none() {
                    first_violation_step = Some(k);
                }
            }
        }
    }
    InvariantObserved {
        violation_count,
        first_violation_step,
        initial_value: reference,
        final_value: series.last().copied(),
    }
}

/// Discrete maximum-principle invariant over a measured series: the scalar must
/// never rise above its INITIAL value by more than `tol` (a one-sided upper
/// bound). A "violation" is a step whose value exceeds `series[0] + tol`.
///
/// Like conservation the reference is the FIRST value (stable under prefixing,
/// so a truncated re-run prefix cannot shift it), but only the UPPER side is
/// fenced: the quantity may fall arbitrarily far. Step 0 exceeds its own
/// reference by exactly 0, so it is never a violation. This is the discrete
/// maximum principle a stable diffusion/oscillation obeys; an unstable
/// (energy-injecting) scheme overshoots and is caught. Records the first
/// offending step and the initial/final values; the verdict is derived in
/// [`invariant_passes`] (PASS requires at least two points AND zero violations).
pub fn bounded_by_initial_maximum(series: &[f64], tol: f64) -> InvariantObserved {
    let reference = series.first().copied();
    let mut violation_count = 0usize;
    let mut first_violation_step = None;
    if let Some(ref0) = reference {
        for (k, &value) in series.iter().enumerate() {
            if value > ref0 + tol {
                violation_count += 1;
                if first_violation_step.is_none() {
                    first_violation_step = Some(k);
                }
            }
        }
    }
    InvariantObserved {
        violation_count,
        first_violation_step,
        initial_value: reference,
        final_value: series.last().copied(),
    }
}

/// Discrete energy-identity invariant over a measured series: each value is a
/// per-step energy-BALANCE residual that must stay within `tol` of ZERO. A
/// "violation" is a step whose `abs(value) > tol`.
///
/// Unlike conservation and bounded, the reference is 0 (an absolute bound), so
/// EVERY step is checked, including step 0 (its residual is a real residual
/// too, not a self-reference). This is the family's quantitative member: the
/// series is not a physical trajectory but the residual of a computed identity,
/// which a faithful scheme keeps at roundoff and a broken one does not. Records
/// the first offending step and the initial/final values; the verdict is
/// derived in [`invariant_passes`] (PASS requires at least two points AND zero
/// violations).
pub fn energy_identity_residual(series: &[f64], tol: f64) -> InvariantObserved {
    let mut violation_count = 0usize;
    let mut first_violation_step = None;
    for (k, &value) in series.iter().enumerate() {
        if value.abs() > tol {
            violation_count += 1;
            if first_violation_step.is_none() {
                first_violation_step = Some(k);
            }
        }
    }
    InvariantObserved {
        violation_count,
        first_violation_step,
        initial_value: series.first().copied(),
        final_value: series.last().copied(),
    }
}

/// The invariant PASSes iff the series has at least two points and no step
/// violated the invariant beyond tolerance. Uniform across the family: an
/// invariant's whole verdict is "enough points to witness it, and zero
/// violations".
pub fn invariant_passes(series_len: usize, observed: &InvariantObserved) -> bool {
    series_len >= 2 && observed.violation_count == 0
}

/// Every invariant this build implements and can re-check. The single source of
/// truth for the family: `is_known_invariant` and the verify-side "unsupported
/// invariant" diagnostic both read it, so adding an invariant here cannot leave
/// a hardcoded list stale (the failure mode the C1 review caught in the export
/// evidence). Each name here MUST have arms in `invariant_tolerance` and
/// `invariant_expectation`, and MUST be scored by `evaluate_measurement`: the
/// single-scalar invariants through its `evaluate_invariant` delegate, the
/// relation invariant through its own multi-column arm. Do NOT add the relation
/// name to `evaluate_invariant` (it lacks the `column_count` it needs).
pub const KNOWN_INVARIANTS: &[&str] = &[
    ENERGY_MONOTONE_INVARIANT,
    CONSERVATION_INVARIANT,
    BOUNDED_INVARIANT,
    ENERGY_IDENTITY_INVARIANT,
    RELATION_INVARIANT,
    CONSERVED_BAND_INVARIANT,
];

/// Whether `name` is an invariant this build implements (and can therefore
/// re-check). The oracle binding is pinned to this: a receipt naming an
/// invariant not in this set is `INVARIANT_UNSUPPORTED`.
pub fn is_known_invariant(name: &str) -> bool {
    KNOWN_INVARIANTS.contains(&name)
}

/// The column-count contract for an invariant: the `relation` invariant reads
/// ACROSS a row's columns and needs at least two; every single-scalar invariant
/// reads one value per step and requires exactly one column. Emit enforces this
/// before compiling; verify RE-CHECKS it (FIELD_CONTRACT_VIOLATION) so a
/// resealed receipt cannot present a column structure the invariant's contract
/// forbids, keeping the structural contract symmetric across emit and verify
/// like every other sealed field.
pub fn column_count_matches_invariant(name: &str, column_count: usize) -> bool {
    if name == RELATION_INVARIANT {
        column_count >= 2
    } else {
        column_count == 1
    }
}

/// The FIXED tolerance for a named invariant. Tolerance is a property of the
/// invariant, not an author-tunable knob: pinning it here (and re-checking the
/// sealed value against it at verify) stops a receipt from weakening its own
/// check by sealing a loose tolerance. Unknown names fall back to the strict
/// monotone tolerance (they are rejected upstream by `is_known_invariant`).
pub fn invariant_tolerance(name: &str) -> f64 {
    match name {
        CONSERVATION_INVARIANT => CONSERVATION_TOLERANCE,
        BOUNDED_INVARIANT => BOUNDED_TOLERANCE,
        ENERGY_IDENTITY_INVARIANT => ENERGY_IDENTITY_TOLERANCE,
        RELATION_INVARIANT => RELATION_TOLERANCE,
        CONSERVED_BAND_INVARIANT => CONSERVED_BAND_TOLERANCE,
        _ => ENERGY_MONOTONE_TOLERANCE,
    }
}

/// The human-readable expectation string sealed for a named invariant.
pub fn invariant_expectation(name: &str) -> &'static str {
    match name {
        CONSERVATION_INVARIANT => "no step deviates the conserved quantity beyond tolerance",
        BOUNDED_INVARIANT => "no step exceeds the initial value beyond tolerance",
        ENERGY_IDENTITY_INVARIANT => {
            "every step's energy-balance residual stays within tolerance of zero"
        }
        RELATION_INVARIANT => "every row's columns agree within tolerance",
        CONSERVED_BAND_INVARIANT => {
            "the quantity stays within the error budget of its initial value"
        }
        _ => "no step increases energy beyond tolerance",
    }
}

/// Evaluate the named invariant over a series. The single dispatch point both
/// emit and verify go through, so the two can never disagree on what an
/// invariant means.
pub fn evaluate_invariant(name: &str, series: &[f64], tol: f64) -> InvariantObserved {
    match name {
        // conserved-band reuses conservation's two-sided evaluator; only its
        // (looser, calibrated) tolerance differs, which `invariant_tolerance`
        // supplies. The distinct claim is "approximately conserved within a
        // fixed error budget", not "conserved to roundoff".
        CONSERVATION_INVARIANT | CONSERVED_BAND_INVARIANT => {
            conserved_quantity_constant(series, tol)
        }
        BOUNDED_INVARIANT => bounded_by_initial_maximum(series, tol),
        ENERGY_IDENTITY_INVARIANT => energy_identity_residual(series, tol),
        _ => energy_monotone_nonincreasing(series, tol),
    }
}

/// Cross-column relation invariant over a MULTI-COLUMN series (row-major, with
/// `column_count` values per row): every row must AGREE, i.e. all its columns
/// lie within `tol` of the row's first column. A "violation" is a disagreeing
/// row. The verifier computes this from the raw columns, so a program that
/// prints two independent computations of a quantity cannot conceal a
/// divergence between them. Returns the observed result AND the ROW count (the
/// number of observations, which the verdict's "enough points" rule uses, not
/// the raw token count).
///
/// A `column_count` below 2, an empty series, or a series whose length is not a
/// multiple of `column_count` (ragged rows) yields zero complete rows, which
/// the verdict rule treats as "cannot witness the relation".
pub fn relation_columns_agree(
    series: &[f64],
    tol: f64,
    column_count: usize,
) -> (InvariantObserved, usize) {
    let ragged = column_count < 2 || series.is_empty() || series.len() % column_count != 0;
    let rows = if ragged {
        0
    } else {
        series.len() / column_count
    };
    let mut violation_count = 0usize;
    let mut first_violation_step = None;
    for k in 0..rows {
        let base = k * column_count;
        let col0 = series[base];
        let disagrees = (1..column_count).any(|c| (series[base + c] - col0).abs() > tol);
        if disagrees {
            violation_count += 1;
            if first_violation_step.is_none() {
                first_violation_step = Some(k);
            }
        }
    }
    (
        InvariantObserved {
            violation_count,
            first_violation_step,
            initial_value: series.first().copied(),
            final_value: series.last().copied(),
        },
        rows,
    )
}

/// The observed invariant result plus the EFFECTIVE observation count the
/// verdict's "at least two points" rule uses: `series.len()` for the
/// single-scalar invariants, but the ROW count for a relation invariant (each
/// row of `column_count` values is one observation).
pub struct MeasurementVerdict {
    pub observed: InvariantObserved,
    pub effective_len: usize,
}

/// The SINGLE evaluation dispatch both emit and verify go through, so the two
/// can never disagree on how a measurement is scored. Single-column invariants
/// ignore `column_count`; the relation invariant de-interleaves by it.
pub fn evaluate_measurement(
    name: &str,
    series: &[f64],
    tol: f64,
    column_count: usize,
) -> MeasurementVerdict {
    match name {
        RELATION_INVARIANT => {
            let (observed, rows) = relation_columns_agree(series, tol, column_count);
            MeasurementVerdict {
                observed,
                effective_len: rows,
            }
        }
        _ => MeasurementVerdict {
            observed: evaluate_invariant(name, series, tol),
            effective_len: series.len(),
        },
    }
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
    /// The pass-0122 `compiler_branch` facts probed at emit.
    pub toolchain: ScientificToolchain,
    /// The effect/capability facts derived by the check pipeline at emit.
    pub effect_policy: ScientificEffectPolicy,
    /// Author-declared numerical method description (from `--method`).
    pub method_description: Option<String>,
    /// sha256 over the exact raw stdout bytes the run produced.
    pub raw_stdout_digest: ScientificDigest,
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
    /// The invariant to check over the series (a name from the registry;
    /// `is_known_invariant`). Selects the evaluator, tolerance, expectation,
    /// and the sealed oracle/invariant binding.
    pub invariant_name: String,
    pub metric: String,
    pub units: Option<String>,
    /// How many columns each row of the captured series holds (row-major). `1`
    /// for the single-scalar invariants; `>= 2` for the relation invariant.
    pub column_count: usize,
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
        toolchain,
        effect_policy,
        method_description,
        raw_stdout_digest,
        series,
        series_parsed,
        diverged,
        args,
        invariant_name,
        metric,
        units,
        column_count,
        problem_label,
        negative_fixture,
        flags,
    } = inputs;

    let tolerance = invariant_tolerance(&invariant_name);
    let verdict = evaluate_measurement(&invariant_name, &series, tolerance, column_count);
    let observed = verdict.observed;
    let has_series = series_parsed && !series.is_empty();
    // A diverged run (non-finite value observed) cannot witness the invariant,
    // even if its finite prefix looks monotone, so it never PASSes. The verdict
    // rule uses the effective observation count (rows for a relation invariant,
    // series length for the single-scalar ones).
    let passes = has_series && !diverged && invariant_passes(verdict.effective_len, &observed);

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

    // The typed-effect system doing receipt work: witnessed absences and the
    // determinism statement derive from the observed capability union.
    let (input_dataset, seed, determinism) = witnessed_fields_from_capabilities(
        &effect_policy.observed_capabilities,
        effect_policy.reads_stdin,
    );

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
            toolchain,
        },
        runtime_state: ScientificRuntimeState {
            os: os.to_string(),
            exit_code,
        },
        args,
        problem: ScientificProblem {
            label: problem_label,
        },
        oracle: ScientificOracle {
            kind: "declared_invariant".to_string(),
            name: invariant_name.clone(),
            status: "DECLARED".to_string(),
        },
        input_dataset,
        seed,
        determinism,
        numerical_method: ScientificNumericalMethod {
            status: if method_description.is_some() {
                "DECLARED".to_string()
            } else {
                "UNDECLARED".to_string()
            },
            description: method_description,
        },
        effect_policy,
        measurement: ScientificMeasurement {
            metric,
            observed_values: series,
            count,
            column_count,
            raw_stdout_digest,
            series_extraction_policy: SERIES_EXTRACTION_POLICY.to_string(),
            units,
        },
        invariant: ScientificInvariant {
            name: invariant_name.clone(),
            expectation: invariant_expectation(&invariant_name).to_string(),
            tolerance,
            observed,
            status: invariant_status.to_string(),
        },
        telemetry_branch: ScientificFencedBranch::fenced(),
        lineage_branch: ScientificFencedBranch::fenced(),
        negative_fixture,
        diverged,
        not_claimed: NOT_CLAIMED_BOUNDARY.iter().map(|s| s.to_string()).collect(),
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
    pub violation_count: usize,
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
    invariant_name: &str,
    series: &[f64],
    series_parsed: bool,
    diverged: bool,
    negative_fixture: bool,
    column_count: usize,
) -> RecomputedVerdict {
    let verdict = evaluate_measurement(
        invariant_name,
        series,
        invariant_tolerance(invariant_name),
        column_count,
    );
    let observed = verdict.observed;
    let has_series = series_parsed && !series.is_empty();
    let passes = has_series && !diverged && invariant_passes(verdict.effective_len, &observed);

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
        violation_count: observed.violation_count,
        receipt_status,
    }
}

/// What the `rederive_digests` callback re-derives from the source through
/// the check pipeline: the two input digests plus the effect/capability
/// facts (the sealed effect_policy is compared against these, so the policy
/// block is genuinely re-derived, never trusted).
pub struct RederivedFacts {
    pub source_digest: ScientificDigest,
    pub input_graph_digest: ScientificDigest,
    pub effect_policy: ScientificEffectPolicy,
}

/// What a verify re-run observes, returned by the `rerun_series` callback.
pub struct RerunObservation {
    /// The re-run's parsed numeric stdout.
    pub parsed: ParsedSeries,
    /// The re-run's process exit code (re-checked against the sealed
    /// `runtime_state.exit_code`; drift is `RERUN_EXIT_MISMATCH`).
    pub exit_code: i32,
    /// sha256 of the re-run's RAW stdout bytes (REPORTED as
    /// `raw_stdout_reproduced`; never a failure by itself).
    pub raw_stdout_digest: ScientificDigest,
    /// sha256 of the re-compiled program executable (REPORTED as
    /// `executable_reproduced`; never a failure by itself, since C compiler
    /// output is not byte-stable across compiler versions).
    pub executable_digest: ScientificDigest,
}

/// Verify a `buildlang-scientific-runtime-receipt/v0` receipt by RE-DERIVING,
/// never trusting the stored values (mirrors `corpus verify`'s discipline):
///
/// 1. Deserialize the receipt and confirm its schema and compiler.
/// 2. Recompute the seal over the stored receipt body and confirm it matches
///    the stored `seal.hex` (integrity of the receipt itself). This runs
///    BEFORE any sealed field is interpreted, so an unsealed edit is reported
///    as tampering rather than as whichever field-level contradiction it
///    happens to trip first.
/// 3. Re-derive the source + input-graph digests and the effect/capability
///    policy from the file on disk (via the `rederive_digests` callback, which
///    runs the same check pipeline that produced the stored facts) and
///    compare. A source change fails here.
/// 4. Re-run the program WITH THE STORED ARGS (via `rerun_series`, the shared
///    compile+run+capture path) and re-parse its numeric stdout into a fresh
///    series. The observed-value COUNT is re-checked against the stored
///    `measurement.count` (a deterministic quantity); a mismatch fails.
/// 5. Recompute the invariant + receipt-status verdict from that fresh series
///    and compare the recomputed `invariant.status`, `violation_count`, and
///    `receipt_status` against the stored ones. Any disagreement is a failure.
///    Individual float VALUES are deliberately NOT re-compared: exact floats
///    need not reproduce across platforms, which is precisely why the verdict
///    (monotonicity + count), not the raw series, is the re-checked quantity.
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
/// `probed_toolchain` is the LOCAL C toolchain probed by the caller before
/// dispatch: `None` means no C compiler is available, which is its own
/// `TOOL_UNAVAILABLE` verdict (exit 4) rather than a generic re-run failure.
/// A present-but-different toolchain WARNs (the verdict is the re-checked
/// quantity, so cross-toolchain re-verification is legitimate) and marks any
/// subsequent drift failure as possibly environmental.
///
/// `rerun_series` is called with the receipt's recorded args and returns a
/// [`RerunObservation`]: the parsed series, the process exit code (re-checked
/// against the sealed `runtime_state.exit_code`; drift is
/// `RERUN_EXIT_MISMATCH`, not a tamper-flavored count drift), and the raw
/// stdout + executable digests (REPORTED as reproduced / not reproduced,
/// never failures by themselves).
///
/// Returns Ok(report) for every FAITHFUL receipt regardless of its recorded
/// verdict (PASS, FAIL_EXPECTED, FAIL_UNEXPECTED, UNVERIFIABLE alike); the
/// callers decide what a faithful-but-failed verdict means for THEM
/// (`verify` maps it to exit 3, the export bridge maps it to a DRIFT or
/// unmeasurable Crucible measurement). Err(1) = did not reproduce; Err(4) =
/// no toolchain.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_scientific_runtime_receipt(
    receipt_json: &serde_json::Value,
    source_override: Option<&Path>,
    json: bool,
    current_compiler_version: &str,
    current_language_version: &str,
    probed_toolchain: Option<&ScientificToolchain>,
    rederive_digests: impl FnOnce(&Path) -> Result<RederivedFacts, i32>,
    rerun_series: impl FnOnce(&Path, &[String]) -> Result<RerunObservation, i32>,
) -> Result<ScientificVerifyReport, i32> {
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

    // Integrity gate: recompute the seal over the stored receipt body BEFORE
    // interpreting any sealed field. Schema and compiler are the applicability
    // gate (they decide whether this verifier's seal is even comparable); once
    // past them, the very next question is whether the body was tampered with.
    // Checking the seal first means every field-level rejection below
    // (OVERCLAIM_BOUNDARY_MISSING, FIELD_CONTRACT_VIOLATION, ORACLE_*, ...)
    // reports a genuinely author-sealed invalid VALUE rather than misreporting
    // an unsealed edit as an internal contradiction.
    let recomputed_seal = recompute_seal_hex(&receipt);
    if !recomputed_seal.eq_ignore_ascii_case(&receipt.seal.hex) {
        eprintln!(
            "Error: seal mismatch: receipt sha256:{}, recomputed sha256:{}",
            receipt.seal.hex, recomputed_seal
        );
        return Err(verify_failure_class(json, "SEAL_MISMATCH", 1));
    }

    // The claims boundary is load-bearing honesty (pass-0122
    // non_promotion_boundary): a receipt whose not_claimed omits
    // "physical_law" is overclaiming by omission and is rejected outright.
    if !receipt.not_claimed.iter().any(|c| c == "physical_law") {
        eprintln!(
            "Error: receipt claims boundary missing: not_claimed must include `physical_law`, got {:?}",
            receipt.not_claimed
        );
        return Err(verify_failure_class(json, "OVERCLAIM_BOUNDARY_MISSING", 1));
    }

    // The verifier's series-extraction policy must match the sealed one BY
    // VERSION TAG (the part before the first `:`): a receipt extracted under
    // a different policy family/version cannot be faithfully re-checked by
    // this build's parser, but a prose re-wording of the same versioned
    // policy must not read as tampering.
    let sealed_policy_tag = receipt
        .measurement
        .series_extraction_policy
        .split(':')
        .next()
        .unwrap_or("");
    let verifier_policy_tag = SERIES_EXTRACTION_POLICY.split(':').next().unwrap_or("");
    if sealed_policy_tag != verifier_policy_tag {
        eprintln!(
            "Error: series extraction policy mismatch: receipt `{}`, this verifier `{}`",
            receipt.measurement.series_extraction_policy, SERIES_EXTRACTION_POLICY
        );
        return Err(verify_failure_class(json, "EXTRACTION_POLICY_MISMATCH", 1));
    }

    // Every sealed digest must be a real sha256 (64 hex chars): an empty or
    // malformed digest inside a sealed receipt would let "hash unavailable"
    // masquerade as witnessed provenance.
    for (field, digest) in [
        ("source_digest", &receipt.source_digest),
        ("input_graph_digest", &receipt.input_graph_digest),
        (
            "build_state.toolchain.version_output_digest",
            &receipt.build_state.toolchain.version_output_digest,
        ),
        (
            "build_state.toolchain.buildc_binary_digest",
            &receipt.build_state.toolchain.buildc_binary_digest,
        ),
        (
            "build_state.toolchain.program_executable_digest",
            &receipt.build_state.toolchain.program_executable_digest,
        ),
        (
            "measurement.raw_stdout_digest",
            &receipt.measurement.raw_stdout_digest,
        ),
    ] {
        if !digest_is_well_formed(digest) {
            eprintln!(
                "Error: malformed digest in `{}`: algorithm `{}`, hex `{}` (must be sha256 with 64 hex chars)",
                field, digest.algorithm, digest.hex
            );
            return Err(verify_failure_class(json, "DIGEST_MALFORMED", 1));
        }
    }

    // Oracle binding, pinned to the IMPLEMENTATION rather than to another
    // sealed field: this verifier implements exactly one invariant, so both
    // the invariant name and the oracle name must equal it. Comparing
    // oracle.name against invariant.name alone would be self-referential
    // (both are author-controlled sealed strings that a resealed receipt can
    // set to any equal pair).
    if receipt.oracle.kind != "declared_invariant" {
        eprintln!(
            "Error: unsupported oracle kind `{}` (this verifier re-checks `declared_invariant` only)",
            receipt.oracle.kind
        );
        return Err(verify_failure_class(json, "ORACLE_KIND_UNSUPPORTED", 1));
    }
    if receipt.oracle.status != "DECLARED" {
        eprintln!(
            "Error: unsupported oracle status `{}` for kind `declared_invariant` (a declared oracle cannot claim an executed status)",
            receipt.oracle.status
        );
        return Err(verify_failure_class(json, "ORACLE_STATUS_UNSUPPORTED", 1));
    }
    // The invariant is pinned to the IMPLEMENTATION SET, not to another sealed
    // field: `invariant.name` must be one this build actually re-checks, and
    // the oracle must bind to that same invariant. Validating `invariant.name`
    // against `is_known_invariant` (not against `oracle.name`) is what keeps
    // the binding non-self-referential: a resealed receipt may pick any known
    // invariant, but never one the verifier cannot execute.
    if !is_known_invariant(&receipt.invariant.name) {
        eprintln!(
            "Error: unsupported invariant `{}` (this verifier implements {})",
            receipt.invariant.name,
            KNOWN_INVARIANTS.join(", ")
        );
        return Err(verify_failure_class(json, "INVARIANT_UNSUPPORTED", 1));
    }
    if receipt.oracle.name != receipt.invariant.name {
        eprintln!(
            "Error: oracle binding mismatch: oracle names `{}`, invariant is `{}`",
            receipt.oracle.name, receipt.invariant.name
        );
        return Err(verify_failure_class(json, "ORACLE_BINDING_MISMATCH", 1));
    }
    // Tolerance is a FIXED property of the invariant, never an author knob: a
    // receipt that sealed a different tolerance is trying to redefine (usually
    // to weaken) its own check, so it is rejected rather than re-checked under
    // the loosened bound.
    let canonical_tolerance = invariant_tolerance(&receipt.invariant.name);
    if receipt.invariant.tolerance != canonical_tolerance {
        eprintln!(
            "Error: invariant tolerance mismatch: receipt {}, canonical {} for `{}`",
            receipt.invariant.tolerance, canonical_tolerance, receipt.invariant.name
        );
        return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
    }

    // Re-check the column-count contract emit enforced (relation needs >= 2
    // columns, every single-scalar invariant needs exactly 1), so a resealed
    // receipt cannot present a column structure the invariant forbids. The
    // field is inert for the single-scalar invariants' verdict, but leaving the
    // contract unenforced at verify would let emit and verify disagree on what
    // a well-formed receipt is.
    if !column_count_matches_invariant(&receipt.invariant.name, receipt.measurement.column_count) {
        eprintln!(
            "Error: column_count {} violates the contract for invariant `{}` (relation requires >= 2, every single-scalar invariant requires exactly 1)",
            receipt.measurement.column_count, receipt.invariant.name
        );
        return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
    }

    // The fenced branches are load-bearing honesty: v0 produces neither
    // telemetry nor lineage, so the only valid sealed value is the explicit
    // fence. A branch edited (and resealed) to claim availability must be
    // rejected, or the fence would be decorative.
    if receipt.telemetry_branch.status != "UNAVAILABLE_FENCED"
        || receipt.lineage_branch.status != "UNAVAILABLE_FENCED"
    {
        eprintln!(
            "Error: unexpected fence status: telemetry `{}`, lineage `{}` (v0 produces neither; the only valid value is UNAVAILABLE_FENCED)",
            receipt.telemetry_branch.status, receipt.lineage_branch.status
        );
        return Err(verify_failure_class(json, "FENCE_STATUS_UNEXPECTED", 1));
    }

    // Field contracts the language version pins: v0 has no RNG builtin, so a
    // seed status other than NOT_APPLICABLE claims a capability the language
    // does not have; a numerical_method status must agree with whether a
    // description is present (a DECLARED method with no description, or an
    // UNDECLARED one with a description, is an inconsistent claim).
    if receipt.seed.status != "NOT_APPLICABLE" {
        eprintln!(
            "Error: seed status `{}` is not expressible: the language has no RNG builtin (v0 requires NOT_APPLICABLE)",
            receipt.seed.status
        );
        return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
    }
    let method_consistent = match receipt.numerical_method.status.as_str() {
        "DECLARED" => receipt.numerical_method.description.is_some(),
        "UNDECLARED" => receipt.numerical_method.description.is_none(),
        _ => false,
    };
    if !method_consistent {
        eprintln!(
            "Error: numerical_method status `{}` is inconsistent with its description (present: {})",
            receipt.numerical_method.status,
            receipt.numerical_method.description.is_some()
        );
        return Err(verify_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
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
    let rederived = rederive_digests(&source_path)
        .map_err(|code| verify_failure_class(json, "REDERIVATION_FAILED", code))?;
    let (source_digest, input_graph_digest) =
        (rederived.source_digest, rederived.input_graph_digest);
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

    // (2a) The effect policy is RE-DERIVED, never trusted: the facts digest,
    // the capability union, and every capability-derived witnessed field must
    // agree with what the checker derives from the source right now.
    if !digests_match(
        &rederived.effect_policy.facts_digest,
        &receipt.effect_policy.facts_digest,
    ) || rederived.effect_policy.observed_capabilities
        != receipt.effect_policy.observed_capabilities
        || rederived.effect_policy.reads_stdin != receipt.effect_policy.reads_stdin
    {
        eprintln!(
            "Error: effect policy drift: receipt capabilities {:?} reads_stdin={} (facts sha256:{}), re-derived {:?} reads_stdin={} (facts sha256:{})",
            receipt.effect_policy.observed_capabilities,
            receipt.effect_policy.reads_stdin,
            receipt.effect_policy.facts_digest.hex,
            rederived.effect_policy.observed_capabilities,
            rederived.effect_policy.reads_stdin,
            rederived.effect_policy.facts_digest.hex
        );
        return Err(verify_failure_class(json, "EFFECT_POLICY_DRIFT", 1));
    }
    let (expected_input_dataset, expected_seed, expected_determinism) =
        witnessed_fields_from_capabilities(
            &rederived.effect_policy.observed_capabilities,
            rederived.effect_policy.reads_stdin,
        );
    if receipt.input_dataset != expected_input_dataset
        || receipt.seed != expected_seed
        || receipt.determinism != expected_determinism
    {
        eprintln!(
            "Error: effect policy drift: the sealed input_dataset/seed/determinism fields do not re-derive from the observed capabilities"
        );
        return Err(verify_failure_class(json, "EFFECT_POLICY_DRIFT", 1));
    }

    // Toolchain preflight: no C compiler at all is its own verdict (exit 4),
    // distinct from both drift (1) and faithful-fail (3); a DIFFERENT
    // toolchain warns, and the verdict re-check proceeds (cross-toolchain
    // re-verification is legitimate by design).
    let toolchain_matched = match probed_toolchain {
        None => {
            eprintln!(
                "Error: no C compiler available to re-run the program (receipt was sealed under `{}`)",
                receipt.build_state.toolchain.c_compiler
            );
            return Err(verify_failure_class(json, "TOOL_UNAVAILABLE", 4));
        }
        Some(probed) => {
            let matched = probed.c_compiler == receipt.build_state.toolchain.c_compiler
                && probed.version_output_digest
                    == receipt.build_state.toolchain.version_output_digest
                && probed.target == receipt.build_state.toolchain.target;
            if !matched {
                eprintln!(
                    "Warning: toolchain differs: receipt `{}` ({}) on {}, local `{}` ({}) on {} (re-checking the verdict anyway; any drift below may be environmental)",
                    receipt.build_state.toolchain.c_compiler,
                    receipt.build_state.toolchain.c_compiler_version,
                    receipt.build_state.toolchain.target,
                    probed.c_compiler,
                    probed.c_compiler_version,
                    probed.target
                );
            }
            matched
        }
    };

    // (3) Re-run the program WITH THE STORED ARGS and re-parse its stdout, so an
    // argv-parameterized kernel is reproduced under the same conditions it was
    // emitted under.
    let observation = rerun_series(&source_path, &receipt.args)
        .map_err(|code| verify_failure_class(json, "RERUN_FAILED", code))?;
    let parsed = observation.parsed;

    // (3a) The process exit code is sealed and deterministic; a re-run that
    // exits differently (including a crash) is its own failure class, checked
    // BEFORE the series comparisons so a crashed re-run is not misreported as
    // a tamper-flavored count drift.
    if observation.exit_code != receipt.runtime_state.exit_code {
        eprintln!(
            "Error: exit code drift: receipt {}, re-run {}",
            receipt.runtime_state.exit_code, observation.exit_code
        );
        return Err(verify_failure_class(json, "RERUN_EXIT_MISMATCH", 1));
    }

    // Raw-byte and executable reproduction are REPORTED, never required:
    // exact stdout bytes and C compiler output are platform-dependent by
    // design (the verdict is the re-checked quantity). A match is the
    // strongest reproduction signal; a mismatch with a matching verdict is
    // still faithful.
    let raw_stdout_reproduced = digests_match(
        &observation.raw_stdout_digest,
        &receipt.measurement.raw_stdout_digest,
    );
    let executable_reproduced = digests_match(
        &observation.executable_digest,
        &receipt.build_state.toolchain.program_executable_digest,
    );

    // For a DIVERGED run the finite-prefix length (and hence violation_count
    // over that prefix) is the step index of the first non-finite value: a
    // function of the exact float trajectory, which the design declares
    // non-reproducible across toolchains (a 1-ULP libm difference can shift
    // the divergence step). So when the receipt records divergence AND the
    // re-run also diverges, the prefix-derived checks are skipped and the
    // reproduced divergence itself is the faithfulness signal. A re-run that
    // does NOT diverge when the receipt says it did (or vice versa) falls
    // through to the strict checks and fails as non-reproduction.
    let both_diverged = receipt.diverged && parsed.diverged;

    // (3b) For non-diverged runs the observed-value count IS deterministic
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
        &receipt.invariant.name,
        &parsed.series,
        parsed.any_parsed,
        parsed.diverged,
        receipt.negative_fixture,
        receipt.measurement.column_count,
    );
    let stored_increase = receipt.invariant.observed.violation_count;

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
    if !both_diverged && recomputed.violation_count != stored_increase {
        eprintln!(
            "Error: violation_count drift: receipt {}, re-run {}",
            stored_increase, recomputed.violation_count
        );
        return Err(verify_failure_class(json, "VIOLATION_COUNT_DRIFT", 1));
    }
    if recomputed.receipt_status != receipt.receipt_status {
        eprintln!(
            "Error: receipt_status drift: receipt {}, re-run {}",
            receipt.receipt_status, recomputed.receipt_status
        );
        return Err(verify_failure_class(json, "RECEIPT_STATUS_DRIFT", 1));
    }

    // The receipt is FAITHFUL (seal, digests, count, and verdict all
    // re-check). Whether the recorded verdict is a PASS is a separate
    // question the report carries; the printing wrapper (and the export
    // bridge) decide what to do with it.
    Ok(ScientificVerifyReport {
        invariant_status: recomputed.invariant_status,
        invariant_name: receipt.invariant.name.clone(),
        violation_count: recomputed.violation_count,
        receipt_status: recomputed.receipt_status,
        invariant_held: matches!(recomputed.receipt_status, "PASS" | "FAIL_EXPECTED"),
        toolchain_matched,
        raw_stdout_reproduced,
        executable_reproduced,
        tolerance: receipt.invariant.tolerance,
        negative_fixture: receipt.negative_fixture,
        diverged: receipt.diverged,
        source: source_path.to_string_lossy().to_string(),
        source_digest_hex: receipt.source_digest.hex.clone(),
        raw_stdout_digest_hex: receipt.measurement.raw_stdout_digest.hex.clone(),
        args: receipt.args.clone(),
        seal_hex: receipt.seal.hex.clone(),
    })
}

/// Everything a FAITHFUL re-verification established, for consumers beyond
/// the human/`--json` printer: the export bridge derives Crucible
/// measurements from this report (never from stored receipt values alone).
/// Faithful does NOT mean the invariant held: `invariant_held` carries that.
pub struct ScientificVerifyReport {
    pub invariant_status: &'static str,
    /// The sealed invariant name (e.g. `conserved_quantity_constant`). Carried
    /// so the export bridge labels the witnessed Crucible measurement with the
    /// invariant the receipt ACTUALLY checked, never a hardcoded default.
    pub invariant_name: String,
    pub violation_count: usize,
    pub receipt_status: &'static str,
    pub invariant_held: bool,
    pub toolchain_matched: bool,
    pub raw_stdout_reproduced: bool,
    pub executable_reproduced: bool,
    pub tolerance: f64,
    pub negative_fixture: bool,
    pub diverged: bool,
    pub source: String,
    pub source_digest_hex: String,
    pub raw_stdout_digest_hex: String,
    pub args: Vec<String>,
    pub seal_hex: String,
}

/// Schema id for the Crucible-measurement export envelope (the Telos bridge).
pub const CRUCIBLE_MEASUREMENT_EXPORT_SCHEMA: &str = "buildlang-crucible-measurement-export/v0";

/// The versioned `method` string carried by exported measurements: names the
/// discipline (re-executed verification, never stored-value copying) so a
/// consumer can distinguish these rows from author-typed ones.
pub const CRUCIBLE_EXPORT_METHOD: &str = "buildc-receipt-verify/reexecuted-v1";

/// Map a FAITHFUL re-verification report into one Crucible measurement row
/// (the shape `crucible assess --measurements` ingests: claim_id,
/// claim_sha256, deviation, tolerance, method, measured_at, evidence,
/// recheck). PURE and total over reports, so the mapping rules are unit
/// tested without IO.
///
/// The honesty rules of the mapping:
/// - `deviation` is DERIVED from the fresh re-run (the report), never copied
///   from stored receipt values. UNVERIFIABLE receipts export deviation null
///   (Crucible reads unmeasurable as UNVERIFIABLE, fail-closed); everything
///   else exports the recomputed violation_count.
/// - `tolerance` is 0.5: violation_count is integral, so 0.5 cleanly separates
///   "no increases" (MATCH) from "any increase" (DRIFT). A FAIL_EXPECTED
///   receipt therefore exports a row that reads DRIFT against a
///   holds-everywhere claim; binding it to a claim whose falsification
///   expects the failure is the thesis side's job, and the receipt_status is
///   carried in evidence so that side can frame it.
/// - `recheck` seals everything an independent replayer needs to re-run
///   buildc and rebuild this row: the measurement is WITNESSED, not asserted
///   (a measurement without a recheck descriptor is exactly the
///   author-supplied pattern the provenance gate exists to catch).
pub fn crucible_measurement_from_report(
    report: &ScientificVerifyReport,
    claim_id: &str,
    claim_sha256: &str,
    claim_expects_failure: bool,
    receipt_path: &str,
    receipt_file_sha256: &str,
    measured_at: f64,
) -> serde_json::Value {
    // Deviation semantics, claim-relative when the binding declares an
    // expected failure (Crucible's verdict math is pure margin arithmetic;
    // there is no thesis-side escape hatch, so the expectation must be bound
    // HERE): a negative-fixture receipt that failed as predicted deviates 0
    // from its claim; one that unexpectedly PASSed deviates 1. Without the
    // expectation the deviation is the recomputed increase count, and an
    // UNVERIFIABLE receipt is unmeasurable (null; Crucible fails closed).
    let deviation = if report.receipt_status == "UNVERIFIABLE" {
        serde_json::Value::Null
    } else if claim_expects_failure {
        if report.receipt_status == "FAIL_EXPECTED" {
            serde_json::json!(0.0)
        } else {
            // The claim predicted failure; the run did not fail.
            serde_json::json!(1.0)
        }
    } else {
        serde_json::json!(report.violation_count as f64)
    };

    // For a DIVERGED receipt the increase count is prefix-derived and
    // platform-dependent (the verifier's own both_diverged rule skips it);
    // sealing a concrete expectation would make an independent replayer
    // wrongly conclude non-reproduction of a faithful receipt.
    let expected_violation_count = if report.diverged {
        serde_json::Value::Null
    } else {
        serde_json::json!(report.violation_count)
    };

    let mut evidence = vec![
        format!("receipt_seal:sha256:{}", report.seal_hex),
        format!("source:sha256:{}", report.source_digest_hex),
        // The digest sealed AT EMISSION, not the re-run's bytes (which are
        // platform-dependent); the reproduction object below says whether
        // the witnessing re-run reproduced it.
        format!("sealed_raw_stdout:sha256:{}", report.raw_stdout_digest_hex),
        format!("receipt_status:{}", report.receipt_status),
        format!("invariant:{}", report.invariant_name),
        format!("negative_fixture:{}", report.negative_fixture),
    ];
    if claim_expects_failure {
        evidence.push("claim_expectation:expects_failure".to_string());
    }

    serde_json::json!({
        "claim_id": claim_id,
        "claim_sha256": claim_sha256,
        "deviation": deviation,
        "tolerance": 0.5,
        "method": CRUCIBLE_EXPORT_METHOD,
        "measured_at": measured_at,
        "evidence": evidence,
        // What the witnessing re-run observed about reproduction. Kept OUT of
        // `evidence` deliberately: Crucible's recheck compares the evidence
        // list for stability, and these flags legitimately differ per replay
        // environment. A top-level key is outside the fixed measurement-seal
        // field list, so it stays visible to auditors without destabilizing
        // recheck.
        "reproduction": {
            "toolchain_matched": report.toolchain_matched,
            "raw_stdout_reproduced": report.raw_stdout_reproduced,
            "executable_reproduced": report.executable_reproduced,
        },
        "recheck": {
            "oracle": "buildc.receipt.verify",
            "receipt_path": receipt_path,
            "receipt_sha256": receipt_file_sha256,
            "source": report.source,
            "source_sha256": report.source_digest_hex,
            "args": report.args,
            "command": ["buildc", "receipt", "verify", receipt_path, "--json"],
            "diverged": report.diverged,
            "expected": {
                "receipt_status": report.receipt_status,
                "invariant_status": report.invariant_status,
                // Null for diverged receipts: reproduced divergence (same
                // receipt_status) is the faithfulness signal, mirroring the
                // verifier's both_diverged rule.
                "violation_count": expected_violation_count,
                // The exit code the sealed replay command must yield: 0 for
                // faithful PASS/FAIL_EXPECTED, 3 for faithful
                // FAIL_UNEXPECTED/UNVERIFIABLE.
                "exit_code": if report.invariant_held { 0 } else { 3 },
            },
        },
    })
}

/// The `receipt verify` entry point: evaluate, print (human or `--json`),
/// and map the report to the exit-code contract (Ok for faithful
/// PASS/FAIL_EXPECTED; Err(3) for faithful FAIL_UNEXPECTED/UNVERIFIABLE;
/// Err(1)/Err(4) propagate from evaluation).
#[allow(clippy::too_many_arguments)]
pub fn verify_scientific_runtime_receipt(
    receipt_json: &serde_json::Value,
    source_override: Option<&Path>,
    json: bool,
    current_compiler_version: &str,
    current_language_version: &str,
    probed_toolchain: Option<&ScientificToolchain>,
    rederive_digests: impl FnOnce(&Path) -> Result<RederivedFacts, i32>,
    rerun_series: impl FnOnce(&Path, &[String]) -> Result<RerunObservation, i32>,
) -> Result<(), i32> {
    let report = evaluate_scientific_runtime_receipt(
        receipt_json,
        source_override,
        json,
        current_compiler_version,
        current_language_version,
        probed_toolchain,
        rederive_digests,
        rerun_series,
    )?;

    if json {
        let mut out = serde_json::json!({
            "schema": SCIENTIFIC_RUNTIME_SCHEMA,
            "status": if report.invariant_held { "match" } else { "invariant_not_held" },
            "faithful": true,
            "invariant_held": report.invariant_held,
            "source": report.source,
            "invariant_status": report.invariant_status,
            "violation_count": report.violation_count,
            "receipt_status": report.receipt_status,
            "toolchain_matched": report.toolchain_matched,
            "raw_stdout_reproduced": report.raw_stdout_reproduced,
            "executable_reproduced": report.executable_reproduced,
            "seal": { "algorithm": "sha256", "hex": report.seal_hex },
        });
        if !report.invariant_held {
            out["failure_class"] = serde_json::Value::String("INVARIANT_NOT_HELD".to_string());
        }
        let text = serde_json::to_string_pretty(&out).map_err(|err| {
            eprintln!(
                "Error serializing scientific-runtime verification report: {}",
                err
            );
            1
        })?;
        println!("{}", text);
    } else if report.invariant_held {
        println!(
            "MATCH: scientific-runtime receipt re-runs and re-checks clean ({}, violation_count={}; toolchain_matched={}, raw_stdout_reproduced={}, executable_reproduced={})",
            report.receipt_status,
            report.violation_count,
            report.toolchain_matched,
            report.raw_stdout_reproduced,
            report.executable_reproduced
        );
    } else {
        eprintln!(
            "FAIL: scientific-runtime receipt faithfully reproduces, but the invariant did not hold ({}, violation_count={}). `receipt verify` exits nonzero so it is safe as a pass/fail gate.",
            report.receipt_status, report.violation_count
        );
    }

    if report.invariant_held {
        Ok(())
    } else {
        // The class line goes to stderr in both modes (the json report above
        // already carries the field; the human FAIL line is prose).
        eprintln!("failure_class: INVARIANT_NOT_HELD");
        Err(3)
    }
}

/// Two digests match iff their algorithm and (case-insensitive) hex agree AND
/// both carry a real hex value. Two EMPTY digests never match: an absent hash
/// must not report as the strongest reproduction signal (a vacuous
/// `reproduced=true` from two failed reads would fabricate provenance).
fn digests_match(actual: &ScientificDigest, expected: &ScientificDigest) -> bool {
    !actual.hex.is_empty()
        && actual.algorithm.eq_ignore_ascii_case(&expected.algorithm)
        && actual.hex.eq_ignore_ascii_case(&expected.hex)
}

/// A sealed digest field must be a real sha256: exactly 64 hex chars. A
/// receipt carrying an empty or malformed digest is rejected outright
/// (`DIGEST_MALFORMED`), so "digest unavailable" can never masquerade as a
/// witnessed hash inside a sealed receipt.
fn digest_is_well_formed(digest: &ScientificDigest) -> bool {
    digest.algorithm.eq_ignore_ascii_case("sha256")
        && digest.hex.len() == 64
        && digest.hex.chars().all(|ch| ch.is_ascii_hexdigit())
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
/// - `OVERCLAIM_BOUNDARY_MISSING`: `not_claimed` omits "physical_law".
/// - `EXTRACTION_POLICY_MISMATCH`: the sealed series-extraction policy's
///   version tag is not the one this verifier implements.
/// - `DIGEST_MALFORMED`: a sealed digest field is not a real sha256 (64 hex
///   chars), so "hash unavailable" cannot masquerade as witnessed provenance.
/// - `ORACLE_KIND_UNSUPPORTED`, `ORACLE_STATUS_UNSUPPORTED`,
///   `ORACLE_BINDING_MISMATCH`, `INVARIANT_UNSUPPORTED`: the oracle/invariant
///   block names a kind, status, or criterion this verifier does not
///   implement, or the oracle does not bind to the implemented invariant
///   (binding is pinned to the implementation, never to another sealed field).
/// - `FENCE_STATUS_UNEXPECTED`: a telemetry/lineage fence was edited to claim
///   availability v0 does not produce.
/// - `FIELD_CONTRACT_VIOLATION`: a sealed field claims something the language
///   version cannot express (a seed when no RNG builtin exists) or is
///   internally inconsistent (a DECLARED method with no description).
/// - `EFFECT_POLICY_DRIFT`: the sealed effect/capability facts, or the
///   witnessed fields derived from them, do not re-derive from the source.
/// - `TOOL_UNAVAILABLE` (exit 4): no C compiler is available for the re-run.
/// - `REDERIVATION_FAILED`, `RERUN_FAILED`: the source could not be re-checked
///   or re-run (missing file, toolchain failure), distinct from drift.
/// - `SOURCE_DIGEST_MISMATCH`, `INPUT_GRAPH_DIGEST_MISMATCH`: the source
///   changed since sealing.
/// - `RERUN_EXIT_MISMATCH`: the re-run's process exit code differs from the
///   sealed `runtime_state.exit_code` (covers a crashing re-run).
/// - `MEASUREMENT_COUNT_DRIFT`, `INVARIANT_STATUS_DRIFT`,
///   `VIOLATION_COUNT_DRIFT`, `RECEIPT_STATUS_DRIFT`: the re-run disagrees with
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

    fn hex_digest(fill: char) -> ScientificDigest {
        ScientificDigest {
            algorithm: "sha256".to_string(),
            hex: fill.to_string().repeat(64),
        }
    }

    /// The toolchain fixture sealed into test receipts; also what tests pass
    /// as the verify-time probe (matching by default).
    fn test_toolchain() -> ScientificToolchain {
        ScientificToolchain {
            c_compiler: "test-cc".to_string(),
            c_compiler_version: "test-cc 1.0".to_string(),
            version_output_digest: hex_digest('d'),
            target: "test-os/test-arch".to_string(),
            buildc_binary_digest: hex_digest('e'),
            program_executable_digest: hex_digest('f'),
        }
    }

    /// The effect-policy fixture sealed into test receipts; the rederive
    /// helper returns the same facts so faithful tests re-derive cleanly.
    fn test_effect_policy() -> ScientificEffectPolicy {
        ScientificEffectPolicy {
            facts_digest: hex_digest('7'),
            observed_capabilities: vec!["Console".to_string()],
            reads_stdin: false,
        }
    }

    fn rederive_facts(
        source_digest: ScientificDigest,
        input_graph_digest: ScientificDigest,
    ) -> RederivedFacts {
        RederivedFacts {
            source_digest,
            input_graph_digest,
            effect_policy: test_effect_policy(),
        }
    }

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
            source_digest: hex_digest('a'),
            input_graph_digest: hex_digest('b'),
            target: "c",
            os: "test-os",
            exit_code: 0,
            toolchain: test_toolchain(),
            effect_policy: test_effect_policy(),
            method_description: None,
            raw_stdout_digest: hex_digest('c'),
            series,
            series_parsed: parsed,
            diverged: false,
            args: Vec::new(),
            invariant_name: ENERGY_MONOTONE_INVARIANT.to_string(),
            metric: "series".to_string(),
            units: None,
            column_count: 1,
            problem_label: None,
            negative_fixture,
            flags: Vec::new(),
        }
    }

    /// A `base_inputs` variant that seals a DIFFERENT invariant, for the
    /// conservation-family tests.
    fn base_inputs_for<'a>(
        invariant_name: &str,
        path: &'a Path,
        series: Vec<f64>,
        parsed: bool,
        negative_fixture: bool,
    ) -> ScientificReceiptInputs<'a> {
        ScientificReceiptInputs {
            invariant_name: invariant_name.to_string(),
            ..base_inputs(path, series, parsed, negative_fixture)
        }
    }

    #[test]
    fn monotone_series_has_zero_increases() {
        let series = [4.0, 3.0, 3.0, 2.5, 1.0];
        let observed = energy_monotone_nonincreasing(&series, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.violation_count, 0);
        assert_eq!(observed.first_violation_step, None);
        assert_eq!(observed.initial_value, Some(4.0));
        assert_eq!(observed.final_value, Some(1.0));
        assert!(invariant_passes(series.len(), &observed));
    }

    #[test]
    fn one_bump_is_counted_with_first_violation_step() {
        // Increase happens at k = 2 (index 2 -> index 3: 2.0 -> 5.0).
        let series = [4.0, 3.0, 2.0, 5.0, 1.0];
        let observed = energy_monotone_nonincreasing(&series, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.violation_count, 1);
        assert_eq!(observed.first_violation_step, Some(2));
        assert!(!invariant_passes(series.len(), &observed));
    }

    #[test]
    fn tolerance_absorbs_tiny_jitter_but_not_real_growth() {
        // A sub-tolerance wiggle up is NOT an increase.
        let jitter = [1.0, 1.0 + 5e-13, 1.0];
        let observed = energy_monotone_nonincreasing(&jitter, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.violation_count, 0);

        // A supra-tolerance step up IS an increase.
        let growth = [1.0, 1.0 + 1e-9, 1.0 + 2e-9];
        let observed = energy_monotone_nonincreasing(&growth, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.violation_count, 2);
        assert_eq!(observed.first_violation_step, Some(0));
    }

    #[test]
    fn single_point_series_does_not_pass() {
        let series = [1.0];
        let observed = energy_monotone_nonincreasing(&series, ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.violation_count, 0);
        // Zero increases but only one point: cannot witness monotonicity.
        assert!(!invariant_passes(series.len(), &observed));
    }

    #[test]
    fn empty_series_does_not_pass() {
        let observed = energy_monotone_nonincreasing(&[], ENERGY_MONOTONE_TOLERANCE);
        assert_eq!(observed.violation_count, 0);
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
        assert_eq!(receipt.invariant.observed.violation_count, 0);
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
        assert_eq!(receipt.invariant.observed.violation_count, 2);
        assert_eq!(receipt.invariant.observed.first_violation_step, Some(0));
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
        receipt.invariant.observed.violation_count = 99;
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
        let verdict = recompute_verdict(
            ENERGY_MONOTONE_INVARIANT,
            &[4.0, 3.0, 2.0],
            true,
            false,
            false,
            1,
        );
        assert_eq!(verdict.invariant_status, "PASS");
        assert_eq!(verdict.receipt_status, "PASS");
        assert_eq!(verdict.violation_count, 0);
    }

    #[test]
    fn recompute_verdict_distinguishes_expected_from_unexpected_failure() {
        // The negative-fixture flag (read back from the stored receipt) is what
        // separates FAIL_EXPECTED from FAIL_UNEXPECTED on a re-run.
        let expected = recompute_verdict(
            ENERGY_MONOTONE_INVARIANT,
            &[1.0, 2.0, 3.0],
            true,
            false,
            true,
            1,
        );
        assert_eq!(expected.invariant_status, "FAIL");
        assert_eq!(expected.receipt_status, "FAIL_EXPECTED");
        assert_eq!(expected.violation_count, 2);

        let unexpected = recompute_verdict(
            ENERGY_MONOTONE_INVARIANT,
            &[1.0, 2.0, 3.0],
            true,
            false,
            false,
            1,
        );
        assert_eq!(unexpected.receipt_status, "FAIL_UNEXPECTED");
    }

    #[test]
    fn recompute_verdict_is_unverifiable_when_nothing_parsed() {
        let verdict = recompute_verdict(ENERGY_MONOTONE_INVARIANT, &[], false, false, false, 1);
        assert_eq!(verdict.receipt_status, "UNVERIFIABLE");
        assert_eq!(verdict.invariant_status, "FAIL");
    }

    #[test]
    fn recompute_verdict_is_unverifiable_when_diverged() {
        // A monotone finite prefix that diverged is UNVERIFIABLE, not PASS, so a
        // re-run of a diverged program re-derives the same UNVERIFIABLE verdict.
        let verdict =
            recompute_verdict(ENERGY_MONOTONE_INVARIANT, &[4.0, 3.0], true, true, false, 1);
        assert_eq!(verdict.receipt_status, "UNVERIFIABLE");
        assert_eq!(verdict.invariant_status, "FAIL");
    }

    #[test]
    fn conservation_holds_for_a_constant_series() {
        let obs = conserved_quantity_constant(&[2.0, 2.0, 2.0, 2.0], CONSERVATION_TOLERANCE);
        assert_eq!(obs.violation_count, 0);
        assert!(invariant_passes(4, &obs));
        // Roundoff-scale jitter within tolerance is still conserved.
        let obs =
            conserved_quantity_constant(&[2.0, 2.0 + 1e-12, 2.0 - 1e-12], CONSERVATION_TOLERANCE);
        assert_eq!(obs.violation_count, 0);
    }

    #[test]
    fn conservation_flags_a_leak() {
        // A quantity that drifts from its initial value beyond tolerance: steps
        // 2 and 3 deviate from the reference 2.0.
        let obs = conserved_quantity_constant(&[2.0, 2.0, 1.5, 1.0], CONSERVATION_TOLERANCE);
        assert_eq!(obs.violation_count, 2);
        assert_eq!(obs.first_violation_step, Some(2));
        assert!(!invariant_passes(4, &obs));
        assert_eq!(obs.initial_value, Some(2.0));
        assert_eq!(obs.final_value, Some(1.0));
    }

    #[test]
    fn invariant_registry_dispatches_by_name() {
        assert!(is_known_invariant(ENERGY_MONOTONE_INVARIANT));
        assert!(is_known_invariant(CONSERVATION_INVARIANT));
        assert!(is_known_invariant(BOUNDED_INVARIANT));
        assert!(!is_known_invariant("no_such_invariant"));
        // Every name the family advertises must have a real tolerance and a
        // real evaluator arm (the KNOWN_INVARIANTS list is the single source of
        // truth; this guards against a name being advertised but unimplemented).
        for name in KNOWN_INVARIANTS {
            assert!(is_known_invariant(name));
            let _ = invariant_expectation(name);
            // Route through evaluate_measurement (the real dispatch) with a
            // 2-column series so the relation invariant exercises its own arm,
            // not evaluate_invariant's single-series fallback.
            let _ = evaluate_measurement(name, &[1.0, 1.0], invariant_tolerance(name), 2);
        }
        assert_eq!(
            invariant_tolerance(CONSERVATION_INVARIANT),
            CONSERVATION_TOLERANCE
        );
        assert_eq!(
            invariant_tolerance(ENERGY_MONOTONE_INVARIANT),
            ENERGY_MONOTONE_TOLERANCE
        );
        assert_eq!(invariant_tolerance(BOUNDED_INVARIANT), BOUNDED_TOLERANCE);

        // A monotone-DECREASING series distinguishes the two evaluators: it has
        // zero monotone violations (non-increasing) but DOES deviate from its
        // initial value, so conservation flags it. This proves the dispatcher
        // routes to genuinely different checks, not one aliased to the other.
        let decreasing = [3.0, 2.0, 1.0];
        let mono = evaluate_invariant(
            ENERGY_MONOTONE_INVARIANT,
            &decreasing,
            ENERGY_MONOTONE_TOLERANCE,
        );
        assert_eq!(mono.violation_count, 0);
        let cons = evaluate_invariant(CONSERVATION_INVARIANT, &decreasing, CONSERVATION_TOLERANCE);
        assert_eq!(cons.violation_count, 2);
    }

    #[test]
    fn verify_round_trips_a_conservation_receipt() {
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(base_inputs_for(
            CONSERVATION_INVARIANT,
            path,
            vec![2.0, 2.0, 2.0],
            true,
            false,
        ));
        assert_eq!(receipt.invariant.name, CONSERVATION_INVARIANT);
        assert_eq!(receipt.oracle.name, CONSERVATION_INVARIANT);
        assert_eq!(receipt.invariant.tolerance, CONSERVATION_TOLERANCE);
        assert_eq!(receipt.receipt_status, "PASS");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![2.0, 2.0, 2.0])),
        );
        assert!(
            result.is_ok(),
            "a faithful conservation receipt must verify: {result:?}"
        );
    }

    #[test]
    fn verify_fails_a_faithful_conservation_leak() {
        // A conserved quantity that leaks (drifts past tolerance) faithfully
        // reproduces a FAIL_UNEXPECTED, so `receipt verify` exits 3 (not 0).
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(base_inputs_for(
            CONSERVATION_INVARIANT,
            path,
            vec![2.0, 2.0, 1.0],
            true,
            false,
        ));
        assert_eq!(receipt.receipt_status, "FAIL_UNEXPECTED");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![2.0, 2.0, 1.0])),
        );
        assert_eq!(
            result,
            Err(3),
            "a faithful conservation leak must exit 3 (FAIL_UNEXPECTED)"
        );
    }

    #[test]
    fn verify_rejects_a_non_canonical_invariant_tolerance() {
        // Tolerance is fixed per invariant; a receipt that resealed a loosened
        // tolerance is rejected (FIELD_CONTRACT_VIOLATION), never re-checked
        // under the weaker bound.
        let path = Path::new("k.bld");
        let mut receipt = build_scientific_runtime_receipt(base_inputs_for(
            CONSERVATION_INVARIANT,
            path,
            vec![2.0, 2.0, 2.0],
            true,
            false,
        ));
        receipt.invariant.tolerance = 1e6;
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![2.0, 2.0, 2.0])),
        );
        assert_eq!(result, Err(1), "a non-canonical tolerance must be rejected");
    }

    #[test]
    fn bounded_holds_for_a_capped_series() {
        // Every value sits at or below the initial 1.0, so the maximum
        // principle holds even though the series rises and falls (index 3
        // returns to exactly the reference, which is not an overshoot).
        let obs = bounded_by_initial_maximum(&[1.0, 0.5, 0.8, 1.0, 0.3], BOUNDED_TOLERANCE);
        assert_eq!(obs.violation_count, 0);
        assert!(invariant_passes(5, &obs));
        // Roundoff-scale rise above the reference is still within bound.
        let obs = bounded_by_initial_maximum(&[1.0, 1.0 + 1e-12], BOUNDED_TOLERANCE);
        assert_eq!(obs.violation_count, 0);
    }

    #[test]
    fn bounded_flags_an_overshoot() {
        // Values that exceed the initial 1.0 beyond tolerance: indices 1 and 3
        // overshoot; the quantity dipping in between does not matter.
        let obs = bounded_by_initial_maximum(&[1.0, 1.5, 0.5, 2.0], BOUNDED_TOLERANCE);
        assert_eq!(obs.violation_count, 2);
        assert_eq!(obs.first_violation_step, Some(1));
        assert!(!invariant_passes(4, &obs));
        assert_eq!(obs.initial_value, Some(1.0));
        assert_eq!(obs.final_value, Some(2.0));
    }

    #[test]
    fn bounded_is_distinct_from_the_other_invariants() {
        // A series that dips and returns to its initial value is the witness
        // that `bounded` is a genuinely separate check: it exceeds nothing
        // (bounded PASSes) yet it deviates (conservation flags it) and it rises
        // (monotone flags it). One series, three different verdicts.
        let dip_return = [1.0, 0.0, 1.0];
        let bounded = evaluate_invariant(BOUNDED_INVARIANT, &dip_return, BOUNDED_TOLERANCE);
        assert_eq!(bounded.violation_count, 0, "bounded accepts the capped dip");
        let cons = evaluate_invariant(CONSERVATION_INVARIANT, &dip_return, CONSERVATION_TOLERANCE);
        assert_eq!(cons.violation_count, 1, "conservation flags the deviation");
        let mono = evaluate_invariant(
            ENERGY_MONOTONE_INVARIANT,
            &dip_return,
            ENERGY_MONOTONE_TOLERANCE,
        );
        assert_eq!(
            mono.violation_count, 1,
            "monotone flags the rise back to 1.0"
        );
    }

    #[test]
    fn verify_round_trips_a_bounded_receipt() {
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(base_inputs_for(
            BOUNDED_INVARIANT,
            path,
            vec![1.0, 0.5, 1.0],
            true,
            false,
        ));
        assert_eq!(receipt.invariant.name, BOUNDED_INVARIANT);
        assert_eq!(receipt.oracle.name, BOUNDED_INVARIANT);
        assert_eq!(receipt.invariant.tolerance, BOUNDED_TOLERANCE);
        assert_eq!(receipt.receipt_status, "PASS");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 0.5, 1.0])),
        );
        assert!(
            result.is_ok(),
            "a faithful bounded receipt must verify: {result:?}"
        );
    }

    #[test]
    fn verify_fails_a_faithful_bounded_overshoot() {
        // A quantity that overshoots its initial value faithfully reproduces a
        // FAIL_UNEXPECTED, so `receipt verify` exits 3 (not 0).
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(base_inputs_for(
            BOUNDED_INVARIANT,
            path,
            vec![1.0, 2.0, 1.0],
            true,
            false,
        ));
        assert_eq!(receipt.receipt_status, "FAIL_UNEXPECTED");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 2.0, 1.0])),
        );
        assert_eq!(
            result,
            Err(3),
            "a faithful bounded overshoot must exit 3 (FAIL_UNEXPECTED)"
        );
    }

    #[test]
    fn verify_rejects_a_non_canonical_bounded_tolerance() {
        // Bounded's tolerance is pinned like every family member's: a resealed
        // loosened tolerance is rejected (FIELD_CONTRACT_VIOLATION).
        let path = Path::new("k.bld");
        let mut receipt = build_scientific_runtime_receipt(base_inputs_for(
            BOUNDED_INVARIANT,
            path,
            vec![1.0, 0.5, 1.0],
            true,
            false,
        ));
        receipt.invariant.tolerance = 1e6;
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 0.5, 1.0])),
        );
        assert_eq!(result, Err(1), "a non-canonical tolerance must be rejected");
    }

    #[test]
    fn energy_identity_holds_for_near_zero_residuals() {
        // Residuals at the roundoff scale are all within tolerance of zero.
        let obs = energy_identity_residual(&[1e-12, -5e-13, 2e-14, 0.0], ENERGY_IDENTITY_TOLERANCE);
        assert_eq!(obs.violation_count, 0);
        assert!(invariant_passes(4, &obs));
    }

    #[test]
    fn energy_identity_flags_a_large_residual() {
        // Residuals above tolerance (a broken balance) are violations, and the
        // reference is ZERO so step 0 is checked like any other.
        let obs = energy_identity_residual(&[1e-3, -1e-3, 0.0], ENERGY_IDENTITY_TOLERANCE);
        assert_eq!(obs.violation_count, 2);
        assert_eq!(obs.first_violation_step, Some(0));
        assert!(!invariant_passes(3, &obs));
    }

    #[test]
    fn energy_identity_references_zero_not_the_initial_value() {
        // The witness that energy-identity's reference is 0, not series[0]:
        // the SAME series [0.1, 0.0, 0.0] yields three different verdicts.
        // energy-identity flags step 0 (0.1 is far from 0); conservation
        // references 0.1 and flags the LATER steps (they deviate from 0.1);
        // bounded references 0.1 and PASSes (nothing exceeds it).
        let s = [0.1, 0.0, 0.0];
        let ei = evaluate_invariant(ENERGY_IDENTITY_INVARIANT, &s, ENERGY_IDENTITY_TOLERANCE);
        assert_eq!(ei.violation_count, 1);
        assert_eq!(ei.first_violation_step, Some(0));
        let cons = evaluate_invariant(CONSERVATION_INVARIANT, &s, CONSERVATION_TOLERANCE);
        assert_eq!(cons.violation_count, 2);
        assert_eq!(cons.first_violation_step, Some(1));
        let bounded = evaluate_invariant(BOUNDED_INVARIANT, &s, BOUNDED_TOLERANCE);
        assert_eq!(bounded.violation_count, 0);
    }

    #[test]
    fn verify_round_trips_an_energy_identity_receipt() {
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(base_inputs_for(
            ENERGY_IDENTITY_INVARIANT,
            path,
            vec![1e-12, -1e-13, 2e-14],
            true,
            false,
        ));
        assert_eq!(receipt.invariant.name, ENERGY_IDENTITY_INVARIANT);
        assert_eq!(receipt.oracle.name, ENERGY_IDENTITY_INVARIANT);
        assert_eq!(receipt.invariant.tolerance, ENERGY_IDENTITY_TOLERANCE);
        assert_eq!(receipt.receipt_status, "PASS");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1e-12, -1e-13, 2e-14])),
        );
        assert!(
            result.is_ok(),
            "a faithful energy-identity receipt must verify: {result:?}"
        );
    }

    #[test]
    fn verify_fails_a_faithful_energy_identity_breakage() {
        // A residual series that stays large (a dropped-term balance) faithfully
        // reproduces a FAIL_UNEXPECTED, so `receipt verify` exits 3.
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(base_inputs_for(
            ENERGY_IDENTITY_INVARIANT,
            path,
            vec![1e-3, 1e-3, 1e-3],
            true,
            false,
        ));
        assert_eq!(receipt.receipt_status, "FAIL_UNEXPECTED");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1e-3, 1e-3, 1e-3])),
        );
        assert_eq!(
            result,
            Err(3),
            "a faithful energy-identity breakage must exit 3 (FAIL_UNEXPECTED)"
        );
    }

    #[test]
    fn verify_rejects_a_non_canonical_energy_identity_tolerance() {
        // Energy-identity's tolerance is pinned like every family member's.
        let path = Path::new("k.bld");
        let mut receipt = build_scientific_runtime_receipt(base_inputs_for(
            ENERGY_IDENTITY_INVARIANT,
            path,
            vec![1e-12, -1e-13, 2e-14],
            true,
            false,
        ));
        receipt.invariant.tolerance = 1e6;
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1e-12, -1e-13, 2e-14])),
        );
        assert_eq!(result, Err(1), "a non-canonical tolerance must be rejected");
    }

    /// Inputs for a 2-column relation receipt.
    fn relation_inputs<'a>(
        path: &'a Path,
        series: Vec<f64>,
        parsed: bool,
        negative_fixture: bool,
    ) -> ScientificReceiptInputs<'a> {
        ScientificReceiptInputs {
            invariant_name: RELATION_INVARIANT.to_string(),
            column_count: 2,
            ..base_inputs(path, series, parsed, negative_fixture)
        }
    }

    #[test]
    fn relation_holds_when_every_row_agrees() {
        // 3 rows of (a, a): the columns agree, so no violations, and the
        // EFFECTIVE length is the ROW count (3), not the 6 raw tokens.
        let (obs, rows) =
            relation_columns_agree(&[1.0, 1.0, 2.0, 2.0, 3.0, 3.0], RELATION_TOLERANCE, 2);
        assert_eq!(obs.violation_count, 0);
        assert_eq!(rows, 3);
        assert!(invariant_passes(rows, &obs));
    }

    #[test]
    fn relation_flags_a_disagreeing_row() {
        // Row 1 is (2.0, 9.0): the columns disagree beyond tolerance.
        let (obs, rows) =
            relation_columns_agree(&[1.0, 1.0, 2.0, 9.0, 3.0, 3.0], RELATION_TOLERANCE, 2);
        assert_eq!(obs.violation_count, 1);
        assert_eq!(obs.first_violation_step, Some(1));
        assert_eq!(rows, 3);
        assert!(!invariant_passes(rows, &obs));
    }

    #[test]
    fn relation_reports_zero_rows_for_ragged_or_underwide_data() {
        // A flat length not divisible by column_count cannot form complete rows.
        let (_, rows) = relation_columns_agree(&[1.0, 1.0, 2.0], RELATION_TOLERANCE, 2);
        assert_eq!(rows, 0);
        // A column_count below 2 has nothing to compare across.
        let (_, rows) = relation_columns_agree(&[1.0, 2.0], RELATION_TOLERANCE, 1);
        assert_eq!(rows, 0);
    }

    #[test]
    fn evaluate_measurement_uses_row_count_for_relations_and_len_otherwise() {
        // The relation's effective length is rows (2), so a 4-token / 2-column
        // series is enough to witness (>= 2 rows). A single-scalar invariant on
        // the same 4 tokens uses the token count (4).
        let rel = evaluate_measurement(
            RELATION_INVARIANT,
            &[1.0, 1.0, 2.0, 2.0],
            RELATION_TOLERANCE,
            2,
        );
        assert_eq!(rel.effective_len, 2);
        assert_eq!(rel.observed.violation_count, 0);
        let mono = evaluate_measurement(
            ENERGY_MONOTONE_INVARIANT,
            &[4.0, 3.0, 2.0, 1.0],
            ENERGY_MONOTONE_TOLERANCE,
            1,
        );
        assert_eq!(mono.effective_len, 4);
    }

    #[test]
    fn verify_round_trips_a_relation_receipt() {
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(relation_inputs(
            path,
            vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0],
            true,
            false,
        ));
        assert_eq!(receipt.invariant.name, RELATION_INVARIANT);
        assert_eq!(receipt.oracle.name, RELATION_INVARIANT);
        assert_eq!(receipt.measurement.column_count, 2);
        assert_eq!(receipt.receipt_status, "PASS");
        assert_eq!(receipt.invariant.observed.violation_count, 0);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0])),
        );
        assert!(
            result.is_ok(),
            "a faithful relation receipt must verify: {result:?}"
        );
    }

    #[test]
    fn verify_fails_a_faithful_relation_disagreement() {
        // Row 1's columns disagree; the receipt faithfully reproduces a
        // FAIL_UNEXPECTED, so verify exits 3.
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(relation_inputs(
            path,
            vec![1.0, 1.0, 2.0, 9.0],
            true,
            false,
        ));
        assert_eq!(receipt.receipt_status, "FAIL_UNEXPECTED");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 1.0, 2.0, 9.0])),
        );
        assert_eq!(
            result,
            Err(3),
            "a faithful relation disagreement must exit 3 (FAIL_UNEXPECTED)"
        );
    }

    #[test]
    fn verify_rejects_a_non_canonical_relation_tolerance() {
        let path = Path::new("k.bld");
        let mut receipt = build_scientific_runtime_receipt(relation_inputs(
            path,
            vec![1.0, 1.0, 2.0, 2.0],
            true,
            false,
        ));
        receipt.invariant.tolerance = 1e6;
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 1.0, 2.0, 2.0])),
        );
        assert_eq!(result, Err(1), "a non-canonical tolerance must be rejected");
    }

    #[test]
    fn verify_rejects_a_receipt_missing_column_count() {
        // column_count is a REQUIRED sealed field (no serde default): a receipt
        // lacking it is MALFORMED at load, caught BEFORE any re-derivation or
        // re-run, rather than deserializing to a default and then misreporting
        // as SEAL_MISMATCH. (Regression for the C4 review finding.)
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        let mut value = serde_json::to_value(&receipt).expect("to_value");
        value["measurement"]
            .as_object_mut()
            .expect("measurement object")
            .remove("column_count")
            .expect("column_count was present");
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| panic!("a malformed receipt must be rejected before re-derivation"),
            |_, _| panic!("a malformed receipt must be rejected before the re-run"),
        );
        assert_eq!(
            result,
            Err(1),
            "a receipt missing the required column_count must be rejected as malformed"
        );
    }

    #[test]
    fn conserved_band_holds_within_the_budget_and_flags_drift() {
        // Within the 5e-3 budget: no violations.
        let obs = evaluate_invariant(
            CONSERVED_BAND_INVARIANT,
            &[1.0, 1.002, 0.998, 1.001],
            CONSERVED_BAND_TOLERANCE,
        );
        assert_eq!(obs.violation_count, 0);
        // A drift past the budget is flagged (step 2 leaves the band).
        let obs = evaluate_invariant(
            CONSERVED_BAND_INVARIANT,
            &[1.0, 1.0, 1.1, 1.2],
            CONSERVED_BAND_TOLERANCE,
        );
        assert_eq!(obs.violation_count, 2);
        assert_eq!(obs.first_violation_step, Some(2));
    }

    #[test]
    fn conserved_band_is_distinct_from_conservation_and_bounded() {
        // A quantity oscillating in a small band around its initial value, both
        // above and below: conserved-band ACCEPTS it, while conservation
        // (roundoff-tight) and bounded (one-sided, no rise allowed) both reject
        // it. This is the symplectic-energy case in miniature.
        let band = [1.0, 1.002, 0.998];
        let cb = evaluate_invariant(CONSERVED_BAND_INVARIANT, &band, CONSERVED_BAND_TOLERANCE);
        assert_eq!(cb.violation_count, 0, "conserved-band accepts the band");
        let cons = evaluate_invariant(CONSERVATION_INVARIANT, &band, CONSERVATION_TOLERANCE);
        assert!(
            cons.violation_count > 0,
            "conservation rejects the deviation"
        );
        let bounded = evaluate_invariant(BOUNDED_INVARIANT, &band, BOUNDED_TOLERANCE);
        assert!(
            bounded.violation_count > 0,
            "bounded rejects the rise above s[0]"
        );
    }

    #[test]
    fn verify_round_trips_a_conserved_band_receipt() {
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(base_inputs_for(
            CONSERVED_BAND_INVARIANT,
            path,
            vec![1.0, 1.002, 0.998],
            true,
            false,
        ));
        assert_eq!(receipt.invariant.name, CONSERVED_BAND_INVARIANT);
        assert_eq!(receipt.oracle.name, CONSERVED_BAND_INVARIANT);
        assert_eq!(receipt.invariant.tolerance, CONSERVED_BAND_TOLERANCE);
        assert_eq!(receipt.receipt_status, "PASS");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 1.002, 0.998])),
        );
        assert!(
            result.is_ok(),
            "a faithful conserved-band receipt must verify: {result:?}"
        );
    }

    #[test]
    fn verify_fails_a_faithful_conserved_band_drift() {
        // A quantity that drifts out of the budget faithfully reproduces a
        // FAIL_UNEXPECTED, so verify exits 3.
        let path = Path::new("k.bld");
        let receipt = build_scientific_runtime_receipt(base_inputs_for(
            CONSERVED_BAND_INVARIANT,
            path,
            vec![1.0, 1.0, 1.1],
            true,
            false,
        ));
        assert_eq!(receipt.receipt_status, "FAIL_UNEXPECTED");
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 1.0, 1.1])),
        );
        assert_eq!(
            result,
            Err(3),
            "a faithful conserved-band drift must exit 3 (FAIL_UNEXPECTED)"
        );
    }

    #[test]
    fn verify_rejects_a_non_canonical_conserved_band_tolerance() {
        let path = Path::new("k.bld");
        let mut receipt = build_scientific_runtime_receipt(base_inputs_for(
            CONSERVED_BAND_INVARIANT,
            path,
            vec![1.0, 1.002, 0.998],
            true,
            false,
        ));
        receipt.invariant.tolerance = 1e6;
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 1.002, 0.998])),
        );
        assert_eq!(result, Err(1), "a non-canonical tolerance must be rejected");
    }

    #[test]
    fn verify_rejects_a_column_count_that_violates_the_invariant_contract() {
        // Emit rejects a single-scalar invariant with column_count != 1; verify
        // RE-CHECKS the same contract, so a resealed conserved-band receipt with
        // column_count = 2 is rejected (FIELD_CONTRACT_VIOLATION) rather than
        // silently accepted. (Regression for the D1 review finding: the
        // structural column contract had no verify-side counterpart.)
        let path = Path::new("k.bld");
        let mut receipt = build_scientific_runtime_receipt(base_inputs_for(
            CONSERVED_BAND_INVARIANT,
            path,
            vec![1.0, 1.002, 0.998],
            true,
            false,
        ));
        assert_eq!(receipt.measurement.column_count, 1);
        receipt.measurement.column_count = 2;
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let sd = receipt.source_digest.clone();
        let gd = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 1.002, 0.998])),
        );
        assert_eq!(
            result,
            Err(1),
            "a single-scalar invariant with column_count != 1 must be rejected"
        );

        // The mirror case: a relation receipt resealed to column_count 1 (below
        // the >= 2 contract) is also rejected.
        let mut relation = build_scientific_runtime_receipt(relation_inputs(
            path,
            vec![1.0, 1.0, 2.0, 2.0],
            true,
            false,
        ));
        relation.measurement.column_count = 1;
        seal_receipt(&mut relation);
        let value = serde_json::to_value(&relation).expect("to_value");
        let sd = relation.source_digest.clone();
        let gd = relation.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &relation.compiler_version,
            &relation.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(sd.clone(), gd.clone())),
            |_, _| Ok(rerun(vec![1.0, 1.0, 2.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "a relation invariant with column_count < 2 must be rejected"
        );
    }

    /// Build a faithful re-run observation (finite series, exit 0, raw and
    /// executable digests matching the test receipt) for verify callbacks.
    fn rerun(series: Vec<f64>) -> RerunObservation {
        RerunObservation {
            parsed: ParsedSeries {
                any_parsed: !series.is_empty(),
                diverged: false,
                series,
            },
            exit_code: 0,
            raw_stdout_digest: hex_digest('c'),
            executable_digest: hex_digest('f'),
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
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
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
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(wrong.clone(), graph_digest.clone())),
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
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
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
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
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
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
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
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
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
        // count / violation_count checks must be skipped when both runs
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
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| {
                let mut observation = rerun(vec![4.0, 3.0, 2.5]);
                // Three finite values instead of two: the divergence step
                // shifted by one on the re-run platform.
                observation.parsed.diverged = true;
                Ok(observation)
            },
        );
        assert_eq!(
            result,
            Err(3),
            "a faithful diverged receipt must exit 3 even when the platform-dependent prefix length shifts"
        );
    }

    #[test]
    fn verify_rejects_a_missing_claims_boundary() {
        // A receipt whose not_claimed omits "physical_law" is overclaiming by
        // omission and must be rejected before any expensive work.
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.not_claimed = vec!["convergence".to_string()];
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "a receipt without the physical_law boundary must be rejected"
        );
    }

    #[test]
    fn verify_rejects_an_extraction_policy_mismatch() {
        // A receipt extracted under a different policy cannot be faithfully
        // re-checked by this build's parser.
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.measurement.series_extraction_policy = "whitespace-f64/v99".to_string();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "an unknown extraction policy must be rejected"
        );
    }

    #[test]
    fn verify_rejects_an_unbound_oracle() {
        // The declared oracle must bind to the invariant actually checked; an
        // unknown oracle kind cannot be re-checked by this verifier at all.
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.oracle.name = "some_other_criterion".to_string();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(result, Err(1), "an unbound oracle must be rejected");

        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.oracle.kind = "reference_implementation".to_string();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "an oracle kind this verifier cannot re-check must be rejected"
        );
    }

    #[test]
    fn empty_digests_never_match() {
        // Two absent hashes must not report as the strongest reproduction
        // signal: a vacuous reproduced=true would fabricate provenance.
        let empty = ScientificDigest {
            algorithm: "sha256".to_string(),
            hex: String::new(),
        };
        assert!(!digests_match(&empty, &empty));
        assert!(digests_match(&hex_digest('a'), &hex_digest('a')));
    }

    #[test]
    fn verify_rejects_a_malformed_digest() {
        // A sealed digest that is not a real sha256 (here: empty hex) is
        // rejected outright, so "hash unavailable" cannot masquerade as a
        // witnessed hash inside a sealed receipt.
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.build_state.toolchain.program_executable_digest.hex = String::new();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(result, Err(1), "an empty sealed digest must be rejected");
    }

    #[test]
    fn verify_rejects_a_self_consistent_but_unimplemented_oracle() {
        // The oracle binding is pinned to the IMPLEMENTATION: a receipt whose
        // oracle.name and invariant.name agree with EACH OTHER but name a
        // criterion this verifier does not implement must be rejected
        // (comparing the two sealed fields against each other alone is
        // self-referential and re-sealable to any equal pair).
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.oracle.name = "custom_criterion".to_string();
        receipt.invariant.name = "custom_criterion".to_string();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "a self-consistent but unimplemented oracle/invariant pair must be rejected"
        );

        // An oracle claiming an EXECUTED status on a declared kind is also
        // rejected: a declared criterion cannot claim execution provenance.
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.oracle.status = "EXECUTED".to_string();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "an executed status on a declared oracle must be rejected"
        );
    }

    #[test]
    fn verify_rejects_an_edited_fence() {
        // A fence edited (and resealed) to claim availability v0 does not
        // produce must be rejected, or the fence would be decorative.
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.telemetry_branch.status = "AVAILABLE".to_string();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "a fence claiming availability must be rejected"
        );
    }

    #[test]
    fn extraction_policy_matches_by_version_tag_not_prose() {
        // A prose re-wording of the SAME versioned policy must verify (the
        // tag is the contract; the description is display text)...
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.measurement.series_extraction_policy =
            "whitespace-f64/v1: same discipline, different wording".to_string();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert!(
            result.is_ok(),
            "same policy tag with different prose must verify: {result:?}"
        );
    }

    // --- Crucible export mapping (the Telos bridge) ---------------------------

    fn report_fixture(
        receipt_status: &'static str,
        violation_count: usize,
    ) -> ScientificVerifyReport {
        ScientificVerifyReport {
            invariant_status: if violation_count == 0 { "PASS" } else { "FAIL" },
            invariant_name: ENERGY_MONOTONE_INVARIANT.to_string(),
            violation_count,
            receipt_status,
            invariant_held: matches!(receipt_status, "PASS" | "FAIL_EXPECTED"),
            toolchain_matched: true,
            raw_stdout_reproduced: true,
            executable_reproduced: false,
            tolerance: ENERGY_MONOTONE_TOLERANCE,
            negative_fixture: receipt_status == "FAIL_EXPECTED",
            diverged: false,
            source: "k.bld".to_string(),
            source_digest_hex: "a".repeat(64),
            raw_stdout_digest_hex: "c".repeat(64),
            args: vec!["--mode".to_string()],
            seal_hex: "e".repeat(64),
        }
    }

    #[test]
    fn export_derives_deviation_from_the_rerun_and_seals_the_replay() {
        // A PASS report exports deviation 0.0 against tolerance 0.5 (MATCH in
        // Crucible's margin math), with a recheck descriptor carrying the
        // full replay command: witnessed, never asserted.
        let row = crucible_measurement_from_report(
            &report_fixture("PASS", 0),
            "claim-1",
            &"b".repeat(64),
            false,
            "r.json",
            &"f".repeat(64),
            1000.0,
        );
        assert_eq!(row["deviation"], 0.0);
        assert_eq!(row["tolerance"], 0.5);
        assert_eq!(row["method"], CRUCIBLE_EXPORT_METHOD);
        assert_eq!(row["recheck"]["oracle"], "buildc.receipt.verify");
        assert_eq!(row["recheck"]["command"][0], "buildc");
        assert_eq!(row["recheck"]["expected"]["violation_count"], 0);
        assert!(row["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "receipt_status:PASS"));
    }

    #[test]
    fn export_labels_the_invariant_the_receipt_actually_checked() {
        // The witnessed evidence must name the sealed invariant, not a
        // hardcoded default: a conservation receipt exported through the bridge
        // records `invariant:conserved_quantity_constant`, never the monotone
        // name. (Regression for the C1 review finding: the export string was
        // hardcoded to ENERGY_MONOTONE_INVARIANT and mislabeled every
        // non-monotone receipt.)
        let mut conservation = report_fixture("PASS", 0);
        conservation.invariant_name = CONSERVATION_INVARIANT.to_string();
        let row = crucible_measurement_from_report(
            &conservation,
            "claim-c",
            &"b".repeat(64),
            false,
            "r.json",
            &"f".repeat(64),
            1000.0,
        );
        let evidence = row["evidence"].as_array().unwrap();
        assert!(
            evidence
                .iter()
                .any(|e| e == &format!("invariant:{}", CONSERVATION_INVARIANT)),
            "conservation export must name conserved_quantity_constant: {evidence:?}"
        );
        assert!(
            !evidence
                .iter()
                .any(|e| e == &format!("invariant:{}", ENERGY_MONOTONE_INVARIANT)),
            "conservation export must NOT carry the monotone name: {evidence:?}"
        );

        // The monotone default still labels itself correctly.
        let mono = crucible_measurement_from_report(
            &report_fixture("PASS", 0),
            "claim-m",
            &"b".repeat(64),
            false,
            "r.json",
            &"f".repeat(64),
            1000.0,
        );
        assert!(mono["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == &format!("invariant:{}", ENERGY_MONOTONE_INVARIANT)));
    }

    #[test]
    fn export_maps_unverifiable_to_null_deviation() {
        // An UNVERIFIABLE receipt exports deviation null: Crucible reads an
        // unmeasurable deviation as UNVERIFIABLE, fail-closed. It must NOT
        // export 0.0 (that would read as a witnessed MATCH).
        let row = crucible_measurement_from_report(
            &report_fixture("UNVERIFIABLE", 0),
            "claim-1",
            &"b".repeat(64),
            false,
            "r.json",
            &"f".repeat(64),
            1000.0,
        );
        assert!(row["deviation"].is_null());
    }

    #[test]
    fn export_reports_a_failing_receipt_honestly() {
        // FAIL_UNEXPECTED and FAIL_EXPECTED both export the REAL recomputed
        // increase count (which reads DRIFT against a holds-everywhere claim);
        // the receipt_status in evidence lets the thesis side frame an
        // expected failure. The exporter never launders a failure into 0.0.
        for status in ["FAIL_UNEXPECTED", "FAIL_EXPECTED"] {
            let row = crucible_measurement_from_report(
                &report_fixture(status, 199),
                "claim-1",
                &"b".repeat(64),
                false,
                "r.json",
                &"f".repeat(64),
                1000.0,
            );
            assert_eq!(row["deviation"], 199.0, "status={status}");
            assert!(row["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e.as_str().unwrap() == format!("receipt_status:{status}")));
        }
    }

    #[test]
    fn export_expected_failure_binding_is_claim_relative() {
        // With --claim-expects-failure, deviation measures the claim's
        // PREDICTION: a fixture failing as predicted deviates 0 (MATCH in
        // Crucible's margin math); a fixture that unexpectedly PASSes
        // deviates 1 (DRIFT: the prediction was violated). The binding is
        // recorded in evidence.
        let row = crucible_measurement_from_report(
            &report_fixture("FAIL_EXPECTED", 199),
            "claim-1",
            &"b".repeat(64),
            true,
            "r.json",
            &"f".repeat(64),
            1000.0,
        );
        assert_eq!(row["deviation"], 0.0);
        assert!(row["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "claim_expectation:expects_failure"));

        let mut passed = report_fixture("PASS", 0);
        passed.negative_fixture = true;
        let row = crucible_measurement_from_report(
            &passed,
            "claim-1",
            &"b".repeat(64),
            true,
            "r.json",
            &"f".repeat(64),
            1000.0,
        );
        assert_eq!(
            row["deviation"], 1.0,
            "a fixture that unexpectedly passes violates the failure-predicting claim"
        );
    }

    #[test]
    fn export_never_seals_a_platform_dependent_expectation_for_diverged_receipts() {
        // A diverged receipt's increase count is prefix-derived and
        // platform-dependent (the verifier's own both_diverged rule skips
        // comparing it); the recheck expectation must be null so an
        // independent replayer matches on receipt_status, not on a number
        // that legitimately differs across toolchains.
        let mut report = report_fixture("UNVERIFIABLE", 1957);
        report.diverged = true;
        let row = crucible_measurement_from_report(
            &report,
            "claim-1",
            &"b".repeat(64),
            false,
            "r.json",
            &"f".repeat(64),
            1000.0,
        );
        assert!(row["recheck"]["expected"]["violation_count"].is_null());
        assert_eq!(row["recheck"]["diverged"], true);
        assert!(row["deviation"].is_null());
        assert_eq!(row["recheck"]["expected"]["exit_code"], 3);
    }

    #[test]
    fn export_carries_the_reproduction_flags_outside_evidence() {
        // The witnessing re-run's reproduction signals are visible to
        // auditors as a top-level object, deliberately NOT inside evidence
        // (Crucible's recheck compares evidence for stability, and these
        // flags legitimately differ per replay environment).
        let row = crucible_measurement_from_report(
            &report_fixture("PASS", 0),
            "claim-1",
            &"b".repeat(64),
            false,
            "r.json",
            &"f".repeat(64),
            1000.0,
        );
        assert_eq!(row["reproduction"]["toolchain_matched"], true);
        assert_eq!(row["reproduction"]["raw_stdout_reproduced"], true);
        assert_eq!(row["reproduction"]["executable_reproduced"], false);
        assert_eq!(row["recheck"]["expected"]["exit_code"], 0);
        // The sealed-time digest is labeled as such.
        assert!(row["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str().unwrap().starts_with("sealed_raw_stdout:sha256:")));
    }

    #[test]
    fn witnessed_fields_derive_from_capabilities() {
        // Console writing stdout only (reads_stdin=false): no external dataset
        // POSSIBLE, deterministic modulo args - the effect system proving
        // absences. This is the pure-println flagship kernel; it MUST keep its
        // true absence claims.
        let (dataset, seed, determinism) =
            witnessed_fields_from_capabilities(&["Console".to_string()], false);
        assert_eq!(dataset.status, "NONE_WITNESSED");
        assert_eq!(seed.status, "NOT_APPLICABLE");
        assert!(determinism.deterministic_modulo_args);

        // Console READING stdin: stdin is an external input, so both absences
        // collapse (the Console NAME alone would have missed this).
        let (dataset, _, determinism) =
            witnessed_fields_from_capabilities(&["Console".to_string()], true);
        assert_eq!(dataset.status, "POSSIBLE_UNWITNESSED");
        assert!(dataset.grounds.contains("stdin"));
        assert!(!determinism.deterministic_modulo_args);
        assert!(determinism.grounds.contains("stdin"));

        // FileSystem present: the dataset field fences honestly, and
        // determinism cannot be claimed.
        let caps = vec!["Console".to_string(), "FileSystem".to_string()];
        let (dataset, _, determinism) = witnessed_fields_from_capabilities(&caps, false);
        assert_eq!(dataset.status, "POSSIBLE_UNWITNESSED");
        assert!(dataset.grounds.contains("FileSystem"));
        assert!(!determinism.deterministic_modulo_args);

        // Clock alone breaks determinism but not the dataset absence (the wall
        // clock is a scalar nondeterminism source, not a data channel).
        let caps = vec!["Clock".to_string(), "Console".to_string()];
        let (dataset, _, determinism) = witnessed_fields_from_capabilities(&caps, false);
        assert_eq!(dataset.status, "NONE_WITNESSED");
        assert!(!determinism.deterministic_modulo_args);
        assert!(determinism.grounds.contains("Clock"));

        // Foreign (extern C) can do arbitrary IO: hazard for BOTH claims.
        let caps = vec!["Console".to_string(), "Foreign".to_string()];
        let (dataset, _, determinism) = witnessed_fields_from_capabilities(&caps, false);
        assert_eq!(dataset.status, "POSSIBLE_UNWITNESSED");
        assert!(dataset.grounds.contains("Foreign"));
        assert!(!determinism.deterministic_modulo_args);

        // Gpu: hazard for BOTH claims.
        let caps = vec!["Console".to_string(), "Gpu".to_string()];
        let (dataset, _, determinism) = witnessed_fields_from_capabilities(&caps, false);
        assert_eq!(dataset.status, "POSSIBLE_UNWITNESSED");
        assert!(dataset.grounds.contains("Gpu"));
        assert!(!determinism.deterministic_modulo_args);

        // Environment (getenv / argv): hazard for BOTH claims.
        let caps = vec!["Console".to_string(), "Environment".to_string()];
        let (dataset, _, determinism) = witnessed_fields_from_capabilities(&caps, false);
        assert_eq!(dataset.status, "POSSIBLE_UNWITNESSED");
        assert!(!determinism.deterministic_modulo_args);

        // Process (exit) reads nothing and is deterministic: safe for BOTH.
        let caps = vec!["Console".to_string(), "Process".to_string()];
        let (dataset, _, determinism) = witnessed_fields_from_capabilities(&caps, false);
        assert_eq!(dataset.status, "NONE_WITNESSED");
        assert!(determinism.deterministic_modulo_args);

        // An unrecognised capability is default-unsafe (fail closed): a
        // capability added to the checker later cannot silently widen either
        // absence claim.
        let caps = vec!["Console".to_string(), "Bluetooth".to_string()];
        let (dataset, _, determinism) = witnessed_fields_from_capabilities(&caps, false);
        assert_eq!(dataset.status, "POSSIBLE_UNWITNESSED");
        assert!(dataset.grounds.contains("Bluetooth"));
        assert!(!determinism.deterministic_modulo_args);
    }

    #[test]
    fn verify_rejects_effect_policy_drift() {
        // The sealed capability union disagrees with what the checker
        // re-derives from source: EFFECT_POLICY_DRIFT, exit 1. This is the
        // "type/effect policy" field being genuinely re-derived.
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
            Some(&test_toolchain()),
            |_| {
                let mut facts = rederive_facts(src_digest.clone(), graph_digest.clone());
                // The re-derivation now observes an extra capability.
                facts
                    .effect_policy
                    .observed_capabilities
                    .push("FileSystem".to_string());
                facts.effect_policy.facts_digest = hex_digest('8');
                Ok(facts)
            },
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(result, Err(1), "effect-policy drift must fail verify");
    }

    #[test]
    fn verify_rejects_witnessed_fields_that_do_not_rederive() {
        // The sealed input_dataset claims NONE_WITNESSED but was edited (and
        // resealed) while the capabilities say otherwise... here simulated by
        // editing the sealed determinism flag: the fields must re-derive from
        // the re-derived capabilities, not merely be internally consistent.
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.determinism.deterministic_modulo_args = false;
        receipt.determinism.grounds = "edited".to_string();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "witnessed fields that do not re-derive must fail verify"
        );
    }

    #[test]
    fn verify_rejects_field_contract_violations() {
        // A seed status other than NOT_APPLICABLE claims a capability the
        // language does not have (no RNG builtin in v0).
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.seed.status = "RECORDED".to_string();
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "an inexpressible seed status must be rejected"
        );

        // A DECLARED method with no description is internally inconsistent.
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.numerical_method.status = "DECLARED".to_string();
        receipt.numerical_method.description = None;
        seal_receipt(&mut receipt);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert_eq!(
            result,
            Err(1),
            "an inconsistent numerical_method must be rejected"
        );
    }

    #[test]
    fn verify_checks_the_seal_before_interpreting_fields() {
        // The C5 ordering contract: an UNSEALED edit to a field is caught by
        // the integrity gate (SEAL_MISMATCH) rather than by whichever
        // field-level contradiction it happens to trip first. Here the seed
        // status is edited to an inexpressible value WITHOUT resealing: the
        // receipt no longer re-seals, so it is rejected as tampering, not as a
        // FIELD_CONTRACT_VIOLATION. (The validly-sealed contradiction is the
        // separate `verify_rejects_field_contract_violations` case, which
        // reseals and therefore reaches the field-contract check - proving
        // that check stays reachable after the seal gate.)
        let path = Path::new("k.bld");
        let mut receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        receipt.seed.status = "RECORDED".to_string();
        // Deliberately do NOT reseal.
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| panic!("an unsealed receipt must be rejected before any re-run"),
        );
        assert_eq!(result, Err(1), "an unsealed field edit must be rejected");
    }

    #[test]
    fn verify_fails_with_exit_4_when_no_toolchain_is_available() {
        // No C compiler is its own verdict (TOOL_UNAVAILABLE, exit 4),
        // distinct from drift (1) and faithful-fail (3), checked BEFORE any
        // re-run attempt.
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
            None,
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| panic!("the re-run must never be attempted without a toolchain"),
        );
        assert_eq!(result, Err(4), "a missing toolchain must exit 4");
    }

    #[test]
    fn verify_warns_but_proceeds_on_a_different_toolchain() {
        // Cross-toolchain re-verification is legitimate by design: a
        // different-but-present toolchain WARNs and the verdict re-check
        // proceeds to a faithful pass.
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let mut other = test_toolchain();
        other.c_compiler = "other-cc".to_string();
        other.version_output_digest = hex_digest('9');
        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&other),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            |_, _| Ok(rerun(vec![4.0, 3.0, 2.0])),
        );
        assert!(
            result.is_ok(),
            "a different toolchain must warn, not fail: {result:?}"
        );
    }

    #[test]
    fn verify_rejects_an_exit_code_drift() {
        // The sealed exit code is deterministic; a re-run exiting differently
        // (including a crash) is non-reproduction with its own class, checked
        // before any series comparison.
        let path = Path::new("k.bld");
        let receipt =
            build_scientific_runtime_receipt(base_inputs(path, vec![4.0, 3.0, 2.0], true, false));
        assert_eq!(receipt.runtime_state.exit_code, 0);
        let value = serde_json::to_value(&receipt).expect("to_value");
        let src_digest = receipt.source_digest.clone();
        let graph_digest = receipt.input_graph_digest.clone();

        let result = verify_scientific_runtime_receipt(
            &value,
            None,
            true,
            &receipt.compiler_version,
            &receipt.language_version,
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
            // Same series, but the process exited 9 instead of the sealed 0.
            |_, _| {
                let mut observation = rerun(vec![4.0, 3.0, 2.0]);
                observation.exit_code = 9;
                Ok(observation)
            },
        );
        assert_eq!(result, Err(1), "an exit-code drift must fail verify");
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
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
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
            Some(&test_toolchain()),
            |_| Ok(rederive_facts(src_digest.clone(), graph_digest.clone())),
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
