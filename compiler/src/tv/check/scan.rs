// SPDX-License-Identifier: AGPL-3.0-only/n// Copyright (c) 2022-2026 Zain Dana Harper./n/n//! Scans of a source body that decide, before any symbolic run, whether/n//! the checker can model it./n
/// The first macro in `body` other than the print macros. Others (`vec!`,
/// `assert!`) may expand to loops and calls the source does not show.
pub(super) fn other_macro(body: &crate::ast::Block) -> Option<String> {
    use crate::ast::{ExprKind, StmtKind};
    use crate::verify::infer::children;
    fn expr(e: &crate::ast::Expr) -> Option<String> {
        if let ExprKind::Macro { path, .. } = &e.kind {
            let n = path.segments.last()?.ident.name.to_string();
            if !matches!(n.as_str(), "print" | "println" | "eprint" | "eprintln") {
                return Some(n);
            }
        }
        children(e).into_iter().find_map(expr)
    }
    body.stmts.iter().find_map(|s| match &s.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => expr(e),
        StmtKind::Local(l) => l.init.as_ref().and_then(|i| expr(&i.expr)),
        _ => None,
    })
}

/// Every `while` and `loop` in `body`, at any depth.
pub(super) fn count_loops(body: &crate::ast::Block) -> usize {
    use crate::ast::{ExprKind, StmtKind};
    use crate::verify::infer::children;
    fn expr(e: &crate::ast::Expr) -> usize {
        let here = matches!(
            e.kind,
            ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::For { .. }
        ) as usize;
        here + children(e).into_iter().map(expr).sum::<usize>()
    }
    body.stmts
        .iter()
        .map(|s| match &s.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => expr(e),
            StmtKind::Local(l) => l.init.as_ref().map_or(0, |i| expr(&i.expr)),
            _ => 0,
        })
        .sum()
}
