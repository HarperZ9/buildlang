//! Where `resume` may stand in a handler clause.
//!
//! A clause resumes only from tail position: the clause body itself, the
//! last expression of a block, a branch of an `if` or an arm of a `match`.
//! Such a clause is lowered as a function the `perform` site calls, so
//! `perform` returns the resumed value and the code after it runs. Full
//! one-shot continuations (a `resume` anywhere) are a later step.

use crate::ast::{Block, Expr, ExprKind, StmtKind};
use crate::lexer::Span;

/// The spans of the `resume` expressions in tail position of `body`, and
/// whether some exit of `body` ends without one.
pub(crate) fn tail_resumes(body: &Expr) -> (Vec<Span>, bool) {
    let mut spans = Vec::new();
    let mut plain = false;
    walk(body, &mut spans, &mut plain);
    (spans, plain)
}

fn walk(e: &Expr, spans: &mut Vec<Span>, plain: &mut bool) {
    match &e.kind {
        ExprKind::Resume(_) => spans.push(e.span),
        ExprKind::Paren(inner) => walk(inner, spans, plain),
        ExprKind::Block(b) => walk_block(b, spans, plain),
        ExprKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            walk_block(then_branch, spans, plain);
            match else_branch {
                Some(other) => walk(other, spans, plain),
                None => *plain = true,
            }
        }
        ExprKind::Match { arms, .. } => {
            for arm in arms {
                walk(&arm.body, spans, plain);
            }
        }
        _ => *plain = true,
    }
}

fn walk_block(b: &Block, spans: &mut Vec<Span>, plain: &mut bool) {
    match b.stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Expr(e)) => walk(e, spans, plain),
        _ => *plain = true,
    }
}
