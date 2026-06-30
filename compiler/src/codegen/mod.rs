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
        assert!(g.is_extern_decl, "foreign static should be an external declaration");
        assert_eq!(g.link_header.as_deref(), Some("<errno.h>"));
        assert_eq!(g.link_lib.as_deref(), Some("c"));
    }

    #[test]
    fn extern_without_header_has_no_link_header() {
        let module = crate::parser::parse_source(
            "test.bld",
            "extern \"C\" { fn puts(s: &str) -> i32; }",
        )
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
        let code = source_to_c(
            "extern \"C\" header \"<sqlite3.h>\" { fn sqlite3_libversion() -> i32; }",
        );
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
        assert!(i_sqlite < i_zlib, "FFI headers should be emitted in sorted order:\n{code}");
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
        let code = source_to_c(
            "extern \"C\" link \"sqlite3\" header \"<sqlite3.h>\" { fn s() -> i32; }",
        );
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
            mir.functions.iter().any(|f| &*f.name == "ex" && f.is_c_export),
            "an extern \"C\" fn should be marked a C export"
        );
        assert!(
            !mir.functions.iter().any(|f| &*f.name == "internal" && f.is_c_export),
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

        assert!(header.contains("#pragma once"), "missing include guard:\n{header}");
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
    fn variadic_extern_emits_ellipsis_in_c() {
        // A non-standard variadic extern is synthesized with a trailing `, ...`.
        let code = source_to_c("extern \"C\" { fn my_printf(fmt: &str, ...) -> i32; }");
        assert!(code.contains("my_printf"), "declaration should be present:\n{code}");
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
