// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! BuildDB tables for a compiled program, and BuildQL over them.
//!
//! BuildDB is qdb, the SQL engine written in BuildLang (`bdb/db.bld` in this
//! repository), and BuildQL is qdb's SQL dialect. This
//! module is the compiler side of that pair:
//!
//! - [`Store`] turns one compiled program into tables: `functions`, `effects`,
//!   `calls` and `items`, derived from the AST, the type checker's effect
//!   summaries and MIR.
//! - [`export::to_qdb`] writes those tables in qdb's checkpoint file format, so
//!   `qdb program.qdb "SELECT ..."` runs the full dialect over them.
//! - [`query`] runs a small subset of the same dialect in process, so the
//!   compiler and its tests can ask questions without a qdb binary. Anything
//!   outside the subset is refused with a message that names qdb.
//! - [`rename`] is the first transform: rename a free function and every
//!   reference to it, then re-lower the result and check that MIR is
//!   unchanged apart from the name. A receipt records both digests.
//! - [`drift`] is the reference route for incremental views: the store after
//!   a rename made from the old rows, and a table-by-table diff of any store
//!   against a full rebuild.

pub mod drift;
pub mod export;
pub mod query;
pub mod rename;
pub mod store;

#[cfg(test)]
mod drift_tests;
#[cfg(test)]
mod tests;

pub use drift::{diff_stores, verify_against_source, DriftReport};
pub use query::{run_query, QueryResult};
pub use rename::{rename_checked, rename_function, RenameOutcome, RenameReceipt};
pub use store::{lower_source, Store, BDB_SCHEMA};

/// Errors from loading, querying or transforming a program store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BqlError {
    /// The program did not compile.
    #[error("compile error: {0}")]
    Compile(String),
    /// The program or request is outside what this version handles.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// The query text is malformed or names something that does not exist.
    #[error("query error: {0}")]
    Query(String),
    /// A transform was refused, with the reason.
    #[error("transform refused: {0}")]
    Refused(String),
}
