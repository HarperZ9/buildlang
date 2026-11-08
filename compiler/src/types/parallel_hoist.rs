// ===============================================================================
// BUILDLANG TYPE SYSTEM - HOIST AN EFFECT OUT OF A PARALLEL BODY (M5)
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! The exact edits behind `BL0406`'s fix. The effectful top-level statements
//! of the body move into a serial `for` loop placed after the statement that
//! holds the `par_for` / `par_map` call. The loop binds the body's parameters
//! for each index, so a statement that reads only those parameters prints the
//! same bytes in the same order as the serial program. The body loses its
//! effect row; the enclosing function gains the effects if it lacks them,
//! which widens its authority, so the fix is never machine-applicable.
//!
//! No fix is offered when an effect sits anywhere other than a top-level
//! statement of the body (a `let` initializer, a nested block, the tail).

use std::collections::BTreeSet;

use crate::ast::{self, Expr, ExprKind, StmtKind};
use crate::diagnostics::{Fix, TextEdit};
use crate::lexer::{BytePos, Span};

use super::parallel::FnTable;
use super::parallel_walk as walk;

/// One `par_for` / `par_map` call and where it sits.
pub(super) struct HoistSite<'s> {
    pub(super) kind: &'static str,
    pub(super) args: &'s [Expr],
    pub(super) stmt: &'s ast::Stmt,
    pub(super) enclosing: &'s ast::FnDef,
}

fn text_of(src: &str, span: Span) -> Option<String> {
    src.get(span.start.0 as usize..span.end.0 as usize)
        .map(|t| t.trim().to_string())
}

fn point(at: u32, like: Span) -> Span {
    Span::new(BytePos(at), BytePos(at), like.source_id)
}

