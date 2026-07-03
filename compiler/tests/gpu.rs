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

fn matmul_example() -> PathBuf {
    repo_root().join("examples").join("gpu").join("matmul.bld")
}

fn stencil_example() -> PathBuf {
    repo_root().join("examples").join("gpu").join("stencil.bld")
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

/// Parse a SPIR-V binary (little-endian 32-bit words) into its word stream.
/// Panics if the module is shorter than the 5-word header or not word-aligned.
fn spv_words(bytes: &[u8]) -> Vec<u32> {
    assert!(
        bytes.len() >= 20 && bytes.len() % 4 == 0,
        "SPIR-V module must be word-aligned and at least a 5-word header"
    );
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// True iff the SPIR-V word stream contains any instruction with the given
/// opcode. Walks the instruction stream by the word-count in each opcode's high
/// 16 bits (SPIR-V encodes `(word_count << 16) | opcode` in word 0 of each
/// instruction), so it inspects only opcode words -- never mistaking an operand
/// that happens to equal `opcode` for an instruction.
fn contains_opcode(words: &[u32], opcode: u16) -> bool {
    // Skip the 5-word module header.
    let mut i = 5usize;
    while i < words.len() {
        let word_count = (words[i] >> 16) as usize;
        let op = (words[i] & 0xFFFF) as u16;
        if word_count == 0 {
            break; // malformed; stop rather than loop forever
        }
        if op == opcode {
            return true;
        }
        i += word_count;
    }
    false
}

/// Count the number of `OpVariable` instructions (opcode 59) whose storage
/// class operand is `Workgroup` (SpvStorageClass::Workgroup == 4). Walks the
/// instruction stream by word-count so it inspects only real instructions.
/// OpVariable layout: word0 = opcode/count, word1 = result type id, word2 =
/// result id, word3 = storage class. Used to prove that two logically distinct
/// `workgroupArray(N)` scratch buffers lower to two DISTINCT workgroup-class
/// variables rather than aliasing a single deduplicated one.
fn count_workgroup_variables(words: &[u32]) -> usize {
    const OP_VARIABLE: u16 = 59;
    const STORAGE_CLASS_WORKGROUP: u32 = 4;
    let mut count = 0usize;
    let mut i = 5usize; // skip 5-word header
    while i < words.len() {
        let word_count = (words[i] >> 16) as usize;
        let op = (words[i] & 0xFFFF) as u16;
        if word_count == 0 {
            break; // malformed; stop rather than loop forever
        }
        if op == OP_VARIABLE && word_count >= 4 && words[i + 3] == STORAGE_CLASS_WORKGROUP {
            count += 1;
        }
        i += word_count;
    }
    count
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

/// Phase 2: the 2D-grid proof kernel `matmul` (three u32 shape push constants +
/// two read-only inputs of differing lengths + one writable output, an inner
/// loop over `kk`, and a 2D grid reading BOTH `.x` and `.y`) emits VALID
/// dispatchable compute SPIR-V. This exercises the per-kernel 2D workgroup size
/// (16x16x1) and the u32-loop-counter signedness reconciliation on the strict
/// SPIR-V typing path.
#[test]
fn matmul_emits_valid_compute_spirv() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping matmul_emits_valid_compute_spirv: spirv-val not on PATH");
        return;
    };
    let out = temp_spv("matmul_valid");
    compile_kernel_spirv(&matmul_example(), &out);

    let (ok, stderr) = spirv_val_ok(tool, &out);
    assert!(
        ok,
        "spirv-val should accept the emitted matmul compute module:\n{stderr}"
    );

    let _ = std::fs::remove_file(&out);
}

/// Phase 3: the 1D stencil proof kernel `blur` (a 3-point blur with CLAMPED
/// edges: one u32 length push constant + one read-only input + one writable
/// output, neighbor reads `a[i-1]`/`a[i+1]`, and a nested `if/else` selecting
/// the clamped boundary value) emits VALID dispatchable compute SPIR-V. This is
/// the device-free proof that the boundary `if/else` inside the `if i < n` guard
/// validates -- the exact structured-control-flow shape the recent fix enables.
#[test]
fn stencil_emits_valid_compute_spirv() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping stencil_emits_valid_compute_spirv: spirv-val not on PATH");
        return;
    };
    let out = temp_spv("stencil_valid");
    compile_kernel_spirv(&stencil_example(), &out);

    let (ok, stderr) = spirv_val_ok(tool, &out);
    assert!(
        ok,
        "spirv-val should accept the emitted stencil compute module:\n{stderr}"
    );

    let _ = std::fs::remove_file(&out);
}

