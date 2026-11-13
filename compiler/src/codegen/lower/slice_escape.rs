//! A function that returns `&[T]` must not return a view of its own locals.
//!
//! `&v[a..b]` on a Vec and `slice::from_raw_parts(p, n)` build a fat pointer
//! into memory the pointer does not own. When that memory belongs to the
//! function (a local, a by-value parameter or a temporary) it is freed on
//! return and the caller gets a dangling pointer. `buildc check` refuses that.
//!
//! The rule is syntactic and errs toward refusal. Every returned value (the
//! body's tail and each `return`) gets a class: `Rooted` when it is a view of
//! a parameter passed by reference, as a slice or as a raw pointer, `Value`
//! when it holds no borrow at all, `Local(x)` when it borrows the local `x`,
//! and `Unknown` otherwise. Only `Rooted` and `Value` may be returned. A `let`
//! binding takes the class of its initialiser unless it is `mut`; names bound
//! by `match`, `if let`, `for`, closures and handlers are `Unknown`.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::{self, ExprKind, PatternKind, StmtKind, TypeKind};
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::types::parallel_walk::children;

#[derive(Clone, Debug, PartialEq)]
enum Class {
    Value,
    Rooted,
    Unknown,
    Local(Arc<str>),
}

/// The class of two values flowing into one place: the worst of them.
fn join(a: Class, b: Class) -> Class {
    fn rank(c: &Class) -> u8 {
        match c {
            Class::Value => 0,
            Class::Rooted => 1,
            Class::Unknown => 2,
            Class::Local(_) => 3,
        }
    }
    if rank(&b) > rank(&a) {
        b
    } else {
        a
    }
}

type Env = HashMap<Arc<str>, Class>;

/// Methods whose result holds no borrow of the receiver.
const OWNED_RESULT_METHODS: &[&str] = &[
    "len",
    "is_empty",
    "capacity",
    "to_vec",
    "to_string",
    "to_owned",
];

fn returns_slice_ref(ty: &ast::Type) -> bool {
    match &ty.kind {
        TypeKind::Ref { ty, .. } => matches!(ty.kind, TypeKind::Slice(_)),
        TypeKind::Paren(inner) => returns_slice_ref(inner),
        _ => false,
    }
}

fn is_borrow_type(ty: &ast::Type) -> bool {
    match &ty.kind {
        TypeKind::Ref { .. } | TypeKind::Ptr { .. } | TypeKind::Slice(_) => true,
        TypeKind::Paren(inner) => is_borrow_type(inner),
        _ => false,
    }
}

fn bind_pattern(p: &ast::Pattern, class: &Class, env: &mut Env) {
    match &p.kind {
        PatternKind::Ident {
            mutability,
            name,
            subpattern,
        } => {
            let c = if mutability.is_mut() {
                Class::Unknown
            } else {
                class.clone()
            };
            env.insert(name.name.clone(), c);
            if let Some(s) = subpattern {
                bind_pattern(s, &Class::Unknown, env);
            }
        }
        PatternKind::Tuple(ps)
        | PatternKind::Slice(ps)
        | PatternKind::Or(ps)
        | PatternKind::TupleStruct { patterns: ps, .. } => ps
            .iter()
            .for_each(|q| bind_pattern(q, &Class::Unknown, env)),
        PatternKind::Struct { fields, .. } => {
            for f in fields {
                env.insert(f.name.name.clone(), Class::Unknown);
                bind_pattern(&f.pattern, &Class::Unknown, env);
            }
        }
        PatternKind::Ref { pattern, .. }
        | PatternKind::Box(pattern)
        | PatternKind::Paren(pattern) => bind_pattern(pattern, &Class::Unknown, env),
        _ => {}
    }
}

struct Checker<'a> {
    fn_name: &'a str,
    bad: Option<(Class, ast::Span)>,
}

