// ===============================================================================
// BUILDLANG GPU RECEIPT - Layer C sealed, re-checkable GPU cross-check receipt
// ===============================================================================
// Copyright (c) 2026 Zain Dana Harper. BuildLang Fair-Source License v1.0.
// ===============================================================================
//
//! Layer C: emit a sealed, third-party-re-checkable receipt for a GPU cross-check
//! run. The receipt records the GPU readback as the measurement series, the
//! witnessed `Gpu` capability as the effect policy, and a `gpu_cpu_agreement`
//! invariant (max abs deviation <= tolerance) beside it. A SHA-256 seal over the
//! canonical body makes any tamper detectable by `receipt verify`.

use std::path::Path;

use sha2::{Digest, Sha256};

/// Emit the GPU cross-check receipt to `receipt_path`. Used by the device
/// dispatch path (feature "gpu"); verification below is always compiled.
#[cfg_attr(not(feature = "gpu"), allow(dead_code))]
pub fn emit_gpu_receipt(
    source: &Path,
    receipt_path: &Path,
    gpu_out: &[f32],
    cpu_out: &[f32],
    max_dev: f32,
    tolerance: f32,
) -> Result<(), String> {
    let status = if max_dev <= tolerance {
        "PASS"
    } else {
        "FAIL_UNEXPECTED"
    };

    let source_text = std::fs::read_to_string(source)
        .map_err(|e| format!("read source {}: {e}", source.display()))?;
    let source_digest = sha256_hex(source_text.as_bytes());

    let series: Vec<f64> = gpu_out.iter().map(|&x| x as f64).collect();
    let cpu_series: Vec<f64> = cpu_out.iter().map(|&x| x as f64).collect();

    // Canonical body (the sealed content). Field order is fixed so the seal is
    // reproducible across runs of the same result.
    let body = serde_json::json!({
        "schema": "buildlang.gpu-receipt/v0",
        "source": {
            "path": source.to_string_lossy(),
            "digest": { "algorithm": "sha256", "hex": source_digest },
        },
        "effect_policy": {
            "witnessed_capabilities": ["Gpu"],
        },
        "measurement": {
            "metric": "gpu_readback",
            "series": series,
        },
        "reference": {
            "metric": "cpu_c_scalar_loop",
            "series": cpu_series,
        },
        "invariant": {
            "name": "gpu_cpu_agreement",
            "tolerance": tolerance as f64,
            "observed": {
                "max_abs_deviation": max_dev as f64,
            },
        },
        "receipt_status": status,
    });

    let body_canonical =
        serde_json::to_string(&body).map_err(|e| format!("serialize receipt body: {e}"))?;
    let seal = sha256_hex(body_canonical.as_bytes());

    let sealed = serde_json::json!({
        "body": body,
        "seal": { "algorithm": "sha256", "hex": seal },
    });

    let text =
        serde_json::to_string_pretty(&sealed).map_err(|e| format!("serialize receipt: {e}"))?;
    std::fs::write(receipt_path, text)
        .map_err(|e| format!("write receipt {}: {e}", receipt_path.display()))?;
    Ok(())
}

/// Verify a GPU receipt: recompute the seal over the body and re-check the
/// agreement invariant against the recorded series. Returns `Ok(())` on a
/// verified PASS, `Err` describing the first failure otherwise.
pub fn verify_gpu_receipt(receipt_path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(receipt_path)
        .map_err(|e| format!("read receipt {}: {e}", receipt_path.display()))?;
    let sealed: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("parse receipt json: {e}"))?;

    let body = sealed
        .get("body")
        .ok_or_else(|| "receipt has no body".to_string())?;
    let recorded_seal = sealed
        .pointer("/seal/hex")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "receipt has no seal".to_string())?;

    // Recompute the seal over the canonical body.
    let body_canonical = serde_json::to_string(body).map_err(|e| format!("serialize body: {e}"))?;
    let recomputed = sha256_hex(body_canonical.as_bytes());
    if recomputed != recorded_seal {
        return Err(format!(
            "seal mismatch: recorded {recorded_seal}, recomputed {recomputed} (receipt tampered)"
        ));
    }

    // Re-check the agreement invariant against the recorded series/reference.
    let tolerance = body
        .pointer("/invariant/tolerance")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "receipt has no invariant tolerance".to_string())?;
    let series = body
        .pointer("/measurement/series")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "receipt has no measurement series".to_string())?;
    let reference = body
        .pointer("/reference/series")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "receipt has no reference series".to_string())?;
    if series.len() != reference.len() {
        return Err("series and reference length differ".to_string());
    }
    let mut max_dev = 0.0f64;
    for (g, c) in series.iter().zip(reference.iter()) {
        let g = g
            .as_f64()
            .ok_or_else(|| "non-numeric series value".to_string())?;
        let c = c
            .as_f64()
            .ok_or_else(|| "non-numeric reference value".to_string())?;
        let dev = (g - c).abs();
        if dev > max_dev {
            max_dev = dev;
        }
    }
    let status = body
        .get("receipt_status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if max_dev <= tolerance {
        if status != "PASS" {
            return Err(format!(
                "recomputed agreement holds (dev {max_dev:.3e} <= tol {tolerance:.3e}) but status is {status:?}"
            ));
        }
        Ok(())
    } else {
        Err(format!(
            "agreement re-check FAILED: max abs deviation {max_dev:.3e} > tol {tolerance:.3e}"
        ))
    }
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
