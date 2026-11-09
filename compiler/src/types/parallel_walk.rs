// ===============================================================================
// BUILDLANG TYPE SYSTEM - AST WALK FOR THE PARALLEL-BODY CHECK (M5)
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Small, scope-insensitive AST queries the parallel-body check needs: the
//! child expressions of an expression, the names a pattern binds, the names an
//! expression reads, the effects an expression performs directly, and the
//! names it writes. Macro arguments are parsed back into expressions so a
//! `println!("{}", read_file(p))` counts both effects and every name it reads.

use std::collections::{BTreeSet, HashMap};

use crate::ast::{self, Expr, ExprKind, Pattern, PatternKind, StmtKind};

use super::capabilities::{capability_effect_for_call, capability_effect_for_macro};

/// Methods that change their receiver in place.
const MUTATING_METHODS: &[&str] = &[
    "push", "pop", "insert", "remove", "clear", "release", "set", "sort", "extend", "truncate",
    "push_str", "append", "swap", "reverse", "retain", "drain", "fill",
];

fn macro_name(path: &ast::Path) -> String {
    path.segments
        .last()
        .map(|s| s.ident.name.to_string())
        .unwrap_or_default()
}

fn simple_name(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(i) => Some(i.name.to_string()),
        ExprKind::Path(p) if p.is_simple() => p.last_ident().map(|i| i.name.to_string()),
        _ => None,
    }
}

/// Every expression directly inside a block, in order.
pub(super) fn block_exprs(block: &ast::Block, out: &mut Vec<Expr>) {
    for stmt in &block.stmts {
        stmt_exprs(stmt, out);
    }
}

/// The expressions of one statement. A macro statement contributes its
/// parsed arguments wrapped back into a macro expression's arguments.
pub(super) fn stmt_exprs(stmt: &ast::Stmt, out: &mut Vec<Expr>) {
    match &stmt.kind {
        StmtKind::Local(l) => {
            if let Some(init) = &l.init {
                out.push((*init.expr).clone());
                if let Some(d) = &init.diverge {
                    out.push((**d).clone());
                }
            }
        }
        StmtKind::Expr(e) | StmtKind::Semi(e) => out.push((**e).clone()),
        StmtKind::Macro { path, tokens, .. } => out.push(Expr {
            kind: ExprKind::Macro {
                path: path.clone(),
                delimiter: crate::lexer::Delimiter::Paren,
                tokens: tokens.clone(),
            },
            span: stmt.span,
            id: stmt.id,
            attrs: Vec::new(),
        }),
        StmtKind::Item(_) | StmtKind::Empty => {}
    }
}