impl Checker<'_> {
    fn returned(&mut self, e: &ast::Expr, env: &Env) {
        let c = self.value(e, env);
        if matches!(c, Class::Local(_) | Class::Unknown) && self.bad.is_none() {
            self.bad = Some((c, e.span));
        }
    }

    /// What borrowing the place `e` (`&e`, `e.method()`, `e[..]`) points into.
    fn place(&mut self, e: &ast::Expr, env: &Env) -> Class {
        match &e.kind {
            ExprKind::Paren(x) => self.place(x, env),
            ExprKind::Ident(id) => Self::place_of_name(&id.name, env),
            ExprKind::Path(p) if p.segments.len() == 1 => {
                Self::place_of_name(&p.segments[0].ident.name, env)
            }
            ExprKind::Field { expr, .. }
            | ExprKind::TupleField { expr, .. }
            | ExprKind::Index { expr, .. } => self.place(expr, env),
            ExprKind::Deref(x) => match self.value(x, env) {
                Class::Value => Class::Unknown,
                c => c,
            },
            _ => match self.value(e, env) {
                // A temporary dies at the end of the statement.
                Class::Value => Class::Unknown,
                c => c,
            },
        }
    }

    fn place_of_name(name: &Arc<str>, env: &Env) -> Class {
        match env.get(name) {
            Some(Class::Value) => Class::Local(name.clone()),
            Some(c) => c.clone(),
            None => Class::Unknown,
        }
    }

    fn value(&mut self, e: &ast::Expr, env: &Env) -> Class {
        match &e.kind {
            ExprKind::Literal(_) | ExprKind::Macro { .. } => {
                self.scan(e, env);
                Class::Value
            }
            ExprKind::Ident(id) => env.get(&id.name).cloned().unwrap_or(Class::Unknown),
            ExprKind::Path(p) if p.segments.len() == 1 => env
                .get(&p.segments[0].ident.name)
                .cloned()
                .unwrap_or(Class::Unknown),
            ExprKind::Path(_) => Class::Value,
            ExprKind::Paren(x) | ExprKind::Try(x) | ExprKind::TypeAscription { expr: x, .. } => {
                self.value(x, env)
            }
            ExprKind::Ref { expr, .. } => match &expr.kind {
                ExprKind::Array(xs) if xs.is_empty() => Class::Rooted,
                _ => self.place(expr, env),
            },
            ExprKind::Field { .. } | ExprKind::TupleField { .. } | ExprKind::Index { .. } => {
                self.scan(e, env);
                self.place(e, env)
            }
            ExprKind::Deref(x) => self.value(x, env),
            ExprKind::Unary { .. } | ExprKind::Binary { .. } => {
                self.scan(e, env);
                Class::Value
            }
            ExprKind::Cast { expr, ty } if is_borrow_type(ty) => self.value(expr, env),
            ExprKind::Cast { .. } => {
                self.scan(e, env);
                Class::Value
            }
            ExprKind::Call { args, .. } => args
                .iter()
                .fold(Class::Value, |acc, a| join(acc, self.value(a, env))),
            ExprKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let mut c = self.place(receiver, env);
                for a in args {
                    c = join(c, self.value(a, env));
                }
                if OWNED_RESULT_METHODS.contains(&&*method.name) {
                    Class::Value
                } else {
                    c
                }
            }
            ExprKind::Array(xs) | ExprKind::Tuple(xs) => xs
                .iter()
                .fold(Class::Value, |acc, x| join(acc, self.value(x, env))),
            ExprKind::Block(b) | ExprKind::Unsafe(b) => self.block(b, env),
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.scan(condition, env);
                let t = self.block(then_branch, env);
                match else_branch {
                    Some(x) => join(t, self.value(x, env)),
                    None => t,
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.scan(scrutinee, env);
                let mut c = Class::Value;
                for arm in arms {
                    let mut inner = env.clone();
                    bind_pattern(&arm.pattern, &Class::Unknown, &mut inner);
                    if let Some(g) = &arm.guard {
                        self.scan(g, &inner);
                    }
                    c = join(c, self.value(&arm.body, &inner));
                }
                c
            }
            ExprKind::Return(x) => {
                if let Some(x) = x {
                    self.returned(x, env);
                }
                Class::Value
            }
            _ => {
                self.scan(e, env);
                Class::Unknown
            }
        }
    }

    /// Visit a block in its own scope and give the class of its tail.
    fn block(&mut self, b: &ast::Block, env: &Env) -> Class {
        let mut env = env.clone();
        let last = b.stmts.len().saturating_sub(1);
        let mut tail = Class::Value;
        for (i, stmt) in b.stmts.iter().enumerate() {
            match &stmt.kind {
                StmtKind::Local(l) => {
                    let c = match &l.init {
                        Some(init) => {
                            if let Some(d) = &init.diverge {
                                self.scan(d, &env);
                            }
                            self.value(&init.expr, &env)
                        }
                        None => Class::Unknown,
                    };
                    bind_pattern(&l.pattern, &c, &mut env);
                }
                StmtKind::Expr(x) if i == last => tail = self.value(x, &env),
                StmtKind::Expr(x) | StmtKind::Semi(x) => {
                    self.value(x, &env);
                }
                _ => {}
            }
        }
        tail
    }

    /// Look for `return` anywhere under `e`, binding the names that loops,
    /// `if let`, closures and handlers introduce.
    fn scan(&mut self, e: &ast::Expr, env: &Env) {
        match &e.kind {
            ExprKind::Return(_)
            | ExprKind::Block(_)
            | ExprKind::Unsafe(_)
            | ExprKind::If { .. }
            | ExprKind::Match { .. } => {
                self.value(e, env);
            }
            ExprKind::Loop { body, .. } | ExprKind::Async { body, .. } => {
                self.block(body, env);
            }
            ExprKind::While {
                condition, body, ..
            } => {
                self.scan(condition, env);
                self.block(body, env);
            }
            ExprKind::IfLet {
                pattern,
                expr,
                then_branch,
                else_branch,
            } => {
                self.scan(expr, env);
                let mut inner = env.clone();
                bind_pattern(pattern, &Class::Unknown, &mut inner);
                self.block(then_branch, &inner);
                if let Some(x) = else_branch {
                    self.scan(x, env);
                }
            }
            ExprKind::WhileLet {
                pattern,
                expr: iter,
                body,
                ..
            }
            | ExprKind::For {
                pattern,
                iter,
                body,
                ..
            } => {
                self.scan(iter, env);
                let mut inner = env.clone();
                bind_pattern(pattern, &Class::Unknown, &mut inner);
                self.block(body, &inner);
            }
            // A `return` in a closure leaves the closure, not the function.
            ExprKind::Closure { .. } => {}
            ExprKind::Handle { handlers, body, .. } => {
                self.block(body, env);
                for h in handlers {
                    let mut inner = env.clone();
                    for p in &h.params {
                        bind_pattern(&p.pattern, &Class::Unknown, &mut inner);
                    }
                    self.scan(&h.body, &inner);
                }
            }
            _ => {
                for c in children(e) {
                    self.scan(&c, env);
                }
            }
        }
    }
}

