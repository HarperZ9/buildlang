// ===============================================================================
// BUILDLANG CODE GENERATOR
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! # Code Generation
//!
//! This module implements code generation for BuildLang, transforming
//! type-checked AST into executable code through multiple backends.
//!
//! ## Architecture
//!
//! The code generator uses a multi-stage lowering approach:
//!
//! ```text
//! AST -> MIR (Mid-level IR) -> Backend-specific output
//! ```
//!
//! ## Unwrap Policy
//!
//! Code generation operates on ASTs that have already been parsed, resolved,
//! and type-checked. `.unwrap()` calls in codegen are assertions that the
//! type checker's guarantees hold - an unwrap failure here indicates a
//! compiler bug in an earlier phase, not malformed user input.
//!
//! This is consistent with how `rustc`, `cranelift`, and other production
//! compilers handle post-validation code generation.
//!
//! ## Supported Backends
//!
//! - **C**: Transpiles to C99 for maximum portability (production)
//! - **x86-64**: Native machine code for x86-64 processors (experimental)
//! - **ARM64**: Native machine code for ARM64/AArch64 processors (experimental)
//! - **WASM**: WebAssembly for web and edge deployment (experimental)
//! - **SPIR-V**: GPU shaders and compute kernels (experimental)
//!
//! ## Example
//!
//! ```rust,ignore
//! use buildlang::codegen::{CodeGenerator, Target};
//! use buildlang::types::TypeContext;
//!
//! let mut ctx = TypeContext::new();
//! let mut codegen = CodeGenerator::new(&ctx, Target::C);
//! let output = codegen.generate(&module)?;
//! ```

pub mod backend;
pub mod builder;
pub mod debug;
pub mod ir;
pub mod lower;
pub mod runtime;

pub use backend::{Backend, CodegenError, CodegenResult, Target};
pub use builder::*;
pub use ir::*;
pub use lower::*;

use std::sync::Arc;

use crate::ast;
use crate::types::TypeContext;

/// The main code generator.
pub struct CodeGenerator<'ctx> {
    /// Type context from type checking.
    ctx: &'ctx TypeContext,
    /// The target backend.
    target: Target,
    /// Generated MIR.
    mir: Option<MirModule>,
    /// Source code for macro expansion.
    source: Option<Arc<str>>,
    /// Generate ReShade .fx boilerplate for HLSL target.
    pub reshade: bool,
}

impl<'ctx> CodeGenerator<'ctx> {
    /// Create a new code generator.
    pub fn new(ctx: &'ctx TypeContext, target: Target) -> Self {
        Self {
            ctx,
            target,
            mir: None,
            source: None,
            reshade: false,
        }
    }

    /// Create a new code generator with source code for macro expansion.
    pub fn with_source(ctx: &'ctx TypeContext, target: Target, source: Arc<str>) -> Self {
        Self {
            ctx,
            target,
            mir: None,
            source: Some(source),
            reshade: false,
        }
    }

    /// Generate code from a type-checked module.
    pub fn generate(&mut self, module: &ast::Module) -> CodegenResult<GeneratedCode> {
        // Lower AST to MIR
        let lowerer = if let Some(ref source) = self.source {
            MirLowerer::with_source(self.ctx, source.clone())
        } else {
            MirLowerer::new(self.ctx)
        };
        let mir = lowerer.lower_module(module)?;
        self.mir = Some(mir);

        // Select backend and generate
        let mir = self.mir.as_ref().unwrap();

        match self.target {
            Target::C => {
                let mut backend = backend::c::CBackend::new();
                backend.generate(mir)
            }
            Target::X86_64 => {
                let mut backend = backend::x86_64::X86_64Backend::new();
                backend.generate(mir)
            }
            Target::Arm64 => {
                let mut backend = backend::arm64::Arm64Backend::new();
                backend.generate(mir)
            }
            Target::Wasm => {
                let mut backend = backend::wasm::WasmBackend::new();
                backend.generate(mir)
            }
            Target::SpirV => {
                let mut backend = backend::spirv::SpirvBackend::new();
                backend.generate(mir)
            }
            Target::LlvmIr => {
                let mut backend = backend::llvm::LlvmBackend::new();
                backend.generate(mir)
            }
            Target::Hlsl => {
                let mut backend = backend::hlsl::HlslBackend::new();
                let hlsl_code = if self.reshade {
                    backend.generate_reshade(mir)?
                } else {
                    backend.generate(mir)?
                };
                Ok(GeneratedCode::new(
                    OutputFormat::Hlsl,
                    hlsl_code.into_bytes(),
                ))
            }
            Target::Glsl => {
                let mut backend = backend::glsl::GlslBackend::new();
                let glsl_code = backend.generate(mir)?;
                Ok(GeneratedCode::new(
                    OutputFormat::Glsl,
                    glsl_code.into_bytes(),
                ))
            }
            Target::Rust => {
                let mut backend = backend::rust::RustBackend::new();
                backend.generate(mir)
            }
        }
    }

    /// Get the generated MIR (for debugging/inspection).
    pub fn mir(&self) -> Option<&MirModule> {
        self.mir.as_ref()
    }

    /// Generate a C header declaring the last-generated module's `extern "C"`
    /// exports. Returns `None` if no module has been generated yet.
    pub fn c_export_header(&self) -> Option<String> {
        let mir = self.mir.as_ref()?;
        Some(backend::c::CBackend::new().generate_c_header(mir))
    }
}

/// Generated code output.
#[derive(Debug)]
pub struct GeneratedCode {
    /// The output format.
    pub format: OutputFormat,
    /// The generated code/data.
    pub data: Vec<u8>,
    /// Optional debug information.
    pub debug_info: Option<DebugInfo>,
    /// Libraries this output must be linked against, from extern blocks'
    /// `link "..."` clauses. The build driver passes these to the C compiler.
    /// Empty unless the program declares foreign libraries.
    pub link_libraries: Vec<String>,
}

impl GeneratedCode {
    /// Create new generated code.
    pub fn new(format: OutputFormat, data: Vec<u8>) -> Self {
        Self {
            format,
            data,
            debug_info: None,
            link_libraries: Vec::new(),
        }
    }

    /// Add debug information.
    pub fn with_debug_info(mut self, debug_info: DebugInfo) -> Self {
        self.debug_info = Some(debug_info);
        self
    }

    /// Attach the libraries this output must be linked against.
    pub fn with_link_libraries(mut self, link_libraries: Vec<String>) -> Self {
        self.link_libraries = link_libraries;
        self
    }

    /// Get the code as a string (for text formats).
    pub fn as_string(&self) -> Option<String> {
        match self.format {
            OutputFormat::CSource
            | OutputFormat::Assembly
            | OutputFormat::Wat
            | OutputFormat::LlvmIr
            | OutputFormat::Hlsl
            | OutputFormat::Glsl
            | OutputFormat::RustSource => String::from_utf8(self.data.clone()).ok(),
            _ => None,
        }
    }
}

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// C source code.
    CSource,
    /// Assembly source.
    Assembly,
    /// Object file.
    Object,
    /// Executable.
    Executable,
    /// WebAssembly binary.
    Wasm,
    /// WebAssembly text format (WAT).
    Wat,
    /// SPIR-V binary.
    SpirV,
    /// LLVM IR text format.
    LlvmIr,
    /// HLSL source code.
    Hlsl,
    /// GLSL source code.
    Glsl,
    /// Rust source code.
    RustSource,
}

/// Debug information for generated code.
#[derive(Debug, Clone)]
pub struct DebugInfo {
    /// Source file mappings.
    pub source_maps: Vec<SourceMap>,
}

/// Source map entry.
#[derive(Debug, Clone)]
pub struct SourceMap {
    /// Generated code offset.
    pub generated_offset: usize,
    /// Original source file.
    pub source_file: String,
    /// Original line number.
    pub line: u32,
    /// Original column.
    pub column: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // GeneratedCode Tests
    // =========================================================================

