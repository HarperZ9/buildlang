// ===============================================================================
// BUILDLANG GPU PATH - device dispatch + cross-check (feature "gpu")
// ===============================================================================
// Copyright (c) 2026 Zain Dana Harper. BuildLang Fair-Source License v1.0.
// ===============================================================================
//
//! Layer B/C: compile a `#[compute]` kernel to SPIR-V, dispatch it on the
//! physical Vulkan device, run the SAME kernel body as a CPU-C scalar loop over
//! the same grid, and cross-check the GPU readback against the CPU result within
//! tolerance. Compiled ONLY under `--features gpu`.

pub mod vulkan_host;

use std::path::Path;

use buildlang::codegen::{CodeGenerator, Target};
use buildlang::lexer::{Lexer, SourceFile};
use buildlang::parser::Parser;
use buildlang::types::{TypeChecker, TypeContext};

/// Element count for the canonical cross-check (one invocation per element).
const N: usize = 1024;
/// The kernel's declared workgroup X size (matches the SPIR-V LocalSize default).
const LOCAL_SIZE_X: u32 = 64;
/// Agreement tolerance for the GPU-vs-CPU cross-check.
const TOLERANCE: f32 = 1e-6;

/// Compile `source` to a byte blob for `target` (SPIR-V or C). Returns the raw
/// bytes or a diagnostic string.
fn compile_to(source_path: &Path, target: Target) -> Result<Vec<u8>, String> {
    let text = std::fs::read_to_string(source_path)
        .map_err(|e| format!("read {}: {e}", source_path.display()))?;
    let source_file = SourceFile::new(source_path.to_string_lossy(), text);

    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| format!("lex: {e}"))?;
    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse().map_err(|e| format!("parse: {e}"))?;

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.check_module(&ast);
    if checker.has_errors() {
        let msgs: Vec<String> = checker
            .errors()
            .iter()
            .map(|e| e.error.to_string())
            .collect();
        return Err(format!("type errors: {}", msgs.join("; ")));
    }

    let mut codegen = CodeGenerator::with_source(&ctx, target, source_file.source().into());
    let generated = codegen
        .generate(&ast)
        .map_err(|e| format!("codegen: {e}"))?;
    Ok(generated.data)
}

/// Convert a SPIR-V byte blob to its little-endian u32 word stream.
fn bytes_to_words(bytes: &[u8]) -> Result<Vec<u32>, String> {
    if bytes.len() % 4 != 0 {
        return Err("SPIR-V blob length is not a multiple of 4".to_string());
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

/// Extract the balanced-brace body of the first `static void <name>(` definition
/// in the emitted C. Returns the full function text (signature + body). Used to
/// build the CPU reference driver from the SAME kernel body the C backend
/// lowers, rather than re-implementing the arithmetic.
fn extract_c_function(c_source: &str, name: &str) -> Option<String> {
    let needle = format!("static void {}(", name);
    // Find the DEFINITION (has a body `{`), not the forward declaration (ends `;`).
    let mut search_from = 0;
    loop {
        let rel = c_source[search_from..].find(&needle)?;
        let start = search_from + rel;
        // Locate the first `{` or `;` after the signature.
        let after_sig = &c_source[start..];
        let brace = after_sig.find('{');
        let semi = after_sig.find(';');
        match (brace, semi) {
            (Some(b), Some(s)) if s < b => {
                // Forward declaration; skip past it.
                search_from = start + s + 1;
                continue;
            }
            (Some(b), _) => {
                // Definition: walk balanced braces from `start + b`.
                let body_start = start + b;
                let mut depth = 0i32;
                for (i, ch) in c_source[body_start..].char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                let end = body_start + i + 1;
                                return Some(c_source[start..end].to_string());
                            }
                        }
                        _ => {}
                    }
                }
                return None;
            }
            _ => return None,
        }
    }
}