/// The immediate child expressions of `e`.
pub(super) fn children(e: &Expr) -> Vec<Expr> {
    let mut out: Vec<Expr> = Vec::new();
    let one = |x: &Expr, out: &mut Vec<Expr>| out.push(x.clone());
    match &e.kind {
        ExprKind::Array(xs) | ExprKind::Tuple(xs) => out.extend(xs.iter().cloned()),
        ExprKind::ArrayRepeat { element, count } => {
            one(element, &mut out);
            one(count, &mut out);
        }
        ExprKind::Struct { fields, rest, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    one(v, &mut out);
                }
            }
            if let Some(r) = rest {
                one(r, &mut out);
            }
        }
        ExprKind::Unary { expr, .. }
        | ExprKind::Field { expr, .. }
        | ExprKind::TupleField { expr, .. }
        | ExprKind::Deref(expr)
        | ExprKind::Ref { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::TypeAscription { expr, .. }
        | ExprKind::Try(expr)
        | ExprKind::Await(expr)
        | ExprKind::Paren(expr)
        | ExprKind::AIInfer { expr, .. } => one(expr, &mut out),
        ExprKind::Binary { left, right, .. } => {
            one(left, &mut out);
            one(right, &mut out);
        }
        ExprKind::Assign { target, value, .. } => {
            one(target, &mut out);
            one(value, &mut out);
        }
        ExprKind::Index { expr, index } => {
            one(expr, &mut out);
            one(index, &mut out);
        }
        ExprKind::Call { func, args } => {
            one(func, &mut out);
            out.extend(args.iter().cloned());
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            one(receiver, &mut out);
            out.extend(args.iter().cloned());
        }
        ExprKind::Perform { args, .. } => out.extend(args.iter().cloned()),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            one(condition, &mut out);
            block_exprs(then_branch, &mut out);
            if let Some(e) = else_branch {
                one(e, &mut out);
            }
        }
        ExprKind::IfLet {
            expr,
            then_branch,
            else_branch,
            ..
        } => {
            one(expr, &mut out);
            block_exprs(then_branch, &mut out);
            if let Some(e) = else_branch {
                one(e, &mut out);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            one(scrutinee, &mut out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    one(g, &mut out);
                }
                one(&arm.body, &mut out);
            }
        }
        ExprKind::While {
            condition, body, ..
        } => {
            one(condition, &mut out);
            block_exprs(body, &mut out);
        }
        ExprKind::WhileLet { expr, body, .. }
        | ExprKind::For {
            iter: expr, body, ..
        } => {
            one(expr, &mut out);
            block_exprs(body, &mut out);
        }
        ExprKind::Loop { body, .. }
        | ExprKind::Block(body)
        | ExprKind::Unsafe(body)
        | ExprKind::Async { body, .. } => block_exprs(body, &mut out),
        ExprKind::Handle { handlers, body, .. } => {
            block_exprs(body, &mut out);
            for h in handlers {
                one(&h.body, &mut out);
            }
        }
        ExprKind::Return(Some(x)) | ExprKind::Resume(Some(x)) => one(x, &mut out),
        ExprKind::Break { value: Some(x), .. } => one(x, &mut out),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                one(s, &mut out);
            }
            if let Some(x) = end {
                one(x, &mut out);
            }
        }
        ExprKind::Closure { body, .. } => one(body, &mut out),
        ExprKind::AIQuery { prompt, options } => {
            one(prompt, &mut out);
            for (_, v) in options {
                one(v, &mut out);
            }
        }
        ExprKind::Macro { path, tokens, .. } => {
            out.extend(super::macro_args::macro_call_arg_exprs(
                &macro_name(path),
                tokens,
            ));
        }
        _ => {}
    }
    out
}

/// Every expression in the tree rooted at `e`, `e` first.
pub(super) fn descendants(e: &Expr) -> Vec<Expr> {
    let mut out = Vec::new();
    let mut stack = vec![e.clone()];
    while let Some(x) = stack.pop() {
        let kids = children(&x);
        out.push(x);
        stack.extend(kids.into_iter().rev());
    }
    out
}

/// The names a pattern binds.
pub(super) fn pattern_names(p: &Pattern, out: &mut BTreeSet<String>) {
    match &p.kind {
        PatternKind::Ident {
            name, subpattern, ..
        } => {
            out.insert(name.name.to_string());
            if let Some(s) = subpattern {
                pattern_names(s, out);
            }
        }
        PatternKind::Tuple(ps) | PatternKind::Slice(ps) | PatternKind::Or(ps) => {
            ps.iter().for_each(|x| pattern_names(x, out))
        }
        PatternKind::TupleStruct { patterns, .. } => {
            patterns.iter().for_each(|x| pattern_names(x, out))
        }
        PatternKind::Struct { fields, .. } => {
            fields.iter().for_each(|f| pattern_names(&f.pattern, out))
        }
        PatternKind::Ref { pattern, .. }
        | PatternKind::Box(pattern)
        | PatternKind::Paren(pattern) => pattern_names(pattern, out),
        _ => {}
    }
}