    #[test]
    fn test_generated_code_new() {
        let code = GeneratedCode::new(OutputFormat::CSource, b"int main() { return 0; }".to_vec());
        assert_eq!(code.format, OutputFormat::CSource);
        assert_eq!(code.data, b"int main() { return 0; }");
        assert!(code.debug_info.is_none());
    }

    #[test]
    fn test_generated_code_as_string() {
        let code = GeneratedCode::new(OutputFormat::CSource, b"int main() { return 0; }".to_vec());
        assert_eq!(
            code.as_string(),
            Some("int main() { return 0; }".to_string())
        );
    }

    #[test]
    fn test_generated_code_as_string_assembly() {
        let code = GeneratedCode::new(OutputFormat::Assembly, b"mov rax, 42\nret".to_vec());
        assert_eq!(code.as_string(), Some("mov rax, 42\nret".to_string()));
    }

    #[test]
    fn test_generated_code_as_string_wat() {
        let code = GeneratedCode::new(
            OutputFormat::Wat,
            b"(module (func (export \"main\")))".to_vec(),
        );
        assert_eq!(
            code.as_string(),
            Some("(module (func (export \"main\")))".to_string())
        );
    }

    #[test]
    fn test_generated_code_as_string_rust_source() {
        let code = GeneratedCode::new(OutputFormat::RustSource, b"fn main() {}\n".to_vec());
        assert_eq!(code.as_string(), Some("fn main() {}\n".to_string()));
    }

    #[test]
    fn test_generated_code_as_string_binary_formats() {
        // Binary formats should return None
        let wasm = GeneratedCode::new(OutputFormat::Wasm, vec![0x00, 0x61, 0x73, 0x6d]);
        assert!(wasm.as_string().is_none());

        let object = GeneratedCode::new(OutputFormat::Object, vec![0x7f, 0x45, 0x4c, 0x46]);
        assert!(object.as_string().is_none());

        let executable = GeneratedCode::new(OutputFormat::Executable, vec![0x4d, 0x5a]);
        assert!(executable.as_string().is_none());

        let spirv = GeneratedCode::new(OutputFormat::SpirV, vec![0x03, 0x02, 0x23, 0x07]);
        assert!(spirv.as_string().is_none());
    }

    #[test]
    fn test_generated_code_as_string_invalid_utf8() {
        let code = GeneratedCode::new(
            OutputFormat::CSource,
            vec![0xff, 0xfe, 0x00, 0x01], // Invalid UTF-8
        );
        assert!(code.as_string().is_none());
    }

    #[test]
    fn test_generated_code_with_debug_info() {
        let debug_info = DebugInfo {
            source_maps: vec![SourceMap {
                generated_offset: 0,
                source_file: "main.qta".to_string(),
                line: 1,
                column: 0,
            }],
        };

        let code = GeneratedCode::new(OutputFormat::CSource, b"int main() {}".to_vec())
            .with_debug_info(debug_info);

        assert!(code.debug_info.is_some());
        let info = code.debug_info.unwrap();
        assert_eq!(info.source_maps.len(), 1);
        assert_eq!(info.source_maps[0].source_file, "main.qta");
    }

    #[test]
    fn test_generated_code_empty() {
        let code = GeneratedCode::new(OutputFormat::CSource, vec![]);
        assert!(code.data.is_empty());
        assert_eq!(code.as_string(), Some(String::new()));
    }

    // =========================================================================
    // OutputFormat Tests
    // =========================================================================

    #[test]
    fn test_output_format_equality() {
        assert_eq!(OutputFormat::CSource, OutputFormat::CSource);
        assert_eq!(OutputFormat::Assembly, OutputFormat::Assembly);
        assert_eq!(OutputFormat::Object, OutputFormat::Object);
        assert_eq!(OutputFormat::Executable, OutputFormat::Executable);
        assert_eq!(OutputFormat::Wasm, OutputFormat::Wasm);
        assert_eq!(OutputFormat::Wat, OutputFormat::Wat);
        assert_eq!(OutputFormat::SpirV, OutputFormat::SpirV);
        assert_eq!(OutputFormat::RustSource, OutputFormat::RustSource);
    }

    #[test]
    fn test_output_format_inequality() {
        assert_ne!(OutputFormat::CSource, OutputFormat::Assembly);
        assert_ne!(OutputFormat::Wasm, OutputFormat::Wat);
        assert_ne!(OutputFormat::Object, OutputFormat::Executable);
    }

    #[test]
    fn test_output_format_clone() {
        let format = OutputFormat::CSource;
        let cloned = format.clone();
        assert_eq!(format, cloned);
    }

    #[test]
    fn test_output_format_copy() {
        let format = OutputFormat::Assembly;
        let copied = format;
        assert_eq!(format, copied); // format still usable because Copy
    }

    #[test]
    fn test_output_format_debug() {
        assert_eq!(format!("{:?}", OutputFormat::CSource), "CSource");
        assert_eq!(format!("{:?}", OutputFormat::Assembly), "Assembly");
        assert_eq!(format!("{:?}", OutputFormat::Object), "Object");
        assert_eq!(format!("{:?}", OutputFormat::Executable), "Executable");
        assert_eq!(format!("{:?}", OutputFormat::Wasm), "Wasm");
        assert_eq!(format!("{:?}", OutputFormat::Wat), "Wat");
        assert_eq!(format!("{:?}", OutputFormat::SpirV), "SpirV");
        assert_eq!(format!("{:?}", OutputFormat::RustSource), "RustSource");
    }

    // =========================================================================
    // DebugInfo Tests
    // =========================================================================

    #[test]
    fn test_debug_info_new() {
        let debug_info = DebugInfo {
            source_maps: vec![],
        };
        assert!(debug_info.source_maps.is_empty());
    }

    #[test]
    fn test_debug_info_with_source_maps() {
        let debug_info = DebugInfo {
            source_maps: vec![
                SourceMap {
                    generated_offset: 0,
                    source_file: "lib.qta".to_string(),
                    line: 1,
                    column: 0,
                },
                SourceMap {
                    generated_offset: 100,
                    source_file: "lib.qta".to_string(),
                    line: 10,
                    column: 4,
                },
                SourceMap {
                    generated_offset: 200,
                    source_file: "util.qta".to_string(),
                    line: 5,
                    column: 8,
                },
            ],
        };
        assert_eq!(debug_info.source_maps.len(), 3);
    }

    #[test]
    fn test_debug_info_clone() {
        let debug_info = DebugInfo {
            source_maps: vec![SourceMap {
                generated_offset: 42,
                source_file: "test.qta".to_string(),
                line: 5,
                column: 10,
            }],
        };
        let cloned = debug_info.clone();
        assert_eq!(cloned.source_maps.len(), 1);
        assert_eq!(cloned.source_maps[0].generated_offset, 42);
    }

    // =========================================================================
    // SourceMap Tests
    // =========================================================================

    #[test]
    fn test_source_map_new() {
        let map = SourceMap {
            generated_offset: 256,
            source_file: "main.qta".to_string(),
            line: 42,
            column: 8,
        };
        assert_eq!(map.generated_offset, 256);
        assert_eq!(map.source_file, "main.qta");
        assert_eq!(map.line, 42);
        assert_eq!(map.column, 8);
    }

    #[test]
    fn test_source_map_clone() {
        let map = SourceMap {
            generated_offset: 100,
            source_file: "test.qta".to_string(),
            line: 1,
            column: 0,
        };
        let cloned = map.clone();
        assert_eq!(cloned.generated_offset, map.generated_offset);
        assert_eq!(cloned.source_file, map.source_file);
        assert_eq!(cloned.line, map.line);
        assert_eq!(cloned.column, map.column);
    }

    #[test]
    fn test_source_map_debug() {
        let map = SourceMap {
            generated_offset: 0,
            source_file: "x.qta".to_string(),
            line: 1,
            column: 0,
        };
        let debug = format!("{:?}", map);
        assert!(debug.contains("SourceMap"));
        assert!(debug.contains("generated_offset"));
        assert!(debug.contains("x.qta"));
    }

