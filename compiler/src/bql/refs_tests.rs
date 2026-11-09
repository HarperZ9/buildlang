// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The `refs` table: resolved uses of module-scope names.
//!
//! Two independent routes must agree. The type checker binds each use of a
//! name (the `refs` rows). The rename transform replaces every identifier
//! token spelled the name and then proves, by comparing MIR, that nothing but
//! that function changed. So when a rename is accepted, the uses the checker
//! resolved to the function are exactly the tokens spelled its name, less its
//! definition. A shadowed use is a local and is in neither.

use std::collections::BTreeSet;

use super::drift::diff_stores;
use super::store::ItemRow;
use super::*;
use crate::lexer::{Lexer, SourceFile, TokenKind};

/// Spans of identifier tokens spelled `name`.
fn ident_spans(src: &str, name: &str) -> BTreeSet<(u32, u32)> {
    let file = SourceFile::new("t.bld".to_string(), src.to_string());
    let tokens = Lexer::new(&file).tokenize().expect("lexes");
    tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Ident)
        .map(|t| (t.span.start.0, t.span.end.0))
        .filter(|(s, e)| &src[*s as usize..*e as usize] == name)
        .collect()
}

fn ref_spans(store: &Store, name: &str) -> BTreeSet<(u32, u32)> {
    store
        .refs
        .iter()
        .filter(|r| r.symbol == name)
        .map(|r| r.span)
        .collect()
}

#[test]
fn refs_resolve_calls_and_skip_a_shadowing_local() {
    let src = "fn step(x: i64) -> i64 { x + 1 }\n\
               fn uses_local() -> i64 { let step = 5; step }\n\
               fn main() ~ Console { let f = step; println!(\"{} {} {}\", step(1), f(2), uses_local()); }\n";
    let store = Store::load_source("p.bld", src).unwrap();
    let refs: Vec<_> = store.refs.iter().filter(|r| r.symbol == "step").collect();
    // `step(1)` is a call and `let f = step` a value; the local `step` in
    // uses_local is neither.
    assert_eq!(refs.len(), 2, "{refs:?}");
    assert!(refs.iter().any(|r| r.role == "call"));
    assert!(refs.iter().any(|r| r.role == "value"));
    let local = src.find("let step = 5; step").unwrap() as u32 + 14;
    assert!(
        refs.iter().all(|r| r.span.0 != local),
        "the shadowed use is a local"
    );
    for r in &refs {
        assert_eq!(&src[r.span.0 as usize..r.span.1 as usize], "step");
    }
}

#[test]
fn refs_equal_name_tokens_whenever_a_rename_is_accepted() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/programs");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("tests/programs exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "bld"))
        .collect();
    paths.sort();
    let (mut checked, mut refused, mut programs) = (0, 0, 0);
    let mut wrong = Vec::new();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(store) = Store::load_source(&name, &src) else {
            continue;
        };
        programs += 1;
        for f in store
            .functions
            .iter()
            .filter(|f| f.kind == "defined" && f.name != "main")
        {
            let Some(def) = f.name_span else { continue };
            match rename_checked(&name, &src, &f.name, "bql_refs_probe_name") {
                Ok(_) => {
                    checked += 1;
                    let mut tokens = ident_spans(&src, &f.name);
                    tokens.remove(&def);
                    let refs = ref_spans(&store, &f.name);
                    if tokens != refs {
                        let missing: Vec<_> = tokens.difference(&refs).collect();
                        let extra: Vec<_> = refs.difference(&tokens).collect();
                        wrong.push(format!(
                            "{name} {}: missing {missing:?} extra {extra:?}",
                            f.name
                        ));
                    }
                }
                Err(BqlError::Refused(_)) => refused += 1,
                Err(e) => panic!("{name} {}: {e}", f.name),
            }
        }
    }
    eprintln!(
        "bql refs: {} of {checked} accepted renames have refs equal to their name tokens \
         ({refused} refused, {programs} programs)",
        checked - wrong.len()
    );
    assert!(
        checked >= 50,
        "too few functions to mean anything: {checked}"
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_planted_wrong_ref_span_is_drift() {
    let src = "fn one() -> i64 { 1 }\nfn main() ~ Console { println!(\"{}\", one() + one()); }\n";
    let good = Store::load_source("p.bld", src).unwrap();
    assert_eq!(ref_spans(&good, "one").len(), 2);
    let mut bad = good.clone();
    bad.refs[0].span.0 += 1;
    let r = diff_stores(&bad, &good);
    assert!(!r.is_clean());
    assert!(r
        .tables
        .iter()
        .any(|t| t.table == "refs" && !t.extra.is_empty() && !t.missing.is_empty()));
    let mut dropped = good.clone();
    dropped.refs.pop();
    assert!(!diff_stores(&dropped, &good).is_clean());
}

#[test]
fn export_escapes_a_pipe_and_a_line_break() {
    let mut store =
        Store::load_source("p.bld", "fn main() ~ Console { println!(\"x\"); }\n").unwrap();
    store.items.push(ItemRow {
        name: "a|b\nc\\d".to_string(),
        kind: "struct".to_string(),
    });
    let text = export::to_qdb(&store).unwrap();
    assert!(text.starts_with("QDB 1 0\n"));
    assert!(text.contains("ROW a\\|b\\nc\\\\d|struct\n"), "{text}");
}
