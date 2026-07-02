// ===============================================================================
// BUILDLANG GPU PATH - INTEGRATION TESTS
// ===============================================================================
// Copyright (c) 2026 Zain Dana Harper. BuildLang Fair-Source License v1.0.
// ===============================================================================
//
//! End-to-end tests for the real GPU path (`buildc ... --target spirv` compute
//! kernels, plus device dispatch behind the `gpu` feature).
//!
//! Layer A (this file, always compiled): the compiler emits VALID dispatchable
//! compute SPIR-V for `examples/gpu/vec_add.bld`, validated by shelling out to
//! the real `spirv-val`. Device execution is NOT claimed here; only emission +
//! external validation.
//!
//! Device tests (Layer B/C) are gated behind `vulkan_device_available()` and the
//! `gpu` cargo feature so CI (no device, default features) stays green and no
//! test lies about hardware.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn vec_add_example() -> PathBuf {
    repo_root().join("examples").join("gpu").join("vec_add.bld")
}

/// Resolve `spirv-val` on PATH (Vulkan SDK adds it). Returns the program name
/// to invoke, or `None` if the tool is not installed -> the caller skips.
fn spirv_val_available() -> Option<&'static str> {
    let ok = Command::new("spirv-val")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok {
        Some("spirv-val")
    } else {
        None
    }
}

/// A unique temp path for this test process + label.
fn temp_spv(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "buildlang_gpu_{}_{}.spv",
        label,
        std::process::id()
    ))
}

/// Compile the canonical compute kernel to SPIR-V. Panics with full diagnostics
/// on failure so a codegen regression is legible.
fn compile_vec_add_spirv(out: &Path) {
    let output = buildc()
        .arg(vec_add_example())
        .arg("--target")
        .arg("spirv")
        .arg("-o")
        .arg(out)
        .output()
        .expect("run buildc to compile vec_add.bld");
    assert!(
        output.status.success(),
        "buildc should compile vec_add.bld to SPIR-V\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(out.exists(), "expected .spv output at {}", out.display());
}

/// Run spirv-val on a module; return true iff it validated (exit 0).
fn spirv_val_ok(tool: &str, spv: &Path) -> (bool, String) {
    let output = Command::new(tool)
        .arg("--target-env")
        .arg("vulkan1.0")
        .arg(spv)
        .output()
        .expect("run spirv-val");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

// ---------------------------------------------------------------------------
// LAYER A: valid dispatchable compute SPIR-V, validated by spirv-val.
// ---------------------------------------------------------------------------

#[test]
fn emits_valid_compute_spirv() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping emits_valid_compute_spirv: spirv-val not on PATH");
        return;
    };
    let out = temp_spv("valid");
    compile_vec_add_spirv(&out);

    let (ok, stderr) = spirv_val_ok(tool, &out);
    assert!(
        ok,
        "spirv-val should accept the emitted compute module:\n{stderr}"
    );

    let _ = std::fs::remove_file(&out);
}

/// CAN-IT-FAIL negative: a corrupted module MUST be rejected by spirv-val. This
/// proves the validation gate discriminates (a gate that always passes is not a
/// gate). We flip a word in the middle of the valid module and require a
/// non-zero exit.
#[test]
fn spirv_val_rejects_corrupt_module() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping spirv_val_rejects_corrupt_module: spirv-val not on PATH");
        return;
    };
    let out = temp_spv("corrupt");
    compile_vec_add_spirv(&out);

    // Sanity: the pristine module validates.
    let (ok, stderr) = spirv_val_ok(tool, &out);
    assert!(ok, "pristine module should validate first:\n{stderr}");

    // Corrupt a word past the 5-word header (header words must stay intact for
    // spirv-val to parse far enough to reject the body). SPIR-V is little-endian
    // 32-bit words; flipping bits mid-body yields an invalid instruction stream.
    let mut bytes = std::fs::read(&out).expect("read valid spv");
    assert!(bytes.len() > 40, "module unexpectedly tiny");
    // Word index 8 -> byte offset 32. XOR the low byte with 0xFF.
    bytes[32] ^= 0xFF;
    bytes[33] ^= 0xFF;
    let corrupt = temp_spv("corrupt_out");
    std::fs::write(&corrupt, &bytes).expect("write corrupt spv");

    let (ok, _stderr) = spirv_val_ok(tool, &corrupt);
    assert!(
        !ok,
        "spirv-val must REJECT a corrupted module (non-zero exit); if it accepts, the gate does not discriminate"
    );

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&corrupt);
}

