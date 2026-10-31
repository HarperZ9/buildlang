// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! The CPU-C reference drivers build and run without a device. Each test lowers
//! a real example kernel to C, extracts the kernel function as the `--gpu` path
//! does, and runs the reference over a small grid with integer-valued inputs, so
//! fused and unfused rounding agree and the expected values are exact.
//!
//! The 2026-10-31 workstation study found `buildc run examples/gpu/saxpy.bld
//! --gpu` failing before dispatch: the extracted kernel indexes through
//! `bl_idx_chk`, which the driver never defined, and names its slices
//! `BlSlice_f32`, which the driver never declared. A device test skips on a
//! machine with no Vulkan device, so these run everywhere a C compiler does.

use super::*;

fn example(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .join("examples")
        .join("gpu")
        .join(name)
}

fn kernel_c(path: &Path, entry: &str) -> String {
    let c_bytes = compile_to(path, Target::C).expect("kernel lowers to C");
    let c_source = String::from_utf8_lossy(&c_bytes);
    extract_kernel_with_runtime(&c_source, entry).expect("kernel function in the emitted C")
}

#[test]
fn saxpy_cpu_reference_builds_and_runs() {
    let path = example("saxpy.bld");
    let sig = discover_kernel_sig(&path).expect("kernel signature");
    let kernel = kernel_c(&path, &sig.entry);
    let n = 8;
    let a: Vec<f32> = (0..n).map(|i| i as f32).collect();
    let b: Vec<f32> = (0..n).map(|i| 100.0 + i as f32).collect();
    let names: Vec<&str> = sig.buffers().map(|(name, _, _)| name).collect();
    let out = vec![0.0f32; n];
    let buffers = vec![
        (names[0], a.clone()),
        (names[1], b.clone()),
        (names[2], out),
    ];
    let alpha = sig.scalars().next().expect("alpha").0;
    let got = cpu_c_reference(&sig, &kernel, &buffers, &[(alpha, 2.0)], n)
        .expect("the saxpy CPU reference compiles and runs");
    let want: Vec<f32> = (0..n).map(|i| 2.0 * a[i] + b[i]).collect();
    assert_eq!(got, want);
}

#[test]
fn stencil_cpu_reference_builds_and_runs() {
    let path = example("stencil.bld");
    let sig = discover_kernel_sig(&path).expect("kernel signature");
    let kernel = kernel_c(&path, &sig.entry);
    let a = [1.0f32, 2.0, 3.0, 4.0, 5.0];
    let got = cpu_c_stencil_reference(&sig, &kernel, &a, a.len())
        .expect("the stencil CPU reference compiles and runs");
    assert_eq!(got.len(), a.len());
}

#[test]
fn matmul_cpu_reference_builds_and_runs() {
    let path = example("matmul.bld");
    let sig = discover_kernel_sig(&path).expect("kernel signature");
    let kernel = kernel_c(&path, &sig.entry);
    let (m, k, n) = (4, 4, 4);
    let ident: Vec<f32> = (0..m * k)
        .map(|i| if i / k == i % k { 1.0 } else { 0.0 })
        .collect();
    let b: Vec<f32> = (0..k * n).map(|i| i as f32).collect();
    let got = cpu_c_matmul_reference(&sig, &kernel, &ident, &b, m, k, n)
        .expect("the matmul CPU reference compiles and runs");
    assert_eq!(got, b, "identity x B == B");
}
