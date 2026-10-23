// ===============================================================================
// BUILDLANG TYPE SYSTEM - MACRO ARGUMENTS
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Recover the argument expressions of a macro call from its token trees.
//!
//! The parser keeps macro arguments as raw tokens. Before this pass the
//! checker never looked inside them, so `println!("{}", f("x"))` with
//! `fn f(p: i32)` passed `buildc check` and failed later in the C compiler.
//! The checker now re-parses each top-level argument and type-checks it.
//! An argument that does not parse as a whole expression is skipped here;
//! the lowering reports it.

use crate::ast::{self, Expr};
use crate::lexer::{SourceFile, Token, TokenKind};
use crate::parser::Parser;

/// Macros whose arguments are ordinary expressions separated by commas.
/// For the formatting family the first argument is the format string.
pub(crate) fn macro_arg_policy(name: &str) -> Option<bool> {
    match name {
        "println" | "print" | "eprintln" | "eprint" | "format" | "panic" => Some(true),
        "assert" | "assert_eq" | "assert_ne" | "debug_assert" | "debug_assert_eq"
        | "debug_assert_ne" | "vec" => Some(false),
        _ => None,
    }
}

/// Flatten token trees into a token stream, writing delimiter groups back as
/// open and close tokens.
fn flatten(trees: &[ast::TokenTree], out: &mut Vec<Token>) {
    for tree in trees {
        match tree {
            ast::TokenTree::Token(tok) => out.push(tok.clone()),
            ast::TokenTree::Delimited {
                delimiter,
                tokens,
                span,
            } => {
                out.push(Token::new(TokenKind::OpenDelim(*delimiter), *span));
                flatten(tokens, out);
                out.push(Token::new(TokenKind::CloseDelim(*delimiter), *span));
            }
        }
    }
}

/// Split a token stream at top-level commas. Returns `None` when a top-level
/// `;` appears (`vec![x; n]`), which is not a comma-separated list.
fn split_top_level(tokens: &[Token]) -> Option<Vec<Vec<Token>>> {
    let mut parts = vec![Vec::new()];
    let mut depth = 0usize;
    for tok in tokens {
        match &tok.kind {
            TokenKind::OpenDelim(_) => depth += 1,
            TokenKind::CloseDelim(_) => depth = depth.saturating_sub(1),
            TokenKind::Comma if depth == 0 => {
                parts.push(Vec::new());
                continue;
            }
            TokenKind::Semi if depth == 0 => return None,
            _ => {}
        }
        parts
            .last_mut()
            .expect("parts is never empty")
            .push(tok.clone());
    }
    if parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop(); // trailing comma
    }
    Some(parts)
}

/// The argument expressions of a macro call, parsed from its tokens.
pub(crate) fn macro_arg_exprs(
    trees: &[ast::TokenTree],
    skip_first: bool,
    file: &SourceFile,
) -> Vec<Expr> {
    // The parser may hand over the arguments bare or wrapped in the macro's
    // own delimiter group.
    let inner: &[ast::TokenTree] = match trees {
        [ast::TokenTree::Delimited { tokens, .. }] => tokens,
        other => other,
    };
    let mut flat = Vec::new();
    flatten(inner, &mut flat);
    let Some(parts) = split_top_level(&flat) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, mut part) in parts.into_iter().enumerate() {
        if skip_first && i == 0 {
            continue;
        }
        // Named format argument: `name = expr`.
        if part.len() > 2
            && matches!(part[0].kind, TokenKind::Ident)
            && matches!(part[1].kind, TokenKind::Eq)
        {
            part.drain(..2);
        }
        let Some(last) = part.last() else { continue };
        let eof_span = last.span;
        part.push(Token::new(TokenKind::Eof, eof_span));
        let mut parser = Parser::new(file, part);
        if let Some(expr) = parser.parse_standalone_expr() {
            out.push(expr);
        }
    }
    out
}

/// The argument expressions of a macro call, parsed from the file its tokens
/// came from. The module merge walks these to find module calls inside macro
/// arguments. Empty when the macro's arguments are not expressions or the
/// source text is not registered.
pub fn macro_call_arg_exprs(macro_name: &str, trees: &[ast::TokenTree]) -> Vec<Expr> {
    let Some(skip_first) = macro_arg_policy(macro_name) else {
        return Vec::new();
    };
    let Some(source_id) = first_source(trees) else {
        return Vec::new();
    };
    let Some(text) = crate::lexer::source_text_for_id(source_id) else {
        return Vec::new();
    };
    let file = SourceFile::with_registered_id(source_id, text);
    macro_arg_exprs(trees, skip_first, &file)
}

fn first_source(trees: &[ast::TokenTree]) -> Option<crate::lexer::SourceId> {
    trees.iter().find_map(|tree| match tree {
        ast::TokenTree::Token(tok) => Some(tok.span.source_id),
        ast::TokenTree::Delimited { tokens, span, .. } => {
            first_source(tokens).or(Some(span.source_id))
        }
    })
}