/// Refuse a function returning `&[T]` that returns a view of its own memory.
pub(super) fn check_fn(f: &ast::FnDef) -> CodegenResult<()> {
    let (Some(ret), Some(body)) = (&f.sig.return_ty, &f.body) else {
        return Ok(());
    };
    if !returns_slice_ref(ret) {
        return Ok(());
    }
    let mut env = Env::new();
    for p in &f.sig.params {
        let c = if is_borrow_type(&p.ty) {
            Class::Rooted
        } else {
            Class::Value
        };
        bind_pattern(&p.pattern, &c, &mut env);
    }
    let mut checker = Checker {
        fn_name: &f.name.name,
        bad: None,
    };
    let tail = checker.block(body, &env);
    if checker.bad.is_none() && matches!(tail, Class::Local(_) | Class::Unknown) {
        let span = body.stmts.last().map(|s| s.span).unwrap_or(body.span);
        checker.bad = Some((tail, span));
    }
    let Some((class, span)) = checker.bad else {
        return Ok(());
    };
    let name = checker.fn_name;
    let message = match &class {
        Class::Local(local) => format!(
            "`{name}` returns a slice view of the local `{local}`, which is freed when `{name}` returns"
        ),
        _ => format!(
            "`{name}` returns a slice that cannot be shown to outlive the call: a returned `&[T]` must be a view of a parameter passed by reference or as a slice"
        ),
    };
    let what = match &class {
        Class::Local(local) => format!("`{local}`"),
        _ => "the data".to_string(),
    };
    Err(CodegenError::Rejected {
        message,
        location: (span.end.0 > span.start.0).then_some((span.start.0, span.end.0)),
        help: Some(format!(
            "return the Vec itself (`-> Vec<T>`), or take {what} as a parameter (`v: &[T]` or `v: &Vec<T>`) and return a view of it"
        )),
    })
}