/// NESTED STRUCTURED CONTROL FLOW (Layer A, device-free): a `while` loop nested
/// inside a selection (`if i < 4 { ... while ... }`) AND an `&&`-guarded loop
/// (`if i < n && j < m { ... while ... }`, the matmul in-kernel bounds-guard
/// shape) must both emit SPIR-V that `spirv-val` accepts.
///
/// This is the regression guard for the nested-structured-control-flow defect:
/// the old backend guessed merge/continue targets by branch-following and reused
/// the nested loop header as the outer selection's merge block, producing a
/// module `spirv-val` rejected with "block N branches to the selection construct,
/// but not to the selection header". A pass here proves the dominator/
/// post-dominator-driven structured-CFG reconstruction emits correct nested
/// `OpSelectionMerge`/`OpLoopMerge`.
#[test]
fn loop_in_selection_emits_valid_spirv() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping loop_in_selection_emits_valid_spirv: spirv-val not on PATH");
        return;
    };
    let dir = std::env::temp_dir().join(format!("buildlang_gpu_nestcf_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    // Case 1: a `while` loop nested directly inside a bare `if`.
    let loop_in_if = dir.join("loop_in_if.bld");
    std::fs::write(
        &loop_in_if,
        "#[compute]\n\
         fn k(a: &mut [f32]) ~ Gpu {\n\
        \x20   let i = gl_GlobalInvocationID.x;\n\
        \x20   if i < 4 {\n\
        \x20       let mut s: f32 = 0.0;\n\
        \x20       let mut j: u32 = 0;\n\
        \x20       while j < 3 {\n\
        \x20           s = s + a[i];\n\
        \x20           j = j + 1;\n\
        \x20       }\n\
        \x20       a[i] = s;\n\
        \x20   }\n\
         }\n",
    )
    .expect("write loop_in_if.bld");
    let out1 = dir.join("loop_in_if.spv");
    compile_kernel_spirv(&loop_in_if, &out1);
    let (ok1, stderr1) = spirv_val_ok(tool, &out1);
    assert!(
        ok1,
        "a `while` loop nested inside an `if` must emit valid structured control flow:\n{stderr1}"
    );

    // Case 2: an `&&` short-circuit guarding a nested loop (the matmul shape).
    let and_loop = dir.join("and_loop.bld");
    std::fs::write(
        &and_loop,
        "#[compute]\n\
         fn k(a: &mut [f32]) ~ Gpu {\n\
        \x20   let i = gl_GlobalInvocationID.x;\n\
        \x20   let n: u32 = 4;\n\
        \x20   let m: u32 = 8;\n\
        \x20   if i < n && i < m {\n\
        \x20       let mut s: f32 = 0.0;\n\
        \x20       let mut j: u32 = 0;\n\
        \x20       while j < 3 {\n\
        \x20           s = s + a[i];\n\
        \x20           j = j + 1;\n\
        \x20       }\n\
        \x20       a[i] = s;\n\
        \x20   }\n\
         }\n",
    )
    .expect("write and_loop.bld");
    let out2 = dir.join("and_loop.spv");
    compile_kernel_spirv(&and_loop, &out2);
    let (ok2, stderr2) = spirv_val_ok(tool, &out2);
    assert!(
        ok2,
        "an `&&`-guarded nested loop (the matmul in-kernel bounds-guard shape) must \
         emit valid structured control flow:\n{stderr2}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// NESTED STRUCTURED CONTROL FLOW, the DUAL shape (Layer A, device-free): a
/// selection nested inside a LOOP BODY -- `if-in-while` and `if-else-in-while` --
/// must also emit SPIR-V that `spirv-val` accepts.
///
/// This is the complement of `loop_in_selection_emits_valid_spirv`. That test
/// covers a loop inside a selection (the matmul guard); this one covers a
/// selection inside a loop, which exercises a DIFFERENT structured-CFG path: the
/// loop-body classification (`loop_body_blocks`) must include the inner
/// selection's blocks, and the inner selection's merge must be computed as its
/// own immediate post-dominator (the block both arms reconverge at, which lies
/// INSIDE the loop body and back-edges to the header), NOT the loop merge and NOT
/// the loop header. The old branch-following heuristic mis-nested these too; a
/// pass here proves the post-dominance analysis handles arbitrary nesting in both
/// directions, closing the if-in-while / if-else-in-while coverage gap.
#[test]
fn selection_in_loop_emits_valid_spirv() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping selection_in_loop_emits_valid_spirv: spirv-val not on PATH");
        return;
    };
    let dir = std::env::temp_dir().join(format!("buildlang_gpu_selloop_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    // Case 1: a bare `if` (no else) nested inside a `while` loop body.
    let if_in_while = dir.join("if_in_while.bld");
    std::fs::write(
        &if_in_while,
        "#[compute]\n\
         fn k(a: &mut [f32]) ~ Gpu {\n\
        \x20   let i = gl_GlobalInvocationID.x;\n\
        \x20   let mut s: f32 = 0.0;\n\
        \x20   let mut j: u32 = 0;\n\
        \x20   while j < 8 {\n\
        \x20       if j < 4 {\n\
        \x20           s = s + a[i];\n\
        \x20       }\n\
        \x20       j = j + 1;\n\
        \x20   }\n\
        \x20   a[i] = s;\n\
         }\n",
    )
    .expect("write if_in_while.bld");
    let out1 = dir.join("if_in_while.spv");
    compile_kernel_spirv(&if_in_while, &out1);
    let (ok1, stderr1) = spirv_val_ok(tool, &out1);
    assert!(
        ok1,
        "an `if` (no else) nested inside a `while` loop body must emit valid \
         structured control flow:\n{stderr1}"
    );

    // Case 2: an `if / else` (both arms) nested inside a `while` loop body.
    let if_else_in_while = dir.join("if_else_in_while.bld");
    std::fs::write(
        &if_else_in_while,
        "#[compute]\n\
         fn k(a: &mut [f32]) ~ Gpu {\n\
        \x20   let i = gl_GlobalInvocationID.x;\n\
        \x20   let mut s: f32 = 0.0;\n\
        \x20   let mut j: u32 = 0;\n\
        \x20   while j < 8 {\n\
        \x20       if j < 4 {\n\
        \x20           s = s + a[i];\n\
        \x20       } else {\n\
        \x20           s = s - a[i];\n\
        \x20       }\n\
        \x20       j = j + 1;\n\
        \x20   }\n\
        \x20   a[i] = s;\n\
         }\n",
    )
    .expect("write if_else_in_while.bld");
    let out2 = dir.join("if_else_in_while.spv");
    compile_kernel_spirv(&if_else_in_while, &out2);
    let (ok2, stderr2) = spirv_val_ok(tool, &out2);
    assert!(
        ok2,
        "an `if / else` nested inside a `while` loop body must emit valid \
         structured control flow:\n{stderr2}"
    );

    let _ = std::fs::remove_dir_all(&dir);
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

fn reduce_example() -> PathBuf {
    repo_root().join("examples").join("gpu").join("reduce.bld")
}

fn two_scratch_example() -> PathBuf {
    repo_root()
        .join("examples")
        .join("gpu")
        .join("reduce_two_scratch.bld")
}

/// PHASE 4a ALIASING REGRESSION (Layer A, device-free): a kernel that declares
/// TWO distinct `workgroupArray(64)` scratch buffers of the SAME shape must emit
/// TWO distinct `Workgroup`-class `OpVariable`s -- not one shared/deduplicated
/// variable. If the backend keyed workgroup variables by (element type, length)
/// only, both `scratch_a` and `scratch_b` would silently alias the same physical
/// buffer and corrupt each other. This test compiles such a kernel, asserts
/// spirv-val accepts it, and asserts the module contains exactly TWO
/// workgroup-class variables.
#[test]
fn two_same_shape_workgroup_arrays_do_not_alias() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping two_same_shape_workgroup_arrays_do_not_alias: spirv-val not on PATH");
        return;
    };
    let out = temp_spv("two_scratch");
    compile_kernel_spirv(&two_scratch_example(), &out);

    let (ok, stderr) = spirv_val_ok(tool, &out);
    assert!(
        ok,
        "spirv-val should accept the two-scratch compute module:\n{stderr}"
    );

    let bytes = std::fs::read(&out).expect("read two_scratch spv");
    let words = spv_words(&bytes);
    let workgroup_vars = count_workgroup_variables(&words);
    assert_eq!(
        workgroup_vars, 2,
        "two distinct workgroupArray(64) locals must lower to two distinct \
         Workgroup-class OpVariables (found {workgroup_vars}); if this is 1 the \
         backend aliased two logically independent scratch buffers into one, a \
         silent data-corruption bug"
    );

    let _ = std::fs::remove_file(&out);
}

/// PHASE 4a GATING MILESTONE (Layer A, device-free): the `sum_reduce` tree
/// reduction -- a 64-element WORKGROUP-shared `scratch` array + `workgroupBarrier()`
/// between the load and each collapse step -- emits VALID compute SPIR-V that
/// `spirv-val` ACCEPTS, and that module CONTAINS an `OpControlBarrier` (opcode
/// 224). The `contains_opcode` assertion is load-bearing: it proves the barrier
/// (and thus the shared-memory synchronization) is really emitted, not silently
/// dropped so that a barrier-free module trivially validated.
///
/// This is the shared-memory + barrier MACHINERY validating. It is NOT yet a
/// working reduction on the device (that is Phase 4b: device dispatch + CPU
/// cross-check of the summed per-workgroup partials).
#[test]
fn reduce_emits_valid_compute_spirv() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping reduce_emits_valid_compute_spirv: spirv-val not on PATH");
        return;
    };
    let out = temp_spv("reduce_valid");
    compile_kernel_spirv(&reduce_example(), &out);

    let (ok, stderr) = spirv_val_ok(tool, &out);
    assert!(
        ok,
        "spirv-val should accept the emitted sum_reduce compute module:\n{stderr}"
    );

    // OpControlBarrier == 224. Prove the workgroup barrier is present so a
    // barrier-free module cannot pass this gate by accident.
    let bytes = std::fs::read(&out).expect("read reduce spv");
    let words = spv_words(&bytes);
    assert!(
        contains_opcode(&words, 224),
        "the emitted sum_reduce module must contain an OpControlBarrier (224); \
         if it does not, the workgroup barrier was silently dropped"
    );

    let _ = std::fs::remove_file(&out);
}

