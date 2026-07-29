// ===============================================================================
// BUILDLANG MODEL BOUNDARY RECEIPT - offline verify arm for a harness-emitted
// provenance artifact over a `Model`-capability boundary crossing
// ===============================================================================
// Copyright (c) 2026 Zain Dana Harper. BuildLang Fair-Source License v1.0.
// ===============================================================================
//
//! `buildlang-model-boundary-receipt/v0`: a receipt about a boundary crossing
//! the harness-side shim (`harness/model_shim.py` in local-model) witnessed,
//! never emitted by buildc itself. This module implements ONLY the read side
//! -- schema struct, seal recompute, and field-shape contracts for `buildc
//! receipt verify` -- because emission belongs to the shim (see
//! docs/superpowers/specs/2026-07-29-model-boundary-receipts-design.md,
//! section 1: the shim is the only party that observes prompt bytes, reply
//! bytes, and its own adapter identity at once).
//!
//! This is deliberately a SEPARATE module from `scientific_runtime`, not a
//! section of it: the whole point of the artifact is that it is NOT
//! scientific evidence. A model receipt carries no invariant, no oracle, no
//! verdict -- it witnesses that a proposal crossed the boundary and what
//! bytes crossed, nothing more. The scientific verifier's
//! `CAPABILITY_INADMISSIBLE` refusal of any `Model`-observing program is
//! untouched by this module; the two artifact kinds share a seal idiom and a
//! verifier binary, never a claim vocabulary.
//!
//! Every field in the schema is tagged in the design doc as either
//! SHIM-WITNESSED (the shim observed the bytes or performed the act itself)
//! or DECLARED (someone's say-so passed through unwitnessed). The seal makes
//! tampering evident; it does not upgrade a declaration into a witness, and
//! this module's verify arm does not pretend otherwise: it checks integrity
//! and internal coherence only, never anything about the model itself (there
//! is no re-run -- the artifact witnesses a PAST crossing).

use sha2::{Digest, Sha256};

/// Schema tag for a model boundary receipt (see the module doc and design
/// section 2). Flat top-level `schema` + top-level `seal`, mirroring the
/// scientific receipt's shape so the existing chain pointers `/schema` and
/// `/seal/hex` read it unchanged (design section 6).
pub const MODEL_RECEIPT_SCHEMA: &str = "buildlang-model-boundary-receipt/v0";

/// `{ algorithm, hex }`, the receipt's own integrity seal. A distinct type
/// from `scientific_runtime::ScientificDigest` on purpose: this module does
/// not depend on the scientific module, by design (see the module doc).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelReceiptSeal {
    pub algorithm: String,
    pub hex: String,
}

/// `shim`: SHIM-WITNESSED self-identity (design section 2 row 3).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelReceiptShim {
    pub name: String,
    pub version: String,
    /// `"echo"` | `"ollama"`.
    pub mode: String,
}

/// `session`: timestamps are SHIM-CLOCK-DECLARED (ordering witnessed, wall
/// accuracy is the host's); `reply_written_utc` is `null` for a receipt whose
/// outcome never reached a reply (design section 2 row 4).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelReceiptSession {
    pub listen: String,
    pub nonce: String,
    pub request_received_utc: String,
    pub reply_written_utc: Option<String>,
}

/// The `prompt` / `reply` blocks: SHIM-WITNESSED `{ sha256, bytes }` over raw
/// bytes, never plaintext (design section 2 rows 5-6, section 3). `bytes` is
/// a byte COUNT, not content: the receipt is shareable, and whoever holds the
/// plaintext can re-hash it to check `sha256`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelReceiptHashedBytes {
    pub sha256: String,
    pub bytes: u64,
}

/// `model.daemon_digest`: DAEMON-DECLARED even when `FETCHED` -- the shim
/// witnesses the fetch, not the weights (design section 3). `hex` is present
/// IFF `status == "FETCHED"`; present alongside `"UNAVAILABLE"` is a
/// `FIELD_CONTRACT_VIOLATION` (design section 5).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelReceiptDaemonDigest {
    /// `"FETCHED"` | `"UNAVAILABLE"`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
}

