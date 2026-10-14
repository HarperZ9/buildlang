// ===============================================================================
// BUILDLANG DIAGNOSTICS - SHARED CHECK
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! One check pipeline for a single source buffer: lex, parse, type-check, then
//! lower to C so a program the C backend cannot compile fails the check. The
//! language server uses it, so an editor sees the same codes and help lines as
//! `buildc check --error-format=json`.

use std::sync::Arc;

use super::Diagnostic;
use crate::ast::{self, ItemKind, Module};
use crate::codegen::{CodeGenerator, Target};
use crate::lexer::{Lexer, SourceFile, Span};
use crate::parser::Parser;
use crate::types::{TypeChecker, TypeContext, TypeError, TypeErrorWithSpan};

/// Check one buffer with no imports or module files. Returns every
/// diagnostic in the order `buildc check` reports them.
pub fn check_buffer(file: &SourceFile) -> Vec<Diagnostic> {
    let mut lexer = Lexer::new(file);
    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,
        Err(err) => return vec![Diagnostic::from_lexer(&err, file)],
    };
    let mut parser = Parser::new(file, tokens);
    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(err) => {
            let mut out: Vec<_> = parser
                .errors()
                .iter()
                .map(|e| Diagnostic::from_parse(e, file))
                .collect();
            out.push(Diagnostic::from_parse(&err, file));
            return out;
        }
    };
    let mut out: Vec<_> = parser
        .errors()
        .iter()
        .map(|e| Diagnostic::from_parse(e, file))
        .collect();

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(file);
    checker.check_module(&ast);
    let mut type_errors = checker.errors().to_vec();

    if out.is_empty() && type_errors.is_empty() {
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(file.source()));
        match codegen.generate(&ast) {
            Ok(_) => type_errors.extend(codegen.linear_errors().to_vec()),
            Err(e) if !has_gpu_entry_point(&ast) => {
                type_errors.push(codegen_error_as_type_error(&e, file))
            }
            Err(_) => {}
        }
    }
    out.extend(type_errors.iter().map(|e| Diagnostic::from_type(e, file)));
    out
}

/// Turn a code-generation failure into a source-located type error, so `buildc
/// check` fails on any program the C backend cannot compile.
pub fn codegen_error_as_type_error(
    err: &crate::codegen::backend::CodegenError,
    source_file: &SourceFile,
) -> TypeErrorWithSpan {
    use crate::codegen::backend::CodegenError;
    let id = source_file.id;
    let (message, location, help) = match err {
        CodegenError::Rejected {
            message,
            location,
            help,
        } => (message.clone(), *location, help.clone()),
        other => (
            format!("the C backend cannot compile this program: {other}"),
            None,
            None,
        ),
    };
    let span = match location {
        Some((start, end)) => Span::from_offsets(start, end, id),
        None => Span::from_offsets(0, 0, id),
    };
    let mut err = TypeErrorWithSpan::new(TypeError::BackendRejected { message }, span);
    if let Some(help) = help {
        err = err.with_help(help);
    }
    err
}

/// True when the module declares a shader stage or GPU kernel entry point
/// (`#[fragment]`, `#[vertex]`, `#[compute]`, `#[kernel]`), at any nesting depth.
pub fn has_gpu_entry_point(module: &Module) -> bool {
    fn item_has(item: &ast::Item) -> bool {
        let gpu_attr = item.attrs.iter().any(|attr| {
            attr.path.segments.first().map_or(false, |seg| {
                matches!(
                    seg.ident.name.as_ref(),
                    "fragment" | "vertex" | "compute" | "kernel"
                )
            })
        });
        gpu_attr
            || matches!(&item.kind, ItemKind::Mod(m)
                if m.content.as_ref().map_or(false, |c| c.items.iter().any(item_has)))
    }
    module.items.iter().any(item_has)
}
