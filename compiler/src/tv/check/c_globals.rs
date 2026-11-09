// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Top-level definitions of the C file the checker reads: constants and
//! struct definitions.

use super::c_expr::parse_expr;
use super::c_parse::{is_ident, CExpr};

/// The top-level `const` definitions of a C file whose type is a modelled
/// scalar and whose initializer is a literal: name to value and type.
pub fn c_consts(
    c: &str,
) -> std::collections::HashMap<String, (crate::verify::term::Term, crate::verify::ty::Ty)> {
    use crate::verify::{float, term::Term, ty::Ty};
    let mut out = std::collections::HashMap::new();
    for line in c.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let rest = line
            .trim_end()
            .strip_prefix("static ")
            .unwrap_or(line.trim_end());
        let Some(rest) = rest.strip_prefix("const ") else {
            continue;
        };
        let Some((decl, init)) = rest.split_once(" = ") else {
            continue;
        };
        let Some(init) = init.strip_suffix(';') else {
            continue;
        };
        let mut words: Vec<&str> = decl.split_whitespace().collect();
        let (Some(name), Some(ty)) = (words.pop(), words.pop()) else {
            continue;
        };
        if !words.is_empty() || !is_ident(name) {
            continue;
        }
        let Some(t) = super::c_exec::c_ty(ty) else {
            continue;
        };
        let term = match (parse_expr(init), t) {
            (Ok(CExpr::Int(v)), t) if t.is_int() && t.holds(v) => Term::Int(v),
            (Ok(CExpr::Bool(b)), Ty::Bool) => Term::Bool(b),
            (Ok(CExpr::Float(text)), Ty::Float { bits: 64 }) if !text.ends_with(['f', 'F']) => {
                match text.parse::<f64>() {
                    Ok(v) => float::fconst(Ty::F64, v),
                    Err(_) => continue,
                }
            }
            (Ok(CExpr::Float(text)), Ty::Float { bits: 32 }) => {
                match text.trim_end_matches(['f', 'F']).parse::<f32>() {
                    Ok(v) => float::fconst32_bits(v.to_bits()),
                    Err(_) => continue,
                }
            }
            _ => continue,
        };
        out.insert(name.to_string(), (term, t));
    }
    out
}

/// The C file's struct definitions: `struct Name {` then one `type field;`
/// per line, then `};`. Name to fields (name, C type) in C order.
pub fn c_structs(c: &str) -> std::collections::HashMap<String, Vec<(String, String)>> {
    let mut out = std::collections::HashMap::new();
    let mut cur: Option<(String, Vec<(String, String)>)> = None;
    for line in c.lines() {
        let t = line.trim();
        if let Some((name, fields)) = cur.take() {
            if t == "};" {
                out.insert(name, fields);
                continue;
            }
            let words: Vec<&str> = t
                .strip_suffix(';')
                .unwrap_or("")
                .split_whitespace()
                .collect();
            if let [ty, f] = words.as_slice() {
                if is_ident(f) && line.starts_with(' ') {
                    let mut fields = fields;
                    fields.push((f.to_string(), ty.to_string()));
                    cur = Some((name, fields));
                }
            }
            continue;
        }
        if let Some(name) = line
            .strip_prefix("struct ")
            .and_then(|r| r.strip_suffix(" {"))
        {
            if is_ident(name) {
                cur = Some((name.to_string(), Vec::new()));
            }
        }
    }
    out
}
