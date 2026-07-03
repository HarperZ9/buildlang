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

use buildlang::ast;
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

/// Matmul cross-check dimensions (`m` x `k` x `n`). Kept small + checkable and a
/// multiple of the 2D workgroup size (16) so `div_ceil` group counts cover the
/// grid exactly with no out-of-range invocations. Square so the identity-matrix
/// closed-form sanity (`identity(m) x B == B`) is well-defined.
const MM_M: usize = 64;
const MM_K: usize = 64;
const MM_N: usize = 64;
/// The 2D workgroup size the SPIR-V backend emits for a kernel reading `.y`.
const LOCAL_SIZE_2D: (u32, u32) = (16, 16);

/// The element (scalar) type of a kernel parameter, post the GPU path's
/// F64->F32 coercion boundary. Phase 1 is f32-only on the device; f64 is
/// diagnosed rather than silently coerced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScalarKind {
    F32,
    F64,
    /// An unsigned 32-bit scalar (a shape parameter like matmul's `m`/`k`/`n`),
    /// lowered to a `u32` push-constant member. Distinct from the float kinds so
    /// the CPU-C driver declares it `uint32_t` and packs it as a 4-byte integer.
    U32,
}

/// A single parameter of the discovered `#[compute]` kernel, in declaration
/// order. Drives host binding (buffers) / push constants (scalars) and the
/// CPU-C reference driver's declarations + call argument list.
#[derive(Clone, Debug)]
enum KernelParam {
    /// A by-value scalar (e.g. `alpha: f32`) -> a push constant.
    Scalar { name: String, kind: ScalarKind },
    /// A slice buffer (`&[f32]` read-only, `&mut [f32]` writable) -> an SSBO.
    Buffer {
        name: String,
        writable: bool,
        elem: ScalarKind,
    },
}

/// The signature of the single `#[compute]` kernel in a source file: its entry
/// point name and ordered parameters. Discovered ONCE from the AST so nothing
/// downstream hardcodes `"vec_add"` or a fixed input arity.
#[derive(Clone, Debug)]
struct KernelSig {
    entry: String,
    params: Vec<KernelParam>,
}

impl KernelSig {
    /// Buffer parameters in declaration order (the descriptor bindings 0..N).
    fn buffers(&self) -> impl Iterator<Item = (&str, bool, ScalarKind)> {
        self.params.iter().filter_map(|p| match p {
            KernelParam::Buffer {
                name,
                writable,
                elem,
            } => Some((name.as_str(), *writable, *elem)),
            KernelParam::Scalar { .. } => None,
        })
    }

    /// Scalar parameters in declaration order (the push-constant members).
    fn scalars(&self) -> impl Iterator<Item = (&str, ScalarKind)> {
        self.params.iter().filter_map(|p| match p {
            KernelParam::Scalar { name, kind } => Some((name.as_str(), *kind)),
            KernelParam::Buffer { .. } => None,
        })
    }

    /// Index of the single writable output buffer among the buffer bindings,
    /// or an error if there is not exactly one. Phase 1 elementwise kernels
    /// have exactly one `&mut [f32]` output.
    fn output_buffer_index(&self) -> Result<usize, String> {
        let writable: Vec<usize> = self
            .buffers()
            .enumerate()
            .filter(|(_, (_, w, _))| *w)
            .map(|(i, _)| i)
            .collect();
        match writable.as_slice() {
            [i] => Ok(*i),
            [] => Err("kernel has no `&mut [_]` output buffer".to_string()),
            _ => Err(
                "Phase-1 elementwise GPU path supports exactly one `&mut [_]` output \
                      buffer"
                    .to_string(),
            ),
        }
    }
}

/// Map an AST parameter type to a scalar element kind, if it is (or wraps) a
/// float scalar. Returns `None` for non-float element types.
fn ast_scalar_kind(ty: &ast::Type) -> Option<ScalarKind> {
    if let ast::TypeKind::Path(path) = &ty.kind {
        if let Some(ident) = path.last_ident() {
            return match ident.name.as_ref() {
                "f32" => Some(ScalarKind::F32),
                "f64" => Some(ScalarKind::F64),
                "u32" => Some(ScalarKind::U32),
                _ => None,
            };
        }
    }
    None
}

