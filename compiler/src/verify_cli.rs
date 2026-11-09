// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! `buildc verify FILE`: the verdict on every law and contract in a file.

use buildlang::diagnostics::check::check_buffer;
use buildlang::lexer::{Lexer, SourceFile};
use buildlang::parser::Parser;
use buildlang::verify::{report, verify_module};
use std::path::Path;

/// Exit 0 when nothing is refuted, 1 on a refutation or an inconsistency,
/// 2 when the file does not check.
pub fn cmd_verify(file: &Path, json: bool) -> Result<(), i32> {
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        2
    })?;
    let sf = SourceFile::new(file.to_string_lossy(), source);
    // The specs must type-check before they are verified; a refuted spec is
    // reported below, not as a check failure.
    let blocking: Vec<_> = check_buffer(&sf)
        .into_iter()
        .filter(|d| d.code != "BL0801" && d.code != "BL0802")
        .collect();
    if !blocking.is_empty() {
        for d in &blocking {
            eprintln!("[{}] {}", d.code, d.message);
        }
        eprintln!(
            "'{}' does not check; fix it before verifying",
            file.display()
        );
        return Err(2);
    }
    let tokens = Lexer::new(&sf).tokenize().map_err(|e| {
        eprintln!("Lexer error: {e}");
        2
    })?;
    let module = Parser::new(&sf, tokens).parse().map_err(|e| {
        eprintln!("Parse error: {}", e.message());
        2
    })?;
    let outcomes = verify_module(&module);
    if json {
        let doc = report::to_json(&file.to_string_lossy(), &outcomes);
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
    } else {
        print!("{}", report::human(&outcomes));
    }
    let (_, refuted, _, inconsistent) = report::counts(&outcomes);
    if refuted + inconsistent > 0 {
        return Err(1);
    }
    Ok(())
}
