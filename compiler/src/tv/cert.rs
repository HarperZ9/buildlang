// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Certificates for the C backend's output, one per function.
//!
//! A certificate names the function, hashes its source and its emitted C,
//! and states the relation the checker must prove: which C label heads each
//! source loop (source pre-order) and which C variable holds each source
//! variable. The relation is a claim; a wrong one only makes the checker
//! refuse the function. The generator reads the C text it is given; the
//! checker in `check` does not use this module.

use crate::ast::{ItemKind, Module, PatternKind};
use crate::lexer::SourceFile;
use crate::verify::ty::Ty;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The emitted text of function `name`: header line to closing brace.
fn c_function<'c>(c: &'c str, name: &str) -> Option<&'c str> {
    let needle_static = format!(" {name}(");
    let mut pos = 0;
    while let Some(i) = c[pos..].find(&needle_static) {
        let at = pos + i;
        let line_start = c[..at].rfind('\n').map_or(0, |p| p + 1);
        let line_end = c[at..].find('\n').map_or(c.len(), |p| at + p);
        let line = &c[line_start..line_end];
        if line.trim_end().ends_with(") {") && !line.starts_with(' ') {
            let end = c[line_start..].find("\n}")? + line_start + 2;
            return Some(&c[line_start..end]);
        }
        pos = at + 1;
    }
    None
}

/// The loop heads of a function body: targets of the back edges a
/// depth-first walk from the entry finds, in text order. Unreachable blocks
/// (the backend leaves some behind) are never walked.
fn back_edge_targets(body: &str) -> Vec<String> {
    // Blocks in text order: (label, successors, text position).
    let mut blocks: Vec<(String, Vec<String>, bool)> = vec![("<entry>".into(), Vec::new(), false)];
    for line in body.lines().skip(1) {
        let t = line.trim();
        if let Some(l) = t.strip_suffix(':') {
            if !blocks.last().unwrap().2 {
                let next = l.to_string();
                blocks.last_mut().unwrap().1.push(next);
            }
            blocks.push((l.to_string(), Vec::new(), false));
            continue;
        }
        let last = blocks.last_mut().unwrap();
        if last.2 {
            continue;
        }
        for part in t.split("goto ").skip(1) {
            let target = part.split(';').next().unwrap_or("").trim();
            last.1.push(target.to_string());
        }
        if t.starts_with("goto ") || t.starts_with("if (") || t.starts_with("return") {
            last.2 = true;
        }
    }
    let index = |l: &str| blocks.iter().position(|b| b.0 == l);
    let mut heads: Vec<usize> = Vec::new();
    let mut state = vec![0u8; blocks.len()]; // 0 new, 1 on stack, 2 done
    let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
    state[0] = 1;
    while let Some((b, i)) = stack.pop() {
        if let Some(succ) = blocks[b].1.get(i) {
            stack.push((b, i + 1));
            if let Some(n) = index(succ) {
                match state[n] {
                    0 => {
                        state[n] = 1;
                        stack.push((n, 0));
                    }
                    1 if !heads.contains(&n) => heads.push(n),
                    _ => {}
                }
            }
        } else {
            state[b] = 2;
        }
    }
    heads.sort();
    heads.into_iter().map(|i| blocks[i].0.clone()).collect()
}

fn local_names(block: &crate::ast::Block, out: &mut Vec<String>) {
    use crate::ast::{ExprKind, StmtKind};
    for s in &block.stmts {
        match &s.kind {
            StmtKind::Local(l) => {
                if let PatternKind::Ident { name, .. } = &l.pattern.kind {
                    out.push(name.name.to_string());
                }
            }
            StmtKind::Expr(e) | StmtKind::Semi(e) => match &e.kind {
                ExprKind::While { body, .. }
                | ExprKind::Loop { body, .. }
                | ExprKind::Block(body) => local_names(body, out),
                ExprKind::If { then_branch, .. } => local_names(then_branch, out),
                _ => {}
            },
            _ => {}
        }
    }
}

/// The `buildlang-tv-cert/v1` certificate for every function of `module`
/// (parsed from `sf`) that `c` defines.
pub fn generate(file: &str, sf: &SourceFile, module: &Module, c: &str) -> Value {
    let mut fns = Vec::new();
    for item in &module.items {
        let ItemKind::Function(f) = &item.kind else {
            continue;
        };
        let name = f.name.name.to_string();
        let Some(c_text) = c_function(c, &name) else {
            continue;
        };
        let scalar_sig = f.sig.params.iter().all(|p| Ty::from_ast(&p.ty).is_some())
            && f.sig.return_ty.as_deref().and_then(Ty::from_ast).is_some()
            && f.generics.params.is_empty();
        let mut entry = Map::new();
        entry.insert("name".into(), json!(name));
        entry.insert("source_sha256".into(), json!(sha(sf.slice(item.span))));
        entry.insert("c_sha256".into(), json!(sha(c_text)));
        entry.insert("in_subset".into(), json!(scalar_sig));
        if !scalar_sig {
            entry.insert(
                "reason".into(),
                json!("a parameter or the return type is not an integer or bool"),
            );
        }
        entry.insert("loop_heads".into(), json!(back_edge_targets(c_text)));
        let mut names: Vec<String> = Vec::new();
        for p in &f.sig.params {
            if let PatternKind::Ident { name, .. } = &p.pattern.kind {
                names.push(name.name.to_string());
            }
        }
        if let Some(b) = f.body.as_deref() {
            local_names(b, &mut names);
        }
        let mut vars = Map::new();
        for n in names {
            let decl = format!(" {n};");
            let is_param = c_text
                .lines()
                .next()
                .is_some_and(|h| h.contains(&format!(" {n},")) || h.contains(&format!(" {n})")));
            if is_param || c_text.lines().any(|l| l.trim_end().ends_with(&decl)) {
                vars.insert(n.clone(), json!(n));
            }
        }
        entry.insert("vars".into(), Value::Object(vars));
        fns.push(Value::Object(entry));
    }
    json!({
        "schema": "buildlang-tv-cert/v1",
        "source": file,
        "c_sha256": sha(c),
        "functions": fns,
    })
}
