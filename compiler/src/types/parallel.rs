// ===============================================================================
// BUILDLANG TYPE SYSTEM - PARALLEL BODIES (M5)
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! The checker side of `par_for(n, ctx, f)` and `par_map(xs, f)`.
//!
//! A body may run on any thread, in any order, with no lock, so the checker
//! proves three things from the source before the C backend splits the work:
//!
//! - its effect row is empty (`BL0406`): no `Console`, `FileSystem` or other
//!   capability, no user effect, and no call to a function that declares one.
//!   The fix hoists the effectful statements out of the body into a serial
//!   loop after the call, in index order;
//! - it cannot write shared state (`BL0515`): no `&mut` or `*mut` parameter,
//!   no write to a `static mut` (directly or through a function it calls), and
//!   for a closure, no write to a variable from outside the closure;
//! - the checker can see it (`BL0407`): a named function, or a closure that
//!   captures nothing and writes its parameter and return types.
//!
//! The body's own effect row is checked by the ordinary effect pass, so a
//! function with an empty row is proved effect-free there.

use std::collections::{BTreeSet, HashMap};

use crate::ast::{self, Expr, ExprKind, ItemKind, Mutability, TypeKind};
use crate::lexer::Span;

use super::error::{TypeError, TypeErrorWithSpan};
use super::parallel_hoist::{hoist_fix, HoistSite};
use super::parallel_walk as walk;

/// What the check knows about the program's functions.
pub(super) struct FnTable<'a> {
    pub(super) fns: HashMap<String, &'a ast::FnDef>,
    pub(super) rows: HashMap<String, Vec<String>>,
    static_writers: HashMap<String, String>,
}

fn collect_items<'a>(
    items: &'a [ast::Item],
    fns: &mut HashMap<String, &'a ast::FnDef>,
    statics: &mut BTreeSet<String>,
) {
    for item in items {
        match &item.kind {
            ItemKind::Function(f) => {
                fns.insert(f.name.name.to_string(), f);
            }
            ItemKind::Static(s) if s.mutability == Mutability::Mutable => {
                statics.insert(s.name.name.to_string());
            }
            ItemKind::Mod(m) => {
                if let Some(content) = &m.content {
                    collect_items(&content.items, fns, statics);
                }
            }
            _ => {}
        }
    }
}