/// CAN-IT-FAIL negative for the barrier gate (Layer A, device-free): corrupt the
/// scope operand of the emitted `OpControlBarrier` to an invalid scope constant
/// and assert `spirv-val` REJECTS the result. This proves the gating test's
/// acceptance actually discriminates on the barrier -- a validator that accepted
/// a barrier with a nonsense execution scope would make the positive test
/// vacuous.
#[test]
fn reduce_corrupt_barrier_is_rejected() {
    let Some(tool) = spirv_val_available() else {
        eprintln!("skipping reduce_corrupt_barrier_is_rejected: spirv-val not on PATH");
        return;
    };
    let out = temp_spv("reduce_corrupt_barrier");
    compile_kernel_spirv(&reduce_example(), &out);

    // Pristine module validates first (else the corruption proves nothing).
    let (ok, stderr) = spirv_val_ok(tool, &out);
    assert!(
        ok,
        "pristine reduce module should validate first:\n{stderr}"
    );

    // The barrier's execution scope is a %uint constant id operand of the
    // OpControlBarrier instruction. Rather than hunt the scope constant, corrupt
    // the OpControlBarrier's FIRST operand word in place to a bogus id (0), which
    // makes the instruction reference an undefined id -> spirv-val rejects. We
    // locate the instruction by walking the word stream for opcode 224.
    let mut bytes = std::fs::read(&out).expect("read reduce spv");
    let words = spv_words(&bytes);
    // Find the byte offset of the first operand word of the first OpControlBarrier.
    let mut i = 5usize;
    let mut operand_word_index: Option<usize> = None;
    while i < words.len() {
        let word_count = (words[i] >> 16) as usize;
        let op = (words[i] & 0xFFFF) as u16;
        if word_count == 0 {
            break;
        }
        if op == 224 {
            // Word 0 is the opcode; word 1 is the execution-scope id operand.
            operand_word_index = Some(i + 1);
            break;
        }
        i += word_count;
    }
    let operand_word_index =
        operand_word_index.expect("emitted reduce module must contain an OpControlBarrier");
    // Overwrite the execution-scope id operand with 0 (an undefined id): a valid
    // OpControlBarrier requires its scope to be a defined constant, so this must
    // be rejected.
    let byte_off = operand_word_index * 4;
    bytes[byte_off] = 0;
    bytes[byte_off + 1] = 0;
    bytes[byte_off + 2] = 0;
    bytes[byte_off + 3] = 0;
    let corrupt = temp_spv("reduce_corrupt_barrier_out");
    std::fs::write(&corrupt, &bytes).expect("write corrupt reduce spv");

    let (ok, _stderr) = spirv_val_ok(tool, &corrupt);
    assert!(
        !ok,
        "spirv-val must REJECT a reduce module whose OpControlBarrier scope operand \
         was corrupted; if it accepts, the barrier gate does not discriminate on the barrier"
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

    /// Phase 2: the 2D-grid `matmul` kernel dispatches on the physical device
    /// over a 2D grid (16x16x1 workgroup, div_ceil group counts) and agrees with
    /// the CPU-C nested-loop reference within tolerance. The cross-check ALSO
    /// asserts the closed-form correctness sanity (identity(m) x B == B), so a
    /// PASS proves the kernel computes matmul -- not merely that two lowerings of
    /// the same body agree. Both verdicts must appear in the output.
    #[test]
    fn matmul_gpu_matches_cpu() {
        if !vulkan_device_available() {
            eprintln!("skipping matmul_gpu_matches_cpu: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(matmul_example())
            .arg("--gpu")
            .output()
            .expect("run buildc run --gpu on matmul");
        assert!(
            output.status.success(),
            "buildc run --gpu on matmul should agree with CPU-C within tolerance\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("gpu-cpu agreement") && stdout.contains("PASS"),
            "expected a gpu-cpu agreement verdict in output:\n{stdout}"
        );
        assert!(
            stdout.contains("matmul identity sanity: PASS"),
            "expected the closed-form identity(m) x B == B sanity to PASS:\n{stdout}"
        );
    }

    /// CAN-IT-FAIL negative for the 2D matmul path: a corrupted matmul readback
    /// MUST be caught by the tolerance gate (non-zero exit).
    #[test]
    fn wrong_matmul_result_is_caught() {
        if !vulkan_device_available() {
            eprintln!("skipping wrong_matmul_result_is_caught: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(matmul_example())
            .arg("--gpu")
            .env("BUILDLANG_GPU_CORRUPT_READBACK", "1")
            .output()
            .expect("run buildc run --gpu on matmul with a corrupted readback");
        assert!(
            !output.status.success(),
            "a divergent matmul readback MUST be caught (non-zero exit)\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    /// ARBITRARY-DIM MATMUL (the in-kernel bounds-guard proof): a matmul whose
    /// dimensions are NOT multiples of the 16x16 workgroup (40x40x40, so each axis
    /// over-launches 8 extra invocations under the `div_ceil` grid) dispatches on
    /// the physical device and agrees with the CPU-C reference within 1e-6. The
    /// in-body `if i < m && j < n { ... }` guard makes the over-launched edge
    /// invocations NO-OP, so nothing writes past the exactly-sized C buffer -- the
    /// old workgroup-multiple constraint is gone. The closed-form identity sanity
    /// (identity x B == B) must ALSO pass, proving the guard did not drop any
    /// in-range output. Dims are square (m == k) so identity holds exactly.
    #[test]
    fn matmul_nonmultiple_dims_match_cpu() {
        if !vulkan_device_available() {
            eprintln!("skipping matmul_nonmultiple_dims_match_cpu: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(matmul_example())
            .arg("--gpu")
            // 40 is not a multiple of 16 on either grid axis; the guard must make
            // the over-launched invocations safe.
            .env("BUILDLANG_MM_DIMS", "40x40x40")
            .output()
            .expect("run buildc run --gpu on a non-multiple matmul");
        assert!(
            output.status.success(),
            "a NON-workgroup-multiple matmul must dispatch and agree with CPU-C within \
             tolerance (the in-kernel bounds guard makes the edge invocations safe)\n\
             stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("gpu-cpu agreement") && stdout.contains("PASS"),
            "expected a gpu-cpu agreement verdict for the 40x40x40 matmul:\n{stdout}"
        );
        assert!(
            stdout.contains("40x40x40"),
            "the agreement verdict should report the overridden 40x40x40 dims:\n{stdout}"
        );
        assert!(
            stdout.contains("matmul identity sanity: PASS"),
            "the identity(m) x B == B sanity must still PASS at the non-multiple dim, \
             proving the guard dropped no in-range output:\n{stdout}"
        );
    }

    /// CAN-IT-FAIL for the arbitrary-dim path: a corrupted readback at the
    /// non-multiple dim MUST still be caught (the tolerance gate discriminates
    /// even when the grid over-launches). This is the negative that proves the
    /// non-multiple PASS above is a real agreement, not a gate that never fires.
    #[test]
    fn wrong_nonmultiple_matmul_result_is_caught() {
        if !vulkan_device_available() {
            eprintln!("skipping wrong_nonmultiple_matmul_result_is_caught: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(matmul_example())
            .arg("--gpu")
            .env("BUILDLANG_MM_DIMS", "40x40x40")
            .env("BUILDLANG_GPU_CORRUPT_READBACK", "1")
            .output()
            .expect("run buildc run --gpu on a corrupted non-multiple matmul");
        assert!(
            !output.status.success(),
            "a divergent readback at a non-multiple dim MUST be caught (non-zero exit)\n\
             stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    /// Phase 3: the 1D stencil `blur` kernel (a 3-point clamped blur: one u32
    /// length push constant + one input + one output, neighbor reads) dispatches
    /// on the physical device over a 1D grid and agrees with the CPU-C scalar
    /// loop within tolerance. The cross-check ALSO asserts a closed-form
    /// clamped-edge correctness sanity (on the ramp input `a[i] = i+1`, the
    /// clamped `out[0]` and `out[n-1]` equal their exact formulas), so a PASS
    /// proves the kernel computes the clamped blur -- not merely that two
    /// lowerings of the same body agree. Both verdicts must appear in the output.
    #[test]
    fn stencil_gpu_matches_cpu() {
        if !vulkan_device_available() {
            eprintln!("skipping stencil_gpu_matches_cpu: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(stencil_example())
            .arg("--gpu")
            .output()
            .expect("run buildc run --gpu on stencil");
        assert!(
            output.status.success(),
            "buildc run --gpu on stencil should agree with CPU-C within tolerance\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("gpu-cpu agreement") && stdout.contains("PASS"),
            "expected a gpu-cpu agreement verdict in output:\n{stdout}"
        );
        assert!(
            stdout.contains("stencil clamped-edge sanity: PASS"),
            "expected the closed-form clamped-edge sanity to PASS:\n{stdout}"
        );
    }

    /// Phase 3 BOUNDARY-CORRECTNESS closed-form check on a small KNOWN input
    /// `a = [1,2,3,4,5]` (n = 5). The clamped edges have exact closed forms:
    ///   out[0]   = (a[0] + a[0] + a[1]) / 3 = (1 + 1 + 2) / 3 = 4/3
    ///   out[n-1] = (a[n-2] + a[n-1] + a[n-1]) / 3 = (4 + 5 + 5) / 3 = 14/3
    /// The cross-check emits `stencil boundary out[0]=... out[n-1]=...` so this
    /// test can assert the CLAMPED formula holds exactly (within 1e-6), not just
    /// GPU-vs-CPU agreement. This is the assertion that the clamp -- not an
    /// out-of-range `a[i-1]`/`a[i+1]` read -- produced the edge values.
    #[test]
    fn stencil_clamped_boundary_is_exact() {
        if !vulkan_device_available() {
            eprintln!("skipping stencil_clamped_boundary_is_exact: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(stencil_example())
            .arg("--gpu")
            .env("BUILDLANG_STENCIL_N", "5")
            .output()
            .expect("run buildc run --gpu on stencil with n=5");
        assert!(
            output.status.success(),
            "n=5 stencil should agree + pass the clamped-edge sanity\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse the reported boundary values and assert the CLAMPED closed form.
        let line = stdout
            .lines()
            .find(|l| l.contains("stencil boundary"))
            .unwrap_or_else(|| panic!("expected a `stencil boundary` line:\n{stdout}"));
        // Format: "stencil boundary out[0]=<f> out[n-1]=<f>"
        let parse_after = |key: &str| -> f64 {
            let seg = line
                .split(key)
                .nth(1)
                .unwrap_or_else(|| panic!("missing {key} in: {line}"));
            seg.split_whitespace()
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap_or_else(|e| panic!("parse {key} value from '{line}': {e}"))
        };
        let out0 = parse_after("out[0]=");
        let out_last = parse_after("out[n-1]=");
        let expected_0 = (1.0 + 1.0 + 2.0) / 3.0; // 4/3
        let expected_last = (4.0 + 5.0 + 5.0) / 3.0; // 14/3
        assert!(
            (out0 - expected_0).abs() <= 1e-6,
            "clamped out[0] must be (a[0]+a[0]+a[1])/3 = 4/3 = {expected_0}; got {out0}"
        );
        assert!(
            (out_last - expected_last).abs() <= 1e-6,
            "clamped out[n-1] must be (a[n-2]+a[n-1]+a[n-1])/3 = 14/3 = {expected_last}; got {out_last}"
        );
    }

    /// CAN-IT-FAIL negative for the stencil path: a corrupted stencil readback
    /// MUST be caught by the tolerance gate (non-zero exit). This is the negative
    /// that proves the stencil agreement PASS is a real agreement, not a gate that
    /// never fires.
    #[test]
    fn wrong_stencil_result_is_caught() {
        if !vulkan_device_available() {
            eprintln!("skipping wrong_stencil_result_is_caught: no Vulkan device");
            return;
        }
        let output = buildc()
            .arg("run")
            .arg(stencil_example())
            .arg("--gpu")
            .env("BUILDLANG_GPU_CORRUPT_READBACK", "1")
            .output()
            .expect("run buildc run --gpu on stencil with a corrupted readback");
        assert!(
            !output.status.success(),
            "a divergent stencil readback MUST be caught (non-zero exit)\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    /// F64 REFUSAL (device-free, no `vulkan_device_available` gate): the GPU
    /// path is f32-only. A `#[compute]` kernel that declares an f64 parameter
    /// MUST be refused with a clear diagnostic BEFORE any device dispatch,
    /// rather than silently coercing precision away. The refusal fires in
    /// `run_gpu_cross_check` ahead of the device probe, so this test needs no
    /// Vulkan hardware -- it exercises the diagnostic on any machine with the
    /// `gpu` feature built.
    ///
    /// Without this test the diagnostic could be silently removed by a future
    /// refactor with no failing test.
    #[test]
    fn f64_kernel_is_refused_on_gpu_path() {
        let dir = std::env::temp_dir().join(format!("buildlang_gpu_f64_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let src = dir.join("f64_kernel.bld");
        // `alpha: f64` is a scalar the f32-only GPU path must refuse.
        std::fs::write(
            &src,
            "#[compute]\n\
             fn f64_scale(alpha: f64, a: &[f32], out: &mut [f32]) ~ Gpu {\n\
            \x20   let i = gl_GlobalInvocationID.x;\n\
            \x20   out[i] = a[i];\n\
             }\n",
        )
        .expect("write f64_kernel.bld");

        let output = buildc()
            .arg("run")
            .arg(&src)
            .arg("--gpu")
            .output()
            .expect("run buildc run --gpu on an f64 kernel");
        assert!(
            !output.status.success(),
            "an f64 parameter on the f32-only GPU path MUST be refused (non-zero exit); \
             if it passes, precision would be silently coerced\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("f32") && stderr.contains("f64"),
            "the refusal should name the f32/f64 mismatch; got:\n{stderr}"
        );

        let _ = std::fs::remove_dir_all(&dir);
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
