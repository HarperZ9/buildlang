// ===============================================================================
// BUILDLANG DIAGNOSTICS - SHARED CHECK
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
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
    if out.is_empty() && type_errors.is_empty() {
        type_errors.extend(spec_errors(&ast, file));
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

/// Laws and contracts that `buildc verify` refutes, as check errors. A proved
/// or unknown obligation adds nothing: `check` fails only on a counterexample
/// the evaluator re-ran, or on a verifier that disagrees with itself.
pub fn spec_errors(module: &Module, source_file: &SourceFile) -> Vec<TypeErrorWithSpan> {
    use crate::verify::{Kind, Verdict};
    let has_specs = module.items.iter().any(|item| match &item.kind {
        ast::ItemKind::Function(f) => {
            f.name.name.starts_with(crate::parser::ENSURES_PREFIX)
                || item.attrs.iter().any(|a| {
                    a.path.segments.len() == 1
                        && &*a.path.segments[0].ident.name == crate::parser::LAW_ATTR
                })
        }
        _ => false,
    });
    if !has_specs {
        return Vec::new();
    }
    let span_of = |name: &str| {
        module
            .items
            .iter()
            .find_map(|item| match &item.kind {
                ast::ItemKind::Function(f) if &*f.name.name == name => Some(f.name.span),
                _ => None,
            })
            .unwrap_or_else(|| Span::from_offsets(0, 0, source_file.id))
    };
    let mut out = Vec::new();
    for o in crate::verify::verify_module(module) {
        let what = match o.kind {
            Kind::Law => format!("law `{}`", o.name),
            Kind::Ensures => format!("the ensures of `{}`", o.name),
        };
        let show = |input: &[(String, String)]| {
            input
                .iter()
                .map(|(n, v)| format!("{n} = {v}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let err = match &o.verdict {
            Verdict::Refuted { input, why } => TypeError::SpecRefuted {
                message: format!("{what} is refuted at {}: {why}", show(input)),
            },
            Verdict::Inconsistent { input } => TypeError::SpecInconsistent {
                message: format!(
                    "{what}: the prover claimed it, yet {} is a counterexample",
                    show(input)
                ),
            },
            _ => continue,
        };
        out.push(
            TypeErrorWithSpan::new(err, span_of(&o.name))
                .with_help("run `buildc verify` on this file for every obligation's verdict"),
        );
    }
    out
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
