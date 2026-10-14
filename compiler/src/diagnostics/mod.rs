// ===============================================================================
// BUILDLANG DIAGNOSTICS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Structured diagnostics: one shape for lexer, parser, checker and backend
//! errors, rendered as JSON by `buildc check --error-format=json`.
//!
//! The JSON shape is `buildlang.diagnostics/v1`, described by the committed
//! schema `docs/schemas/buildlang.diagnostics.v1.schema.json`. Every diagnostic
//! carries a stable code from [`codes`], one primary span when the error has a
//! location, its help and note lines, and its fix suggestions.
//!
//! A suggestion that would widen a function's authority (add a capability to
//! an effect row, or open an `unsafe` block) is marked `widens_authority` and is
//! never `MachineApplicable`: an agent must not grant itself capabilities by
//! applying a fix without review.

pub mod codes;

use serde::Serialize;

use crate::lexer::{LexerError, SourceFile, Span};
use crate::parser::ParseError;
use crate::types::TypeErrorWithSpan;

/// Schema identifier written into every JSON report.
pub const SCHEMA_ID: &str = "buildlang.diagnostics/v1";

/// How safe a suggestion is to apply without review. Same four levels as
/// rustc's JSON output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Applicability {
    /// The replacement is correct and can be applied mechanically.
    MachineApplicable,
    /// The suggestion may be wrong or change meaning; review it.
    MaybeIncorrect,
    /// The suggestion contains placeholders a person or agent must fill in.
    HasPlaceholders,
    /// No claim about correctness.
    Unspecified,
}

/// A fix suggestion attached to a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    pub message: String,
    pub applicability: Applicability,
    /// True when applying the suggestion grants the code more authority.
    pub widens_authority: bool,
}

impl Suggestion {
    /// Build a suggestion. A suggestion that widens authority is downgraded
    /// from `MachineApplicable` to `MaybeIncorrect`.
    pub fn new(message: impl Into<String>, applicability: Applicability, widens: bool) -> Self {
        let applicability = if widens && applicability == Applicability::MachineApplicable {
            Applicability::MaybeIncorrect
        } else {
            applicability
        };
        Self {
            message: message.into(),
            applicability,
            widens_authority: widens,
        }
    }

    /// Classify free-text fix advice by its code and wording.
    pub fn from_text(code: &str, text: &str) -> Self {
        let widens = suggestion_widens_authority(code, text);
        let applicability = if widens {
            Applicability::MaybeIncorrect
        } else if text.contains("...") || text.contains('<') {
            Applicability::HasPlaceholders
        } else {
            Applicability::Unspecified
        };
        Self::new(text, applicability, widens)
    }
}

/// True when following the advice would add a capability or unsafe authority.
pub fn suggestion_widens_authority(code: &str, text: &str) -> bool {
    matches!(code, "BL0401" | "BL0402" | "BL0403" | "BL0382")
        || text.contains("unsafe")
        || text.contains("~ ")
}

/// Location of a diagnostic: 1-based lines and byte columns, and the byte range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticSpan {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub byte_start: u32,
    pub byte_end: u32,
}

impl DiagnosticSpan {
    /// Resolve a byte span against its source file. A dummy span (`end == 0`)
    /// has no location.
    pub fn resolve(span: Span, file: &SourceFile) -> Option<Self> {
        if span.end.0 == 0 && span.start.0 == 0 {
            return None;
        }
        let (line, column) = line_col(file, span.start);
        let end = span.end.max(span.start);
        let (end_line, end_column) = line_col(file, end);
        Some(Self {
            line,
            column,
            end_line,
            end_column,
            byte_start: span.start.0,
            byte_end: end.0,
        })
    }
}

fn line_col(file: &SourceFile, pos: crate::lexer::BytePos) -> (usize, usize) {
    let line = file.lookup_line(pos);
    let start = file.line_start(line).unwrap_or(pos);
    (line + 1, pos.0.saturating_sub(start.0) as usize + 1)
}

/// Severity. Only errors exist today; the field is there so warnings can be
/// added without a schema change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
}

/// One diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub file: String,
    pub span: Option<DiagnosticSpan>,
    pub help: Vec<String>,
    pub notes: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    /// An error with no location.
    pub fn bare(code: &'static str, file: &str, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            file: file.to_string(),
            span: None,
            help: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    fn with_advice(mut self, explicit_help: Option<&str>, suggestion: Option<&str>) -> Self {
        for text in help_lines(explicit_help, suggestion) {
            self.suggestions
                .push(Suggestion::from_text(self.code, &text));
            self.help.push(text);
        }
        self
    }

    /// From a lexer error.
    pub fn from_lexer(err: &LexerError, file: &SourceFile) -> Self {
        let code = codes::lexer_code(&err.kind);
        let mut d = Self::bare(code, file.name(), err.message());
        d.span = DiagnosticSpan::resolve(err.span, file);
        d.with_advice(err.help.as_deref(), err.kind.help())
    }

    /// From a parse error.
    pub fn from_parse(err: &ParseError, file: &SourceFile) -> Self {
        let code = codes::parse_code(&err.kind);
        let mut d = Self::bare(code, file.name(), err.message());
        d.span = DiagnosticSpan::resolve(err.span, file);
        d.notes = err.notes.clone();
        d.with_advice(err.help.as_deref(), err.kind.suggestion())
    }

    /// From a type-checker, linear-checker or backend error.
    pub fn from_type(err: &TypeErrorWithSpan, file: &SourceFile) -> Self {
        let code = codes::type_code(&err.error);
        let mut d = Self::bare(code, file.name(), err.error.to_string());
        d.span = DiagnosticSpan::resolve(err.span, file);
        d.notes = err.notes.clone();
        let suggestion = err.error.suggestion();
        d.with_advice(err.help.as_deref(), suggestion.as_deref())
    }
}

/// The help lines a renderer prints: the explicit help, then the stored
/// suggestion when it says something the help does not.
pub fn help_lines(explicit: Option<&str>, suggestion: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(h) = explicit {
        out.push(h.to_string());
    }
    if let Some(s) = suggestion {
        if explicit != Some(s) {
            out.push(s.to_string());
        }
    }
    out
}

/// The JSON report for one `buildc check` run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub compiler_version: &'static str,
    pub file: String,
    pub ok: bool,
    pub error_count: usize,
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn new(file: impl Into<String>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            schema: SCHEMA_ID,
            compiler_version: crate::VERSION,
            file: file.into(),
            ok: diagnostics.is_empty(),
            error_count: diagnostics.len(),
            diagnostics,
        }
    }

    /// Pretty JSON text.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("diagnostic report serializes")
    }
}

#[cfg(test)]
mod tests;