    // =========================================================================
    // CodeGenerator Tests
    // =========================================================================

    #[test]
    fn test_code_generator_new() {
        let ctx = TypeContext::new();
        let codegen = CodeGenerator::new(&ctx, Target::C);
        assert_eq!(codegen.target, Target::C);
        assert!(codegen.mir.is_none());
    }

    #[test]
    fn test_code_generator_new_all_targets() {
        let ctx = TypeContext::new();

        let cg_c = CodeGenerator::new(&ctx, Target::C);
        assert_eq!(cg_c.target, Target::C);

        let cg_x86 = CodeGenerator::new(&ctx, Target::X86_64);
        assert_eq!(cg_x86.target, Target::X86_64);

        let cg_arm = CodeGenerator::new(&ctx, Target::Arm64);
        assert_eq!(cg_arm.target, Target::Arm64);

        let cg_wasm = CodeGenerator::new(&ctx, Target::Wasm);
        assert_eq!(cg_wasm.target, Target::Wasm);

        let cg_spirv = CodeGenerator::new(&ctx, Target::SpirV);
        assert_eq!(cg_spirv.target, Target::SpirV);

        let cg_rust = CodeGenerator::new(&ctx, Target::Rust);
        assert_eq!(cg_rust.target, Target::Rust);
    }

    #[test]
    fn test_code_generator_mir_initially_none() {
        let ctx = TypeContext::new();
        let codegen = CodeGenerator::new(&ctx, Target::C);
        assert!(codegen.mir().is_none());
    }

    // =========================================================================
    // Target Tests
    // =========================================================================

    #[test]
    fn test_target_variants() {
        let targets = [
            Target::C,
            Target::X86_64,
            Target::Arm64,
            Target::Wasm,
            Target::SpirV,
            Target::Rust,
        ];
        assert_eq!(targets.len(), 6);
    }

    #[test]
    fn test_target_equality() {
        assert_eq!(Target::C, Target::C);
        assert_eq!(Target::X86_64, Target::X86_64);
        assert_ne!(Target::C, Target::X86_64);
        assert_ne!(Target::Wasm, Target::SpirV);
        assert_eq!(Target::Rust.extension(), "rs");
        assert_eq!(Target::Rust.to_string(), "rust");
        assert_eq!(Target::Rust.pointer_size(), 64);
    }

    // =========================================================================
    // Integration Tests
    // =========================================================================

    #[test]
    fn test_full_pipeline_empty_module() {
        // Test the full pipeline with an empty module
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::CSource);