/// `model`: echo mode carries only `name`; ollama mode additionally carries
/// `endpoint`, `request_body_sha256` (SHIM-WITNESSED: the exact JSON POSTed),
/// and `daemon_digest` (design section 2 row 7). The three ollama-only fields
/// are OMITTED (not null) on an echo receipt, which is why they are
/// `skip_serializing_if`, unlike `prompt`/`reply` which are explicit `null`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelReceiptModel {
    /// DECLARED: a string is not a digest.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_digest: Option<ModelReceiptDaemonDigest>,
}

/// `seed`: SHIM-WITNESSED as to what was SENT, never a claim the daemon
/// honored it (design section 2 row 8). v1's shim sends no `options.seed`,
/// so every v1 receipt carries `{ status: "NOT_SENT" }`; the `SENT` variant
/// is schema headroom for a future `--seed` flag, not exercised by v1.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelReceiptSeed {
    /// `"NOT_SENT"` | `"SENT"`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
}

/// The model boundary receipt itself. Field order is FIXED and matches the
/// design's schema table exactly: it is the canonical (sealed) order, and
/// `serde_json::to_vec` preserves struct field order, which is the Rust half
/// of the cross-language canonicalization contract (design section 2,
/// "Seal and the cross-language canonicalization contract"). Do not reorder
/// these fields without re-deriving the golden fixture's seal.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelBoundaryReceipt {
    pub schema: String,
    /// `model:<mode>:<name>`, e.g. `model:echo:echo/v1`. DECLARED label; lets
    /// `ReceiptChainLink.source` carry a human-readable member label with
    /// zero chain-code change (design section 2 row 2, section 6).
    pub source: String,
    pub shim: ModelReceiptShim,
    pub session: ModelReceiptSession,
    /// `null` IFF `outcome == "PROTOCOL_VIOLATION"` (the line never
    /// terminated, so there is nothing to hash).
    pub prompt: Option<ModelReceiptHashedBytes>,
    /// `null` unless `outcome == "COMPLETED"`.
    pub reply: Option<ModelReceiptHashedBytes>,
    pub model: ModelReceiptModel,
    pub seed: ModelReceiptSeed,
    /// `"COMPLETED"` | `"FAILED_CLOSED"` | `"PROTOCOL_VIOLATION"`.
    pub outcome: String,
    pub seal: ModelReceiptSeal,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

/// Seal a receipt in place: same idiom as
/// `scientific_runtime::seal_receipt` (sha256 over the canonical JSON with
/// `seal.hex` blanked and `seal.algorithm` fixed to `"sha256"`). Used by
/// tests to build valid fixtures; buildc itself never emits this artifact
/// (emission is the shim's job -- see the module doc), so this is dead code
/// in a non-test build, same as `gpu_receipt::emit_gpu_receipt` outside the
/// `gpu` feature.
#[allow(dead_code)]
pub fn seal_model_receipt(receipt: &mut ModelBoundaryReceipt) {
    receipt.seal.algorithm = "sha256".to_string();
    receipt.seal.hex.clear();
    let canonical = serde_json::to_vec(receipt).expect("serialize model boundary receipt");
    receipt.seal.hex = sha256_hex(&canonical);
}

/// Re-derive the seal from a receipt read back from disk and compare against
/// the stored `seal.hex`. This is the Rust half of the cross-language
/// canonicalization contract: the Python emitter must produce byte-identical
/// canonical bytes for the same logical receipt, pinned by the golden
/// fixture test in both repos.
pub fn recompute_seal_hex(receipt: &ModelBoundaryReceipt) -> String {
    let mut probe = receipt.clone();
    probe.seal.algorithm = "sha256".to_string();
    probe.seal.hex.clear();
    let canonical = serde_json::to_vec(&probe).expect("serialize model boundary receipt");
    sha256_hex(&canonical)
}