impl<'a> FnTable<'a> {
    fn new(module: &'a ast::Module) -> Self {
        let mut fns = HashMap::new();
        let mut statics = BTreeSet::new();
        collect_items(&module.items, &mut fns, &mut statics);
        let rows = fns
            .iter()
            .filter(|(_, f)| !f.sig.effects.is_empty())
            .map(|(n, f)| {
                let row = f
                    .sig
                    .effects
                    .iter()
                    .filter_map(|p| p.last_ident().map(|i| i.name.to_string()))
                    .collect();
                (n.clone(), row)
            })
            .collect();
        // A function writes a static when its body assigns one it does not
        // shadow, or calls a function that does (fixpoint over direct calls).
        let mut static_writers: HashMap<String, String> = HashMap::new();
        let mut calls: HashMap<String, BTreeSet<String>> = HashMap::new();
        for (name, f) in &fns {
            let Some(body) = walk::body_expr(f) else {
                continue;
            };
            let mut local = walk::bound_names(&body);
            for p in &f.sig.params {
                walk::pattern_names(&p.pattern, &mut local);
            }
            let hit = walk::written_names(&body)
                .into_iter()
                .find(|w| statics.contains(w) && !local.contains(w));
            if let Some(s) = hit {
                static_writers.insert(name.clone(), format!("it writes `static mut {s}`"));
            }
            calls.insert(name.clone(), walk::called_names(&body));
        }
        loop {
            let mut changed = false;
            for (name, callees) in &calls {
                if static_writers.contains_key(name) {
                    continue;
                }
                if let Some(c) = callees.iter().find(|c| static_writers.contains_key(*c)) {
                    let why = format!("it calls `{c}`, which writes shared state");
                    static_writers.insert(name.clone(), why);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Self {
            fns,
            rows,
            static_writers,
        }
    }
}

/// Every parallel-body error in the module.
pub(super) fn parallel_body_errors(module: &ast::Module) -> Vec<TypeErrorWithSpan> {
    let table = FnTable::new(module);
    let mut errors = Vec::new();
    let mut names: Vec<&String> = table.fns.keys().collect();
    names.sort();
    for name in names {
        let f = table.fns[name];
        let Some(body) = &f.body else {
            continue;
        };
        for stmt in &body.stmts {
            let mut exprs = Vec::new();
            walk::stmt_exprs(stmt, &mut exprs);
            for root in &exprs {
                for e in walk::descendants(root) {
                    if let ExprKind::Call { func, args } = &e.kind {
                        check_call(&table, f, stmt, func, args, &mut errors);
                    }
                }
            }
        }
    }
    errors
}

fn simple_name(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(i) => Some(i.name.to_string()),
        ExprKind::Path(p) if p.is_simple() => p.last_ident().map(|i| i.name.to_string()),
        _ => None,
    }
}

fn par_kind(table: &FnTable<'_>, func: &Expr) -> Option<&'static str> {
    let name = simple_name(func)?;
    let kind = match name.as_str() {
        "par_for" => "par_for",
        "par_map" => "par_map",
        _ => return None,
    };
    (!table.fns.contains_key(kind)).then_some(kind)
}

fn check_call(
    table: &FnTable<'_>,
    enclosing: &ast::FnDef,
    stmt: &ast::Stmt,
    func: &Expr,
    args: &[Expr],
    errors: &mut Vec<TypeErrorWithSpan>,
) {
    let Some(kind) = par_kind(table, func) else {
        return;
    };
    let Some(body) = args.last() else {
        return;
    };
    let site = HoistSite {
        kind,
        args,
        stmt,
        enclosing,
    };
    let mut body_expr = body;
    while let ExprKind::Paren(inner) = &body_expr.kind {
        body_expr = inner;
    }
    match &body_expr.kind {
        ExprKind::Path(_) | ExprKind::Ident(_) if simple_name(body_expr).is_some() => {
            let name = simple_name(body_expr).unwrap_or_default();
            match table.fns.get(&name) {
                Some(f) => check_named(table, &site, &name, f, body.span, errors),
                None => errors.push(unknown(
                    kind,
                    format!("`{name}` is not a function the checker can see"),
                    body.span,
                )),
            }
        }
        ExprKind::Closure {
            params,
            return_type,
            body: cbody,
            ..
        } => check_closure(
            table,
            &site,
            params,
            return_type.is_some(),
            cbody,
            body.span,
            errors,
        ),
        _ => errors.push(unknown(
            kind,
            "the body is an expression, not a function name or closure".to_string(),
            body.span,
        )),
    }
}

fn unknown(kind: &str, detail: String, span: Span) -> TypeErrorWithSpan {
    let mut e = TypeErrorWithSpan::new(
        TypeError::ParallelBodyUnknown {
            call: kind.to_string(),
            detail,
        },
        span,
    );
    e.help = Some(format!(
        "pass a named function, or a closure that captures nothing and writes its types; data the body reads goes through `par_for`'s context argument"
    ));
    e
}

fn shared_write(kind: &str, body: &str, detail: String, span: Span) -> TypeErrorWithSpan {
    let mut e = TypeErrorWithSpan::new(
        TypeError::ParallelSharedWrite {
            call: kind.to_string(),
            body: body.to_string(),
            detail,
        },
        span,
    );
    e.help = Some(format!(
        "a `{kind}` body returns its result; the runtime writes it to the index's own slot. Take inputs by value or `&`, and fold the results after the call"
    ));
    e
}

fn mut_param(params: &[(String, Option<&ast::Type>)]) -> Option<String> {
    params.iter().find_map(|(name, ty)| {
        let ty = (*ty)?;
        match &ty.kind {
            TypeKind::Ref {
                mutability: Mutability::Mutable,
                ..
            } => Some(format!("parameter `{name}` is `&mut`")),
            TypeKind::Ptr {
                mutability: Mutability::Mutable,
                ..
            } => Some(format!("parameter `{name}` is `*mut`")),
            _ => None,
        }
    })
}

fn param_list<'b>(
    pats: impl Iterator<Item = (&'b ast::Pattern, Option<&'b ast::Type>)>,
) -> Vec<(String, Option<&'b ast::Type>)> {
    pats.map(|(p, t)| {
        let mut n = BTreeSet::new();
        walk::pattern_names(p, &mut n);
        (n.into_iter().next().unwrap_or_else(|| "_".to_string()), t)
    })
    .collect()
}

fn check_named(
    table: &FnTable<'_>,
    site: &HoistSite<'_>,
    name: &str,
    f: &ast::FnDef,
    span: Span,
    errors: &mut Vec<TypeErrorWithSpan>,
) {
    let params = param_list(f.sig.params.iter().map(|p| (&p.pattern, Some(&*p.ty))));
    if let Some(row) = table.rows.get(name) {
        errors.push(effect_error(table, site, name, row, Some(f), None, span));
    }
    if let Some(detail) = mut_param(&params) {
        errors.push(shared_write(site.kind, name, detail, span));
    } else if let Some(detail) = table.static_writers.get(name) {
        errors.push(shared_write(site.kind, name, detail.clone(), span));
    }
}

fn check_closure(
    table: &FnTable<'_>,
    site: &HoistSite<'_>,
    params: &[ast::ClosureParam],
    has_ret: bool,
    cbody: &Expr,
    span: Span,
    errors: &mut Vec<TypeErrorWithSpan>,
) {
    let label = "closure";
    let plist = param_list(params.iter().map(|p| (&p.pattern, p.ty.as_deref())));
    let closure = Expr {
        kind: ExprKind::Closure {
            is_move: false,
            is_async: false,
            params: params.to_vec(),
            return_type: None,
            body: Box::new(cbody.clone()),
        },
        span,
        id: cbody.id,
        attrs: Vec::new(),
    };
    let inner = walk::bound_names(&closure);
    let effects = walk::direct_effects(cbody, &table.rows);
    if !effects.is_empty() {
        let row: Vec<String> = effects.into_iter().collect();
        errors.push(effect_error(
            table,
            site,
            label,
            &row,
            None,
            Some((params, cbody)),
            span,
        ));
    }
    let outer = outer_names(site.enclosing);
    let writes: Vec<String> = walk::written_names(cbody)
        .into_iter()
        .filter(|w| !inner.contains(w))
        .collect();
    if let Some(detail) = mut_param(&plist) {
        errors.push(shared_write(site.kind, label, detail, span));
    } else if let Some(w) = writes.first() {
        let what = if outer.contains(w) {
            "a variable from outside the closure"
        } else {
            "shared state"
        };
        errors.push(shared_write(
            site.kind,
            label,
            format!("it writes `{w}`, {what}"),
            span,
        ));
    } else if let Some(c) = walk::called_names(cbody)
        .iter()
        .find_map(|c| table.static_writers.get(c).map(|d| (c.clone(), d.clone())))
    {
        errors.push(shared_write(
            site.kind,
            label,
            format!("it calls `{}`: {}", c.0, c.1),
            span,
        ));
    }
    let captures: Vec<String> = walk::read_names(cbody)
        .into_iter()
        .filter(|n| outer.contains(n) && !inner.contains(n))
        .collect();
    if !captures.is_empty() {
        errors.push(unknown(
            site.kind,
            format!("the closure captures {}", quote_list(&captures)),
            span,
        ));
    } else if !has_ret || plist.iter().any(|(_, t)| t.is_none()) {
        errors.push(unknown(
            site.kind,
            "a parallel closure must write its parameter and return types".to_string(),
            span,
        ));
    }
}

/// Names bound in the enclosing function: its parameters and every `let`.
fn outer_names(f: &ast::FnDef) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for p in &f.sig.params {
        walk::pattern_names(&p.pattern, &mut out);
    }
    if let Some(b) = walk::body_expr(f) {
        out.extend(walk::bound_names(&b));
    }
    out
}

fn quote_list(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn effect_error(
    table: &FnTable<'_>,
    site: &HoistSite<'_>,
    body: &str,
    row: &[String],
    named: Option<&ast::FnDef>,
    closure: Option<(&[ast::ClosureParam], &Expr)>,
    span: Span,
) -> TypeErrorWithSpan {
    let effects = row.join(" + ");
    let mut e = TypeErrorWithSpan::new(
        TypeError::ParallelBodyEffect {
            call: site.kind.to_string(),
            body: body.to_string(),
            effects: effects.clone(),
        },
        span,
    );
    e.help = Some(format!(
        "compute the values in parallel with no effect, then perform `{effects}` in a serial loop after the call, in index order"
    ));
    e.fix = hoist_fix(table, site, row, named, closure, &mut e.notes);
    if e.fix.is_none() {
        e.notes.push(
            "no exact edit: the effect is not in a top-level statement of the body".to_string(),
        );
    }
    e
}
