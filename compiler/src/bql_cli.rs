// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! `buildc bql`: a compiled program as BuildDB tables, queried with BuildQL.

use std::path::{Path, PathBuf};

use buildlang::bql::{export, rename_function, run_query, verify_against_source, Store};
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum BqlCommands {
    /// Write the program's tables in qdb's file format (or as JSON with --json)
    Export {
        /// Input BuildLang source file (single file, no imports)
        file: PathBuf,
        /// Output file (defaults to stdout)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Write the store as JSON instead of qdb tables
        #[arg(long)]
        json: bool,
    },
    /// Run a SELECT from the in-process BuildQL subset over the program's tables
    Query {
        /// Input BuildLang source file (single file, no imports)
        file: PathBuf,
        /// The query, for example "SELECT DISTINCT function FROM effects WHERE effect = 'Network'"
        sql: String,
    },
    /// Rename a top-level function and every reference, checked on MIR
    Rename {
        /// Input BuildLang source file (single file, no imports)
        file: PathBuf,
        /// Current function name
        old: String,
        /// New function name
        new: String,
        /// Where to write the renamed source (defaults to stdout)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Where to write the JSON receipt (defaults to stderr)
        #[arg(long, value_name = "FILE")]
        receipt: Option<PathBuf>,
    },
    /// Compare a stored JSON store with a full rebuild from FILE; exit 1 on drift
    Verify {
        /// Input BuildLang source file (single file, no imports)
        file: PathBuf,
        /// A store written by `bql export --json`
        store: PathBuf,
    },
}

fn read(file: &Path) -> Result<String, i32> {
    std::fs::read_to_string(file).map_err(|err| {
        eprintln!("Error reading file '{}': {err}", file.display());
        1
    })
}

fn write_or_print(output: Option<&Path>, text: &str) -> Result<(), i32> {
    match output {
        Some(path) => std::fs::write(path, text).map_err(|err| {
            eprintln!("Error writing '{}': {err}", path.display());
            1
        }),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

fn fail(err: impl std::fmt::Display) -> i32 {
    eprintln!("bql: {err}");
    1
}

/// The module name a store records: the file name, so a digest does not
/// depend on the directory the command ran from.
fn module_name(file: &Path) -> String {
    file.file_name()
        .map_or_else(|| file.to_string_lossy(), |n| n.to_string_lossy())
        .into_owned()
}

fn load(file: &Path) -> Result<Store, i32> {
    let source = read(file)?;
    Store::load_source(&module_name(file), &source).map_err(fail)
}

pub(crate) fn cmd_bql(command: BqlCommands) -> Result<(), i32> {
    match command {
        BqlCommands::Export { file, output, json } => {
            let store = load(&file)?;
            let text = if json {
                store.to_json() + "\n"
            } else {
                export::to_qdb(&store).map_err(fail)?
            };
            write_or_print(output.as_deref(), &text)
        }
        BqlCommands::Query { file, sql } => {
            let store = load(&file)?;
            let result = run_query(&store, &sql).map_err(fail)?;
            print!("{}", result.render());
            Ok(())
        }
        BqlCommands::Verify { file, store } => {
            let stored: Store = serde_json::from_str(&read(&store)?).map_err(fail)?;
            let source = read(&file)?;
            let report =
                verify_against_source(&stored, &module_name(&file), &source).map_err(fail)?;
            print!("{}", report.render());
            if report.is_clean() {
                Ok(())
            } else {
                Err(1)
            }
        }
        BqlCommands::Rename {
            file,
            old,
            new,
            output,
            receipt,
        } => {
            let source = read(&file)?;
            let (renamed, rec) =
                rename_function(&module_name(&file), &source, &old, &new).map_err(fail)?;
            let rec_json = serde_json::to_string_pretty(&rec).map_err(fail)? + "\n";
            write_or_print(output.as_deref(), &renamed)?;
            match receipt {
                Some(path) => std::fs::write(&path, rec_json).map_err(|err| {
                    eprintln!("Error writing '{}': {err}", path.display());
                    1
                }),
                None => {
                    eprint!("{rec_json}");
                    Ok(())
                }
            }
        }
    }
}