/// Discover the single `#[compute]` kernel's signature from a source file's
/// AST. Errors if there is not exactly one compute kernel, or a parameter has
/// an unsupported shape (Phase 1: scalar `f32`/`f64`, or `&[f32]`/`&mut [f32]`
/// slices).
fn discover_kernel_sig(source_path: &Path) -> Result<KernelSig, String> {
    let text = std::fs::read_to_string(source_path)
        .map_err(|e| format!("read {}: {e}", source_path.display()))?;
    let source_file = SourceFile::new(source_path.to_string_lossy(), text);
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| format!("lex: {e}"))?;
    let mut parser = Parser::new(&source_file, tokens);
    let module = parser.parse().map_err(|e| format!("parse: {e}"))?;

    let mut found: Option<KernelSig> = None;
    for item in &module.items {
        let ast::ItemKind::Function(f) = &item.kind else {
            continue;
        };
        let is_compute = item.attrs.iter().any(|a| {
            a.path
                .segments
                .first()
                .map(|s| s.ident.name.as_ref() == "compute")
                .unwrap_or(false)
        });
        if !is_compute {
            continue;
        }
        if found.is_some() {
            return Err(
                "more than one `#[compute]` kernel in the file; the GPU path \
                        expects exactly one"
                    .to_string(),
            );
        }

        let entry = f.name.name.to_string();
        let mut params = Vec::with_capacity(f.sig.params.len());
        for p in &f.sig.params {
            let name = match &p.pattern.kind {
                ast::PatternKind::Ident { name, .. } => name.name.to_string(),
                _ => return Err("compute kernel parameters must be simple identifiers".to_string()),
            };
            match &p.ty.kind {
                // A reference to a slice: `&[T]` / `&mut [T]`.
                ast::TypeKind::Ref { mutability, ty, .. }
                    if matches!(ty.kind, ast::TypeKind::Slice(_)) =>
                {
                    let ast::TypeKind::Slice(elem) = &ty.kind else {
                        unreachable!()
                    };
                    let elem = ast_scalar_kind(elem).filter(|k| *k != ScalarKind::U32);
                    let elem = elem.ok_or_else(|| {
                        format!("buffer parameter `{name}` must be a float slice (`&[f32]`)")
                    })?;
                    params.push(KernelParam::Buffer {
                        name,
                        writable: matches!(mutability, ast::Mutability::Mutable),
                        elem,
                    });
                }
                // A by-value float scalar: `alpha: f32`.
                _ => {
                    let kind = ast_scalar_kind(&p.ty).ok_or_else(|| {
                        format!(
                            "parameter `{name}` has an unsupported type for the GPU path \
                             (Phase 1: float scalars or float slices)"
                        )
                    })?;
                    params.push(KernelParam::Scalar { name, kind });
                }
            }
        }
        found = Some(KernelSig { entry, params });
    }

    found.ok_or_else(|| "no `#[compute]` kernel found in the file".to_string())
}

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

