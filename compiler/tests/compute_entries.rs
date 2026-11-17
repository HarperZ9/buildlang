// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! Compute entry points on the SPIR-V target: `#[compute(workgroup_size =
//! [x, y, z])]` sets each entry's `LocalSize`, a file may hold several entries,
//! and `--entry NAME` keeps one.
//!
//! Every positive test reads the emitted words directly (OpEntryPoint name to
//! id, OpExecutionMode LocalSize per id), so it checks something on any machine.
//! Where `spirv-val` and `spirv-dis` are on PATH the module is also validated
//! and the disassembly must hold the expected execution-mode line. With
//! `BUILDLANG_SHADER_TOOLS_REQUIRE=1` (CI on Linux) a missing tool fails.

mod spirv_support;

use std::collections::BTreeMap;

use spirv_support::{
    buildc, compile_spirv, example, local_sizes, spirv_words, temp_path, text,
    validate_and_disassemble, write_source,
};

const ONE_KERNEL: &str = "#[compute(workgroup_size = [8, 4, 2])]
fn k(a: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    out[i] = a[i] + 1.0;
}
";

#[test]
fn declared_size_is_the_local_size() {
    let src = write_source("one", ONE_KERNEL);
    let (spv, words) = spirv_words("one", &src, &[]);
    let sizes = local_sizes(&words);
    assert_eq!(sizes.len(), 1, "{sizes:?}");
    assert_eq!(sizes["k"], (8, 4, 2), "x, y and z in declaration order");
    validate_and_disassemble(
        "declared_size_is_the_local_size",
        &spv,
        &["OpExecutionMode %k LocalSize 8 4 2"],
    );
}

#[test]
fn each_entry_gets_its_own_size() {
    let (spv, words) = spirv_words("multi", &example("compute_entries.bld"), &[]);
    let sizes = local_sizes(&words);
    let want: BTreeMap<String, (u32, u32, u32)> = [
        ("cs_scale".to_string(), (64, 1, 1)),
        ("cs_grid".to_string(), (8, 8, 1)),
        // Bare `#[compute]` reading only `.x`: the 1D default, not 16x16x1,
        // although cs_grid in the same file reads `.y`.
        ("cs_bare".to_string(), (64, 1, 1)),
    ]
    .into_iter()
    .collect();
    assert_eq!(sizes, want);
    validate_and_disassemble(
        "each_entry_gets_its_own_size",
        &spv,
        &[
            "OpExecutionMode %cs_scale LocalSize 64 1 1",
            "OpExecutionMode %cs_grid LocalSize 8 8 1",
            "OpExecutionMode %cs_bare LocalSize 64 1 1",
        ],
    );
}

/// Kernels without the attribute keep the sizes they had: 64x1x1 for a 1D
/// kernel, 16x16x1 for one that reads `gl_GlobalInvocationID.y`.
#[test]
fn bare_kernels_keep_their_sizes() {
    let (_, words) = spirv_words("vec_add", &example("vec_add.bld"), &[]);
    assert_eq!(local_sizes(&words)["vec_add"], (64, 1, 1));
    let (_, words) = spirv_words("matmul", &example("matmul.bld"), &[]);
    assert_eq!(local_sizes(&words)["matmul"], (16, 16, 1));
}

#[test]
fn entry_option_keeps_one_entry() {
    let src = example("compute_entries.bld");
    let (spv, words) = spirv_words("entry", &src, &["--entry", "cs_scale"]);
    let sizes = local_sizes(&words);
    assert_eq!(sizes.len(), 1, "only the selected entry: {sizes:?}");
    assert_eq!(sizes["cs_scale"], (64, 1, 1));
    validate_and_disassemble(
        "entry_option_keeps_one_entry",
        &spv,
        &["OpExecutionMode %cs_scale LocalSize 64 1 1"],
    );
}

#[test]
fn unknown_entry_is_refused_with_the_names_there_are() {
    let out = temp_path("unknown_entry", "spv");
    let _ = std::fs::remove_file(&out);
    let o = compile_spirv(&example("compute_entries.bld"), &out, &["--entry", "nope"]);
    assert!(
        !o.status.success(),
        "an unknown --entry must fail\n{}",
        text(&o)
    );
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("no shader entry point named `nope`"), "{err}");
    assert!(err.contains("cs_scale, cs_grid, cs_bare"), "{err}");
    assert!(!out.exists(), "nothing is written for a refused build");
}

#[test]
fn entry_option_is_refused_on_a_cpu_target() {
    let o = buildc()
        .arg(example("compute_entries.bld"))
        .args(["--target", "c", "--entry", "cs_scale", "-o"])
        .arg(temp_path("entry_c", "c"))
        .output()
        .expect("run buildc");
    assert!(!o.status.success(), "{}", text(&o));
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("not a shader target"),
        "{}",
        text(&o)
    );
}

/// Each bad attribute is refused with a message that names the problem.
#[test]
fn bad_sizes_are_refused() {
    for (label, attr, needle) in [
        ("zero", "workgroup_size = [8, 0, 1]", "dimension y is 0"),
        (
            "wide",
            "workgroup_size = [2048, 1, 1]",
            "dimension x is 2048",
        ),
        ("deep", "workgroup_size = [1, 1, 65]", "dimension z is 65"),
        ("many", "workgroup_size = [64, 32, 1]", "2048 invocations"),
        ("two_dims", "workgroup_size = [8, 8]", "three dimensions"),
        (
            "twice",
            "workgroup_size = [8, 1, 1], workgroup_size = [4, 1, 1]",
            "given twice",
        ),
        (
            "unknown_key",
            "threads = [8, 1, 1]",
            "no argument `threads`",
        ),
    ] {
        let source = format!(
            "#[compute({attr})]\nfn k(a: &[f32], out: &mut [f32]) ~ Gpu {{\n    let i = gl_GlobalInvocationID.x;\n    out[i] = a[i];\n}}\n"
        );
        let src = write_source(label, &source);
        let out = temp_path(label, "spv");
        let _ = std::fs::remove_file(&out);
        let o = compile_spirv(&src, &out, &[]);
        assert!(
            !o.status.success(),
            "{label}: `{attr}` must be refused\n{}",
            text(&o)
        );
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(err.contains(needle), "{label}: expected `{needle}`\n{err}");
        assert!(
            !out.exists(),
            "{label}: nothing is written for a refused build"
        );
    }
}

#[test]
fn duplicate_entries_are_refused() {
    let source = "#[compute(workgroup_size = [64, 1, 1])]
fn k(a: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    out[i] = a[i];
}

#[compute(workgroup_size = [32, 1, 1])]
fn k(out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    out[i] = 1.0;
}
";
    let src = write_source("dup", source);
    let o = compile_spirv(&src, &temp_path("dup", "spv"), &[]);
    assert!(
        !o.status.success(),
        "two entries named `k` must be refused\n{}",
        text(&o)
    );
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("duplicate shader entry point `k`"), "{err}");
}

#[test]
fn two_stage_attributes_are_refused() {
    let source = "#[compute]
#[fragment]
fn k(a: &[f32], out: &mut [f32]) ~ Gpu {
    let i = gl_GlobalInvocationID.x;
    out[i] = a[i];
}
";
    let src = write_source("two_stages", source);
    let o = compile_spirv(&src, &temp_path("two_stages", "spv"), &[]);
    assert!(!o.status.success(), "{}", text(&o));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("one shader-stage attribute"), "{err}");
}
