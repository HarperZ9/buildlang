// ===============================================================================
// BUILDLANG LSP CODE ACTIONS
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Code action provider for BuildLang.

use super::document::Document;
use super::types::*;
use crate::diagnostics::check::check_buffer;
use crate::lexer::SourceFile;

// =============================================================================
// CODE ACTION PROVIDER
// =============================================================================

/// Provides code actions (quick fixes, refactorings, etc.).
pub struct CodeActionProvider;

impl CodeActionProvider {
    /// Create a new code action provider.
    pub fn new() -> Self {
        Self
    }

    /// Get code actions for a range.
    pub fn provide(
        &self,
        doc: &Document,
        range: Range,
        diagnostics: &[Diagnostic],
    ) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Quick fixes come from the compiler's own check of this buffer.
        actions.extend(compiler_quick_fixes(doc, diagnostics));

        // Generate refactoring actions
        actions.extend(self.refactoring_actions(doc, range));

        // Generate source actions
        actions.extend(self.source_actions(doc));

        actions
    }

    /// Quick fixes for one diagnostic: the exact edits the compiler's own
    /// check attached to the diagnostic with the same code and range (see
    /// `buildc check --error-format=json`). A fix that widens authority says
    /// so in its title and is never preferred, so an editor never applies it
    /// as the default action. The text-pattern fixes that guessed from the
    /// message (a `;` at the range end, Rust `std::` imports, deleting a line)
    /// were retired with the pattern checks.
    fn quick_fixes_for_diagnostic(
        &self,
        doc: &Document,
        diagnostic: &Diagnostic,
        compiler: &[crate::diagnostics::Diagnostic],
    ) -> Vec<CodeAction> {
        let Some(DiagnosticCode::String(code)) = &diagnostic.code else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        for d in compiler {
            if d.code != code.as_str()
                || super::diagnostics::to_lsp(d.clone()).range != diagnostic.range
            {
                continue;
            }
            for s in &d.suggestions {
                if s.edits.is_empty() {
                    continue;
                }
                let mut edit = WorkspaceEdit::new();
                for e in &s.edits {
                    let range = Range::new(
                        Position::new(e.span.line as u32 - 1, e.span.column as u32 - 1),
                        Position::new(e.span.end_line as u32 - 1, e.span.end_column as u32 - 1),
                    );
                    edit.add_edit(
                        doc.uri.clone(),
                        TextEdit {
                            range,
                            new_text: e.replacement.clone(),
                        },
                    );
                }
                let title = if s.widens_authority {
                    format!("{} (widens authority)", s.message)
                } else {
                    s.message.clone()
                };
                let mut action = CodeAction::quick_fix(title).with_edit(edit);
                if s.applicability == crate::diagnostics::Applicability::MachineApplicable {
                    action = action.preferred();
                }
                action.diagnostics = vec![diagnostic.clone()];
                actions.push(action);
            }
        }
        actions
    }

    /// Generate refactoring actions.
    fn refactoring_actions(&self, doc: &Document, range: Range) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Only offer refactorings if there's a selection
        if range.start != range.end {
            let selected_text = doc.text_in_range(range);

            // Extract variable
            if !selected_text.trim().is_empty() && !selected_text.contains('\n') {
                actions.push(self.extract_variable_action(doc, range, &selected_text));
            }

            // Extract function
            if selected_text.contains('\n') || selected_text.len() > 50 {
                actions.push(self.extract_function_action(doc, range));
            }
        }

        // Inline variable (if cursor is on a variable)
        // This would require more context analysis

        actions
    }

    /// Extract variable refactoring.
    fn extract_variable_action(&self, doc: &Document, range: Range, text: &str) -> CodeAction {
        // Create a placeholder for the new variable
        let var_name = "extracted";
        let let_stmt = format!("let {} = {};\n    ", var_name, text.trim());

        let mut edit = WorkspaceEdit::new();

        // Insert let statement before the line
        let line_start = Position::new(range.start.line, 0);
        let indent = doc
            .line(range.start.line)
            .map(|l| l.len() - l.trim_start().len())
            .unwrap_or(0);
        let indented_let = format!("{}{}", " ".repeat(indent), let_stmt);
        edit.add_edit(doc.uri.clone(), TextEdit::insert(line_start, indented_let));

        // Replace selection with variable name
        edit.add_edit(
            doc.uri.clone(),
            TextEdit::replace(range, var_name.to_string()),
        );

        CodeAction::new("Extract to variable")
            .with_kind(CodeActionKind::refactor_extract())
            .with_edit(edit)
    }

    /// Extract function refactoring.
    fn extract_function_action(&self, doc: &Document, range: Range) -> CodeAction {
        let selected_text = doc.text_in_range(range);
        let fn_body = selected_text.trim();

        // Create a new function
        let fn_def = format!(
            "\nfn extracted() {{\n    {}\n}}\n",
            fn_body.replace('\n', "\n    ")
        );

        let mut edit = WorkspaceEdit::new();

        // Insert function at end of file
        let last_line = doc.line_count().saturating_sub(1) as u32;
        let end_pos = Position::new(
            last_line,
            doc.line(last_line).map(|l| l.len() as u32).unwrap_or(0),
        );
        edit.add_edit(doc.uri.clone(), TextEdit::insert(end_pos, fn_def));

        // Replace selection with function call
        edit.add_edit(
            doc.uri.clone(),
            TextEdit::replace(range, "extracted()".to_string()),
        );

        CodeAction::new("Extract to function")
            .with_kind(CodeActionKind::refactor_extract())
            .with_edit(edit)
    }

    /// Generate source actions (organize imports, etc.).
    fn source_actions(&self, doc: &Document) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Organize imports
        if let Some(edit) = self.organize_imports(doc) {
            actions.push(
                CodeAction::new("Organize imports")
                    .with_kind(CodeActionKind::organize_imports())
                    .with_edit(edit),
            );
        }

        // Add missing derive
        actions.push(CodeAction::new("Add #[derive(Debug)]").with_kind(CodeActionKind::source()));

        actions
    }

    /// Organize imports in a document.
    fn organize_imports(&self, doc: &Document) -> Option<WorkspaceEdit> {
        let content = &doc.content;
        let lines: Vec<&str> = content.lines().collect();

        // Find all use statements
        let mut imports: Vec<(usize, String)> = Vec::new();
        let mut first_import_line: Option<usize> = None;
        let mut last_import_line: Option<usize> = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ") {
                if first_import_line.is_none() {
                    first_import_line = Some(i);
                }
                last_import_line = Some(i);
                imports.push((i, line.to_string()));
            }
        }

        if imports.len() <= 1 {
            return None;
        }

        // Sort imports
        imports.sort_by(|a, b| {
            let a_trimmed = a.1.trim();
            let b_trimmed = b.1.trim();

            // std comes first, then external crates, then local
            let a_priority = import_priority(a_trimmed);
            let b_priority = import_priority(b_trimmed);

            a_priority
                .cmp(&b_priority)
                .then_with(|| a_trimmed.cmp(b_trimmed))
        });

        let first = first_import_line?;
        let last = last_import_line?;

        // Build new import block
        let mut new_imports = String::new();
        for (_, import) in &imports {
            new_imports.push_str(import.trim());
            new_imports.push('\n');
        }

        let mut edit = WorkspaceEdit::new();
        edit.add_edit(
            doc.uri.clone(),
            TextEdit::replace(
                Range::new(
                    Position::new(first as u32, 0),
                    Position::new(last as u32 + 1, 0),
                ),
                new_imports,
            ),
        );

        Some(edit)
    }
}

impl Default for CodeActionProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// The compiler's exact-edit quick fixes for the diagnostics a client sent
/// with a code action request. The buffer is checked once.
pub(crate) fn compiler_quick_fixes(doc: &Document, diagnostics: &[Diagnostic]) -> Vec<CodeAction> {
    if diagnostics.is_empty() {
        return Vec::new();
    }
    let file = SourceFile::new("buffer", doc.content.clone());
    let compiler = check_buffer(&file);
    let provider = CodeActionProvider::new();
    diagnostics
        .iter()
        .flat_map(|d| provider.quick_fixes_for_diagnostic(doc, d, &compiler))
        .collect()
}

/// Get import priority for sorting.
fn import_priority(import: &str) -> u8 {
    if import.contains("std::") {
        0 // std first
    } else if import.contains("crate::") || import.contains("self::") || import.contains("super::")
    {
        2 // local last
    } else {
        1 // external crates middle
    }
}
