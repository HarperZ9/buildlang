// ===============================================================================
// `buildc check <dir>` AND `buildc lint <dir>`: A PACKAGE AS ONE UNIT
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! A package is `Build.toml` plus `src/*.bld`. The compiler sees it as one
//! joined text, the same text `buildc pkg build` compiles. This module joins
//! the package, runs the check on the joined text and rewrites every position
//! to the file and line the text was copied from, so a diagnostic names a
//! place the user can open.

use crate::{
    source_digest_hex, CheckOutcome, CheckReceiptSourceDigest, CheckUnit, InputDigestLedger,
};
use buildlang::diagnostics::{codes as diag_codes, Diagnostic, DiagnosticSpan};
use buildlang::pkg::local::linemap::LineMap;
use buildlang::pkg::local::source::{check_input, BuildInput};
use std::path::Path;

/// Join the package at `dir`. A failure is printed and also returned as a
/// diagnostic. `include!` has no meaning in a joined package (the build
/// cannot expand it either), so a package that uses it is refused here with
/// the place of each use.
pub(crate) fn load(dir: &Path) -> Result<BuildInput, Diagnostic> {
    let shown = dir.to_string_lossy().to_string();
    if !dir.join("Build.toml").is_file() {
        let message = format!(
            "'{}' is a directory with no Build.toml; name a .bld file, or a package directory",
            dir.display()
        );
        eprintln!("error[{}]: {}", diag_codes::IO_ERROR, message);
        return Err(Diagnostic::bare(diag_codes::IO_ERROR, &shown, message));
    }
    let input = check_input(dir, None).map_err(|e| {
        eprintln!("{}", e);
        Diagnostic::bare(e.code, &shown, e.message.clone())
    })?;
    let mut places = Vec::new();
    for (index, line) in input.source.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("include!(\"") && t.ends_with("\");") {
            if let Some(origin) = input.line_map.origin(index + 1) {
                places.push(format!("{}:{}", origin.file, origin.line));
            }
        }
    }
    if !places.is_empty() {
        let message = format!(
            "`include!` is not available inside a package; put the file in src/ instead ({})",
            places.join(", ")
        );
        eprintln!("error[{}]: {}", diag_codes::INPUT_GRAPH_ERROR, message);
        return Err(Diagnostic::bare(
            diag_codes::INPUT_GRAPH_ERROR,
            &shown,
            message,
        ));
    }
    Ok(input)
}

/// `buildc check <dir>`: check the package at `dir` as one unit.
pub(crate) fn run(dir: &Path, early: &mut Option<Diagnostic>) -> Result<CheckOutcome, i32> {
    let input = load(dir).map_err(|d| {
        *early = Some(d);
        1
    })?;
    let mut ledger = InputDigestLedger::default();
    let manifest = dir.join("Build.toml");
    if let Ok(bytes) = std::fs::read(&manifest) {
        ledger.record("entry", &manifest, &bytes);
    }
    for file in input.line_map.files() {
        let path = Path::new(file);
        if let Ok(bytes) = std::fs::read(path) {
            ledger.record("package-source", path, &bytes);
        }
    }
    let unit = CheckUnit {
        shown: dir.to_string_lossy().to_string(),
        base: dir.join("src"),
        source_digest: CheckReceiptSourceDigest {
            algorithm: "sha256",
            hex: source_digest_hex(input.source.as_bytes()),
        },
        source: input.source,
        ledger,
        origins: Some(&input.line_map),
    };
    crate::run_check_unit(unit, early)
}

/// Rewrite one joined-text span to its original file. Returns that file.
/// Columns stay as they are: a copied line keeps its text. A span that ends
/// in another file is cut to its start.
fn remap_span(span: &mut DiagnosticSpan, map: &LineMap) -> Option<String> {
    let start = map.origin(span.line)?;
    let column_offset = span.column.saturating_sub(1) as u32;
    let joined_line_start = span.byte_start.saturating_sub(column_offset);
    let length = span.byte_end.saturating_sub(span.byte_start);
    let same_line = span.end_line == span.line;
    let end = map.origin(span.end_line).filter(|e| e.file == start.file);
    span.byte_start = start.byte_start + (span.byte_start - joined_line_start);
    match end {
        Some(_) if same_line => {
            span.byte_end = span.byte_start + length;
        }
        Some(end) => {
            let end_column_offset = span.end_column.saturating_sub(1) as u32;
            span.byte_end = end.byte_start + end_column_offset;
            span.end_line = end.line;
        }
        None => {
            span.byte_end = span.byte_start;
            span.end_column = span.column;
        }
    }
    span.line = start.line;
    if same_line || end.is_none() {
        span.end_line = start.line;
    }
    Some(start.file.to_string())
}

/// Rewrite a diagnostic of the joined text to its original file and line.
/// An exact edit is kept only when it lands in that same file.
pub(crate) fn remap_diagnostic(diagnostic: &mut Diagnostic, map: &LineMap) {
    let Some(span) = diagnostic.span.as_mut() else {
        return;
    };
    let Some(file) = remap_span(span, map) else {
        return;
    };
    for suggestion in &mut diagnostic.suggestions {
        let mut kept = Vec::new();
        for mut edit in std::mem::take(&mut suggestion.edits) {
            if remap_span(&mut edit.span, map).as_deref() == Some(file.as_str()) {
                kept.push(edit);
            }
        }
        suggestion.edits = kept;
    }
    diagnostic.file = file;
}

/// Rewrite every position of a package check to its original file and line.
pub(crate) fn remap_outcome(outcome: &mut CheckOutcome, map: &LineMap) {
    for diag in &mut outcome.parse_errors {
        if let Some(origin) = map.origin(diag.line) {
            diag.file = Some(origin.file.to_string());
            diag.line = origin.line;
        }
    }
    outcome.type_error_files = outcome
        .type_error_locations
        .iter_mut()
        .map(|location| {
            let (line, _) = location.as_mut()?;
            let origin = map.origin(*line)?;
            *line = origin.line;
            Some(origin.file.to_string())
        })
        .collect();
    for diagnostic in &mut outcome.diagnostics {
        remap_diagnostic(diagnostic, map);
    }
}

/// Where a joined line is, for the lint report: `(file, line, own)`.
pub(crate) fn place(map: &LineMap, joined_line: usize) -> Option<(String, usize, bool)> {
    map.origin(joined_line)
        .map(|o| (o.file.to_string(), o.line, o.own))
}
