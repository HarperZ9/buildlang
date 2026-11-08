// ===============================================================================
// BUILDLANG COMPILER - MEASURED CONSTANTS WITH PROVENANCE
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! `#[measured(...)]` constants: a value that came from a measurement carries
//! where it came from and how well it is known.
//!
//! ```text
//! #[measured(source = "CODATA 2018", uncertainty = 1.5e-15)]
//! const G: f64<m^3/(kg*s^2)> = 6.67430e-11;
//! ```
//!
//! The checker refuses a `#[measured]` attribute on anything but a `const`,
//! one without a non-empty `source` string, an unknown key, and an
//! `uncertainty` that is not a non-negative number. The check receipt lists
//! every measured constant with its value as written, its unit, its source and
//! its uncertainty, so a reader of the receipt sees which numbers in a result
//! rest on outside measurements and where those came from.
//!
//! The attribute changes nothing in the emitted code. The uncertainty is a
//! recorded label: no arithmetic propagates it.

use crate::ast;
use crate::lexer::Span;
use crate::lexer::TokenKind;

/// One measured constant, as the check receipt records it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MeasuredConstant {
    /// The constant's name.
    pub name: String,
    /// The value exactly as written in the source.
    pub value: String,
    /// The canonical unit, when the type carries one (`f64<m/s>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Where the value came from.
    pub source: String,
    /// The standard uncertainty in the constant's own unit, as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncertainty: Option<String>,
}

/// A refused `#[measured]` attribute: the constant (or item) name, the
/// reason, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredError {
    pub name: String,
    pub reason: String,
    pub span: Span,
}

fn is_measured(attr: &ast::Attribute) -> bool {
    attr.path.segments.len() == 1 && attr.path.segments[0].ident.name.as_ref() == "measured"
}

fn text(source: &str, span: Span) -> &str {
    let (s, e) = (span.start.to_usize(), span.end.to_usize());
    source.get(s..e).unwrap_or("")
}

/// The `key = literal` pairs of a `#[measured(...)]` attribute, or a reason.
fn parse_pairs(attr: &ast::Attribute, source: &str) -> Result<Vec<(String, String)>, String> {
    let ast::AttrArgs::Delimited(tokens) = &attr.args else {
        return Err("expected `#[measured(source = \"...\")]`".to_string());
    };
    let flat: Vec<&crate::lexer::Token> = tokens
        .iter()
        .map(|t| match t {
            ast::TokenTree::Token(tok) => Ok(tok),
            ast::TokenTree::Delimited { .. } => Err("nested groups are not allowed".to_string()),
        })
        .collect::<Result<_, _>>()?;
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < flat.len() {
        let key = flat[i];
        if !matches!(key.kind, TokenKind::Ident) {
            return Err(format!(
                "expected a key, found `{}`",
                text(source, key.span)
            ));
        }
        let eq = flat.get(i + 1).map(|t| text(source, t.span));
        if eq != Some("=") {
            return Err(format!("expected `=` after `{}`", text(source, key.span)));
        }
        let Some(value) = flat.get(i + 2) else {
            return Err(format!("`{}` has no value", text(source, key.span)));
        };
        let mut value_text = text(source, value.span).to_string();
        let mut next = i + 3;
        // A leading minus is its own token; keep it with the number.
        if value_text == "-" {
            if let Some(num) = flat.get(i + 3) {
                value_text.push_str(text(source, num.span));
                next = i + 4;
            }
        }
        pairs.push((text(source, key.span).to_string(), value_text));
        match flat.get(next).map(|t| text(source, t.span)) {
            None => break,
            Some(",") => i = next + 1,
            Some(other) => return Err(format!("expected `,`, found `{other}`")),
        }
    }
    Ok(pairs)
}

fn unquote(s: &str) -> Option<&str> {
    s.strip_prefix('"').and_then(|r| r.strip_suffix('"'))
}

fn unit_of(ty: &ast::Type) -> Option<String> {
    match &ty.kind {
        ast::TypeKind::WithUnit { dim, .. } => Some(dim.to_canonical_string()),
        _ => None,
    }
}

/// Every measured constant in `module`, and every refused `#[measured]`.
pub fn collect(module: &ast::Module, source: &str) -> (Vec<MeasuredConstant>, Vec<MeasuredError>) {
    let mut found = Vec::new();
    let mut errors = Vec::new();
    for item in &module.items {
        let Some(attr) = item.attrs.iter().find(|a| is_measured(a)) else {
            continue;
        };
        let refuse = |name: &str, reason: String| MeasuredError {
            name: name.to_string(),
            reason,
            span: attr.span,
        };
        let ast::ItemKind::Const(def) = &item.kind else {
            errors.push(refuse(
                "<item>",
                "`#[measured]` applies only to a `const`".to_string(),
            ));
            continue;
        };
        let name = def.name.name.to_string();
        let pairs = match parse_pairs(attr, source) {
            Ok(p) => p,
            Err(reason) => {
                errors.push(refuse(&name, reason));
                continue;
            }
        };
        let mut src = None;
        let mut uncertainty = None;
        let mut bad = None;
        for (key, value) in &pairs {
            match key.as_str() {
                "source" => match unquote(value) {
                    Some(s) if !s.trim().is_empty() => src = Some(s.to_string()),
                    _ => bad = Some("`source` must be a non-empty string".to_string()),
                },
                "uncertainty" => {
                    let v = unquote(value).unwrap_or(value);
                    match v.parse::<f64>() {
                        Ok(u) if u >= 0.0 && u.is_finite() => uncertainty = Some(v.to_string()),
                        _ => {
                            bad = Some(format!(
                                "`uncertainty` must be a non-negative number, found `{value}`"
                            ))
                        }
                    }
                }
                other => {
                    bad = Some(format!(
                        "unknown key `{other}`; the keys are `source` and `uncertainty`"
                    ))
                }
            }
        }
        if let Some(reason) = bad {
            errors.push(refuse(&name, reason));
            continue;
        }
        let Some(source_name) = src else {
            errors.push(refuse(
                &name,
                "a measured constant needs `source = \"...\"` naming where the value came from"
                    .to_string(),
            ));
            continue;
        };
        let value = def
            .value
            .as_ref()
            .map(|v| text(source, v.span).trim().to_string())
            .unwrap_or_default();
        found.push(MeasuredConstant {
            name,
            value,
            unit: unit_of(&def.ty),
            source: source_name,
            uncertainty,
        });
    }
    (found, errors)
}
