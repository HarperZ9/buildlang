// ===============================================================================
// BUILDLANG CODEGEN - ASSERTION AND ABORTING MACROS
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Lowers the macros that stop a program:
//!
//! - `assert!(cond)`, `assert_eq!(a, b)`, `assert_ne!(a, b)` and their
//!   `debug_` spellings. Each becomes an `Assert` terminator: when the check
//!   fails the program prints the source text with its line and aborts. The
//!   `debug_` forms are always checked; there is no build mode that removes
//!   them.
//! - `unreachable!()`, `todo!()` and `unimplemented!()`, which abort when
//!   reached.
//!
//! Any further arguments are a message in `println!` form, printed to stderr
//! before the abort.
//!
//! Before this pass every one of these was dropped with no diagnostic, so a
//! failing check let the program run on and exit 0. A macro name the compiler
//! does not implement is now a compile error (`unknown_macro`).

use std::sync::Arc;

use crate::ast;
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::*;
use crate::lexer::{SourceFile, TokenKind};
use crate::parser::Parser;

use super::MirLowerer;

pub(super) fn rejected(message: String, location: Option<(u32, u32)>, help: &str) -> CodegenError {
    CodegenError::Rejected {
        message,
        location,
        help: Some(help.to_string()),
    }
}

fn no_fn() -> CodegenError {
    CodegenError::Internal("No current function".to_string())
}

/// Byte range of one token tree.
fn tree_span(tree: &ast::TokenTree) -> (u32, u32) {
    match tree {
        ast::TokenTree::Token(t) => (t.span.start.0, t.span.end.0),
        ast::TokenTree::Delimited { span, .. } => (span.start.0, span.end.0),
    }
}

/// Byte range of a run of token trees.
pub(super) fn trees_span(trees: &[ast::TokenTree]) -> Option<(u32, u32)> {
    let (start, _) = tree_span(trees.first()?);
    let (_, end) = tree_span(trees.last()?);
    Some((start, end))
}

/// The macro's arguments without its own delimiter group.
pub(super) fn inner(tokens: &[ast::TokenTree]) -> &[ast::TokenTree] {
    match tokens {
        [ast::TokenTree::Delimited { tokens, .. }] => tokens,
        other => other,
    }
}

/// Split at top-level commas. A trailing comma adds no empty part.
pub(super) fn split_commas(trees: &[ast::TokenTree]) -> Vec<&[ast::TokenTree]> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    for (i, tree) in trees.iter().enumerate() {
        if let ast::TokenTree::Token(t) = tree {
            if matches!(t.kind, TokenKind::Comma) {
                parts.push(&trees[start..i]);
                start = i + 1;
            }
        }
    }
    if start < trees.len() {
        parts.push(&trees[start..]);
    }
    parts
}

/// The compile error for a macro name the compiler does not implement.
pub(super) fn unknown_macro(name: &str, tokens: &[ast::TokenTree]) -> CodegenError {
    rejected(
        format!("macro `{name}!` is not implemented, and it would be dropped from the program"),
        trees_span(tokens),
        "implemented macros: println, print, eprintln, eprint, dbg, format, vec, panic, asm, \
         assert, assert_eq, assert_ne, debug_assert, debug_assert_eq, debug_assert_ne, \
         unreachable, todo, unimplemented, env",
    )
}

/// `Some(n)` when `name` is an assertion macro taking `n` checked arguments.
pub(super) fn assert_arity(name: &str) -> Option<usize> {
    match name {
        "assert" | "debug_assert" => Some(1),
        "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" => Some(2),
        _ => None,
    }
}

/// True for the macros that abort whenever they are reached.
pub(super) fn is_abort_macro(name: &str) -> bool {
    matches!(name, "unreachable" | "todo" | "unimplemented")
}