/// Every name bound anywhere inside `e` (lets, loop and arm patterns,
/// closure parameters). Scope-insensitive on purpose: it errs toward treating
/// a name as local, and the C lowering repeats the capture check exactly.
pub(super) fn bound_names(e: &Expr) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for x in descendants(e) {
        match &x.kind {
            ExprKind::Block(b)
            | ExprKind::Loop { body: b, .. }
            | ExprKind::While { body: b, .. }
            | ExprKind::Unsafe(b) => block_bound(b, &mut out),
            ExprKind::If { then_branch, .. } => block_bound(then_branch, &mut out),
            ExprKind::IfLet {
                pattern,
                then_branch,
                ..
            } => {
                pattern_names(pattern, &mut out);
                block_bound(then_branch, &mut out);
            }
            ExprKind::WhileLet { pattern, body, .. } | ExprKind::For { pattern, body, .. } => {
                pattern_names(pattern, &mut out);
                block_bound(body, &mut out);
            }
            ExprKind::Match { arms, .. } => arms
                .iter()
                .for_each(|a| pattern_names(&a.pattern, &mut out)),
            ExprKind::Closure { params, .. } => params
                .iter()
                .for_each(|p| pattern_names(&p.pattern, &mut out)),
            _ => {}
        }
    }
    out
}

fn block_bound(b: &ast::Block, out: &mut BTreeSet<String>) {
    for s in &b.stmts {
        if let StmtKind::Local(l) = &s.kind {
            pattern_names(&l.pattern, out);
        }
    }
}

/// Every simple name read anywhere inside `e`.
pub(super) fn read_names(e: &Expr) -> BTreeSet<String> {
    descendants(e).iter().filter_map(simple_name).collect()
}

/// The effects `e` performs directly: capability macros and calls, `perform`,
/// and calls to functions whose declared row is not empty (`rows`).
pub(super) fn direct_effects(e: &Expr, rows: &HashMap<String, Vec<String>>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for x in descendants(e) {
        match &x.kind {
            ExprKind::Macro { path, .. } => {
                if let Some(eff) = capability_effect_for_macro(&macro_name(path)) {
                    out.insert(eff.to_string());
                }
            }
            ExprKind::Perform { effect, .. } => {
                out.insert(effect.name.to_string());
            }
            ExprKind::Call { func, .. } => {
                if let Some(name) = simple_name(func) {
                    if let Some(row) = rows.get(&name) {
                        out.extend(row.iter().cloned());
                    } else if let Some(eff) = capability_effect_for_call(&name) {
                        out.insert(eff.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The root variable an assignment target or method receiver names.
fn root_name(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Field { expr, .. }
        | ExprKind::TupleField { expr, .. }
        | ExprKind::Index { expr, .. }
        | ExprKind::Deref(expr)
        | ExprKind::Paren(expr) => root_name(expr),
        _ => simple_name(e),
    }
}

/// The names `e` writes: assignment roots and receivers of mutating methods.
pub(super) fn written_names(e: &Expr) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for x in descendants(e) {
        match &x.kind {
            ExprKind::Assign { target, .. } => out.extend(root_name(target)),
            ExprKind::MethodCall {
                receiver, method, ..
            } if MUTATING_METHODS.contains(&method.name.as_ref()) => {
                out.extend(root_name(receiver))
            }
            _ => {}
        }
    }
    out
}

/// The names of functions `e` calls directly.
pub(super) fn called_names(e: &Expr) -> BTreeSet<String> {
    descendants(e)
        .iter()
        .filter_map(|x| match &x.kind {
            ExprKind::Call { func, .. } => simple_name(func),
            _ => None,
        })
        .collect()
}

/// A function body as one expression, for the queries above.
pub(super) fn body_expr(f: &ast::FnDef) -> Option<Expr> {
    let b = f.body.as_ref()?;
    Some(Expr {
        kind: ExprKind::Block(b.clone()),
        span: b.span,
        id: b.id,
        attrs: Vec::new(),
    })
}