// ---------------------------------------------------------------------------
// LAYER B/C: device execution + sealed receipt. Gated on the `gpu` feature and
// an actual Vulkan device. Compiled in only under `--features gpu`.
// ---------------------------------------------------------------------------

#[cfg(feature = "gpu")]
mod device {
    use super::*;

    /// True iff a Vulkan compute device is enumerable. Probed via `buildc doctor`
    /// which prints the gpu row.
    fn vulkan_device_available() -> bool {
        let output = buildc().arg("doctor").output().expect("run buildc doctor");
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.contains("gpu      ready")
    }

    #[test]
    fn gpu_matches_cpu_within_tolerance() {
        if !vulkan_device_available() {
            eprintln!("skipping gpu_matches_cpu_within_tolerance: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(vec_add_example())
            .arg("--gpu")
            .output()
            .expect("run buildc run --gpu");
        assert!(
            output.status.success(),
            "buildc run --gpu should agree with CPU-C within tolerance\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("gpu-cpu agreement") || stdout.contains("PASS"),
            "expected an agreement verdict in output:\n{stdout}"
        );
    }

    /// CAN-IT-FAIL negative: the cross-check must catch a GPU/CPU divergence.
    /// The SPIR-V and C paths lower the SAME source, so a wrong *algorithm* makes
    /// both wrong identically (and they would still agree -- that is correct: the
    /// tool verifies GPU matches CPU semantics, not that the algorithm is right).
    /// To prove the tolerance gate discriminates, we inject a real divergence:
    /// BUILDLANG_GPU_CORRUPT_READBACK perturbs one readback element, and the
    /// cross-check MUST report a mismatch and exit non-zero.
    #[test]
    fn wrong_gpu_result_is_caught() {
        if !vulkan_device_available() {
            eprintln!("skipping wrong_gpu_result_is_caught: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(vec_add_example())
            .arg("--gpu")
            .env("BUILDLANG_GPU_CORRUPT_READBACK", "1")
            .output()
            .expect("run buildc run --gpu with a corrupted readback");
        assert!(
            !output.status.success(),
            "a divergent GPU readback MUST be caught (non-zero exit); if it passes, the gate does not discriminate\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[test]
    fn gpu_receipt_verifies_and_detects_tamper() {
        if !vulkan_device_available() {
            eprintln!("skipping gpu_receipt_verifies_and_detects_tamper: no Vulkan device");
            return;
        }
        let receipt =
            std::env::temp_dir().join(format!("buildlang_gpu_receipt_{}.json", std::process::id()));
        let output = buildc()
            .arg("run")
            .arg(vec_add_example())
            .arg("--gpu")
            .arg("--emit-receipt")
            .arg(&receipt)
            .output()
            .expect("emit gpu receipt");
        assert!(
            output.status.success(),
            "emitting a passing gpu receipt should succeed\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr),
        );

        // Verify: PASS.
        let verify = buildc()
            .arg("receipt")
            .arg("verify")
            .arg(&receipt)
            .output()
            .expect("verify gpu receipt");
        assert!(
            verify.status.success(),
            "a pristine gpu receipt should verify\nstderr:\n{}",
            String::from_utf8_lossy(&verify.stderr),
        );

        // Tamper: mutate one series value inside the sealed body; verify MUST
        // fail (the seal no longer matches the recomputed body digest).
        let text = std::fs::read_to_string(&receipt).expect("read receipt");
        let mut json: serde_json::Value = serde_json::from_str(&text).expect("receipt is json");
        let series = json
            .pointer_mut("/body/measurement/series")
            .and_then(|v| v.as_array_mut())
            .expect("gpu receipt has /body/measurement/series");
        *series.first_mut().expect("series is non-empty") = serde_json::json!(999999.0);
        std::fs::write(&receipt, serde_json::to_string_pretty(&json).unwrap())
            .expect("write tampered receipt");
        let verify = buildc()
            .arg("receipt")
            .arg("verify")
            .arg(&receipt)
            .output()
            .expect("verify tampered receipt");
        assert!(
            !verify.status.success(),
            "a tampered gpu receipt MUST fail verification (seal/status)"
        );

        let _ = std::fs::remove_file(&receipt);
    }
}
