// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Store tables as rows, and the qdb checkpoint file format.
//!
//! qdb (`bdb/db.bld`) saves a database as text: a `QDB 1 <txid>` header,
//! then per table `TABLE name`, a `SCHEMA col:TYPE ...` line, one `ROW a|b|c`
//! line per row and `END`. An empty cell reads back as NULL. Format 1 escapes
//! a backslash as `\\`, a pipe as `\|`, a line feed as `\n` and a carriage
//! return as `\r`, so any text is stored; format 0 (no header) had no escapes.

use super::store::Store;
use super::BqlError;

/// Column type, using qdb's three storage types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColType {
    Integer,
    Text,
}

/// One table: column names and types, then rows of optional text cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub name: &'static str,
    pub columns: Vec<(&'static str, ColType)>,
    pub rows: Vec<Vec<Option<String>>>,
}

impl Table {
    /// Index of a column by name.
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|(c, _)| *c == name)
    }
}

fn text(s: &str) -> Option<String> {
    Some(s.to_string())
}

fn int(v: impl ToString) -> Option<String> {
    Some(v.to_string())
}

/// Table names in export order.
pub const TABLES: &[&str] = &["functions", "effects", "calls", "items", "refs"];

/// Every table of the store, in [`TABLES`] order.
pub fn tables(store: &Store) -> Vec<Table> {
    use ColType::{Integer, Text};
    let functions = Table {
        name: "functions",
        columns: vec![
            ("name", Text),
            ("kind", Text),
            ("is_public", Integer),
            ("span_start", Integer),
            ("span_end", Integer),
            ("shader_stage", Text),
        ],
        rows: store
            .functions
            .iter()
            .map(|f| {
                vec![
                    text(&f.name),
                    text(&f.kind),
                    int(u8::from(f.public)),
                    f.name_span.and_then(|s| int(s.0)),
                    f.name_span.and_then(|s| int(s.1)),
                    f.shader_stage.clone(),
                ]
            })
            .collect(),
    };
    let effects = Table {
        name: "effects",
        columns: vec![
            ("function", Text),
            ("effect", Text),
            ("how", Text),
            ("via", Text),
        ],
        rows: store
            .effects
            .iter()
            .map(|e| {
                vec![
                    text(&e.function),
                    text(&e.effect),
                    text(&e.how),
                    e.via.clone(),
                ]
            })
            .collect(),
    };
    let calls = Table {
        name: "calls",
        columns: vec![("caller", Text), ("callee", Text)],
        rows: store
            .calls
            .iter()
            .map(|c| vec![text(&c.caller), text(&c.callee)])
            .collect(),
    };
    let items = Table {
        name: "items",
        columns: vec![("name", Text), ("kind", Text)],
        rows: store
            .items
            .iter()
            .map(|i| vec![text(&i.name), text(&i.kind)])
            .collect(),
    };
    let refs = Table {
        name: "refs",
        columns: vec![
            ("symbol", Text),
            ("span_start", Integer),
            ("span_end", Integer),
            ("role", Text),
        ],
        rows: store
            .refs
            .iter()
            .map(|r| vec![text(&r.symbol), int(r.span.0), int(r.span.1), text(&r.role)])
            .collect(),
    };
    vec![functions, effects, calls, items, refs]
}

/// Write the store in qdb's checkpoint format.
/// A cell in qdb's format 1.
fn escape_cell(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    for c in cell.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '|' => out.push_str("\\|"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

pub fn to_qdb(store: &Store) -> Result<String, BqlError> {
    let mut out = String::from("QDB 1 0\n");
    let all = tables(store);
    for (i, table) in all.iter().enumerate() {
        out.push_str("TABLE ");
        out.push_str(table.name);
        out.push('\n');
        out.push_str("SCHEMA");
        for (name, ty) in &table.columns {
            let ty = match ty {
                ColType::Integer => "INTEGER",
                ColType::Text => "TEXT",
            };
            out.push_str(&format!(" {name}:{ty}"));
        }
        out.push('\n');
        for row in &table.rows {
            let mut cells = Vec::with_capacity(row.len());
            for cell in row {
                cells.push(escape_cell(cell.as_deref().unwrap_or("")));
            }
            out.push_str("ROW ");
            out.push_str(&cells.join("|"));
            out.push('\n');
        }
        out.push_str("END\n");
        if i + 1 < all.len() {
            out.push('\n');
        }
    }
    Ok(out)
}