/// Rewrite the emitted anonymous slice struct type
/// (`struct { float* ptr; size_t len; }`) to the named `bl_slice_f32` so the
/// extracted kernel function's parameters match the driver's typed buffers.
/// Whitespace-tolerant: collapses internal spacing before matching.
fn normalize_slice_type(kernel_fn: &str) -> String {
    // The C backend emits a stable form; match it directly first.
    let exact = "struct { float* ptr; size_t len; }";
    if kernel_fn.contains(exact) {
        return kernel_fn.replace(exact, "bl_slice_f32");
    }
    // Fallback: scan for `struct {` ... `}` spans whose interior mentions
    // `ptr` and `len`, and replace them. Emitted C is ASCII, so byte-indexed
    // stepping is safe.
    let mut result = String::with_capacity(kernel_fn.len());
    let mut i = 0;
    while i < kernel_fn.len() {
        if kernel_fn[i..].starts_with("struct") {
            if let Some(open_rel) = kernel_fn[i..].find('{') {
                let open = i + open_rel;
                let mut depth = 0i32;
                let mut close = None;
                for (j, ch) in kernel_fn[open..].char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                close = Some(open + j);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(close) = close {
                    let interior = &kernel_fn[open + 1..close];
                    if interior.contains("ptr") && interior.contains("len") {
                        result.push_str("bl_slice_f32");
                        i = close + 1;
                        continue;
                    }
                }
            }
        }
        result.push_str(&kernel_fn[i..i + 1]);
        i += 1;
    }
    result
}

