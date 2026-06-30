// ===============================================================================
// BUILDLANG COMPILER - FLAGSHIP-ACTION <-> BDF BRIDGE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! A lossless bridge between the ecosystem interop envelope
//! `project-telos.flagship-action/v1` and a [`BdfMessage`].
//!
//! The flagships (gather, index, forum, crucible, telos) interoperate through a
//! plain-JSON `project-telos.flagship-action/v1` envelope. This bridge proves
//! BDF can carry that real interop format with nothing lost: a flagship-action
//! JSON round-trips through a [`BdfMessage`] and back, semantically identical,
//! through both the JSON and the canonical-binary forms.
//!
//! ## Mapping (flagship-action field -> BdfMessage)
//!
//! | flagship-action/v1                 | BdfMessage                              |
//! |------------------------------------|-----------------------------------------|
//! | `tool`, `tool_version`             | `produced_by.{tool, tool_version}`      |
//! | `next_actions[]` (projected)       | `next[]` (`tool`, `action`, `reason`)   |
//! | `receipts[].sha256`                | `receipt.derived_from[]` (lineage)      |
//! | *(carrier exercises no effect)*    | `effects = []`                          |
//! | **the whole envelope, verbatim**   | `payload` (a `BdfValue`), under         |
//! |                                    | `payload_schema = flagship-action/v1`   |
//!
//! The full envelope - `command`, `status`, `inputs`, `outputs`, the complete
//! `receipts` (with `kind` / `ref` / `method` / `derived_from`), `native`, the
//! complete `next_actions` (with `inputs` / `priority`), `diagnostics`, and any
//! unknown or extra fields - is carried verbatim in `payload`, so reconstruction
//! drops nothing. The projected `produced_by` / `next` / `receipt` fields are a
//! typed convenience view over that payload, not the source of truth.
//!
//! ## Verdict is not admission
//!
//! `status` (MATCH / DRIFT / UNVERIFIABLE) is a *native verdict*. It is carried
//! verbatim inside `payload` and is **never** collapsed into BDF-level
//! admission: BDF effects and the receipt record what was produced; a separate
//! gate, and Crucible's verdict, remain independent axes. The bridge claims no
//! effects (`effects = []`) and never reinterprets the verdict.

use crate::bdf::json::{bdf_to_plain_json, bdf_to_plain_json_pretty, plain_json_to_bdf};
use crate::bdf::{BdfError, BdfMessage, BdfResult, BdfValue, NextAction, ProducedBy};

/// Schema id of the ecosystem flagship-action interop envelope (v1).
pub const FLAGSHIP_ACTION_SCHEMA: &str = "project-telos.flagship-action/v1";

/// Convert a `project-telos.flagship-action/v1` JSON envelope into a
/// [`BdfMessage`], carrying the entire envelope losslessly in the payload.
///
/// Fails closed if the JSON is malformed, is not an object, or does not carry
/// the flagship-action/v1 schema string.
pub fn flagship_action_to_bdf(json: &str) -> BdfResult<BdfMessage> {
    let value = plain_json_to_bdf(json)?;
    let map = match &value {
        BdfValue::Map(m) => m,
        _ => {
            return Err(BdfError::Json(
                "flagship-action envelope must be a JSON object".to_string(),
            ))
        }
    };

    // Fail closed on a wrong / missing schema. The verdict is NOT collapsed into
    // admission; only the carrier format is validated here.
    match map.get("schema") {
        Some(BdfValue::Str(s)) if s == FLAGSHIP_ACTION_SCHEMA => {}
        Some(BdfValue::Str(other)) => {
            return Err(BdfError::UnsupportedSchema {
                found: other.clone(),
                expected: FLAGSHIP_ACTION_SCHEMA,
            })
        }
        _ => {
            return Err(BdfError::UnsupportedSchema {
                found: "<missing>".to_string(),
                expected: FLAGSHIP_ACTION_SCHEMA,
            })
        }
    }

    let produced_by = ProducedBy {
        tool: string_field(map, "tool"),
        tool_version: string_field(map, "tool_version"),
    };
    let next = project_next_actions(map);
    let derived_from = project_receipt_lineage(map);

    // The whole envelope is the payload, verbatim. `BdfMessage::new` stamps the
    // BDF-level receipt: a sha256 over the canonical-binary payload, with the
    // flagship receipts' digests recorded as `derived_from` lineage. The
    // flagship's own receipts remain inside the payload untouched.
    Ok(BdfMessage::new(
        produced_by,
        Vec::new(), // the carrier bridge exercises no capability effect
        FLAGSHIP_ACTION_SCHEMA,
        value.clone(),
        derived_from,
        next,
    ))
}

