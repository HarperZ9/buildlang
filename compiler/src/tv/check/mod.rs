// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The independent checker for C-backend certificates.
//!
//! Inputs are three texts: the BuildLang source, the emitted C and the
//! certificate. The checker re-derives everything a certificate claims: it
//! hashes each function on both sides, parses the C itself, executes source
//! and C symbolically and proves the simulation in [`relate`]. It imports
//! nothing from the code generator or the MIR; it shares the lexer, parser
//! and AST, and the verifier's term and proof modules. A test holds that
//! boundary.

pub mod c_exec;
pub mod c_parse;
pub mod relate;
pub mod src_exec;

use crate::ast::{FnDef, ItemKind};
use crate::lexer::{Lexer, SourceFile};
use crate::parser::Parser;
use crate::verify::float;
use crate::verify::infer::Sigs;
use crate::verify::term::Term;
use crate::verify::ty::Ty;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// SHA-256 of a text, as hex.
pub fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The checker's verdict on one function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Certified { segments: usize },
    NotCertified { reason: String },
    OutsideSubset { reason: String },
}

#[derive(Clone, Debug)]
pub struct FnResult {
    pub name: String,
    pub status: Status,
}

/// Check every function the certificate lists.
pub fn check(source: &str, c: &str, cert: &Value) -> Result<Vec<FnResult>, String> {
    let sf = SourceFile::new("source.bld", source.to_string());
    let tokens = Lexer::new(&sf)
        .tokenize()
        .map_err(|e| format!("lexer: {e}"))?;
    let module = Parser::new(&sf, tokens)
        .parse()
        .map_err(|e| format!("parser: {}", e.message()))?;
    let mut fns: HashMap<String, &FnDef> = HashMap::new();
    let mut spans = HashMap::new();
    for item in &module.items {
        if let ItemKind::Function(f) = &item.kind {
            fns.insert(f.name.name.to_string(), f);
            spans.insert(f.name.name.to_string(), item.span);
        }
    }
    let sigs = Sigs::from_module_tv(&module.items);
    let consts = relate::Consts {
        source: source_consts(&module.items),
        c: c_parse::c_consts(c),
    };
    let entries = cert["functions"]
        .as_array()
        .ok_or("certificate has no functions")?;
    let mut callees = HashMap::new();
    for e in entries {
        let name = e["name"].as_str().unwrap_or_default();
        if let Some(text) = c_parse::function_text(c, name) {
            if let Ok(f) = c_parse::parse_function(text) {
                if let Some(t) = c_exec::c_ty(&f.ret) {
                    callees.insert(name.to_string(), t);
                }
            }
        }
    }
    let mut out = Vec::new();
    for e in entries {
        let name = e["name"]
            .as_str()
            .ok_or("a function without a name")?
            .to_string();
        let status = check_one(&name, e, &fns, &spans, &sf, &sigs, c, &callees, &consts);
        out.push(FnResult { name, status });
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn check_one(
    name: &str,
    e: &Value,
    fns: &HashMap<String, &FnDef>,
    spans: &HashMap<String, crate::lexer::Span>,
    sf: &SourceFile,
    sigs: &Sigs,
    c: &str,
    callees: &HashMap<String, Ty>,
    consts: &relate::Consts,
) -> Status {
    let not = |reason: String| Status::NotCertified { reason };
    let Some(f) = fns.get(name) else {
        return Status::OutsideSubset {
            reason: "not defined in this source file".into(),
        };
    };
    if e["in_subset"] != Value::Bool(true) {
        return Status::OutsideSubset {
            reason: e["reason"]
                .as_str()
                .unwrap_or("outside the subset")
                .to_string(),
        };
    }
    let src_text = sf.slice(spans[name]);
    if e["source_sha256"].as_str() != Some(sha256_hex(src_text).as_str()) {
        return not("the source hash does not match".into());
    }
    let Some(c_text) = c_parse::function_text(c, name) else {
        return not("the C file does not define it".into());
    };
    if e["c_sha256"].as_str() != Some(sha256_hex(c_text).as_str()) {
        return not("the C hash does not match".into());
    }
    let cf = match c_parse::parse_function(c_text) {
        Ok(cf) => cf,
        Err(why) => {
            return Status::OutsideSubset {
                reason: format!("C line outside the parser: {why}"),
            }
        }
    };
    let hints = relate::Hints {
        loop_heads: e["loop_heads"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        vars: e["vars"]
            .as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
    };
    let fns_ref: HashMap<String, &FnDef> = fns.iter().map(|(k, v)| (k.clone(), *v)).collect();
    match relate::check(f, &fns_ref, sigs, &cf, &hints, callees, consts) {
        Ok(segments) => Status::Certified { segments },
        Err(why) => match why.strip_prefix("mismatch: ") {
            Some(m) => not(m.to_string()),
            None => Status::OutsideSubset { reason: why },
        },
    }
}

/// The source's named constants with a literal value, and the built-in
/// `PI`, `E` and `TAU` where the program defines no constant of that name.
fn source_consts(items: &[crate::ast::Item]) -> HashMap<String, (Term, Ty)> {
    use crate::ast::{ExprKind, Literal, UnaryOp};
    let mut out = HashMap::new();
    for item in items {
        let ItemKind::Const(k) = &item.kind else {
            continue;
        };
        let name = k.name.name.to_string();
        let Some(ty) = Ty::from_ast_tv(&k.ty) else {
            out.remove(&name);
            continue;
        };
        let (negate, lit) = match k.value.as_deref().map(|e| &e.kind) {
            Some(ExprKind::Unary {
                op: UnaryOp::Neg,
                expr,
            }) => (true, &expr.kind),
            Some(other) => (false, other),
            None => continue,
        };
        let term = match (lit, ty) {
            (ExprKind::Literal(Literal::Int { value, .. }), t) if t.is_int() => {
                let v = i128::try_from(*value)
                    .ok()
                    .map(|v| if negate { -v } else { v });
                match v.filter(|v| t.holds(*v)) {
                    Some(v) => Term::Int(v),
                    None => continue,
                }
            }
            (ExprKind::Literal(Literal::Float { value, .. }), t) if t.is_float() => {
                float::fconst(t, if negate { -*value } else { *value })
            }
            (ExprKind::Literal(Literal::Bool(b)), Ty::Bool) if !negate => Term::Bool(*b),
            _ => continue,
        };
        out.insert(name, (term, ty));
    }
    let user: Vec<String> = items
        .iter()
        .filter_map(|i| i.name().map(|n| n.name.to_string()))
        .collect();
    for (n, v) in [
        ("PI", std::f64::consts::PI),
        ("E", std::f64::consts::E),
        ("TAU", std::f64::consts::TAU),
    ] {
        if !user.iter().any(|u| u == n) {
            out.insert(n.to_string(), (float::fconst(Ty::F64, v), Ty::F64));
        }
    }
    out
}

/// The `buildlang-tv-report/v1` JSON and the counts behind it.
pub fn report(file: &str, results: &[FnResult]) -> Value {
    let mut certified = 0;
    let mut not_certified = 0;
    let mut outside = 0;
    let rows: Vec<Value> = results
        .iter()
        .map(|r| {
            let (status, detail) = match &r.status {
                Status::Certified { segments } => {
                    certified += 1;
                    ("certified", json!({ "segments": segments }))
                }
                Status::NotCertified { reason } => {
                    not_certified += 1;
                    ("not-certified", json!({ "reason": reason }))
                }
                Status::OutsideSubset { reason } => {
                    outside += 1;
                    ("outside-subset", json!({ "reason": reason }))
                }
            };
            json!({ "name": r.name, "status": status, "detail": detail })
        })
        .collect();
    json!({
        "schema": "buildlang-tv-report/v1",
        "file": file,
        "functions": rows,
        "coverage": {
            "compiled": results.len(),
            "certified": certified,
            "not_certified": not_certified,
            "outside_subset": outside,
        },
    })
}