/// A sealed digest field must be a real sha256: exactly 64 hex chars. Same
/// rule as the scientific verifier's `digest_is_well_formed`: an absent or
/// malformed hash cannot masquerade as witnessed provenance
/// (`DIGEST_MALFORMED`).
fn digest_hex_is_well_formed(hex: &str) -> bool {
    hex.len() == 64 && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Report a stable machine-readable `failure_class` for a model-receipt
/// verify failure and return the exit code to propagate. Same shape as
/// `scientific_runtime::verify_failure_class`: a `failure_class: <CODE>` line
/// on stderr always, plus a JSON failure report on stdout in `--json` mode.
/// Deliberately reuses the SHARED failure taxonomy (no new classes for v1,
/// design section 5): a reader of any buildc refusal already knows these
/// words.
fn model_failure_class(json: bool, failure_class: &str, exit_code: i32) -> i32 {
    eprintln!("failure_class: {failure_class}");
    if json {
        let report = serde_json::json!({
            "status": "failed",
            "failure_class": failure_class,
        });
        if let Ok(text) = serde_json::to_string_pretty(&report) {
            println!("{text}");
        }
    }
    exit_code
}

/// Verify a model boundary receipt: offline only, no re-run (there is
/// nothing to re-run -- the artifact witnesses a past crossing). Checks, in
/// order (design section 5):
///
/// 1. The document deserializes into the typed schema (`MALFORMED`
///    otherwise; schema-tag mismatch is caught by the caller's dispatch
///    before this function is reached).
/// 2. Seal integrity (`SEAL_MISMATCH`), BEFORE any sealed field is
///    interpreted -- same ordering discipline as the scientific verifier, so
///    every field-level rejection below is known to concern a genuinely
///    author-sealed value.
/// 3. Digest well-formedness (`DIGEST_MALFORMED`): `prompt.sha256`,
///    `reply.sha256` (when present), `model.request_body_sha256` (when
///    present), `model.daemon_digest.hex` (when present) must each be 64 hex
///    chars.
/// 4. Status coherence (`FIELD_CONTRACT_VIOLATION`), exactly the three cases
///    the design names: `daemon_digest.hex` present alongside status
///    `UNAVAILABLE`; a `COMPLETED` outcome with a `null` reply; a
///    `PROTOCOL_VIOLATION` outcome with a present (non-null) prompt.
///
/// Deliberately NO new failure classes (design section 5): the shared
/// taxonomy with the scientific verifier is a feature.
pub fn verify_model_boundary_receipt(
    receipt_json: &serde_json::Value,
    json: bool,
) -> Result<(), i32> {
    let receipt: ModelBoundaryReceipt =
        serde_json::from_value(receipt_json.clone()).map_err(|err| {
            eprintln!("Error: model boundary receipt is malformed: {err}");
            model_failure_class(json, "MALFORMED", 1)
        })?;

    // Integrity gate FIRST, before any sealed field is interpreted (mirrors
    // the scientific verifier's ordering contract).
    let recomputed_seal = recompute_seal_hex(&receipt);
    if !recomputed_seal.eq_ignore_ascii_case(&receipt.seal.hex) {
        eprintln!(
            "Error: seal mismatch: receipt sha256:{}, recomputed sha256:{}",
            receipt.seal.hex, recomputed_seal
        );
        return Err(model_failure_class(json, "SEAL_MISMATCH", 1));
    }

    // Digest well-formedness.
    if let Some(prompt) = &receipt.prompt {
        if !digest_hex_is_well_formed(&prompt.sha256) {
            eprintln!("Error: malformed digest in `prompt.sha256`");
            return Err(model_failure_class(json, "DIGEST_MALFORMED", 1));
        }
    }
    if let Some(reply) = &receipt.reply {
        if !digest_hex_is_well_formed(&reply.sha256) {
            eprintln!("Error: malformed digest in `reply.sha256`");
            return Err(model_failure_class(json, "DIGEST_MALFORMED", 1));
        }
    }
    if let Some(request_body_sha256) = &receipt.model.request_body_sha256 {
        if !digest_hex_is_well_formed(request_body_sha256) {
            eprintln!("Error: malformed digest in `model.request_body_sha256`");
            return Err(model_failure_class(json, "DIGEST_MALFORMED", 1));
        }
    }
    if let Some(daemon_digest) = &receipt.model.daemon_digest {
        if let Some(hex) = &daemon_digest.hex {
            if !digest_hex_is_well_formed(hex) {
                eprintln!("Error: malformed digest in `model.daemon_digest.hex`");
                return Err(model_failure_class(json, "DIGEST_MALFORMED", 1));
            }
        }
    }

    // Status coherence: exactly the three cases the design names.
    if let Some(daemon_digest) = &receipt.model.daemon_digest {
        if daemon_digest.status == "UNAVAILABLE" && daemon_digest.hex.is_some() {
            eprintln!("Error: `model.daemon_digest.hex` is present alongside status `UNAVAILABLE`");
            return Err(model_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
        }
    }
    if receipt.outcome == "COMPLETED" && receipt.reply.is_none() {
        eprintln!("Error: outcome `COMPLETED` carries a null `reply`");
        return Err(model_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
    }
    if receipt.outcome == "PROTOCOL_VIOLATION" && receipt.prompt.is_some() {
        eprintln!("Error: outcome `PROTOCOL_VIOLATION` carries a present `prompt`");
        return Err(model_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
    }

    if json {
        let out = serde_json::json!({
            "schema": MODEL_RECEIPT_SCHEMA,
            "status": "verified",
            "source": receipt.source,
            "outcome": receipt.outcome,
            "seal": { "algorithm": "sha256", "hex": receipt.seal.hex },
        });
        let text = serde_json::to_string_pretty(&out).map_err(|err| {
            eprintln!("Error serializing model receipt verification report: {err}");
            1
        })?;
        println!("{text}");
    } else {
        println!(
            "model receipt: VERIFIED (seal intact, field contracts hold; source={}, outcome={}). \
             This artifact witnesses a past boundary crossing only: no re-run, no claim about \
             model quality, weights, or determinism.",
            receipt.source, receipt.outcome
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a valid, sealed echo-mode COMPLETED receipt matching the golden
    /// fixture's logical content exactly (see
    /// compiler/tests/fixtures/model-receipt-golden.json). Kept in sync by
    /// hand; the golden-fixture test below is the actual cross-repo pin.
    fn sample_receipt() -> ModelBoundaryReceipt {
        let mut r = ModelBoundaryReceipt {
            schema: MODEL_RECEIPT_SCHEMA.to_string(),
            source: "model:echo:echo/v1".to_string(),
            shim: ModelReceiptShim {
                name: "model_shim.py".to_string(),
                version: "0.1.0".to_string(),
                mode: "echo".to_string(),
            },
            session: ModelReceiptSession {
                listen: "127.0.0.1:8931".to_string(),
                nonce: "a1b2c3d4".to_string(),
                request_received_utc: "2026-07-29T00:00:00Z".to_string(),
                reply_written_utc: Some("2026-07-29T00:00:00Z".to_string()),
            },
            prompt: Some(ModelReceiptHashedBytes {
                sha256: "758d61f26a44448384e5c4468a0dcb7a2abe456067b0f7b505bc28b9411fe931"
                    .to_string(),
                bytes: 4,
            }),
            reply: Some(ModelReceiptHashedBytes {
                sha256: "de2406a7ccdb9add6361bdf86cfd31dfaa95806f8d42f91102290ae3abe5afae"
                    .to_string(),
                bytes: 10,
            }),
            model: ModelReceiptModel {
                name: "echo/v1".to_string(),
                endpoint: None,
                request_body_sha256: None,
                daemon_digest: None,
            },
            seed: ModelReceiptSeed {
                status: "NOT_SENT".to_string(),
                value: None,
            },
            outcome: "COMPLETED".to_string(),
            seal: ModelReceiptSeal {
                algorithm: "sha256".to_string(),
                hex: String::new(),
            },
        };
        seal_model_receipt(&mut r);
        r
    }

    #[test]
    fn seal_round_trips_through_serialize_deserialize() {
        let r = sample_receipt();
        let value = serde_json::to_value(&r).expect("to_value");
        let reloaded: ModelBoundaryReceipt = serde_json::from_value(value).expect("from_value");
        assert_eq!(recompute_seal_hex(&reloaded), reloaded.seal.hex);
    }

    #[test]
    fn a_valid_receipt_verifies() {
        let r = sample_receipt();
        let value = serde_json::to_value(&r).expect("to_value");
        assert!(verify_model_boundary_receipt(&value, false).is_ok());
    }

    #[test]
    fn seal_mismatch_is_rejected_before_any_field_contract_is_interpreted() {
        // An unsealed edit to a witnessed field must report SEAL_MISMATCH,
        // not whichever field-contract check it happens to trip. Flip the
        // reply hash's first hex char without resealing.
        let mut r = sample_receipt();
        r.reply.as_mut().unwrap().sha256 = "0".repeat(64);
        let value = serde_json::to_value(&r).expect("to_value");
        let err = verify_model_boundary_receipt(&value, false).unwrap_err();
        assert_eq!(err, 1);
    }

    #[test]
    fn seal_mismatch_reports_the_expected_failure_class() {
        let mut r = sample_receipt();
        r.session.nonce = "ffffffff".to_string(); // unsealed edit
        let value = serde_json::to_value(&r).expect("to_value");
        assert!(verify_model_boundary_receipt(&value, true).is_err());
        // The JSON report and the stderr line both carry SEAL_MISMATCH; the
        // stderr line is asserted at the CLI-integration layer (cli.rs),
        // this unit test only pins the Result is Err.
    }

    #[test]
    fn resealed_daemon_digest_hex_with_unavailable_status_is_field_contract_violation() {
        let mut r = sample_receipt();
        r.shim.mode = "ollama".to_string();
        r.model = ModelReceiptModel {
            name: "llama3.2".to_string(),
            endpoint: Some("http://127.0.0.1:11434".to_string()),
            request_body_sha256: Some("1".repeat(64)),
            daemon_digest: Some(ModelReceiptDaemonDigest {
                status: "UNAVAILABLE".to_string(),
                hex: Some("2".repeat(64)),
            }),
        };
        seal_model_receipt(&mut r); // reseal so this reaches the contract gate
        let value = serde_json::to_value(&r).expect("to_value");
        let err = verify_model_boundary_receipt(&value, false).unwrap_err();
        assert_eq!(err, 1);
    }

    #[test]
    fn resealed_completed_outcome_with_null_reply_is_field_contract_violation() {
        let mut r = sample_receipt();
        r.reply = None;
        seal_model_receipt(&mut r);
        let value = serde_json::to_value(&r).expect("to_value");
        assert!(verify_model_boundary_receipt(&value, false).is_err());
    }

    #[test]
    fn resealed_protocol_violation_with_present_prompt_is_field_contract_violation() {
        let mut r = sample_receipt();
        r.outcome = "PROTOCOL_VIOLATION".to_string();
        r.reply = None; // a PROTOCOL_VIOLATION receipt never has a reply either
        seal_model_receipt(&mut r);
        let value = serde_json::to_value(&r).expect("to_value");
        assert!(verify_model_boundary_receipt(&value, false).is_err());
    }

    #[test]
    fn malformed_digest_is_rejected() {
        let mut r = sample_receipt();
        r.prompt.as_mut().unwrap().sha256 = "not-a-real-digest".to_string();
        seal_model_receipt(&mut r);
        let value = serde_json::to_value(&r).expect("to_value");
        assert!(verify_model_boundary_receipt(&value, false).is_err());
    }

    #[test]
    fn a_required_field_removed_is_malformed() {
        let r = sample_receipt();
        let mut value = serde_json::to_value(&r).expect("to_value");
        value.as_object_mut().unwrap().remove("shim");
        assert!(verify_model_boundary_receipt(&value, false).is_err());
    }

    /// The cross-language canonicalization contract (design section 2): the
    /// golden fixture is checked into BOTH repos with the SAME seal. This
    /// test is the buildlang half of that pin -- if it fails, the Python
    /// sealer and this Rust sealer have diverged and the contract is broken.
    #[test]
    fn golden_fixture_reseals_to_its_pinned_seal() {
        let text = include_str!("../tests/fixtures/model-receipt-golden.json");
        let value: serde_json::Value = serde_json::from_str(text).expect("parse golden fixture");
        let receipt: ModelBoundaryReceipt =
            serde_json::from_value(value.clone()).expect("golden fixture matches schema");
        let recomputed = recompute_seal_hex(&receipt);
        assert_eq!(
            recomputed, receipt.seal.hex,
            "golden fixture seal must reseal identically (cross-language pin)"
        );
        assert_eq!(
            receipt.seal.hex, "6bb2a09c47f5eaa2e3208a5eadcd6d57d1faffa74a567e024e920571c3794035",
            "golden fixture's PINNED seal changed -- this breaks the cross-repo contract"
        );
        assert!(verify_model_boundary_receipt(&value, false).is_ok());
    }
}
