// ===============================================================================
// BUILDLANG TOOL-CALL RECEIPT - offline verify arm for a Flywheel-harness-emitted
// provenance artifact over an agent tool-call boundary
// ===============================================================================
// Copyright (c) 2026 Zain Dana Harper. BuildLang Fair-Source License v1.0.
// ===============================================================================
//
//! `flywheel.tool-call-receipt/v1`: a receipt about a tool invocation the
//! Flywheel agent-loop executor (`harness/local_tools.py` ToolExecutor.execute)
//! witnessed, never emitted by buildc itself. This module implements ONLY the
//! read side — schema struct, seal recompute, and field-shape contracts for
//! `buildc receipt verify` — because emission belongs to the harness (see
//! `harness/tool_call_receipt.py` in local-model).
//!
//! This mirrors `model_receipt.rs` in structure and discipline: the same seal
//! idiom (sha256 over canonical JSON with seal.hex blanked), the same shared
//! failure taxonomy (MALFORMED, SEAL_MISMATCH, DIGEST_MALFORMED,
//! FIELD_CONTRACT_VIOLATION), and the same verify ordering (seal FIRST, before
//! any sealed field is interpreted).
//!
//! Each receipt answers "what was the system allowed to do?" (capability /
//! admission), "what did it actually do?" (witnessed args/output digests),
//! and "can a stranger re-walk it?" (sealed, chain-linked, offline-verifiable).
//! This is the enforced AgentRiskBOM primitive.

use sha2::{Digest, Sha256};

/// Schema tag for a tool-call receipt.
pub const TOOL_RECEIPT_SCHEMA: &str = "flywheel.tool-call-receipt/v1";

/// `{ algorithm, hex }`, the receipt's own integrity seal. Same shape as
/// ModelReceiptSeal — a distinct type to keep this module independent.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolReceiptSeal {
    pub algorithm: String,
    pub hex: String,
}

/// Witnessed hashed bytes: `{ sha256, bytes }` where `bytes` is a byte COUNT,
/// never content. Same discipline as ModelReceiptHashedBytes.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolReceiptHashedBytes {
    pub sha256: String,
    pub bytes: u64,
}

/// The full tool-call receipt. Field order is FIXED and is the canonical sealed
/// order — serde_json::to_vec preserves struct-declaration order, which must
/// match the Python dict insertion order in tool_call_receipt.py exactly.
///
/// Every field is tagged: EXECUTOR-WITNESSED (the executor observed the bytes
/// or performed the act) or DECLARED (a label passed through). The seal makes
/// tampering evident; it does not upgrade a declaration into a witness.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolCallReceipt {
    pub schema: String,
    /// `tool:<run_id>:<seq>`. DECLARED label for chain source extraction.
    pub source: String,
    /// The tool name (e.g. "read_file", "gather.arxiv"). DECLARED.
    pub tool: String,
    /// Capability class: builtin-read, builtin-write, builtin-exec, external-mcp. DECLARED.
    pub capability: String,
    /// Admission decision: ALLOWED, BLOCKED, ESCALATED. EXECUTOR-WITNESSED.
    pub admission: String,
    /// Witnessed args digest. EXECUTOR-WITNESSED (sha256 + byte count, never raw args).
    pub args: ToolReceiptHashedBytes,
    /// Witnessed output digest. EXECUTOR-WITNESSED.
    pub output: ToolReceiptHashedBytes,
    /// `"true"` | `"false"` (string, not bool — no floats in the schema).
    pub ok: String,
    /// Return code. EXECUTOR-WITNESSED.
    pub rc: i64,
    /// Run identifier. DECLARED.
    pub run_id: String,
    /// Sequence number within the run. DECLARED.
    pub seq: i64,
    /// Chain link: sha256 of the prior receipt's canonical sealed bytes, or "" for the first.
    pub prev_receipt_sha256: String,
    /// `"COMPLETED"` | `"BLOCKED"` | `"ERROR"`.
    pub outcome: String,
    pub seal: ToolReceiptSeal,
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