/// Build, compile, and run the CPU-C reference driver: it re-declares the slice
/// fat-pointer type + the ambient thread-index variable, embeds the kernel's own
/// C function, and loops over the grid calling it once per element. Returns the
/// output vector.
fn cpu_c_reference(kernel_fn_c: &str, a: &[f32], b: &[f32], n: usize) -> Result<Vec<f32>, String> {
    // Emit inputs as C initializers.
    let fmt_arr = |v: &[f32]| -> String {
        v.iter()
            .map(|x| format!("{:.9}f", x))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let driver = format!(
        r#"#include <stdio.h>
#include <stdint.h>
#include <stddef.h>

/* Ambient GPU thread-index the kernel body reads; the driver sets it per step. */
uint32_t buildc_gl_global_invocation_x;

typedef struct {{ float* ptr; size_t len; }} bl_slice_f32;

{kernel}

int main(void) {{
    static float a_data[{n}] = {{ {a} }};
    static float b_data[{n}] = {{ {b} }};
    static float out_data[{n}];
    bl_slice_f32 a = {{ a_data, {n} }};
    bl_slice_f32 b = {{ b_data, {n} }};
    bl_slice_f32 out = {{ out_data, {n} }};
    for (uint32_t i = 0; i < {n}; ++i) {{
        buildc_gl_global_invocation_x = i;
        vec_add(&a, &b, &out);
    }}
    for (size_t i = 0; i < {n}; ++i) {{
        printf("%.9g\n", (double)out_data[i]);
    }}
    return 0;
}}
"#,
        kernel = kernel_fn_c,
        n = n,
        a = fmt_arr(a),
        b = fmt_arr(b),
    );

    // The emitted kernel declares its own anonymous-struct slice params
    // (`struct {{ float* ptr; size_t len; }}*`), which is layout-compatible with
    // `bl_slice_f32*`; C lets us pass `&a` because the call is by pointer and the
    // struct layouts match. If the compiler complains about the anonymous type,
    // that is a hard error surfaced to the user.
    let dir = std::env::temp_dir().join(format!("buildlang_gpu_cpu_{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create temp dir: {e}"))?;
    let c_path = dir.join("cpu_ref.c");
    std::fs::write(&c_path, driver).map_err(|e| format!("write cpu_ref.c: {e}"))?;

    let exe_path = dir.join(if cfg!(windows) {
        "cpu_ref.exe"
    } else {
        "cpu_ref"
    });
    compile_c(&c_path, &exe_path)?;

    let output = std::process::Command::new(&exe_path)
        .output()
        .map_err(|e| format!("run cpu_ref: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cpu_ref exited non-zero: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let values: Vec<f32> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.trim()
                .parse::<f32>()
                .map_err(|e| format!("parse cpu output '{l}': {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() != n {
        return Err(format!(
            "cpu_ref produced {} values, expected {}",
            values.len(),
            n
        ));
    }
    Ok(values)
}

/// Compile a single C file to an executable, reusing the compiler's own
/// toolchain discovery (`find_c_compiler` + `invoke_c_compiler`) so the CPU
/// reference is built with the SAME C compiler `buildc run` uses (including MSVC
/// auto-discovery on Windows).
fn compile_c(c_path: &Path, exe_path: &Path) -> Result<(), String> {
    let compiler = crate::find_c_compiler()
        .ok_or_else(|| "no C compiler found to build the CPU reference".to_string())?;
    crate::invoke_c_compiler(&compiler, c_path, exe_path, false, &[])
        .map_err(|code| format!("C compiler failed (exit {code}) building the CPU reference"))
}

/// Run the full Layer-B (and, with `emit_receipt`, Layer-C) cross-check.
pub fn run_gpu_cross_check(file: &Path, emit_receipt: Option<&Path>) -> Result<(), i32> {
    // Fixed, checkable inputs: a = [1, 2, .. N], b = [N, N-1, .. 1].
    let a: Vec<f32> = (0..N).map(|i| (i + 1) as f32).collect();
    let b: Vec<f32> = (0..N).map(|i| (N - i) as f32).collect();

    // 1. Compile the kernel to SPIR-V.
    let spirv_bytes = compile_to(file, Target::SpirV).map_err(|e| {
        eprintln!("GPU: failed to compile kernel to SPIR-V: {e}");
        1
    })?;
    let words = bytes_to_words(&spirv_bytes).map_err(|e| {
        eprintln!("GPU: {e}");
        1
    })?;

    // 2. Dispatch on the physical device.
    let gpu_out = vulkan_host::dispatch_vec_add(&words, "vec_add", &[&a, &b], N, LOCAL_SIZE_X)
        .map_err(|e| {
            eprintln!("GPU: device dispatch failed: {e}");
            1
        })?;

    // 3. CPU-C reference over the same grid, from the SAME kernel body.
    let c_bytes = compile_to(file, Target::C).map_err(|e| {
        eprintln!("GPU: failed to compile kernel to C for the cross-check: {e}");
        1
    })?;
    let c_source = String::from_utf8_lossy(&c_bytes);
    // Rewrite the anonymous slice struct to the named `bl_slice_f32` BEFORE
    // extraction: the anonymous `struct { .. }` in the parameter list contains
    // braces that would otherwise confuse the balanced-brace body extractor.
    let c_source = normalize_slice_type(&c_source);
    let kernel_fn = extract_c_function(&c_source, "vec_add").ok_or_else(|| {
        eprintln!("GPU: could not extract the `vec_add` function from the emitted C");
        1
    })?;
    let cpu_out = cpu_c_reference(&kernel_fn, &a, &b, N).map_err(|e| {
        eprintln!("GPU: CPU-C reference failed: {e}");
        1
    })?;

    // Test hook (can-it-FAIL negative): when BUILDLANG_GPU_CORRUPT_READBACK is
    // set, perturb one readback element so the cross-check MUST report a mismatch
    // and exit non-zero. This proves the tolerance gate discriminates -- a gate
    // that always passes is not a gate. Never triggered in normal use.
    let mut gpu_out = gpu_out;
    if std::env::var("BUILDLANG_GPU_CORRUPT_READBACK").is_ok() && !gpu_out.is_empty() {
        gpu_out[0] += 1.0;
    }

    // 4. Cross-check element-wise.
    let mut max_dev = 0.0f32;
    for i in 0..N {
        let dev = (gpu_out[i] - cpu_out[i]).abs();
        if dev > max_dev {
            max_dev = dev;
        }
    }

    if max_dev <= TOLERANCE {
        println!(
            "gpu-cpu agreement: PASS (N={N}, max abs deviation {max_dev:.3e} <= tol {TOLERANCE:.3e})"
        );
    } else {
        eprintln!(
            "gpu-cpu agreement: FAIL (N={N}, max abs deviation {max_dev:.3e} > tol {TOLERANCE:.3e})"
        );
        return Err(1);
    }

    // 5. Layer C: sealed, re-checkable receipt.
    if let Some(receipt_path) = emit_receipt {
        crate::gpu_receipt::emit_gpu_receipt(
            file,
            receipt_path,
            &gpu_out,
            &cpu_out,
            max_dev,
            TOLERANCE,
        )
        .map_err(|e| {
            eprintln!("GPU: failed to emit receipt: {e}");
            1
        })?;
        println!("wrote gpu receipt to {}", receipt_path.display());
    }

    Ok(())
}