impl<'ctx> MirLowerer<'ctx> {
    /// Source text of a run of token trees as written, the same text with the
    /// module merge's renames applied (the form to lower), and the 1-based
    /// line it starts on.
    fn macro_arg_text(
        &self,
        name: &str,
        trees: &[ast::TokenTree],
    ) -> CodegenResult<(String, String, usize)> {
        let unreadable = |loc| {
            rejected(
                format!("cannot read the arguments of `{name}!` from the source text"),
                loc,
                "write the check as an `if` that calls `panic!`",
            )
        };
        let (start, end) = trees_span(trees).ok_or_else(|| unreadable(None))?;
        let source = self
            .source
            .as_ref()
            .ok_or_else(|| unreadable(Some((start, end))))?;
        let text = source
            .get(start as usize..end as usize)
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| unreadable(Some((start, end))))?;
        let line = source[..start as usize].matches('\n').count() + 1;
        let source_id = match trees.first() {
            Some(ast::TokenTree::Token(t)) => Some(t.span.source_id),
            Some(ast::TokenTree::Delimited { span, .. }) => Some(span.source_id),
            None => None,
        };
        let renamed = super::macros::renamed_slice(source, start as usize, end as usize, source_id);
        Ok((text.to_string(), renamed, line))
    }

    /// Parse `text` as one whole expression and lower it.
    fn lower_expr_text(
        &mut self,
        name: &str,
        text: &str,
        loc: Option<(u32, u32)>,
    ) -> CodegenResult<MirValue> {
        let bad = || {
            rejected(
                format!("the arguments of `{name}!` are not expressions: `{text}`"),
                loc,
                "each checked argument must be one expression",
            )
        };
        let tokens = crate::lexer::tokenize(text).map_err(|_| bad())?;
        let file = SourceFile::anonymous(text);
        let mut parser = Parser::new(&file, tokens);
        let expr = parser.parse_standalone_expr().ok_or_else(bad)?;
        // Spans in `expr` index `text`, so nested macros must read from it.
        let prev = self.source.replace(Arc::from(text));
        let prev_fragment = std::mem::replace(&mut self.in_macro_fragment, true);
        let result = self.lower_expr(&expr);
        self.source = prev;
        self.in_macro_fragment = prev_fragment;
        result
    }

    /// Lower an assertion macro with `arity` checked arguments.
    pub(super) fn lower_assert_macro(
        &mut self,
        name: &str,
        arity: usize,
        tokens: &[ast::TokenTree],
    ) -> CodegenResult<()> {
        let parts = split_commas(inner(tokens));
        if parts.len() < arity || parts[..arity].iter().any(|p| p.is_empty()) {
            return Err(rejected(
                format!("`{name}!` needs {arity} argument(s) to check"),
                trees_span(tokens),
                "write `assert!(cond)` or `assert_eq!(left, right)`",
            ));
        }
        let loc = trees_span(tokens);
        let (first, first_low, line) = self.macro_arg_text(name, parts[0])?;
        let (cond_text, shown) = if arity == 2 {
            let (second, second_low, _) = self.macro_arg_text(name, parts[1])?;
            let op = if name.ends_with("_ne") { "!=" } else { "==" };
            (
                format!("({first_low}) {op} ({second_low})"),
                format!("{name}!({first}, {second})"),
            )
        } else {
            (first_low, format!("{name}!({first})"))
        };
        let cond = self.lower_expr_text(name, &cond_text, loc)?;
        // One line: backends place the message in a string or a comment.
        let shown = shown.split_whitespace().collect::<Vec<_>>().join(" ");
        let msg = format!("{shown} at line {line}");

        let message_parts = &parts[arity..];
        if !message_parts.is_empty() {
            // Print the caller's message only on failure, then fall into the
            // same Assert, which prints the source text and aborts.
            let first_msg = message_parts[0];
            let all = inner(tokens);
            let offset = first_msg.as_ptr() as usize - all.as_ptr() as usize;
            let index = offset / std::mem::size_of::<ast::TokenTree>();
            let message_tokens = &all[index..];
            let builder = self.current_fn.as_mut().ok_or_else(no_fn)?;
            let fail = builder.create_block();
            let cont = builder.create_block();
            builder.branch(cond.clone(), cont, fail);
            builder.switch_to_block(fail);
            self.lower_print_macro(message_tokens, true, true)?;
            let builder = self.current_fn.as_mut().ok_or_else(no_fn)?;
            builder.assert(cond, true, msg, cont);
            builder.switch_to_block(cont);
            return Ok(());
        }

        let builder = self.current_fn.as_mut().ok_or_else(no_fn)?;
        let cont = builder.create_block();
        builder.assert(cond, true, msg, cont);
        builder.switch_to_block(cont);
        Ok(())
    }

    /// Lower `unreachable!`, `todo!` or `unimplemented!`: print the optional
    /// message and the macro's name with its line, then abort.
    pub(super) fn lower_abort_macro(
        &mut self,
        name: &str,
        path_start: u32,
        tokens: &[ast::TokenTree],
    ) -> CodegenResult<()> {
        if !inner(tokens).is_empty() {
            self.lower_print_macro(tokens, true, true)?;
        }
        let line = self
            .source
            .as_ref()
            .and_then(|s| s.get(..path_start as usize))
            .map(|s| s.matches('\n').count() + 1);
        let msg = match line {
            Some(line) => format!("{name}!() reached at line {line}"),
            None => format!("{name}!() reached"),
        };
        let builder = self.current_fn.as_mut().ok_or_else(no_fn)?;
        let dead = builder.create_block();
        builder.assert(MirValue::Const(MirConst::Bool(false)), true, msg, dead);
        builder.switch_to_block(dead);
        Ok(())
    }
}
