// ===============================================================================
// BUILDLANG PARSER - LAWS AND CONTRACTS
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! `law` declarations and `requires` / `ensures` clauses.
//!
//! All three words are contextual: they mean something only in the positions
//! parsed here, so existing programs that use them as names keep working.
//!
//! - `law name(params) requires P { body }` becomes an ordinary function
//!   returning `bool`, carrying the attribute `#[law]`.
//! - `fn f(params) -> T requires P ensures Q { .. }` keeps `f` unchanged and
//!   queues two hidden functions, `__bl_requires__f(params) -> bool { P }` and
//!   `__bl_ensures__f(params, result: T) -> bool { Q }`.
//!
//! Lowering specs to ordinary functions means the type checker checks every
//! spec with its usual diagnostics, and `buildc verify` reads them back by name.

use super::{ParseError, ParseErrorKind, ParseResult, Parser};
use crate::ast::*;
use crate::lexer::{Span, TokenKind};

/// Name prefix of the hidden function holding a function's `requires` clauses.
pub const REQUIRES_PREFIX: &str = "__bl_requires__";
/// Name prefix of the hidden function holding a function's `ensures` clauses.
pub const ENSURES_PREFIX: &str = "__bl_ensures__";
/// The attribute that marks a function produced from a `law` declaration.
pub const LAW_ATTR: &str = "law";

/// Clauses parsed after a signature.
#[derive(Default)]
pub(super) struct Clauses {
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
}

impl<'a> Parser<'a> {
    /// Whether the current token is the identifier `word`.
    pub(super) fn at_word(&self, word: &str) -> bool {
        matches!(self.current_kind(), TokenKind::Ident)
            && self.source.slice(self.current_span()) == word
    }

    /// Whether the parser stands at `law <ident>`.
    pub(super) fn at_law_decl(&self) -> bool {
        self.at_word("law") && matches!(self.peek().kind, TokenKind::Ident)
    }

    /// Parse `requires E` and `ensures E` clauses, in any order and number.
    pub(super) fn parse_contract_clauses(&mut self) -> ParseResult<Clauses> {
        let mut clauses = Clauses::default();
        loop {
            let is_requires = self.at_word("requires");
            if !is_requires && !self.at_word("ensures") {
                return Ok(clauses);
            }
            self.advance();
            let saved = self.restrictions;
            self.restrictions.no_struct_literal = true;
            let expr = self.parse_expr();
            self.restrictions = saved;
            let expr = expr?;
            if is_requires {
                clauses.requires.push(expr);
            } else {
                clauses.ensures.push(expr);
            }
        }
    }

    /// Parse `law name(params) [requires ..] { body }` after attributes.
    pub(super) fn parse_law(
        &mut self,
        mut attrs: Vec<Attribute>,
        vis: Visibility,
        start: Span,
    ) -> ParseResult<Item> {
        let law_span = self.current_span();
        self.advance(); // `law`
        let mut fn_def = self.parse_fn_after_keyword(false, false, false)?;
        if fn_def.sig.return_ty.is_some() {
            return Err(ParseError::new(
                ParseErrorKind::Expected {
                    expected: "a law body without a return type (a law returns bool)".into(),
                    found: "`->`".into(),
                },
                law_span,
            ));
        }
        if fn_def.body.is_none() {
            return Err(self.error_expected("a law body `{ .. }`"));
        }
        fn_def.sig.return_ty = Some(Box::new(named_type("bool", law_span)));
        attrs.push(Attribute {
            path: Path::from_ident(Ident::new(LAW_ATTR, law_span)),
            args: AttrArgs::Empty,
            is_inner: false,
            span: law_span,
        });
        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
        Ok(Item::new(
            ItemKind::Function(Box::new(fn_def)),
            vis,
            attrs,
            span,
        ))
    }

    /// Queue the hidden spec functions for `fn_def`'s clauses.
    pub(super) fn queue_contract_items(&mut self, fn_def: &FnDef, clauses: Clauses) {
        let span = fn_def.name.span;
        let has_self = fn_def.sig.params.iter().any(
            |p| matches!(&p.pattern.kind, PatternKind::Ident { name, .. } if &*name.name == "self"),
        );
        if has_self || !fn_def.generics.params.is_empty() {
            self.errors.push(ParseError::new(
                ParseErrorKind::Expected {
                    expected: "requires/ensures on a free, non-generic function".into(),
                    found: "a method or a generic function".into(),
                },
                span,
            ));
            return;
        }
        let base = &fn_def.name.name;
        if let Some(req) = conjoin(clauses.requires) {
            let params = fn_def.sig.params.clone();
            let name = format!("{REQUIRES_PREFIX}{base}");
            self.pending_items.push(spec_fn(&name, params, req, span));
        }
        if let Some(ens) = conjoin(clauses.ensures) {
            let mut params = fn_def.sig.params.clone();
            let ret = fn_def
                .sig
                .return_ty
                .clone()
                .unwrap_or_else(|| Box::new(Type::new(TypeKind::Tuple(Vec::new()), span)));
            params.push(Param {
                attrs: Vec::new(),
                pattern: Pattern::new(
                    PatternKind::Ident {
                        mutability: Mutability::Immutable,
                        name: Ident::new("result", span),
                        subpattern: None,
                    },
                    span,
                ),
                ty: ret,
                default: None,
                span,
            });
            let name = format!("{ENSURES_PREFIX}{base}");
            self.pending_items.push(spec_fn(&name, params, ens, span));
        }
    }

    /// Move queued spec items into `items`.
    pub(super) fn drain_pending_items(&mut self, items: &mut Vec<Item>) {
        items.append(&mut self.pending_items);
    }
}

fn named_type(name: &str, span: Span) -> Type {
    Type::new(
        TypeKind::Path(Path::from_ident(Ident::new(name, span))),
        span,
    )
}

fn conjoin(mut exprs: Vec<Expr>) -> Option<Expr> {
    if exprs.is_empty() {
        return None;
    }
    let mut acc = exprs.remove(0);
    for e in exprs {
        let span = acc.span.merge(&e.span);
        acc = Expr::new(
            ExprKind::Binary {
                op: BinOp::And,
                left: Box::new(acc),
                right: Box::new(e),
            },
            span,
        );
    }
    Some(acc)
}

fn spec_fn(name: &str, params: Vec<Param>, body: Expr, span: Span) -> Item {
    let body_span = body.span;
    let block = Block::new(
        vec![crate::ast::Stmt::new(
            StmtKind::Expr(Box::new(body)),
            body_span,
        )],
        body_span,
    );
    let def = FnDef {
        name: Ident::new(name, span),
        generics: Generics::default(),
        sig: FnSig {
            params,
            return_ty: Some(Box::new(named_type("bool", span))),
            ..FnSig::default()
        },
        body: Some(Box::new(block)),
    };
    Item::new(
        ItemKind::Function(Box::new(def)),
        Visibility::Private,
        Vec::new(),
        span,
    )
}