/// Reconstruct the `project-telos.flagship-action/v1` JSON envelope from a
/// [`BdfMessage`] produced by [`flagship_action_to_bdf`], byte/structure
/// identical to the original (compact JSON).
///
/// Fails closed if the message does not carry the flagship-action payload schema
/// or if the payload is not a flagship-action object.
pub fn bdf_to_flagship_action(message: &BdfMessage) -> BdfResult<String> {
    let value = validated_payload(message)?;
    bdf_to_plain_json(value)
}

/// As [`bdf_to_flagship_action`], but pretty-printed.
pub fn bdf_to_flagship_action_pretty(message: &BdfMessage) -> BdfResult<String> {
    let value = validated_payload(message)?;
    bdf_to_plain_json_pretty(value)
}

/// Validate that a message carries a flagship-action payload and return it.
fn validated_payload(message: &BdfMessage) -> BdfResult<&BdfValue> {
    if message.payload_schema != FLAGSHIP_ACTION_SCHEMA {
        return Err(BdfError::UnsupportedSchema {
            found: message.payload_schema.clone(),
            expected: FLAGSHIP_ACTION_SCHEMA,
        });
    }
    match &message.payload {
        BdfValue::Map(m) => match m.get("schema") {
            Some(BdfValue::Str(s)) if s == FLAGSHIP_ACTION_SCHEMA => Ok(&message.payload),
            Some(BdfValue::Str(other)) => Err(BdfError::UnsupportedSchema {
                found: other.clone(),
                expected: FLAGSHIP_ACTION_SCHEMA,
            }),
            _ => Err(BdfError::UnsupportedSchema {
                found: "<missing>".to_string(),
                expected: FLAGSHIP_ACTION_SCHEMA,
            }),
        },
        _ => Err(BdfError::Json(
            "flagship-action payload must be a JSON object".to_string(),
        )),
    }
}

