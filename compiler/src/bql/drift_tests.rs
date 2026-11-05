// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Sampled agreement between the incremental store and a full rebuild.
//!
//! Acceptance rule: an incremental view ships only when it agrees with the
//! rebuild on every sampled change. Planted faults show the diff can see
//! a wrong incremental store at all.

use super::drift::{diff_stores, rename_rows, verify_against_source};
use super::*;

const NEW: &str = "bql_drift_renamed_fn";
const SEED: u64 = 0x2026_1104;
const MAX_PROGRAMS: usize = 48;

/// A small deterministic generator, so a failing sample replays.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

/// Every `.bld` under tests/programs that loads as a single file, in name order.
fn corpus() -> Vec<(String, String, Store)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/programs");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("tests/programs exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "bld"))
        .collect();
    paths.sort();
    let mut rng = Lcg(SEED);
    let mut out = Vec::new();
    while !paths.is_empty() && out.len() < MAX_PROGRAMS {
        let path = paths.remove(rng.next(paths.len()));
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        if src.contains(NEW) {
            continue;
        }
        if let Ok(store) = Store::load_source(&name, &src) {
            out.push((name, src, store));
        }
    }
    out
}

fn candidates(store: &Store) -> Vec<String> {
    store
        .functions
        .iter()
        .filter(|f| f.kind == "defined" && f.name_span.is_some() && f.name != "main")
        .map(|f| f.name.clone())
        .collect()
}

#[test]
fn incremental_rename_store_agrees_with_rebuild_on_every_sample() {
    let mut rng = Lcg(SEED ^ 0xb1d);
    let (mut samples, mut agree, mut refused) = (0, 0, 0);
    let mut drift = Vec::new();
    for (name, src, store) in corpus() {
        let fns = candidates(&store);
        if fns.is_empty() {
            continue;
        }
        let old = &fns[rng.next(fns.len())];
        match rename_checked(&name, &src, old, NEW) {
            Ok(o) => {
                samples += 1;
                let report = diff_stores(&o.store_incremental, &o.store_rebuilt);
                assert_eq!(report.is_clean(), o.receipt.store_incremental_agrees);
                if report.is_clean() {
                    agree += 1;
                } else {
                    drift.push(format!("{name} {old}:\n{}", report.render()));
                }
            }
            Err(BqlError::Refused(_)) => refused += 1,
            Err(e) => panic!("{name} {old}: {e}"),
        }
    }
    eprintln!("bql drift: {agree} of {samples} sampled renames agree; {refused} refused");
    assert!(samples >= 20, "too few samples to mean anything: {samples}");
    assert!(drift.is_empty(), "{}", drift.join("\n"));
}

#[test]
fn planted_faults_in_the_incremental_store_are_caught() {
    let src = "fn step(x: i64) -> i64 { x + 1 }\n\
               fn twice(x: i64) -> i64 { step(step(x)) }\n\
               fn main() ~ Console { println!(\"{}\", twice(1)); }\n";
    let o = rename_checked("p.bld", src, "step", NEW).expect("rename");
    assert!(o.receipt.store_incremental_agrees);
    let good = &o.store_incremental;

    // Fault 1: the calls table is not renamed.
    let mut skip_calls = good.clone();
    skip_calls.calls = rename_rows(&o.store_rebuilt, NEW, "step", &[], "", "").calls;
    let r = diff_stores(&skip_calls, &o.store_rebuilt);
    assert!(!r.is_clean());
    assert!(r
        .tables
        .iter()
        .any(|t| t.table == "calls" && !t.extra.is_empty()));

    // Fault 2: names substituted but spans not shifted.
    let before = Store::load_source("p.bld", src).unwrap();
    let mut no_shift = rename_rows(&before, "step", NEW, &[], "", "");
    no_shift.source_sha256 = good.source_sha256.clone();
    no_shift.mir_sha256 = good.mir_sha256.clone();
    let r = diff_stores(&no_shift, &o.store_rebuilt);
    assert!(r
        .tables
        .iter()
        .any(|t| t.table == "functions" && !t.missing.is_empty()));
    assert!(r.header.is_empty(), "only rows differ: {:?}", r.header);
}

#[test]
fn a_stale_store_is_drift() {
    let a = "fn one() -> i64 { 1 }\nfn main() ~ Console { println!(\"{}\", one()); }\n";
    let b = "fn one() -> i64 { 1 }\nfn two() -> i64 { 2 }\n\
             fn main() ~ Console { println!(\"{}\", one() + two()); }\n";
    let stale = Store::load_source("p.bld", a).unwrap();
    let r = verify_against_source(&stale, "p.bld", b).unwrap();
    assert!(!r.is_clean());
    assert!(r.header.iter().any(|h| h.starts_with("source_sha256")));
    assert!(r.render().contains("DRIFT"));
    let fresh = Store::load_source("p.bld", b).unwrap();
    assert!(verify_against_source(&fresh, "p.bld", b)
        .unwrap()
        .is_clean());
}
