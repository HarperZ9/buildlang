// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The incremental store update and its reference route.
//!
//! A cheap path that reports success can still be wrong: CodeNib's incremental
//! symbol repair finished every held-out change without an error and matched a
//! full rebuild on 13 of 24 (arXiv 2607.25431). So every incremental view here
//! has a reference route, a full rebuild from source, and [`diff_stores`]
//! compares the two table by table. The rebuild is the authority.
//!
//! The first incremental view is the store after a rename: the old rows with
//! the name substituted and the name spans shifted, without lowering again.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::store::Store;
use super::BqlError;

/// Rows present in one store and absent from the other, for one table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableDiff {
    pub table: String,
    pub stored_rows: usize,
    pub rebuilt_rows: usize,
    /// Rows the rebuild has and the stored copy lacks, as JSON.
    pub missing: Vec<String>,
    /// Rows the stored copy has and the rebuild lacks, as JSON.
    pub extra: Vec<String>,
}

/// The result of comparing a stored or incremental store with a rebuild.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftReport {
    pub stored_sha256: String,
    pub rebuilt_sha256: String,
    /// Header fields that differ: schema, module, source or MIR digest.
    pub header: Vec<String>,
    pub tables: Vec<TableDiff>,
}

impl DriftReport {
    /// True when the stored copy equals the rebuild in every field and row.
    pub fn is_clean(&self) -> bool {
        self.header.is_empty()
            && self
                .tables
                .iter()
                .all(|t| t.missing.is_empty() && t.extra.is_empty())
    }

    /// One line per table and a verdict line.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for h in &self.header {
            out.push_str(&format!("header  {h}\n"));
        }
        for t in &self.tables {
            out.push_str(&format!(
                "{:<10} stored {:>5}  rebuilt {:>5}  missing {:>4}  extra {:>4}\n",
                t.table,
                t.stored_rows,
                t.rebuilt_rows,
                t.missing.len(),
                t.extra.len()
            ));
            for row in &t.missing {
                out.push_str(&format!("  - missing {row}\n"));
            }
            for row in &t.extra {
                out.push_str(&format!("  + extra   {row}\n"));
            }
        }
        let verdict = if self.is_clean() { "agree" } else { "DRIFT" };
        out.push_str(&format!(
            "{verdict}: stored {} rebuilt {}\n",
            &self.stored_sha256[..12.min(self.stored_sha256.len())],
            &self.rebuilt_sha256[..12.min(self.rebuilt_sha256.len())]
        ));
        out
    }
}

fn rows<T: Serialize>(rows: &[T]) -> BTreeSet<String> {
    rows.iter()
        .map(|r| serde_json::to_string(r).expect("rows are plain data"))
        .collect()
}

fn table<T: Serialize>(name: &str, stored: &[T], rebuilt: &[T]) -> TableDiff {
    let (s, r) = (rows(stored), rows(rebuilt));
    TableDiff {
        table: name.to_string(),
        stored_rows: stored.len(),
        rebuilt_rows: rebuilt.len(),
        missing: r.difference(&s).cloned().collect(),
        extra: s.difference(&r).cloned().collect(),
    }
}

/// Compare a stored store with a rebuilt one, table by table.
pub fn diff_stores(stored: &Store, rebuilt: &Store) -> DriftReport {
    let mut header = Vec::new();
    let fields = [
        ("schema", &stored.schema, &rebuilt.schema),
        ("module", &stored.module, &rebuilt.module),
        (
            "source_sha256",
            &stored.source_sha256,
            &rebuilt.source_sha256,
        ),
        ("mir_sha256", &stored.mir_sha256, &rebuilt.mir_sha256),
    ];
    for (name, s, r) in fields {
        if s != r {
            header.push(format!("{name}: stored {s} rebuilt {r}"));
        }
    }
    DriftReport {
        stored_sha256: stored.digest(),
        rebuilt_sha256: rebuilt.digest(),
        header,
        tables: vec![
            table("functions", &stored.functions, &rebuilt.functions),
            table("effects", &stored.effects, &rebuilt.effects),
            table("calls", &stored.calls, &rebuilt.calls),
            table("items", &stored.items, &rebuilt.items),
        ],
    }
}

/// Rebuild the store from source and compare a stored copy against it.
pub fn verify_against_source(
    stored: &Store,
    module: &str,
    source: &str,
) -> Result<DriftReport, BqlError> {
    let rebuilt = Store::load_source(module, source)?;
    Ok(diff_stores(stored, &rebuilt))
}

/// Where a byte offset moves after every edit before it is replaced by `new`.
fn shifted(pos: u32, edits: &[(u32, u32)], old: &str, new: &str) -> u32 {
    let before = edits.iter().filter(|(s, _)| *s < pos).count() as i64;
    let delta = new.len() as i64 - old.len() as i64;
    (i64::from(pos) + before * delta) as u32
}

fn sub(name: &mut String, old: &str, new: &str) {
    if name == old {
        *name = new.to_string();
    }
}

/// The incremental view: the store after renaming `old` to `new`, made from
/// the old rows without lowering the program again.
///
/// `edits` are the replaced spans in the old source; the digests of the new
/// source and MIR come from the caller. Rows are re-sorted so the result is
/// in canonical order.
pub fn rename_rows(
    store: &Store,
    old: &str,
    new: &str,
    edits: &[(u32, u32)],
    source_sha256: &str,
    mir_sha256: &str,
) -> Store {
    let mut out = store.clone();
    out.source_sha256 = source_sha256.to_string();
    out.mir_sha256 = mir_sha256.to_string();
    for f in &mut out.functions {
        sub(&mut f.name, old, new);
        if let Some((s, e)) = f.name_span {
            f.name_span = Some((shifted(s, edits, old, new), shifted(e, edits, old, new)));
        }
    }
    for e in &mut out.effects {
        sub(&mut e.function, old, new);
        if let Some(via) = &mut e.via {
            sub(via, old, new);
        }
    }
    for c in &mut out.calls {
        sub(&mut c.caller, old, new);
        sub(&mut c.callee, old, new);
    }
    for i in &mut out.items {
        sub(&mut i.name, old, new);
    }
    out.functions.sort();
    out.effects.sort();
    out.calls.sort();
    out.items.sort();
    out
}