/// Read a string field from an envelope map, defaulting to empty if absent or
/// not a string. (The projected fields are a convenience view; the verbatim
/// copy in the payload is the source of truth, so a missing optional here never
/// loses data.)
fn string_field(map: &indexmap::IndexMap<String, BdfValue>, key: &str) -> String {
    match map.get(key) {
        Some(BdfValue::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Project `next_actions[]` into the typed [`NextAction`] view (`tool`,
/// `action`, `reason`). The flagship `action` field is named `action`; its
/// extra fields (`inputs`, `priority`) stay in the payload.
fn project_next_actions(map: &indexmap::IndexMap<String, BdfValue>) -> Vec<NextAction> {
    let Some(BdfValue::Array(items)) = map.get("next_actions") else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let BdfValue::Map(na) = item else {
                return None;
            };
            Some(NextAction {
                tool: na_string(na, "tool"),
                action: na_string(na, "action"),
                reason: na_string(na, "reason"),
            })
        })
        .collect()
}

fn na_string(map: &indexmap::IndexMap<String, BdfValue>, key: &str) -> String {
    match map.get(key) {
        Some(BdfValue::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Collect every `receipts[].sha256` as the BDF receipt's `derived_from`
/// lineage. The full receipts (kind, ref, method, derived_from) stay verbatim
/// in the payload.
fn project_receipt_lineage(map: &indexmap::IndexMap<String, BdfValue>) -> Vec<String> {
    let Some(BdfValue::Array(items)) = map.get("receipts") else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item {
            BdfValue::Map(r) => match r.get("sha256") {
                Some(BdfValue::Str(s)) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A doctor-style envelope: minimal, empty collections, no native block.
    fn doctor_fixture() -> &'static str {
        r#"{"schema":"project-telos.flagship-action/v1","tool":"buildc","tool_version":"1.0.0","command":"doctor","status":"MATCH","inputs":[],"outputs":[],"receipts":[],"native":{"checks":[{"name":"toolchain","ok":true},{"name":"effects-db","ok":true}],"summary":"all green"},"next_actions":[],"diagnostics":[]}"#
    }

    /// A status envelope with a nested `native` block and a DRIFT verdict.
    fn status_fixture() -> &'static str {
        r#"{"schema":"project-telos.flagship-action/v1","tool":"index","tool_version":"2.8.0","command":"status","status":"DRIFT","inputs":["src/"],"outputs":[],"receipts":[{"kind":"document","ref":"index.json","method":"file-read","sha256":"aa11bb22","derived_from":[]}],"native":{"graph":{"nodes":42,"edges":117,"density":0.5},"drift":{"added":["mod-x"],"removed":[],"changed":["mod-y"]},"verdict":"DRIFT"},"next_actions":[{"tool":"forum","action":"route","reason":"drift needs review","inputs":[],"priority":"high"}],"diagnostics":["module mod-y changed signature"]}"#
    }

    /// A workflow envelope with multiple receipts (lineage) and next_actions.
    fn workflow_fixture() -> &'static str {
        r#"{"schema":"project-telos.flagship-action/v1","tool":"forum","tool_version":"1.12.0","command":"run","status":"UNVERIFIABLE","inputs":["plan.json"],"outputs":["result.json"],"receipts":[{"kind":"document","ref":"plan.json","method":"file-read","sha256":"deadbeef01","derived_from":[]},{"kind":"transcript","ref":"run.log","method":"witnessed-ledger","sha256":"cafe1234","derived_from":["deadbeef01"]}],"native":{"agents":["planner","critic"],"steps":3,"ledger_root":"abc999"},"next_actions":[{"tool":"crucible","action":"measure","reason":"verify the artifact","inputs":["result.json"],"priority":"normal"},{"tool":"gather","action":"intake","reason":"archive the run","inputs":[],"priority":"low"}],"diagnostics":[]}"#
    }

    fn fixtures() -> Vec<&'static str> {
        vec![doctor_fixture(), status_fixture(), workflow_fixture()]
    }

    /// Compare two JSON texts by *semantic* equality (order-insensitive,
    /// whitespace-insensitive) - the standard "canonical JSON equality".
    fn json_eq(a: &str, b: &str) -> bool {
        let va: serde_json::Value = serde_json::from_str(a).expect("a is json");
        let vb: serde_json::Value = serde_json::from_str(b).expect("b is json");
        va == vb
    }

    // ---- LOSSLESS ROUND-TRIP (through JSON) ----

    #[test]
    fn round_trip_through_json_is_lossless() {
        for fx in fixtures() {
            let message = flagship_action_to_bdf(fx).expect("to bdf");
            let back = bdf_to_flagship_action(&message).expect("to fa");
            assert!(
                json_eq(fx, &back),
                "json round-trip mismatch\n original: {fx}\n returned: {back}"
            );
        }
    }

    // ---- LOSSLESS ROUND-TRIP (through the binary form) ----

    #[test]
    fn round_trip_through_binary_is_lossless() {
        for fx in fixtures() {
            let message = flagship_action_to_bdf(fx).expect("to bdf");
            let bytes = message.to_bytes();
            let decoded = BdfMessage::from_bytes(&bytes).expect("from bytes");
            assert_eq!(message, decoded, "message changed across binary form");
            let back = bdf_to_flagship_action(&decoded).expect("to fa");
            assert!(
                json_eq(fx, &back),
                "binary round-trip mismatch\n original: {fx}\n returned: {back}"
            );
        }
    }

    // ---- FIELD PROJECTION ----

    #[test]
    fn produced_by_maps_tool_and_version() {
        let m = flagship_action_to_bdf(status_fixture()).expect("to bdf");
        assert_eq!(m.produced_by.tool, "index");
        assert_eq!(m.produced_by.tool_version, "2.8.0");
        assert_eq!(m.payload_schema, FLAGSHIP_ACTION_SCHEMA);
    }

    #[test]
    fn next_actions_project_into_next() {
        let m = flagship_action_to_bdf(workflow_fixture()).expect("to bdf");
        assert_eq!(m.next.len(), 2);
        assert_eq!(m.next[0].tool, "crucible");
        assert_eq!(m.next[0].action, "measure");
        assert_eq!(m.next[1].tool, "gather");
    }

    #[test]
    fn receipt_lineage_collects_flagship_sha256s() {
        let m = flagship_action_to_bdf(workflow_fixture()).expect("to bdf");
        // Both flagship receipt digests become the BDF receipt's derived_from.
        assert_eq!(m.receipt.derived_from, vec!["deadbeef01", "cafe1234"]);
        // The BDF-level receipt sha256 is a fresh digest over the BDF payload,
        // NOT one of the flagship digests (admission anchor is separate).
        assert_eq!(m.receipt.sha256.len(), 64);
        assert_ne!(m.receipt.sha256, "deadbeef01");
    }

    #[test]
    fn carrier_claims_no_effects() {
        let m = flagship_action_to_bdf(doctor_fixture()).expect("to bdf");
        assert!(m.effects.is_empty(), "bridge must not invent effects");
    }

    #[test]
    fn status_verdict_is_preserved_verbatim_not_collapsed() {
        // The verdict survives as a payload field and is NOT turned into
        // BDF-level admission or an effect.
        for (fx, want) in [
            (doctor_fixture(), "MATCH"),
            (status_fixture(), "DRIFT"),
            (workflow_fixture(), "UNVERIFIABLE"),
        ] {
            let m = flagship_action_to_bdf(fx).expect("to bdf");
            let BdfValue::Map(payload) = &m.payload else {
                panic!("payload must be a map");
            };
            assert_eq!(
                payload.get("status"),
                Some(&BdfValue::Str(want.to_string())),
                "status verdict must round-trip verbatim"
            );
        }
    }

    // ---- LOSSLESSNESS OF UNKNOWN / EXTRA FIELDS ----

    #[test]
    fn unknown_extra_fields_survive_round_trip() {
        let fx = r#"{"schema":"project-telos.flagship-action/v1","tool":"telos","tool_version":"0.9.0","command":"perceive","status":"MATCH","inputs":[],"outputs":[],"receipts":[],"native":{},"next_actions":[],"diagnostics":[],"experimental_field":{"weird":[1,2,3],"flag":true},"trace_id":"xyz-123"}"#;
        let m = flagship_action_to_bdf(fx).expect("to bdf");
        let back = bdf_to_flagship_action(&m).expect("to fa");
        assert!(json_eq(fx, &back), "unknown fields dropped:\n{fx}\n{back}");
    }

    // ---- NEGATIVE (fail closed) ----

    #[test]
    fn rejects_unknown_schema() {
        let fx = r#"{"schema":"project-telos.flagship-action/v999","tool":"x","tool_version":"1"}"#;
        assert!(matches!(
            flagship_action_to_bdf(fx),
            Err(BdfError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn rejects_missing_schema() {
        let fx = r#"{"tool":"x","tool_version":"1","command":"y"}"#;
        assert!(matches!(
            flagship_action_to_bdf(fx),
            Err(BdfError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn rejects_non_object_envelope() {
        assert!(flagship_action_to_bdf("[1,2,3]").is_err());
        assert!(flagship_action_to_bdf("\"just a string\"").is_err());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(flagship_action_to_bdf("{not json").is_err());
    }

    #[test]
    fn to_flagship_rejects_wrong_payload_schema() {
        // A BdfMessage whose payload_schema is not flagship-action is refused.
        let m = BdfMessage::new(
            ProducedBy {
                tool: "x".to_string(),
                tool_version: "1".to_string(),
            },
            Vec::new(),
            "buildlang.demo/v0",
            BdfValue::Null,
            Vec::new(),
            Vec::new(),
        );
        assert!(matches!(
            bdf_to_flagship_action(&m),
            Err(BdfError::UnsupportedSchema { .. })
        ));
    }
}