        // Check that MIR was generated
        assert!(codegen.mir().is_some());
    }

    #[test]
    fn extern_header_clause_lowers_to_mir_link_header() {
        // A `header` clause on an extern block must survive lowering so the
        // backend knows which foreign declarations are backed by a real header.
        let module = crate::parser::parse_source(
            "test.bld",
            "extern \"C\" header \"sqlite3.h\" { fn sqlite3_libversion() -> i32; }",
        )
        .expect("source should parse");

        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);
        codegen.generate(&module).expect("codegen should succeed");

        let mir = codegen.mir().expect("mir should be present");
        let decl = mir
            .functions
            .iter()
            .find(|f| &*f.name == "sqlite3_libversion")
            .expect("foreign declaration should be lowered");
        assert_eq!(decl.link_header.as_deref(), Some("sqlite3.h"));
    }

    #[test]
    fn extern_link_clause_lowers_to_mir_link_lib() {
        // A `link` clause must survive lowering so the build driver knows which
        // libraries to pass to the C compiler.
        let module = crate::parser::parse_source(
            "test.bld",
            "extern \"C\" link \"sqlite3\" header \"<sqlite3.h>\" { fn sqlite3_libversion() -> i32; }",
        )
        .expect("source should parse");

        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);
        codegen.generate(&module).expect("codegen should succeed");

        let mir = codegen.mir().expect("mir should be present");
        let decl = mir
            .functions
            .iter()
            .find(|f| &*f.name == "sqlite3_libversion")
            .expect("foreign declaration should be lowered");
        assert_eq!(decl.link_lib.as_deref(), Some("sqlite3"));
        assert_eq!(decl.link_header.as_deref(), Some("<sqlite3.h>"));
    }

    #[test]
    fn extern_static_lowers_to_external_global_with_header() {
        // A foreign `static` must lower to an external-declaration global that
        // carries the block's header, so the backend includes it instead of
        // emitting a conflicting definition.
        let module = crate::parser::parse_source(
            "test.bld",
            "extern \"C\" header \"<errno.h>\" link \"c\" { static errno: i32; }",
        )
        .expect("source should parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);
        codegen.generate(&module).expect("codegen should succeed");
        let mir = codegen.mir().expect("mir should be present");
        let g = mir
            .globals
            .iter()
            .find(|g| &*g.name == "errno")
            .expect("foreign static should be lowered to a global");
        assert!(
            g.is_extern_decl,
            "foreign static should be an external declaration"
        );
        assert_eq!(g.link_header.as_deref(), Some("<errno.h>"));
        assert_eq!(g.link_lib.as_deref(), Some("c"));
    }

    #[test]
    fn extern_without_header_has_no_link_header() {
        let module =
            crate::parser::parse_source("test.bld", "extern \"C\" { fn puts(s: &str) -> i32; }")
                .expect("source should parse");

        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);
        codegen.generate(&module).expect("codegen should succeed");

        let mir = codegen.mir().expect("mir should be present");
        let decl = mir
            .functions
            .iter()
            .find(|f| &*f.name == "puts")
            .expect("foreign declaration should be lowered");
        assert_eq!(decl.link_header, None);
    }

    /// Compile BuildLang source to C text through the full lowering pipeline.
    fn source_to_c(src: &str) -> String {
        let module = crate::parser::parse_source("test.bld", src).expect("source should parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);
        let out = codegen.generate(&module).expect("codegen should succeed");
        out.as_string().expect("C output should be text")
    }

    #[test]
    fn c_backend_emits_angle_include_for_header_backed_extern() {
        let code =
            source_to_c("extern \"C\" header \"<sqlite3.h>\" { fn sqlite3_libversion() -> i32; }");
        assert!(
            code.contains("#include <sqlite3.h>"),
            "expected angle-bracket include for header-backed extern:\n{code}"
        );
        // The header is authoritative for the prototype, so the backend must not
        // synthesize its own declaration (which could conflict with the real one).
        assert!(
            !code.contains("sqlite3_libversion"),
            "must not synthesize a prototype for a header-backed fn:\n{code}"
        );
    }

    #[test]
    fn c_backend_emits_quoted_include_for_local_header() {
        let code = source_to_c("extern \"C\" header \"mylib.h\" { fn my_thing() -> i32; }");
        assert!(
            code.contains("#include \"mylib.h\""),
            "expected quoted include for a local header:\n{code}"
        );
        assert!(
            !code.contains("my_thing"),
            "must not synthesize a prototype for a header-backed fn:\n{code}"
        );
    }

    #[test]
    fn c_backend_dedups_and_sorts_ffi_headers() {
        let code = source_to_c(
            "extern \"C\" header \"<zlib.h>\" { fn zfn() -> i32; }\n\
             extern \"C\" header \"<sqlite3.h>\" { fn s1() -> i32; fn s2() -> i32; }",
        );
        // A header shared by several functions is included exactly once.
        assert_eq!(
            code.matches("#include <sqlite3.h>").count(),
            1,
            "shared header should be included once:\n{code}"
        );
        assert_eq!(code.matches("#include <zlib.h>").count(), 1);
        // Deterministic (sorted) order keeps output reproducible for receipts.
        let i_sqlite = code.find("#include <sqlite3.h>").unwrap();
        let i_zlib = code.find("#include <zlib.h>").unwrap();
        assert!(
            i_sqlite < i_zlib,
            "FFI headers should be emitted in sorted order:\n{code}"
        );
    }

    #[test]
    fn generated_code_carries_link_libraries_from_link_clause() {
        let module = crate::parser::parse_source(
            "test.bld",
            "extern \"C\" link \"sqlite3\" header \"<sqlite3.h>\" { fn s() -> i32; }\n\
             extern \"C\" link \"z\" { fn zf() -> i32; }",
        )
        .expect("source should parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);
        let out = codegen.generate(&module).expect("codegen should succeed");
        // Distinct libraries, sorted and de-duplicated, for reproducible builds.
        assert_eq!(
            out.link_libraries,
            vec!["sqlite3".to_string(), "z".to_string()]
        );
    }

    #[test]
    fn generated_code_has_no_link_libraries_without_link_clause() {
        let module = crate::parser::parse_source(
            "test.bld",
            "extern \"C\" header \"<sqlite3.h>\" { fn s() -> i32; }",
        )
        .expect("source should parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);
        let out = codegen.generate(&module).expect("codegen should succeed");
        assert!(out.link_libraries.is_empty());
    }

    #[test]
    fn c_backend_notes_required_link_libraries_in_source() {
        // The link requirement is consumed at compile time, so surface it as a
        // greppable note in the emitted C for anyone inspecting `--emit c`.
        let code =
            source_to_c("extern \"C\" link \"sqlite3\" header \"<sqlite3.h>\" { fn s() -> i32; }");
        assert!(
            code.contains("buildc-link: sqlite3"),
            "generated C should note the required link library:\n{code}"
        );
    }

    #[test]
    fn c_backend_foreign_static_uses_header_no_definition() {
        let code = source_to_c("extern \"C\" header \"<errno.h>\" { static errno: i32; }");
        assert!(
            code.contains("#include <errno.h>"),
            "header backing a foreign static should be included:\n{code}"
        );
        // The header declares errno; we must not define or re-declare it.
        assert!(
            !code.contains("int32_t errno"),
            "must not define/declare a header-backed static:\n{code}"
        );
    }

    #[test]
    fn c_backend_foreign_static_without_header_emits_extern_decl() {
        let code = source_to_c("extern \"C\" { static my_global: i32; }");
        assert!(
            code.contains("extern int32_t my_global;"),
            "a foreign static without a header should get an extern declaration:\n{code}"
        );
        assert!(
            !code.contains("const int32_t my_global ="),
            "a foreign static is a declaration, never a definition:\n{code}"
        );
    }

    #[test]
    fn c_backend_foreign_static_contributes_link_library() {
        let module = crate::parser::parse_source(
            "test.bld",
            "extern \"C\" link \"c\" { static errno: i32; }",
        )
        .expect("source should parse");
        let ctx = TypeContext::new();
        let mut cg = CodeGenerator::new(&ctx, Target::C);
        let out = cg.generate(&module).expect("codegen should succeed");
        assert_eq!(out.link_libraries, vec!["c".to_string()]);
        let code = out.as_string().expect("C output should be text");
        assert!(
            code.contains("buildc-link: c"),
            "a static's link library should be noted:\n{code}"
        );
    }

    #[test]
    fn extern_c_fn_definition_emits_non_static_export() {
        let code = source_to_c("extern \"C\" fn exported_add(a: i32, b: i32) -> i32 { a + b }");
        assert!(
            code.contains("int32_t exported_add("),
            "exported function signature should be present:\n{code}"
        );
        // A C-ABI export must have external linkage so C code can call it.
        assert!(
            !code.contains("static int32_t exported_add"),
            "a C-ABI export must not be emitted static:\n{code}"
        );
    }

    #[test]
    fn extern_c_fn_is_marked_c_export() {
        let module = crate::parser::parse_source(
            "test.bld",
            "extern \"C\" fn ex(n: i32) -> i32 { n }\nfn internal(n: i32) -> i32 { n }",
        )
        .expect("source should parse");
        let ctx = TypeContext::new();
        let mut cg = CodeGenerator::new(&ctx, Target::C);
        cg.generate(&module).expect("codegen should succeed");
        let mir = cg.mir().expect("mir should be present");
        // Lowering adds a forward-declaration entry plus the definition entry,
        // so check across all entries: the export's definition carries the flag.
        assert!(
            mir.functions
                .iter()
                .any(|f| &*f.name == "ex" && f.is_c_export),
            "an extern \"C\" fn should be marked a C export"
        );
        assert!(
            !mir.functions
                .iter()
                .any(|f| &*f.name == "internal" && f.is_c_export),
            "a regular fn should not be a C export"
        );
    }

    #[test]
    fn c_export_header_declares_exports_only() {
        let module = crate::parser::parse_source(
            "test.bld",
            "extern \"C\" fn ex(n: i32) -> i32 { n }\n\
             fn internal(n: i32) -> i32 { n }\n\
             fn main() {}",
        )
        .expect("source should parse");
        let ctx = TypeContext::new();
        let mut cg = CodeGenerator::new(&ctx, Target::C);
        cg.generate(&module).expect("codegen should succeed");
        let header = cg.c_export_header().expect("a module was generated");

        assert!(
            header.contains("#pragma once"),
            "missing include guard:\n{header}"
        );
        assert!(
            header.contains("extern \"C\""),
            "missing C++ linkage guard:\n{header}"
        );
        assert!(
            header.contains("int32_t ex(int32_t n);"),
            "missing export prototype:\n{header}"
        );
        assert!(
            !header.contains("internal"),
            "internal function must not be exported:\n{header}"
        );
        assert!(
            !header.contains(" main("),
            "main must not be exported:\n{header}"
        );
    }

    #[test]
    fn regular_fn_keeps_internal_static_linkage() {
        // Regression: a non-exported function keeps internal (static) linkage.
        let code = source_to_c("fn helper(a: i32) -> i32 { a + 1 }");
        assert!(
            code.contains("static int32_t helper("),
            "non-exported function should stay static:\n{code}"
        );
    }

    #[test]
    fn string_from_does_not_emit_undefined_symbol() {
        // String::from must lower to the runtime allocator, not an undefined
        // `String_from` symbol that fails to link.
        let code = source_to_c("fn main() { let s = String::from(\"hi\"); }");
        assert!(
            !code.contains("String_from"),
            "String::from must not emit an undefined String_from call:\n{code}"
        );
    }

    #[test]
    fn assignment_to_global_emits_a_store() {
        // Storing to a module global/static is lowered to a GlobalStore and the C
        // backend emits `NAME = value;` (previously this silently dropped the
        // write, then fail-closed; now it is supported).
        let src = "static mut SINK: i32 = 0;\nfn set() { SINK = 5; }";
        let code = source_to_c(src);
        assert!(
            code.contains("SINK = 5;"),
            "a store to global SINK must be emitted:\n{code}"
        );
    }

    #[test]
    fn hashmap_string_value_insert_get_dispatch_to_boxed_wrappers() {
        // HashMap<String,String> must dispatch insert/get through the value-typed
        // wrappers (which box >8-byte values), not the str->f64 family that bit-
        // casts a 24-byte BuildString through an 8-byte double slot (corrupting it
        // and failing to compile: BuildString -> double).
        let code = source_to_c(
            "fn main() { let mut m: HashMap<String, String> = HashMap::new(); \
             m.insert(String::from(\"k\"), String::from(\"v\")); \
             let r = m.get(String::from(\"k\")); }",
        );
        assert!(
            code.contains("build_hmap_insert_val_BuildString("),
            "insert must dispatch to the value-typed wrapper:\n{code}"
        );
        assert!(
            code.contains("build_hmap_get_val_BuildString("),
            "get must dispatch to the value-typed wrapper:\n{code}"
        );
        // The value wrapper must box (malloc) the BuildString rather than memcpy it
        // into the 8-byte double slot.
        assert!(
            code.contains("malloc(sizeof(BuildString))"),
            "the BuildString value wrapper must box the value (malloc), not bit-cast it:\n{code}"
        );
    }

    #[test]
    fn split_returns_a_vec_handle_so_len_dispatches() {
        // s.split(",") must yield a Vec<String> handle so `.len()` dispatches to
        // build_hvec_len, not an undefined bare `len` call.
        let code = source_to_c(
            "fn main() { let s = String::from(\"a,b\"); let p = s.split(\",\"); \
             let _n = p.len(); }",
        );
        assert!(
            code.contains("build_string_split_h("),
            "split must return a Vec handle (build_string_split_h):\n{code}"
        );
        assert!(
            !code.contains("= len("),
            "len() on the split result must not be an undefined bare `len` call:\n{code}"
        );
    }

    #[test]
    fn some_constructs_option_struct_not_bare_call() {
        // Some(x) must construct an Option (has_value + typed union slot), not an
        // undefined `Some(x)` call into a mistyped i32 dest (the old C2440).
        let code = source_to_c(
            "fn opt() -> Option<i32> { return Some(7); }\nfn main() { let _o = opt(); }",
        );
        assert!(
            !code.contains("= Some("),
            "Some must not lower to an undefined `Some` call:\n{code}"
        );
        assert!(
            code.contains(".has_value = true") && code.contains(".value.i ="),
            "Some must construct the Option struct with a typed union slot:\n{code}"
        );
    }

    #[test]
    fn ok_err_construct_result_struct_not_bare_call() {
        // Ok(x)/Err(e) must construct the Result struct (is_ok + typed ok slot /
        // err BuildString), not undefined `Ok(x)`/`Err(e)` calls into a mistyped
        // i32 dest (the same C2440 class the Option fix closed).
        let code = source_to_c(
            "fn parse(b: i32) -> Result<i32, String> { \
               if b == 0 { return Err(String::from(\"bad\")); } \
               return Ok(b); \
             }\n\
             fn main() { let _r = parse(1); }",
        );
        assert!(
            !code.contains("= Ok(") && !code.contains("= Err("),
            "Ok/Err must not lower to undefined `Ok`/`Err` calls:\n{code}"
        );
        assert!(
            code.contains(".is_ok = true") && code.contains(".ok.ok_i ="),
            "Ok must construct the Result with is_ok=true and a typed ok slot:\n{code}"
        );
        assert!(
            code.contains(".is_ok = false") && code.contains(".err.err_p"),
            "Err must construct the Result with is_ok=false and box the String err \
             into the typed err slot:\n{code}"
        );
    }

    #[test]
    fn string_push_str_appends_in_place_via_concat() {
        // `s.push_str(x)` must append to `s` (reassigning it to the concatenated
        // result), not lower to an undefined `push_str` call.
        let code = source_to_c(
            "fn main() { let mut s = String::from(\"Hello\"); s.push_str(\", World\"); }",
        );
        // The undefined-call form would be `= push_str(` or a bare `push_str(s`;
        // `build_hvec_push_str` in the runtime is unrelated.
        assert!(
            !code.contains("= push_str(") && !code.contains(" push_str(s"),
            "push_str must not lower to an undefined `push_str` call:\n{code}"
        );
        assert!(
            code.contains("build_string_concat("),
            "push_str must append via build_string_concat:\n{code}"
        );
    }

    #[test]
    fn vtable_wrapper_passes_pointer_self_for_ref_methods() {
        // A trait method taking `&self` must have its vtable wrapper pass the
        // receiver pointer to the concrete fn (which takes `Type*`), not a
        // dereferenced value. Always dereferencing produced
        // `Dog_say((*(Dog*)__self))` - passing a Dog where a Dog* was expected.
        let code = source_to_c(
            "trait Speak { fn say(self: &Self) -> i32; }\n\
             struct Dog { v: i32 }\n\
             impl Speak for Dog { fn say(self: &Dog) -> i32 { return self.v; } }\n\
             fn main() { let d = Dog { v: 7 }; let _r = d.say(); }",
        );
        assert!(
            code.contains("Dog_say((Dog*)__self)"),
            "the &self vtable wrapper must pass the receiver pointer:\n{code}"
        );
        assert!(
            !code.contains("Dog_say((*(Dog*)__self))"),
            "the &self vtable wrapper must not pass a dereferenced value:\n{code}"
        );
    }

    #[test]
    fn iterator_filter_step_skips_non_matching_elements() {
        // `v.iter().filter(|x| x > 2).sum()` must desugar (filter is a real step),
        // not leave `.iter()` as an undefined call.
        let code = source_to_c(
            "fn main() { \
               let v: Vec<i32> = vec![1, 2, 3, 4]; \
               let _s: i32 = v.iter().filter(|x| x > 2).sum(); \
             }",
        );
        assert!(
            !code.contains("= iter(") && !code.contains(" iter("),
            "the filtered chain must not emit an undefined `iter` call:\n{code}"
        );
        assert!(
            code.contains("build_hvec_get_i32(") && code.contains("build_hvec_len("),
            "the filtered sum must lower to a runtime-length element loop:\n{code}"
        );
    }

    #[test]
    fn format_macro_builds_a_string_from_args_not_a_bare_template() {
        // `format!("{} is {}", name, age)` must build an owned BuildString via
        // build_sprintf with the args, not return the raw template string (the
        // old stub dropped the args and yielded a `const char*`, so the result
        // printed as a pointer).
        let code = source_to_c(
            "fn main() { \
               let name = String::from(\"Bob\"); \
               let age = 30; \
               let _msg = format!(\"{} is {}\", name, age); \
             }",
        );
        assert!(
            code.contains("build_sprintf("),
            "format! must build the string via build_sprintf:\n{code}"
        );
        // The result must be a BuildString local, not a raw char pointer.
        assert!(
            code.contains("BuildString _msg") || code.contains("BuildString msg"),
            "the format! result must be a BuildString:\n{code}"
        );
    }

    #[test]
    fn user_function_named_like_c_stdlib_is_escaped() {
        // A user function named `div` (a C stdlib function) must be emitted with
        // a leading underscore at the definition AND every call site, or the C
        // compiler reports a redefinition / binds the call to libc's div.
        let code = source_to_c(
            "fn div(a: i32, b: i32) -> i32 { return a / b; }\n\
             fn main() { let _x = div(20, 4); }",
        );
        assert!(
            code.contains("_div(int32_t a, int32_t b)") && code.contains("= _div(20, 4)"),
            "a user `div` must be escaped at both definition and call:\n{code}"
        );
    }

    #[test]
    fn if_let_some_tests_discriminant_and_binds_payload() {
        // `if let Some(x) = opt { ... } else { ... }` must test has_value and bind
        // x from the payload slot - not bind the whole Option and run both
        // branches unconditionally (the old broken behavior).
        let code = source_to_c(
            "fn get(n: i32) -> Option<i32> { if n > 0 { return Some(n * 2); } return None; }\n\
             fn main() { if let Some(x) = get(5) { let _a = x; } else { let _b = 0; } }",
        );
        assert!(
            code.contains(".has_value"),
            "if-let on Option must branch on has_value:\n{code}"
        );
        assert!(
            code.contains(".value.i"),
            "if-let must bind the payload from the typed value slot:\n{code}"
        );
    }

    #[test]
    fn match_on_ref_enum_dereferences_for_the_tag_path() {
        // `match self { Dir::North => ... }` inside a `&self` enum method: the
        // scrutinee is `Dir*`, so it must be dereferenced to take the enum-tag
        // match path, not a struct `==` comparison on the pointer.
        let code = source_to_c(
            "enum Dir { North, South }\n\
             impl Dir { fn code(self: &Dir) -> i32 { \
               match self { Dir::North => 1, Dir::South => 2 } \
             } }\n\
             fn main() { let d = Dir::South; let _c = d.code(); }",
        );
        // The match must read the tag and compare it to the integer
        // discriminants (`.tag == 0`, `.tag == 1`), via a dereference of `self`.
        assert!(
            code.contains("(*self)"),
            "the &self enum match must dereference the pointer scrutinee:\n{code}"
        );
        assert!(
            code.contains(".tag"),
            "the &self enum match must compare the tag field:\n{code}"
        );
    }

    #[test]
    fn for_over_string_chars_emits_a_byte_loop() {
        // `for c in s.chars()` must emit a runtime-length byte loop, not the
        // silent no-op (zero iterations) it fell through to before.
        let code = source_to_c(
            "fn main() { let s = String::from(\"hi\"); let mut n = 0; \
             for c in s.chars() { n = n + 1; } }",
        );
        assert!(
            code.contains("build_string_len(") && code.contains("build_string_byte_at("),
            "for-over-chars must loop over the string bytes:\n{code}"
        );
    }

    #[test]
    fn vec_sort_dispatches_to_runtime_qsort() {
        // `v.sort()` must dispatch to the element-typed runtime sort, not an
        // undefined `sort` call.
        let code = source_to_c("fn main() { let mut v: Vec<i32> = vec![3, 1, 2]; v.sort(); }");
        assert!(
            !code.contains("= sort(") && !code.contains(" sort(v"),
            "sort must not lower to an undefined call:\n{code}"
        );
        assert!(
            code.contains("build_hvec_sort_i32("),
            "Vec<i32>.sort must call the i32 runtime sort:\n{code}"
        );
    }

    #[test]
    fn vec_indexed_assignment_stores_through_a_setter() {
        // `v[i] = x` must store into the Vec (was silently dropped - the write
        // matched no assignment-target arm, so the element kept its old value).
        let code = source_to_c(
            "fn main() { let mut v: Vec<i32> = vec![10, 20, 30]; v[1] = 99; let _x = v[1]; }",
        );
        assert!(
            code.contains("build_hvec_set_i32("),
            "v[i] = x must dispatch to the typed Vec setter:\n{code}"
        );
    }

    #[test]
    fn nested_vec_of_vec_uses_handle_element_wrappers() {
        // `Vec<Vec<i32>>` push/get must use the BuildVecHandle-sized element
        // wrappers, not build_hvec_push_i32 (which passed a handle as an int32).
        let code = source_to_c(
            "fn main() { \
               let mut grid: Vec<Vec<i32>> = Vec::new(); \
               let mut row: Vec<i32> = Vec::new(); \
               row.push(1); \
               grid.push(row); \
               let _r = grid[0]; \
             }",
        );
        assert!(
            code.contains("build_hvec_push_BuildVecHandle("),
            "Vec<Vec<_>> push must use the handle-sized element wrapper:\n{code}"
        );
    }

    #[test]
    fn vec_contains_dispatches_to_runtime_scan() {
        // `v.contains(x)` must dispatch to the element-typed runtime scan, not an
        // undefined `contains` call.
        let code =
            source_to_c("fn main() { let v: Vec<i32> = vec![3, 1, 2]; let _h = v.contains(2); }");
        assert!(
            !code.contains("= contains(") && !code.contains(" contains("),
            "contains must not lower to an undefined call:\n{code}"
        );
        assert!(
            code.contains("build_hvec_contains_i32("),
            "Vec<i32>.contains must call the i32 runtime scan:\n{code}"
        );
    }

    #[test]
    fn result_methods_is_ok_and_unwrap_or() {
        // `res.is_ok()` reads is_ok; `res.unwrap_or(d)` reads the ok slot when
        // ok else the default. Neither must lower to an undefined call.
        let code = source_to_c(
            "fn div(a: i32, b: i32) -> Result<i32, String> { \
               if b == 0 { return Err(String::from(\"z\")); } return Ok(a / b); }\n\
             fn main() { \
               let r = div(10, 2); \
               let _v = r.unwrap_or(0); \
               let _ok = div(1, 0).is_ok(); \
             }",
        );
        assert!(
            !code.contains("= unwrap_or(") && !code.contains("= is_ok("),
            "Result methods must not lower to undefined calls:\n{code}"
        );
        assert!(
            code.contains(".is_ok"),
            "Result methods must read the is_ok discriminant:\n{code}"
        );
    }

    #[test]
    fn option_methods_is_some_and_unwrap_or() {
        // `opt.is_some()` reads has_value; `opt.unwrap_or(d)` reads the payload
        // slot when present else the default. Neither must lower to an undefined
        // `is_some`/`unwrap_or` call.
        let code = source_to_c(
            "fn find(n: i32) -> Option<i32> { if n > 0 { return Some(n); } return None; }\n\
             fn main() { \
               let x = find(5); \
               let _v = x.unwrap_or(0); \
               let _s = find(0).is_some(); \
             }",
        );
        assert!(
            !code.contains("= unwrap_or(") && !code.contains("= is_some("),
            "Option methods must not lower to undefined calls:\n{code}"
        );
        assert!(
            code.contains(".has_value"),
            "Option methods must read the has_value discriminant:\n{code}"
        );
    }

    #[test]
    fn enumerate_map_binds_a_tuple_param() {
        // `v.iter().enumerate().map(|(i, x)| i + x)` must bind both i (index) and
        // x (element) from the single tuple param - previously neither was bound
        // (C2065 'i' undeclared).
        let code = source_to_c(
            "fn main() { let v: Vec<i32> = vec![10, 20]; \
             let _s: i32 = v.iter().enumerate().map(|(i, x)| i + x).sum(); }",
        );
        // The body `i + x` lowers to an add of the two bound locals; the chain
        // emits the element loop. A successful lowering (no panic) plus the loop
        // markers is the signal here.
        assert!(
            code.contains("build_hvec_get_i32(") && code.contains("build_hvec_len("),
            "the enumerate+map chain must lower to an element loop:\n{code}"
        );
    }

    #[test]
    fn iterator_any_all_predicate_terminals_desugar() {
        // `v.iter().any(|x| pred)` and `.all(|x| pred)` desugar to a boolean
        // accumulator loop, not undefined `iter`/`any`/`all` calls.
        let any_code = source_to_c(
            "fn main() { let v: Vec<i32> = vec![1, 2, 3, 4]; let _a = v.iter().any(|x| x > 3); }",
        );
        assert!(
            !any_code.contains(" iter(") && !any_code.contains("= any("),
            "any must desugar, not emit undefined iter/any calls:\n{any_code}"
        );
        assert!(
            any_code.contains("build_hvec_get_i32(") && any_code.contains("build_hvec_len("),
            "any must lower to a runtime-length element loop:\n{any_code}"
        );
        let all_code = source_to_c(
            "fn main() { let v: Vec<i32> = vec![1, 2, 3, 4]; let _a = v.iter().all(|x| x > 0); }",
        );
        assert!(
            !all_code.contains(" iter(") && !all_code.contains("= all("),
            "all must desugar, not emit undefined iter/all calls:\n{all_code}"
        );
    }

    #[test]
    fn iterator_count_terminal_counts_elements() {
        // `v.iter().filter-less count()` desugars to a +1 accumulator loop, not
        // an undefined `iter`/`count` call.
        let code = source_to_c(
            "fn main() { let v: Vec<i32> = vec![5, 6, 7]; let _c = v.iter().count(); }",
        );
        assert!(
            !code.contains(" count(") && !code.contains("= count("),
            "count must not emit an undefined `count` call:\n{code}"
        );
        assert!(
            code.contains("build_hvec_len("),
            "count must lower to a runtime-length element loop:\n{code}"
        );
    }

    #[test]
    fn iterator_product_terminal_multiplies_elements() {
        // `v.iter().product()` desugars to a `acc = acc * elem` loop from 1.
        let code = source_to_c(
            "fn main() { let v: Vec<i32> = vec![2, 3, 4]; let _p: i32 = v.iter().product(); }",
        );
        assert!(
            !code.contains(" product(") && !code.contains("= product("),
            "product must not emit an undefined `product` call:\n{code}"
        );
        assert!(
            code.contains("build_hvec_get_i32(") && code.contains("build_hvec_len("),
            "product must lower to a runtime-length element loop:\n{code}"
        );
    }

    #[test]
    fn iterator_sum_terminal_desugars_to_an_accumulator_loop() {
        // `v.iter().map(|x| x * 2).sum()` must desugar to a loop with a running
        // accumulator, not leave `.iter()` as an undefined `iter` call (which
        // failed to link).
        let code = source_to_c(
            "fn main() { \
               let v: Vec<i32> = vec![1, 2, 3, 4]; \
               let _total: i32 = v.iter().map(|x| x * 2).sum(); \
             }",
        );
        assert!(
            !code.contains("= iter(") && !code.contains(" iter("),
            "the iterator chain must not emit an undefined `iter` call:\n{code}"
        );
        assert!(
            code.contains("build_hvec_get_i32") && code.contains("build_hvec_len"),
            "the sum chain must lower to a runtime-length element loop:\n{code}"
        );
    }

    #[test]
    fn nested_result_of_option_boxes_a_none_payload() {
        // `Ok(None)` in a `-> Result<Option<i32>, String>` must box the Option
        // payload (it is 16 bytes, > the 8-byte slot), the same as `Ok(Some(n))`.
        // Previously `None` (a non-local const) wasn't detected as an aggregate,
        // so the construct cast the Option struct to int64_t into ok_i - a C
        // error, and a mismatch with the boxed read.
        let code = source_to_c(
            "fn f(n: i32) -> Result<Option<i32>, String> { \
               if n < 0 { return Err(String::from(\"neg\")); } \
               if n == 0 { return Ok(None); } \
               return Ok(Some(n)); \
             }\n\
             fn main() { let _r = f(0); }",
        );
        assert!(
            !code.contains("ok.ok_i = (int64_t)(((Option)"),
            "Ok(None) must not cast the Option payload into the i32 slot:\n{code}"
        );
        // Both Ok arms (Some and None) box the Option payload into ok_p.
        assert!(
            code.matches("Option* __ok_box").count() >= 2,
            "both Ok(Some) and Ok(None) must box the Option payload:\n{code}"
        );
    }

    #[test]
    fn vec_of_struct_uses_sized_element_wrappers() {
        // `Vec<P>` where P is a struct must construct/push via element-sized
        // wrappers, not build_hvec_new_i32 / build_hvec_push_i32 (which take an
        // int32 and reject a struct - a C error).
        let code = source_to_c(
            "struct P { x: i32 }\n\
             fn main() { \
               let mut v: Vec<P> = Vec::new(); \
               v.push(P { x: 7 }); \
               let _q = v[0]; \
             }",
        );
        // The i32 wrappers are always defined in the runtime preamble; assert on
        // the call sites. `= build_hvec_new_P()` is the construction call (the
        // definition is `build_hvec_new_P(void)`), and a `build_hvec_push_P`
        // wrapper is generated + called for the struct element.
        assert!(
            code.contains("= build_hvec_new_P()"),
            "Vec<struct>::new must dispatch to the struct-sized constructor:\n{code}"
        );
        assert!(
            code.contains("build_hvec_push_P(") && code.contains("build_hvec_get_P("),
            "Vec<struct> push/get must use the struct-sized element wrappers:\n{code}"
        );
    }

    #[test]
    fn match_on_method_call_threads_the_result_payload_type() {
        // `match p.run(1) { Ok(x) => ... }` where run returns Result<f64, _>
        // must read the float slot. Method-call scrutinees were not threaded, so
        // the match defaulted to i32 and read ok_i (the double bits as an int).
        let code = source_to_c(
            "struct Parser { base: i32 }\n\
             impl Parser { \
               fn run(self: &Parser, n: i32) -> Result<f64, String> { \
                 if n < 0 { return Err(String::from(\"neg\")); } \
                 return Ok(2.5); \
               } \
             }\n\
             fn main() { \
               let p = Parser { base: 0 }; \
               match p.run(1) { \
                 Ok(x) => { let _a = x; } \
                 Err(e) => { let _b = e; } \
               } \
             }",
        );
        // `run` constructs into ok_f; with threading the match also reads ok_f,
        // so ok_i never appears. Without it the match read the wrong slot
        // (`(int32_t)…ok.ok_i`), reinterpreting the double bits.
        assert!(
            code.contains(".ok.ok_f") && !code.contains(".ok.ok_i"),
            "the match on a method call returning Result<f64,_> must read the \
             float ok slot, not the i32 slot:\n{code}"
        );
    }

    #[test]
    fn result_supports_a_non_string_err_payload() {
        // `Result<i32, i32>` must carry an i32 Err in a typed union slot, not the
        // hardcoded `BuildString err` field (which made `r.err = 404` assign an
        // int to a struct - a C error). Construction writes err_i; the match
        // reads err_i.
        let code = source_to_c(
            "fn check(n: i32) -> Result<i32, i32> { \
               if n < 0 { return Err(404); } \
               return Ok(n); \
             }\n\
             fn main() { \
               match check(-1) { \
                 Ok(v) => { let _a = v; } \
                 Err(code) => { let _b = code; } \
               } \
             }",
        );
        assert!(
            code.contains(".err.err_i = "),
            "Err(404) must construct into the typed i32 err slot:\n{code}"
        );
        assert!(
            code.contains(".err.err_i") && code.contains("(int32_t)"),
            "the Err arm must read the i32 payload from the typed err slot:\n{code}"
        );
    }

    #[test]
    fn option_match_on_direct_call_reads_the_threaded_payload_slot() {
        // `match returns_option_f64() { Some(x) => ... }` must read the float
        // union slot (.value.f), not the i32 slot. Construction writes .value.f;
        // without threading the function's `-> Option<f64>` return type, the
        // match defaulted the inner type to i32 and read .value.i (silent-wrong:
        // float bits reinterpreted as an int).
        let code = source_to_c(
            "fn maybe(n: i32) -> Option<f64> { \
               if n == 1 { return Some(2.5); } \
               return None; \
             }\n\
             fn main() { \
               match maybe(1) { \
                 Some(x) => { let _y = x; } \
                 None => {} \
               } \
             }",
        );
        assert!(
            code.contains(".value.f = "),
            "Some(2.5) must construct into the float slot:\n{code}"
        );
        assert!(
            !code.contains("(int32_t)") || !code.contains(".value.i"),
            "the Option<f64> match must not read the payload via the i32 slot:\n{code}"
        );
        assert!(
            code.contains(".value.f"),
            "the Option<f64> match must read the payload from the float slot:\n{code}"
        );
    }

    #[test]
    fn try_operator_unwraps_result_ok_and_propagates_err() {
        // `let v = parse(s)?;` must unwrap the Ok payload (so `v` is the inner
        // i32, usable in `v * 2`) and early-return the whole Result on Err.
        // Previously `?` was a no-op for runtime Result: `v` stayed the whole
        // Result struct and `v * 2` multiplied a struct (a C error).
        let code = source_to_c(
            "fn parse(s: i32) -> Result<i32, String> { \
               if s < 0 { return Err(String::from(\"neg\")); } \
               return Ok(s); \
             }\n\
             fn doubled(s: i32) -> Result<i32, String> { \
               let v = parse(s)?; \
               return Ok(v * 2); \
             }\n\
             fn main() { let _r = doubled(5); }",
        );
        // `v` must be the unwrapped i32 payload, not the whole Result struct.
        // Before the fix `?` was a no-op and `v` was declared `Result v;`, so
        // `v * 2` multiplied a struct.
        assert!(
            code.contains("int32_t v") && !code.contains("Result v"),
            "the ? operator must bind the unwrapped Ok payload (int32_t v), \
             not the whole Result:\n{code}"
        );
    }

    #[test]
    fn option_string_payload_is_boxed_through_the_pointer_slot() {
        // A 24-byte BuildString does not fit the 8-byte union; `Some(s)` must
        // box it (malloc + store the pointer in .value.p) and the match must
        // deref-read it. Previously the construct cast a struct to int64_t.
        let code = source_to_c(
            "fn lookup(n: i32) -> Option<String> { \
               if n == 1 { return Some(String::from(\"found\")); } \
               return None; \
             }\n\
             fn main() { \
               match lookup(1) { \
                 Some(s) => { let _t = s; } \
                 None => {} \
               } \
             }",
        );
        assert!(
            code.contains("malloc(sizeof(BuildString))") && code.contains(".value.p"),
            "Some(String) must box the payload (malloc + .value.p):\n{code}"
        );
        assert!(
            code.contains("*(BuildString*)"),
            "the Option<String> match must deref-read the boxed payload:\n{code}"
        );
    }

    #[test]
    fn result_match_tests_is_ok_and_binds_typed_slots() {
        // `match r { Ok(n) => ..., Err(e) => ... }` must branch on `is_ok`, read
        // the Ok payload from the typed union slot, and bind Err from the err
        // BuildString. Without this it emitted `if (true)` + whole-struct binds
        // (silent-wrong: both arms read garbage).
        let code = source_to_c(
            "fn parse(b: i32) -> Result<i32, String> { \
               if b == 0 { return Err(String::from(\"bad\")); } \
               return Ok(b); \
             }\n\
             fn main() { \
               match parse(1) { \
                 Ok(n) => { println!(\"ok {}\", n); } \
                 Err(e) => { println!(\"err {}\", e); } \
               } \
             }",
        );
        assert!(
            code.contains(".is_ok"),
            "Result match must branch on the is_ok discriminant:\n{code}"
        );
        assert!(
            code.contains(".ok.ok_i"),
            "Ok arm must read the payload from the typed ok union slot:\n{code}"
        );
        assert!(
            code.contains(".err"),
            "Err arm must bind from the err BuildString field:\n{code}"
        );
    }

    #[test]
    fn for_over_vec_emits_a_runtime_length_loop() {
        // `for x in v` over a Vec must emit a real index loop bounded by
        // build_hvec_len, not the silent no-op that previously dropped the body.
        let code = source_to_c(
            "fn main() { let mut v: Vec<i32> = Vec::new(); v.push(1); \
             let mut s = 0; for x in v { s = s + x; } }",
        );
        assert!(
            code.contains("build_hvec_len("),
            "for-over-Vec must emit a runtime-length loop (build_hvec_len):\n{code}"
        );
    }

    #[test]
    fn vec_new_dispatches_to_element_typed_constructor() {
        // `Vec::new()` must lower to the element-typed runtime constructor
        // (build_hvec_new_str for Vec<String>), not an undefined `Vec_new`.
        let str_code = source_to_c("fn main() { let mut v: Vec<String> = Vec::new(); }");
        assert!(
            !str_code.contains("= Vec_new("),
            "Vec::new must not emit an undefined `Vec_new` call:\n{str_code}"
        );
        assert!(
            str_code.contains("build_hvec_new_str("),
            "Vec<String>::new must call build_hvec_new_str:\n{str_code}"
        );
        let i32_code = source_to_c("fn main() { let mut v: Vec<i32> = Vec::new(); }");
        assert!(
            i32_code.contains("build_hvec_new_i32("),
            "Vec<i32>::new must call build_hvec_new_i32:\n{i32_code}"
        );
    }

    #[test]
    fn numeric_to_string_dispatches_to_runtime_formatter() {
        // `n.to_string()` on a number must lower to the type-specific runtime
        // formatter (build_i64_to_string / build_f64_to_string), not a bare
        // `to_string` symbol that fails to link.
        let int_code = source_to_c("fn main() { let n = 42; let u = n.to_string(); }");
        assert!(
            !int_code.contains("= to_string("),
            "integer to_string must not emit an undefined `to_string` call:\n{int_code}"
        );
        assert!(
            int_code.contains("build_i64_to_string("),
            "integer to_string must call build_i64_to_string:\n{int_code}"
        );
        let flt_code = source_to_c("fn main() { let x = 3.5; let u = x.to_string(); }");
        assert!(
            !flt_code.contains("= to_string("),
            "float to_string must not emit an undefined `to_string` call:\n{flt_code}"
        );
        assert!(
            flt_code.contains("build_f64_to_string("),
            "float to_string must call build_f64_to_string:\n{flt_code}"
        );
    }

    #[test]
    fn string_from_dest_is_typed_buildstring_not_int() {
        // EVERY dest of `build_string_new(...)` must be declared `BuildString`,
        // not `int32_t`. `String::from(...)` lowers to `build_string_new(...)`
        // (returning a BuildString), so a mistyped `int32_t` dest produces a real
        // C2440 `cannot convert from 'BuildString' to 'int32_t'` under a C
        // compiler. The earlier symbol-absence test missed this because the FIRST
        // build_string_new in the output is the (correctly-typed) str literal; the
        // String::from dest is a later one.
        let code = source_to_c("fn main() { let s = String::from(\"hi\"); }");
        let dests: Vec<String> = code
            .lines()
            .filter(|l| l.contains("= build_string_new("))
            .map(|l| {
                l.trim()
                    .split('=')
                    .next()
                    .unwrap()
                    .trim()
                    .rsplit(|c: char| c.is_whitespace())
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert!(
            !dests.is_empty(),
            "expected at least one build_string_new assignment:\n{code}"
        );
        for lhs in &dests {
            assert!(
                code.contains(&format!("BuildString {lhs}")),
                "build_string_new dest `{lhs}` must be declared BuildString:\n{code}"
            );
            assert!(
                !code.contains(&format!("int32_t {lhs}")),
                "build_string_new dest `{lhs}` must NOT be declared int32_t (C2440):\n{code}"
            );
        }
    }

    #[test]
    fn variadic_extern_emits_ellipsis_in_c() {
        // A non-standard variadic extern is synthesized with a trailing `, ...`.
        let code = source_to_c("extern \"C\" { fn my_printf(fmt: &str, ...) -> i32; }");
        assert!(
            code.contains("my_printf"),
            "declaration should be present:\n{code}"
        );
        assert!(
            code.contains(", ...)"),
            "a variadic extern should emit a trailing `, ...`:\n{code}"
        );
    }

    #[test]
    fn c_backend_still_synthesizes_extern_without_header() {
        // Regression guard: an extern block with no header keeps the existing
        // behavior of synthesizing a prototype for non-standard functions.
        let code = source_to_c("extern \"C\" { fn my_custom_fn() -> i32; }");
        assert!(
            code.contains("my_custom_fn"),
            "extern without header should still synthesize a declaration:\n{code}"
        );
    }

    #[test]
    fn test_full_pipeline_c_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::CSource);

        // C output should be valid UTF-8
        assert!(generated.as_string().is_some());
    }

    #[test]
    fn test_full_pipeline_rust_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::Rust);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::RustSource);
        assert!(generated.as_string().is_some());
    }

    #[test]
    fn test_full_pipeline_x86_64_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::X86_64);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::Assembly);

        // Assembly output should be valid UTF-8
        assert!(generated.as_string().is_some());
    }

    #[test]
    fn test_full_pipeline_arm64_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::Arm64);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::Assembly);
    }

    #[test]
    fn test_full_pipeline_wasm_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::Wasm);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::Wat);

        // WAT output should be valid UTF-8
        assert!(generated.as_string().is_some());
    }

    #[test]
    fn test_full_pipeline_spirv_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::SpirV);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::SpirV);

        // SPIR-V is binary, so as_string should return None
        assert!(generated.as_string().is_none());
    }

    #[test]
    fn test_mir_accessible_after_generation() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);

        assert!(codegen.mir().is_none());

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let _ = codegen.generate(&module);

        // MIR should now be accessible
        let mir = codegen.mir();
        assert!(mir.is_some());
    }

    #[test]
    fn test_multiple_generations_overwrite_mir() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);

        let module1 = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };
        let _ = codegen.generate(&module1);
        assert!(codegen.mir().is_some());

        let module2 = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };
        let _ = codegen.generate(&module2);
        assert!(codegen.mir().is_some());
    }
}