/// Build, compile, and run the CPU-C reference driver from the DISCOVERED
/// kernel signature (no hardcoded parameter names, arity, or entry point): it
/// re-declares the slice fat-pointer type + the ambient thread-index variable,
/// embeds the kernel's own C function, declares each buffer as a sized array +
/// slice and each scalar as a plain C value, and loops over the grid calling
/// the kernel once per element with the exact argument list its signature
/// implies. Returns the output vector.
///
/// `buffer_data` maps each buffer parameter (in declaration order) to its
/// initial contents; the writable output's initial contents are zeros.
/// `scalar_vals` maps each scalar parameter to its value.
fn cpu_c_reference(
    sig: &KernelSig,
    kernel_fn_c: &str,
    buffer_data: &[(&str, Vec<f32>)],
    scalar_vals: &[(&str, f32)],
    n: usize,
) -> Result<Vec<f32>, String> {
    // Emit inputs as C initializers.
    let fmt_arr = |v: &[f32]| -> String {
        v.iter()
            .map(|x| format!("{:.9}f", x))
            .collect::<Vec<_>>()
            .join(", ")
    };

    // Declarations: one sized array + slice per buffer, one value per scalar.
    let mut decls = String::new();
    for (name, data) in buffer_data {
        decls.push_str(&format!(
            "    static float {name}_data[{n}] = {{ {init} }};\n",
            name = name,
            n = n,
            init = fmt_arr(data),
        ));
        decls.push_str(&format!(
            "    bl_slice_f32 {name} = {{ {name}_data, {n} }};\n",
            name = name,
            n = n,
        ));
    }
    for (name, val) in scalar_vals {
        decls.push_str(&format!(
            "    float {name} = {val:.9}f;\n",
            name = name,
            val = val
        ));
    }

    // The call argument list, in the kernel's PARAMETER order: a scalar is
    // passed by value, a buffer by address (`&name`, matching the emitted C
    // `bl_slice_f32*` parameter).
    let call_args: Vec<String> = sig
        .params
        .iter()
        .map(|p| match p {
            KernelParam::Scalar { name, .. } => name.clone(),
            KernelParam::Buffer { name, .. } => format!("&{name}"),
        })
        .collect();

    // The output buffer's name (the single writable one) is what we print.
    let out_name = sig
        .buffers()
        .find(|(_, w, _)| *w)
        .map(|(n, _, _)| n.to_string())
        .ok_or_else(|| "kernel has no writable output buffer".to_string())?;

    let driver = format!(
        r#"#include <stdio.h>
#include <stdint.h>
#include <stddef.h>

/* Ambient GPU thread-index the kernel body reads; the driver sets it per step. */
uint32_t buildc_gl_global_invocation_x;

typedef struct {{ float* ptr; size_t len; }} bl_slice_f32;

{kernel}

int main(void) {{
{decls}    for (uint32_t i = 0; i < {n}; ++i) {{
        buildc_gl_global_invocation_x = i;
        {entry}({args});
    }}
    for (size_t i = 0; i < {n}; ++i) {{
        printf("%.9g\n", (double){out}_data[i]);
    }}
    return 0;
}}
"#,
        kernel = kernel_fn_c,
        decls = decls,
        entry = sig.entry,
        args = call_args.join(", "),
        out = out_name,
        n = n,
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

/// True iff the kernel reads `gl_GlobalInvocationID.y` -- i.e. it runs over a 2D
/// grid (matmul). Detected from the source text: the `.y` component read is the
/// same thing the SPIR-V backend keys its 2D workgroup size on. Cheap and does
/// not require re-lowering to MIR.
fn kernel_is_2d(source_path: &Path) -> Result<bool, String> {
    let text = std::fs::read_to_string(source_path)
        .map_err(|e| format!("read {}: {e}", source_path.display()))?;
    Ok(text.contains("gl_GlobalInvocationID.y"))
}

/// Build, compile, and run the CPU-C reference for a 2D matmul kernel over the
/// SAME nested grid the device dispatches: `for gy in 0..m { for gx in 0..n {
/// set the ambient x/y invocation vars; call the kernel } }`. Mirrors the 1D
/// `cpu_c_reference` but with per-buffer sizes, u32 shape scalars, and the
/// 2D loop nest. Returns the flattened `m*n` output.
#[allow(clippy::too_many_arguments)]
fn cpu_c_matmul_reference(
    sig: &KernelSig,
    kernel_fn_c: &str,
    a: &[f32],
    b: &[f32],
    m: usize,
    k: usize,
    n: usize,
) -> Result<Vec<f32>, String> {
    let fmt_arr = |v: &[f32]| -> String {
        v.iter()
            .map(|x| format!("{:.9}f", x))
            .collect::<Vec<_>>()
            .join(", ")
    };

    // Buffer names in declaration order: two read-only inputs + one writable
    // output. The output is sized m*n and starts zeroed.
    let buffers: Vec<(&str, bool)> = sig.buffers().map(|(name, w, _)| (name, w)).collect();
    if buffers.len() != 3 {
        return Err(format!(
            "matmul cross-check expects exactly three buffers (a, b, c); found {}",
            buffers.len()
        ));
    }
    let (a_name, _) = buffers[0];
    let (b_name, _) = buffers[1];
    let (c_name, c_writable) = buffers[2];
    if !c_writable {
        return Err("matmul output buffer (third) must be `&mut [f32]`".to_string());
    }

    // Declarations: sized arrays + fat-pointer slices per buffer, u32 shape
    // scalars. Sizes: a = m*k, b = k*n, c = m*n.
    let mut decls = String::new();
    decls.push_str(&format!(
        "    static float {a_name}_data[{la}] = {{ {ai} }};\n    bl_slice_f32 {a_name} = {{ {a_name}_data, {la} }};\n",
        la = m * k,
        ai = fmt_arr(a),
    ));
    decls.push_str(&format!(
        "    static float {b_name}_data[{lb}] = {{ {bi} }};\n    bl_slice_f32 {b_name} = {{ {b_name}_data, {lb} }};\n",
        lb = k * n,
        bi = fmt_arr(b),
    ));
    decls.push_str(&format!(
        "    static float {c_name}_data[{lc}] = {{ 0 }};\n    bl_slice_f32 {c_name} = {{ {c_name}_data, {lc} }};\n",
        lc = m * n,
    ));
    decls.push_str(&format!(
        "    uint32_t m = {m}; uint32_t k = {k}; uint32_t n = {n};\n"
    ));

    // Call argument list in parameter order: scalars by value, buffers by
    // address (matching the emitted `bl_slice_f32*` parameters).
    let call_args: Vec<String> = sig
        .params
        .iter()
        .map(|p| match p {
            KernelParam::Scalar { name, .. } => name.clone(),
            KernelParam::Buffer { name, .. } => format!("&{name}"),
        })
        .collect();

    let driver = format!(
        r#"#include <stdio.h>
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

/* Ambient GPU thread-index vars the kernel body reads; the driver sets them
   per invocation over the nested 2D grid. */
uint32_t buildc_gl_global_invocation_x;
uint32_t buildc_gl_global_invocation_y;

typedef struct {{ float* ptr; size_t len; }} bl_slice_f32;

{kernel}

int main(void) {{
{decls}    for (uint32_t gy = 0; gy < m; ++gy) {{
        for (uint32_t gx = 0; gx < n; ++gx) {{
            buildc_gl_global_invocation_y = gy;
            buildc_gl_global_invocation_x = gx;
            {entry}({args});
        }}
    }}
    for (size_t i = 0; i < (size_t)m * (size_t)n; ++i) {{
        printf("%.9g\n", (double){out}_data[i]);
    }}
    return 0;
}}
"#,
        kernel = kernel_fn_c,
        decls = decls,
        entry = sig.entry,
        args = call_args.join(", "),
        out = c_name,
    );

    let dir = std::env::temp_dir().join(format!("buildlang_gpu_mm_{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("create temp dir: {e}"))?;
    let c_path = dir.join("cpu_ref_mm.c");
    std::fs::write(&c_path, driver).map_err(|e| format!("write cpu_ref_mm.c: {e}"))?;

    let exe_path = dir.join(if cfg!(windows) {
        "cpu_ref_mm.exe"
    } else {
        "cpu_ref_mm"
    });
    compile_c(&c_path, &exe_path)?;

    let output = std::process::Command::new(&exe_path)
        .output()
        .map_err(|e| format!("run cpu_ref_mm: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cpu_ref_mm exited non-zero: {}",
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
    if values.len() != m * n {
        return Err(format!(
            "cpu_ref_mm produced {} values, expected {}",
            values.len(),
            m * n
        ));
    }
    Ok(values)
}

/// Cross-check a 2D matmul kernel on the physical device against a CPU-C nested
/// loop, PLUS a closed-form correctness sanity: with A = identity(m), the output
/// C must equal B exactly (`identity x B == B`). That proves the kernel computes
/// matmul correctly -- not merely that two lowerings of a possibly-wrong body
/// agree. f32 + 1e-6 tolerance, same sealed receipt as the 1D path.
fn run_matmul_cross_check(
    file: &Path,
    sig: &KernelSig,
    emit_receipt: Option<&Path>,
) -> Result<(), i32> {
    let (m, k, n) = (MM_M, MM_K, MM_N);

    // A = identity(m x k) (square here), so C == B is the closed-form expectation.
    // B = distinct, exactly-representable values so a transposed/mis-indexed
    // kernel would visibly diverge from the identity expectation.
    let mut a = vec![0.0f32; m * k];
    for i in 0..m.min(k) {
        a[i * k + i] = 1.0;
    }
    let b: Vec<f32> = (0..k * n).map(|i| (i as f32) + 1.0).collect();

    // Shape params arrive as u32 push constants, packed in declaration order.
    // Confirm the signature is (m, k, n: u32, a, b: &[f32], c: &mut [f32]).
    let scalar_names: Vec<&str> = sig
        .scalars()
        .map(|(name, kind)| {
            debug_assert_eq!(kind, ScalarKind::U32);
            name
        })
        .collect();
    if scalar_names.len() != 3 {
        eprintln!(
            "GPU: matmul kernel must have exactly three u32 shape scalars (m, k, n); found {}",
            scalar_names.len()
        );
        return Err(1);
    }

    // 1. Compile the kernel to SPIR-V.
    let spirv_bytes = compile_to(file, Target::SpirV).map_err(|e| {
        eprintln!("GPU: failed to compile matmul kernel to SPIR-V: {e}");
        1
    })?;
    let words = bytes_to_words(&spirv_bytes).map_err(|e| {
        eprintln!("GPU: {e}");
        1
    })?;

    // 2. SHAPE VALIDATION (device-free): A = m*k, B = k*n, C = m*n. A mismatch
    //    would read/write out of bounds on the device, so refuse before dispatch.
    let c_zero = vec![0.0f32; m * n];
    vulkan_host::validate_matmul_shapes(m, k, n, a.len(), b.len(), c_zero.len(), LOCAL_SIZE_2D)
        .map_err(|e| {
            eprintln!("GPU: {e}");
            1
        })?;

    let buffer_args = vec![
        vulkan_host::BufferArg {
            data: &a,
            writable: false,
        },
        vulkan_host::BufferArg {
            data: &b,
            writable: false,
        },
        vulkan_host::BufferArg {
            data: &c_zero,
            writable: true,
        },
    ];
    let mut push_bytes: Vec<u8> = Vec::with_capacity(12);
    for v in [m as u32, k as u32, n as u32] {
        push_bytes.extend_from_slice(&v.to_le_bytes());
    }

    let gpu_out = vulkan_host::dispatch_compute(
        &words,
        &sig.entry,
        &buffer_args,
        &push_bytes,
        vulkan_host::Grid::D2 { gx: n, gy: m },
        LOCAL_SIZE_2D,
    )
    .map_err(|e| {
        eprintln!("GPU: matmul device dispatch failed: {e}");
        1
    })?;

    // 3. CPU-C reference over the nested grid, from the SAME kernel body.
    let c_bytes = compile_to(file, Target::C).map_err(|e| {
        eprintln!("GPU: failed to compile matmul kernel to C: {e}");
        1
    })?;
    let c_source = String::from_utf8_lossy(&c_bytes);
    let c_source = normalize_slice_type(&c_source);
    let kernel_fn = extract_c_function(&c_source, &sig.entry).ok_or_else(|| {
        eprintln!(
            "GPU: could not extract the `{}` function from the emitted C",
            sig.entry
        );
        1
    })?;
    let cpu_out = cpu_c_matmul_reference(sig, &kernel_fn, &a, &b, m, k, n).map_err(|e| {
        eprintln!("GPU: CPU-C matmul reference failed: {e}");
        1
    })?;

    // Test hook (can-it-FAIL negative): perturb one readback element so the
    // agreement gate MUST report a mismatch and exit non-zero.
    let mut gpu_out = gpu_out;
    if std::env::var("BUILDLANG_GPU_CORRUPT_READBACK").is_ok() && !gpu_out.is_empty() {
        gpu_out[0] += 1.0;
    }

    let elems = m * n;

    // 3a. CLOSED-FORM CORRECTNESS SANITY: identity(m) x B == B. The CPU-C output
    //     (the SAME body the GPU runs) must reproduce B exactly. This is stronger
    //     than GPU-vs-CPU agreement: it proves the kernel computes matmul, not
    //     that two identical (possibly wrong) lowerings agree. Skipped when the
    //     corrupt-readback hook is active (that hook only perturbs the GPU side).
    if std::env::var("BUILDLANG_GPU_CORRUPT_READBACK").is_err() {
        let mut max_id_dev = 0.0f32;
        for i in 0..elems {
            let dev = (cpu_out[i] - b[i]).abs();
            if dev > max_id_dev {
                max_id_dev = dev;
            }
        }
        if max_id_dev > TOLERANCE {
            eprintln!(
                "matmul identity sanity: FAIL (identity x B != B; max abs deviation \
                 {max_id_dev:.3e} > tol {TOLERANCE:.3e}) -- the kernel does not compute matmul"
            );
            return Err(1);
        }
        println!("matmul identity sanity: PASS (identity({m}) x B == B within tol)");
    }

    // 4. GPU-vs-CPU agreement over the flattened m*n output.
    let mut max_dev = 0.0f32;
    for i in 0..elems {
        let dev = (gpu_out[i] - cpu_out[i]).abs();
        if dev > max_dev {
            max_dev = dev;
        }
    }
    if max_dev <= TOLERANCE {
        println!(
            "gpu-cpu agreement: PASS (matmul {m}x{k}x{n}, N={elems}, max abs deviation \
             {max_dev:.3e} <= tol {TOLERANCE:.3e})"
        );
    } else {
        eprintln!(
            "gpu-cpu agreement: FAIL (matmul {m}x{k}x{n}, N={elems}, max abs deviation \
             {max_dev:.3e} > tol {TOLERANCE:.3e})"
        );
        return Err(1);
    }

    // 5. Layer C: sealed, re-checkable receipt over the flattened series.
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

/// Run the full Layer-B (and, with `emit_receipt`, Layer-C) cross-check for an
/// arbitrary ELEMENTWISE f32 kernel. The kernel's entry name, buffer arity, and
/// scalar params are DISCOVERED from the AST -- nothing is hardcoded to
/// `vec_add`'s single shape.
pub fn run_gpu_cross_check(file: &Path, emit_receipt: Option<&Path>) -> Result<(), i32> {
    // 0. Discover the kernel signature (entry name + ordered params).
    let sig = discover_kernel_sig(file).map_err(|e| {
        eprintln!("GPU: {e}");
        1
    })?;

    // F64 DIAGNOSTIC: the GPU path is f32-only (the F64->F32 coercion pass is
    // on). If the kernel declares any f64 parameter, refuse rather than
    // silently coercing precision away.
    if sig.params.iter().any(|p| {
        matches!(
            p,
            KernelParam::Scalar {
                kind: ScalarKind::F64,
                ..
            } | KernelParam::Buffer {
                elem: ScalarKind::F64,
                ..
            }
        )
    }) {
        eprintln!(
            "GPU: the GPU path is f32; this kernel uses f64. Declare its parameters `f32` \
             (the device path does not run f64 -- values would be silently coerced)."
        );
        return Err(1);
    }

    // Confirm exactly one writable output buffer before doing any work.
    let _out_idx = sig.output_buffer_index().map_err(|e| {
        eprintln!("GPU: {e}");
        1
    })?;

    // 2D branch: a kernel that reads `gl_GlobalInvocationID.y` runs over a 2D
    // grid (matmul). It has a distinct shape (u32 shape push-constants + three
    // differently-sized buffers) and a nested CPU-C driver, so it gets its own
    // cross-check path. Elementwise (1D) kernels fall through below.
    if kernel_is_2d(file).map_err(|e| {
        eprintln!("GPU: {e}");
        1
    })? {
        return run_matmul_cross_check(file, &sig, emit_receipt);
    }

    // 1. Fixed, checkable inputs, derived from the signature. Each read-only
    //    input buffer gets a distinct deterministic fill; each scalar a fixed
    //    value; the output buffer starts zeroed.
    let mut buffer_data: Vec<(&str, Vec<f32>)> = Vec::new();
    for (idx, (name, writable, _elem)) in sig.buffers().enumerate() {
        let data: Vec<f32> = if writable {
            vec![0.0f32; N]
        } else {
            // input k gets [k+1, k+2, ..] offset so buffers differ; keeps values
            // small and exactly representable in f32.
            let base = (idx as f32) * (N as f32);
            (0..N).map(|i| base + (i + 1) as f32).collect()
        };
        buffer_data.push((name, data));
    }
    // Scalars: a fixed, exactly-representable value (2.0) for every scalar.
    let scalar_vals: Vec<(&str, f32)> = sig.scalars().map(|(name, _)| (name, 2.0f32)).collect();

    // 1. Compile the kernel to SPIR-V.
    let spirv_bytes = compile_to(file, Target::SpirV).map_err(|e| {
        eprintln!("GPU: failed to compile kernel to SPIR-V: {e}");
        1
    })?;
    let words = bytes_to_words(&spirv_bytes).map_err(|e| {
        eprintln!("GPU: {e}");
        1
    })?;

    // 2. Dispatch on the physical device: buffers in declaration order (bindings
    //    0..N), scalars packed into the push-constant block at 4-byte offsets in
    //    declaration order (mirrors the SPIR-V push-constant member layout).
    let buffer_args: Vec<vulkan_host::BufferArg<'_>> = sig
        .buffers()
        .zip(buffer_data.iter())
        .map(|((_, writable, _), (_, data))| vulkan_host::BufferArg { data, writable })
        .collect();
    let mut push_bytes: Vec<u8> = Vec::new();
    for (name, kind) in sig.scalars() {
        let val = scalar_vals
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        // Pack each scalar per its SPIR-V push-constant member TYPE, not always
        // as f32. A `u32` shape member must be an integer bit pattern, otherwise
        // the shader reads an f32 bit pattern as `uint` and silently corrupts the
        // value. (Matmul, the only U32-scalar kernel today, is 2D and never
        // reaches here; this keeps the 1D path correct for any future u32 scalar.)
        match kind {
            ScalarKind::U32 => push_bytes.extend_from_slice(&(val as u32).to_le_bytes()),
            ScalarKind::F32 | ScalarKind::F64 => push_bytes.extend_from_slice(&val.to_le_bytes()),
        }
    }
    let gpu_out = vulkan_host::dispatch_compute(
        &words,
        &sig.entry,
        &buffer_args,
        &push_bytes,
        vulkan_host::Grid::D1(N),
        (LOCAL_SIZE_X, 1),
    )
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
    let kernel_fn = extract_c_function(&c_source, &sig.entry).ok_or_else(|| {
        eprintln!(
            "GPU: could not extract the `{}` function from the emitted C",
            sig.entry
        );
        1
    })?;
    let cpu_out =
        cpu_c_reference(&sig, &kernel_fn, &buffer_data, &scalar_vals, N).map_err(|e| {
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