/// The indentation of the line holding byte `at`.
fn indent_at(src: &str, at: usize) -> String {
    let line_start = src[..at].rfind('\n').map_or(0, |i| i + 1);
    src[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// Delete a statement; when it is alone on its line, delete the whole line.
fn delete_stmt(src: &str, span: Span) -> TextEdit {
    let (s, e) = (span.start.0 as usize, span.end.0 as usize);
    let line_start = src[..s].rfind('\n').map_or(0, |i| i + 1);
    let rest = &src[e..];
    let line_end = rest.find('\n').map(|i| e + i + 1);
    let alone = src[line_start..s].trim().is_empty()
        && line_end.map_or(rest.trim().is_empty(), |le| src[e..le].trim().is_empty());
    let (from, to) = match (alone, line_end) {
        (true, Some(le)) => (line_start, le),
        _ => (s, e),
    };
    TextEdit {
        span: Span::new(BytePos(from as u32), BytePos(to as u32), span.source_id),
        replacement: String::new(),
    }
}

/// Delete ` ~ A + B` from a signature.
fn delete_effect_row(src: &str, f: &ast::FnDef) -> Option<TextEdit> {
    let first = f.sig.effects.first()?.span;
    let last = f.sig.effects.last()?.span;
    let before = &src[..first.start.0 as usize];
    let tilde = before.rfind('~')?;
    let from = before[..tilde].trim_end().len();
    Some(TextEdit {
        span: Span::new(BytePos(from as u32), last.end, first.source_id),
        replacement: String::new(),
    })
}

fn stmt_effects(stmt: &ast::Stmt, table: &FnTable<'_>) -> BTreeSet<String> {
    let mut exprs = Vec::new();
    walk::stmt_exprs(stmt, &mut exprs);
    exprs
        .iter()
        .flat_map(|e| walk::direct_effects(e, &table.rows))
        .collect()
}

fn stmt_reads(stmt: &ast::Stmt) -> BTreeSet<String> {
    let mut exprs = Vec::new();
    walk::stmt_exprs(stmt, &mut exprs);
    exprs.iter().flat_map(walk::read_names).collect()
}

/// Split a body block into the statements to move; `None` when an effect
/// would stay behind.
fn movable<'b>(block: &'b ast::Block, table: &FnTable<'_>) -> Option<Vec<&'b ast::Stmt>> {
    let mut moved = Vec::new();
    let n = block.stmts.len();
    for (i, stmt) in block.stmts.iter().enumerate() {
        let effects = stmt_effects(stmt, table);
        if effects.is_empty() {
            continue;
        }
        let tail = i + 1 == n && matches!(stmt.kind, StmtKind::Expr(_));
        let top = matches!(stmt.kind, StmtKind::Semi(_) | StmtKind::Macro { .. });
        if tail || !top {
            return None;
        }
        moved.push(stmt);
    }
    (!moved.is_empty()).then_some(moved)
}

/// The name a `let name = par_map(..);` statement binds.
fn result_binding(stmt: &ast::Stmt) -> Option<String> {
    let StmtKind::Local(l) = &stmt.kind else {
        return None;
    };
    match &l.pattern.kind {
        ast::PatternKind::Ident { name, .. } => Some(name.name.to_string()),
        _ => None,
    }
}

/// The loop header and the `let` lines that bind the body's parameters.
fn loop_head(
    site: &HoistSite<'_>,
    src: &str,
    params: &[String],
    reads: &BTreeSet<String>,
    inner: &str,
) -> Option<(String, String)> {
    let arg = |i: usize| site.args.get(i).and_then(|a| text_of(src, a.span));
    let mut lets = String::new();
    match site.kind {
        "par_for" => {
            let (ctx, idx) = (params.first()?, params.get(1)?);
            if reads.contains(ctx) {
                lets.push_str(&format!("{inner}let {ctx} = {};\n", arg(1)?));
            }
            Some((format!("for {idx} in 0..{}", arg(0)?), lets))
        }
        _ => {
            let x = params.first()?;
            let xs = arg(0)?;
            let xs = xs.strip_prefix('&').unwrap_or(&xs).trim().to_string();
            if reads.contains(x) {
                lets.push_str(&format!("{inner}let {x} = {xs}[__i];\n"));
            }
            // The result vector has the input's length and is an owned local,
            // so its `len()` builds even when the input is a `&Vec` parameter.
            let len_of = result_binding(site.stmt).unwrap_or(xs);
            Some((format!("for __i in 0..{len_of}.len()"), lets))
        }
    }
}

/// The exact edits for `BL0406`, or `None` when the effect cannot be moved.
pub(super) fn hoist_fix(
    table: &FnTable<'_>,
    site: &HoistSite<'_>,
    row: &[String],
    named: Option<&ast::FnDef>,
    closure: Option<(&[ast::ClosureParam], &Expr)>,
    notes: &mut Vec<String>,
) -> Option<Fix> {
    let source_id = site.stmt.span.source_id;
    let src = crate::lexer::source_text_for_id(source_id)?;
    let (block, params, body_bound) = match (named, closure) {
        (Some(f), _) => {
            let mut bound = walk::bound_names(&walk::body_expr(f)?);
            let params: Vec<String> = f
                .sig
                .params
                .iter()
                .map(|p| {
                    let mut n = BTreeSet::new();
                    walk::pattern_names(&p.pattern, &mut n);
                    n.into_iter().next().unwrap_or_default()
                })
                .collect();
            params.iter().for_each(|p| {
                bound.remove(p);
            });
            (f.body.as_deref()?, params, bound)
        }
        (None, Some((cparams, cbody))) => {
            let ExprKind::Block(b) = &cbody.kind else {
                return None;
            };
            let params: Vec<String> = cparams
                .iter()
                .map(|p| {
                    let mut n = BTreeSet::new();
                    walk::pattern_names(&p.pattern, &mut n);
                    n.into_iter().next().unwrap_or_default()
                })
                .collect();
            let mut bound = walk::bound_names(cbody);
            params.iter().for_each(|p| {
                bound.remove(p);
            });
            (&**b, params, bound)
        }
        _ => return None,
    };
    let moved = movable(block, table)?;
    let reads: BTreeSet<String> = moved.iter().flat_map(|s| stmt_reads(s)).collect();
    let indent = indent_at(&src, site.stmt.span.start.0 as usize);
    let inner = format!("{indent}    ");
    let (head, lets) = loop_head(site, &src, &params, &reads, &inner)?;
    let mut text = format!("\n{indent}{head} {{\n{lets}");
    for stmt in &moved {
        let mut t = text_of(&src, stmt.span)?;
        if !t.ends_with(';') && !t.ends_with('}') {
            t.push(';');
        }
        text.push_str(&format!("{inner}{t}\n"));
    }
    text.push_str(&format!("{indent}}}"));

    let mut edits = Vec::new();
    if let Some(f) = named {
        edits.push(delete_effect_row(&src, f)?);
    }
    edits.extend(moved.iter().map(|s| delete_stmt(&src, s.span)));
    edits.push(TextEdit {
        span: point(site.stmt.span.end.0, site.stmt.span),
        replacement: text,
    });
    let declared: BTreeSet<String> = site
        .enclosing
        .sig
        .effects
        .iter()
        .filter_map(|p| p.last_ident().map(|i| i.name.to_string()))
        .collect();
    let missing: Vec<String> = row
        .iter()
        .filter(|r| !declared.contains(*r))
        .cloned()
        .collect();
    if let Some(widen) = super::check::effect_row_fix(site.enclosing, &missing) {
        edits.extend(widen.edits);
    }
    edits.sort_by_key(|e| (e.span.start.0, e.span.end.0));

    let unbound: Vec<String> = reads.intersection(&body_bound).cloned().collect();
    if !unbound.is_empty() {
        notes.push(format!(
            "the moved statements read {} from the body; bind them in the loop by hand",
            unbound
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Some(Fix {
        message: format!(
            "hoist `{}` out of the parallel body into a serial loop after the call",
            row.join(" + ")
        ),
        edits,
        machine_applicable: false,
    })
}
