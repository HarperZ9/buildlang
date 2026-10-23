// ===============================================================================
// BUILDLANG DIAGNOSTICS
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
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

pub mod check;
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

/// One exact source edit: replace the bytes in `span` with `replacement`.
/// An empty span is an insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub span: Span,
    pub replacement: String,
}

/// A fix the compiler can state as exact edits, attached to a lexer, parser
/// or type error where it is produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub message: String,
    pub edits: Vec<TextEdit>,
    /// True when the edits are known to be what the program needs. Ignored
    /// (downgraded) when the fix widens authority.
    pub machine_applicable: bool,
}

impl Fix {
    /// Insert `text` at byte position `at` of the span's source.
    pub fn insert(
        message: impl Into<String>,
        at: Span,
        text: impl Into<String>,
        exact: bool,
    ) -> Self {
        let point = Span::new(at.start, at.start, at.source_id);
        Self {
            message: message.into(),
            edits: vec![TextEdit {
                span: point,
                replacement: text.into(),
            }],
            machine_applicable: exact,
        }
    }
}

/// An edit as written into the JSON report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuggestionEdit {
    pub span: DiagnosticSpan,
    pub replacement: String,
}

/// A fix suggestion attached to a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    pub message: String,
    pub applicability: Applicability,
    /// True when applying the suggestion grants the code more authority.
    pub widens_authority: bool,
    /// Exact edits that carry out the suggestion, when the compiler knows
    /// them. Omitted from JSON when empty. The same edit can appear on several
    /// diagnostics (for example one per missing capability); apply it once.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<SuggestionEdit>,
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
            edits: Vec::new(),
        }
    }

    /// A suggestion carrying exact edits. Resolves each edit against `file`;
    /// an edit whose span does not resolve drops the whole fix to text only.
    pub fn from_fix(code: &str, fix: &Fix, file: &SourceFile) -> Self {
        let widens = suggestion_widens_authority(code, &fix.message)
            || fix
                .edits
                .iter()
                .any(|e| suggestion_widens_authority(code, &e.replacement));
        let edits: Option<Vec<SuggestionEdit>> = fix
            .edits
            .iter()
            .map(|e| {
                resolve_point_or_range(e.span, file).map(|span| SuggestionEdit {
                    span,
                    replacement: e.replacement.clone(),
                })
            })
            .collect();
        let applicability = match (&edits, fix.machine_applicable) {
            (Some(_), true) => Applicability::MachineApplicable,
            (Some(_), false) => Applicability::MaybeIncorrect,
            (None, _) => Applicability::Unspecified,
        };
        let mut s = Self::new(fix.message.clone(), applicability, widens);
        s.edits = edits.unwrap_or_default();
        s
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

/// Like [`DiagnosticSpan::resolve`], but an empty span at offset 0 (an
/// insertion at the start of the file) still resolves when it carries the
/// file's own source id.
fn resolve_point_or_range(span: Span, file: &SourceFile) -> Option<DiagnosticSpan> {
    if span.source_id != file.id || span.end.0 as usize > file.source().len() {
        return None;
    }
    let (line, column) = line_col(file, span.start);
    let end = span.end.max(span.start);
    let (end_line, end_column) = line_col(file, end);
    Some(DiagnosticSpan {
        line,
        column,
        end_line,
        end_column,
        byte_start: span.start.0,
        byte_end: end.0,
    })
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

    fn with_advice(
        mut self,
        explicit_help: Option<&str>,
        suggestion: Option<&str>,
        fix: Option<&Fix>,
    ) -> Self {
        for text in help_lines_with_fix(explicit_help, suggestion, fix) {
            self.suggestions
                .push(Suggestion::from_text(self.code, &text));
            self.help.push(text);
        }
        self
    }

    /// Put an exact-edit fix first among the suggestions. A text-only
    /// suggestion with the same message is replaced by it.
    fn with_fix(mut self, fix: Option<&Fix>, file: &SourceFile) -> Self {
        if let Some(fix) = fix {
            let exact = Suggestion::from_fix(self.code, fix, file);
            if exact.edits.is_empty() {
                return self;
            }
            self.suggestions.retain(|s| s.message != exact.message);
            self.suggestions.insert(0, exact);
        }
        self
    }

    /// From a lexer error.
    pub fn from_lexer(err: &LexerError, file: &SourceFile) -> Self {
        let code = codes::lexer_code(&err.kind);
        let mut d = Self::bare(code, file.name(), err.message());
        d.span = DiagnosticSpan::resolve(err.span, file);
        d.with_advice(err.help.as_deref(), err.kind.help(), None)
    }

    /// From a parse error.
    pub fn from_parse(err: &ParseError, file: &SourceFile) -> Self {
        let code = codes::parse_code(&err.kind);
        let mut d = Self::bare(code, file.name(), err.message());
        d.span = DiagnosticSpan::resolve(err.span, file);
        d.notes = err.notes.clone();
        d.with_advice(err.help.as_deref(), err.kind.suggestion(), err.fix.as_ref())
            .with_fix(err.fix.as_ref(), file)
    }

    /// From a type-checker, linear-checker or backend error.
    pub fn from_type(err: &TypeErrorWithSpan, file: &SourceFile) -> Self {
        let code = codes::type_code(&err.error);
        let mut d = Self::bare(code, file.name(), err.error.to_string());
        d.span = DiagnosticSpan::resolve(err.span, file);
        d.notes = err.notes.clone();
        let suggestion = err.error.suggestion();
        d.with_advice(err.help.as_deref(), suggestion.as_deref(), err.fix.as_ref())
            .with_fix(err.fix.as_ref(), file)
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

/// [`help_lines`] plus the message of an exact-edit fix when no help line
/// already says it. Every renderer (human, JSON, language server) prints these
/// lines, so a suggestion's message is always visible as help.
pub fn help_lines_with_fix(
    explicit: Option<&str>,
    suggestion: Option<&str>,
    fix: Option<&Fix>,
) -> Vec<String> {
    let mut out = help_lines(explicit, suggestion);
    if let Some(fix) = fix {
        if !out.iter().any(|h| h == &fix.message) {
            out.push(fix.message.clone());
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
