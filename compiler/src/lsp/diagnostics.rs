// ===============================================================================
// BUILDLANG LSP DIAGNOSTICS
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Diagnostics provider for BuildLang.
//!
//! Publishes the compiler's own diagnostics (lexer, parser, type checker and
//! the C backend gate) with the same stable codes as `buildc check`.

use super::document::{Document, DocumentStore};
use super::message::PublishDiagnosticsParams;
use super::types::*;
use std::sync::Arc;

use crate::diagnostics::check::check_buffer;
use crate::lexer::SourceFile;

const LEXER_DIAGNOSTIC_SOURCE: &str = "buildlang/lexer";
const PARSER_DIAGNOSTIC_SOURCE: &str = "buildlang/parser";
const TYPE_CHECKER_DIAGNOSTIC_SOURCE: &str = "buildlang/type-checker";

// =============================================================================
// DIAGNOSTICS PROVIDER
// =============================================================================

/// Provides diagnostics for documents.
pub struct DiagnosticsProvider {
    /// Document store reference.
    documents: Arc<DocumentStore>,
}

impl DiagnosticsProvider {
    /// Create a new diagnostics provider.
    pub fn new(documents: Arc<DocumentStore>) -> Self {
        Self { documents }
    }

    /// Compute diagnostics for a document.
    pub fn compute(&self, doc: &Document) -> PublishDiagnosticsParams {
        let mut diagnostics = Vec::new();

        // The compiler's own check is the only source of diagnostics. The
        // text-pattern checks that ran before it (missing `;`, brackets,
        // magic numbers, unused names) were retired: they guessed from line
        // text, flagged valid multi-line statements, and carried no codes.
        self.check_types(&doc.content, &mut diagnostics);

        PublishDiagnosticsParams {
            uri: doc.uri.clone(),
            version: Some(doc.version),
            diagnostics,
        }
    }

    /// Run the compiler's own check (lexer, parser, type checker and the C
    /// backend gate) and publish its diagnostics with the same stable codes
    /// and help lines as `buildc check --error-format=json`.
    fn check_types(&self, content: &str, diagnostics: &mut Vec<Diagnostic>) {
        let source_file = SourceFile::new("buffer", content.to_string());
        for d in check_buffer(&source_file) {
            diagnostics.push(to_lsp(d));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_type_error_diagnostic_uses_type_checker_source() {
        let documents = Arc::new(DocumentStore::new());
        let provider = DiagnosticsProvider::new(documents.clone());
        let doc = documents.open(TextDocumentItem {
            uri: "file:///workspace/type_error.bld".to_string(),
            language_id: "build".to_string(),
            version: 1,
            text: "const BAD: i32 = \"oops\";\nfn main() {}\n".to_string(),
        });
        let published = provider.compute(&doc);

        assert!(
            published.diagnostics.iter().any(|d| {
                d.source.as_deref() == Some("buildlang/type-checker")
                    && d.message.contains("type mismatch")
            }),
            "expected type-checker diagnostic in {:#?}",
            published.diagnostics
        );
    }
}

/// Convert a compiler diagnostic to an LSP diagnostic. Help lines follow the
/// message, as in the CLI's human output; the code is the stable `BLxxxx`.
pub(crate) fn to_lsp(d: crate::diagnostics::Diagnostic) -> Diagnostic {
    let range = match &d.span {
        Some(span) => Range {
            start: Position {
                line: span.line.saturating_sub(1) as u32,
                character: span.column.saturating_sub(1) as u32,
            },
            end: Position {
                line: span.end_line.saturating_sub(1) as u32,
                character: span.end_column.saturating_sub(1) as u32,
            },
        },
        None => Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 1,
            },
        },
    };
    let source = match &d.code[..4] {
        "BL01" => LEXER_DIAGNOSTIC_SOURCE,
        "BL02" => PARSER_DIAGNOSTIC_SOURCE,
        _ => TYPE_CHECKER_DIAGNOSTIC_SOURCE,
    };
    let mut message = d.message;
    for help in &d.help {
        message.push_str("\n  help: ");
        message.push_str(help);
    }
    for note in &d.notes {
        message.push_str("\n  note: ");
        message.push_str(note);
    }
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::Error),
        code: Some(DiagnosticCode::String(d.code.to_string())),
        source: Some(source.to_string()),
        message,
        tags: Vec::new(),
        related_information: Vec::new(),
    }
}