/// Re-derive the seal from a receipt read back from disk. Same idiom as
/// `model_receipt::recompute_seal_hex`: blank seal.hex, fix algorithm, hash.
pub fn recompute_seal_hex(receipt: &ToolCallReceipt) -> String {
    let mut probe = receipt.clone();
    probe.seal.algorithm = "sha256".to_string();
    probe.seal.hex.clear();
    let canonical = serde_json::to_vec(&probe).expect("serialize tool-call receipt");
    sha256_hex(&canonical)
}

/// A sealed digest field must be exactly 64 hex chars. Reuses the same check
/// as model_receipt::digest_hex_is_well_formed.
fn digest_hex_is_well_formed(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn tool_failure_class(json: bool, class: &str, code: i32) -> i32 {
    if json {
        let out = serde_json::json!({
            "schema": TOOL_RECEIPT_SCHEMA,
            "status": "failed",
            "failure_class": class,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        eprintln!("Error: tool-call receipt verification failed: {class}");
    }
    code
}

/// Offline verification of a tool-call receipt. Checks: MALFORMED (deserialize),
/// SEAL_MISMATCH (recompute + compare, FIRST), DIGEST_MALFORMED (digest hex
/// well-formedness), FIELD_CONTRACT_VIOLATION (outcome/ok coherence). Mirrors
/// `verify_model_boundary_receipt` in ordering and taxonomy.
pub fn verify_tool_call_receipt(receipt_json: &serde_json::Value, json: bool) -> Result<(), i32> {
    let receipt: ToolCallReceipt = serde_json::from_value(receipt_json.clone()).map_err(|err| {
        eprintln!("Error: tool-call receipt is malformed: {err}");
        tool_failure_class(json, "MALFORMED", 1)
    })?;

    // Integrity gate FIRST.
    let recomputed_seal = recompute_seal_hex(&receipt);
    if !recomputed_seal.eq_ignore_ascii_case(&receipt.seal.hex) {
        eprintln!(
            "Error: seal mismatch: receipt sha256:{}, recomputed sha256:{}",
            receipt.seal.hex, recomputed_seal
        );
        return Err(tool_failure_class(json, "SEAL_MISMATCH", 1));
    }

    // Digest well-formedness.
    if !digest_hex_is_well_formed(&receipt.args.sha256) {
        eprintln!("Error: malformed digest in `args.sha256`");
        return Err(tool_failure_class(json, "DIGEST_MALFORMED", 1));
    }
    if !digest_hex_is_well_formed(&receipt.output.sha256) {
        eprintln!("Error: malformed digest in `output.sha256`");
        return Err(tool_failure_class(json, "DIGEST_MALFORMED", 1));
    }
    if !receipt.prev_receipt_sha256.is_empty()
        && !digest_hex_is_well_formed(&receipt.prev_receipt_sha256)
    {
        eprintln!("Error: malformed digest in `prev_receipt_sha256`");
        return Err(tool_failure_class(json, "DIGEST_MALFORMED", 1));
    }

    // Field coherence: outcome vs ok.
    if receipt.outcome == "COMPLETED" && receipt.ok != "true" {
        eprintln!("Error: outcome `COMPLETED` carries ok != true");
        return Err(tool_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
    }
    if receipt.outcome == "BLOCKED" && receipt.ok != "false" {
        eprintln!("Error: outcome `BLOCKED` carries ok != false");
        return Err(tool_failure_class(json, "FIELD_CONTRACT_VIOLATION", 1));
    }

    if json {
        let out = serde_json::json!({
            "schema": TOOL_RECEIPT_SCHEMA,
            "status": "verified",
            "source": receipt.source,
            "outcome": receipt.outcome,
            "seal": { "algorithm": "sha256", "hex": receipt.seal.hex },
        });
        let text = serde_json::to_string_pretty(&out).map_err(|err| {
            eprintln!("Error serializing tool-call receipt report: {err}");
            1
        })?;
        println!("{text}");
    } else {
        eprintln!(
            "tool-call receipt verified: {} outcome={}",
            receipt.source, receipt.outcome
        );
    }
    Ok(())
}

// ===============================================================================
// TESTS
// ===============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_receipt() -> ToolCallReceipt {
        let mut r = ToolCallReceipt {
            schema: TOOL_RECEIPT_SCHEMA.to_string(),
            source: "tool:run-test:1".to_string(),
            tool: "read_file".to_string(),
            capability: "builtin-read".to_string(),
            admission: "ALLOWED".to_string(),
            args: ToolReceiptHashedBytes {
                sha256: "a".repeat(64),
                bytes: 20,
            },
            output: ToolReceiptHashedBytes {
                sha256: "b".repeat(64),
                bytes: 11,
            },
            ok: "true".to_string(),
            rc: 0,
            run_id: "run-test".to_string(),
            seq: 1,
            prev_receipt_sha256: String::new(),
            outcome: "COMPLETED".to_string(),
            seal: ToolReceiptSeal {
                algorithm: "sha256".to_string(),
                hex: String::new(),
            },
        };
        r.seal.hex = recompute_seal_hex(&r);
        r
    }

    #[test]
    fn valid_receipt_verifies() {
        let r = sample_receipt();
        let json_val = serde_json::to_value(&r).unwrap();
        assert!(verify_tool_call_receipt(&json_val, false).is_ok());
    }

    #[test]
    fn seal_is_deterministic() {
        let r1 = sample_receipt();
        let r2 = sample_receipt();
        assert_eq!(r1.seal.hex, r2.seal.hex);
    }

    #[test]
    fn tampered_field_breaks_seal() {
        let mut r = sample_receipt();
        r.output.bytes = 999;
        let json_val = serde_json::to_value(&r).unwrap();
        assert!(verify_tool_call_receipt(&json_val, false).is_err());
    }

    #[test]
    fn completed_with_ok_false_is_contract_violation() {
        let mut r = sample_receipt();
        r.ok = "false".to_string();
        r.seal.hex = recompute_seal_hex(&r);
        let json_val = serde_json::to_value(&r).unwrap();
        assert!(verify_tool_call_receipt(&json_val, false).is_err());
    }

    #[test]
    fn field_order_is_stable() {
        // The seal computation uses serde_json::to_vec, which preserves struct
        // declaration order (NOT alphabetical). This test verifies the to_vec
        // output has the schema fields in the declared order — the order the
        // Python emit side must match for the cross-language seal contract.
        let r = sample_receipt();
        let vec = serde_json::to_vec(&r).unwrap();
        let s = std::str::from_utf8(&vec).unwrap();
        let schema_pos = s.find("\"schema\"").unwrap();
        let source_pos = s.find("\"source\"").unwrap();
        let seal_pos = s.rfind("\"seal\"").unwrap();
        assert!(schema_pos < source_pos);
        assert!(source_pos < seal_pos);
    }

    /// The golden fixture is produced by the Python emit side
    /// (harness/tool_call_receipt.py) and pinned in both repos. This test
    /// proves the Rust serde_json canonicalization agrees byte-for-byte with
    /// the Python json.dumps canonicalization — the cross-language seal contract.
    #[test]
    fn golden_fixture_reseals_to_its_pinned_seal() {
        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/tool-receipt-golden.json");
        let fixture_bytes = std::fs::read(&fixture_path)
            .unwrap_or_else(|e| panic!("read golden fixture {:?}: {e}", fixture_path));
        let receipt: ToolCallReceipt =
            serde_json::from_slice(&fixture_bytes).expect("deserialize golden fixture");
        let expected_seal = "fde2a06af85de9ee962f2ea6141126799be4838409874edbf0fab68899535534";
        let recomputed = recompute_seal_hex(&receipt);
        assert_eq!(
            recomputed, expected_seal,
            "Rust recompute must match the Python-pinned golden seal"
        );
        // the fixture must also verify through the full arm
        let json_val = serde_json::to_value(&receipt).unwrap();
        assert!(verify_tool_call_receipt(&json_val, false).is_ok());
    }
}
