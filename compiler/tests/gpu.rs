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

fn saxpy_example() -> PathBuf {
    repo_root().join("examples").join("gpu").join("saxpy.bld")
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

/// Compile a compute kernel at `src` to SPIR-V at `out`. Panics with full
/// diagnostics on failure so a codegen regression is legible.
fn compile_kernel_spirv(src: &Path, out: &Path) {
    let output = buildc()
        .arg(src)
        .arg("--target")
        .arg("spirv")
        .arg("-o")
        .arg(out)
        .output()
        .expect("run buildc to compile a compute kernel");
    assert!(
        output.status.success(),
        "buildc should compile {} to SPIR-V\nstdout:\n{}\nstderr:\n{}",
        src.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(out.exists(), "expected .spv output at {}", out.display());
}

/// Compile the canonical `vec_add` compute kernel to SPIR-V.
fn compile_vec_add_spirv(out: &Path) {
    compile_kernel_spirv(&vec_add_example(), out);
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

/// Phase 1: the arbitrary-elementwise proof kernel `saxpy` (scalar push
/// constant + two read-only inputs + one writable output, entry point named
/// `saxpy`, not `vec_add`) emits VALID dispatchable compute SPIR-V.
#[test]
fn saxpy_emits_valid_compute_spirv() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping saxpy_emits_valid_compute_spirv: spirv-val not on PATH");
        return;
    };
    let out = temp_spv("saxpy_valid");
    compile_kernel_spirv(&saxpy_example(), &out);

    let (ok, stderr) = spirv_val_ok(tool, &out);
    assert!(
        ok,
        "spirv-val should accept the emitted saxpy compute module:\n{stderr}"
    );

    let _ = std::fs::remove_file(&out);
}

/// WRITABILITY PROOF (Layer A, device-free): a kernel that writes into a
/// parameter declared `&[f32]` (read-only, NOT `&mut`) must be REJECTED at
/// compile time. This proves the writability inference is real -- not a
/// decorative decoration -- because the read-only buffer is emitted as a
/// non-writable binding a store cannot target.
///
/// A passing gate that never rejects proves nothing; this is the negative that
/// makes the inference bite.
#[test]
fn readonly_buffer_write_is_rejected() {
    let dir = std::env::temp_dir().join(format!("buildlang_gpu_ro_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let src = dir.join("readonly_write.bld");
    // `a` is `&[f32]` (read-only) yet the body writes `a[i]` -- illegal.
    std::fs::write(
        &src,
        "#[compute]\n\
         fn ro_write(a: &[f32], out: &mut [f32]) ~ Gpu {\n\
        \x20   let i = gl_GlobalInvocationID.x;\n\
        \x20   a[i] = out[i];\n\
         }\n",
    )
    .expect("write readonly_write.bld");

    let out = dir.join("readonly_write.spv");
    let output = buildc()
        .arg(&src)
        .arg("--target")
        .arg("spirv")
        .arg("-o")
        .arg(&out)
        .output()
        .expect("run buildc on the read-only-write kernel");
    assert!(
        !output.status.success(),
        "writing into a read-only `&[f32]` buffer MUST be rejected at compile time; \
         if it compiles, the writability inference is decorative\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("read-only")
            || stderr.to_lowercase().contains("writable")
            || stderr.to_lowercase().contains("non-writable"),
        "the rejection should name the writability cause; got:\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
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

    /// Phase 1: the arbitrary-elementwise `saxpy` kernel (scalar push constant +
    /// two inputs + one output) dispatches on the physical device and agrees
    /// with the CPU-C scalar loop within tolerance. Proves the generalized path
    /// (entry-name discovery, push constant, per-buffer host binding) runs on
    /// real hardware.
    #[test]
    fn saxpy_gpu_matches_cpu() {
        if !vulkan_device_available() {
            eprintln!("skipping saxpy_gpu_matches_cpu: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(saxpy_example())
            .arg("--gpu")
            .output()
            .expect("run buildc run --gpu on saxpy");
        assert!(
            output.status.success(),
            "buildc run --gpu on saxpy should agree with CPU-C within tolerance\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("gpu-cpu agreement") || stdout.contains("PASS"),
            "expected an agreement verdict in output:\n{stdout}"
        );
    }

    /// CAN-IT-FAIL negative for the generalized path: a corrupted saxpy readback
    /// MUST be caught by the tolerance gate (non-zero exit).
    #[test]
    fn wrong_saxpy_result_is_caught() {
        if !vulkan_device_available() {
            eprintln!("skipping wrong_saxpy_result_is_caught: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(saxpy_example())
            .arg("--gpu")
            .env("BUILDLANG_GPU_CORRUPT_READBACK", "1")
            .output()
            .expect("run buildc run --gpu on saxpy with a corrupted readback");
        assert!(
            !output.status.success(),
            "a divergent saxpy readback MUST be caught (non-zero exit)\nstdout:\n{}\nstderr:\n{}",
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
