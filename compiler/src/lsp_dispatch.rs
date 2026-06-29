use std::collections::BTreeSet;
use std::path::Path;

use buildlang::lsp::{dispatch_raw_message, LanguageServer};

use super::SemanticCorpusManifest;
use crate::{module_graph::MODULE_GRAPH_RECEIPT, symbol_graph::SYMBOL_GRAPH_RECEIPT};
use fixture::{build_fixture, fixture_sequence};
pub(crate) use model::LspDispatchReceipt;
use model::{LspDispatchFixture, LspDispatchModel, LspDispatchSourceSet, LspDispatchSummary};

mod fixture;
mod model;
mod observe;
#[cfg(test)]
mod tests;
mod validate;

#[allow(unused_imports)]
pub(crate) use validate::{validate_lsp_dispatch_receipt, verify_lsp_dispatch_receipt};

pub(crate) const LSP_DISPATCH_RECEIPT: &str = "lsp-dispatch-2026-06-18.json";
const LSP_DISPATCH_SCHEMA: &str = "buildlang-lsp-dispatch-receipt/v0";

pub(crate) fn build_lsp_dispatch_receipt(
    _root: &Path,
    manifest: &SemanticCorpusManifest,
) -> Result<LspDispatchReceipt, String> {
    let mut server = LanguageServer::new();
    let mut fixtures = Vec::new();
    for raw in fixture_sequence() {
        let response = dispatch_raw_message(&mut server, &raw.content);
        fixtures.push(build_fixture(raw, response.as_deref())?);
    }
    Ok(LspDispatchReceipt {
        schema: LSP_DISPATCH_SCHEMA.to_string(),
        receipt_id: "lsp-dispatch-semantic-corpus-2026-06-18".to_string(),
        created_at: "2026-06-18".to_string(),
        compiler: "buildc".to_string(),
        language: "buildlang".to_string(),
        source_set: LspDispatchSourceSet {
            kind: "semantic-corpus".to_string(),
            manifest: "manifest.json".to_string(),
            program_count: manifest.programs.len(),
        },
        lsp_model: lsp_model(),
        summary: summarize_fixtures(&fixtures),
        fixtures,
    })
}

fn lsp_model() -> LspDispatchModel {
    LspDispatchModel {
        protocol: "LSP JSON-RPC over stdio".to_string(),
        dispatch: "buildc lsp raw message dispatch".to_string(),
        request_parser: "serde_json structural JSON-RPC parser".to_string(),
        semantic_anchor: "compiler diagnostics and parser-backed document model".to_string(),
        symbol_anchor: format!("receipts/{SYMBOL_GRAPH_RECEIPT}"),
        module_anchor: format!("receipts/{MODULE_GRAPH_RECEIPT}"),
    }
}

fn summarize_fixtures(fixtures: &[LspDispatchFixture]) -> LspDispatchSummary {
    LspDispatchSummary {
        fixture_count: fixtures.len(),
        methods: sorted(
            fixtures
                .iter()
                .map(|fixture| fixture.method.clone())
                .collect(),
        ),
        response_kinds: sorted(
            fixtures
                .iter()
                .map(|fixture| fixture.response_kind.clone())
                .collect(),
        ),
        known_gaps: sorted(
            ["full VS Code extension readiness"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
    }
}

fn sorted(values: BTreeSet<String>) -> Vec<String> {
    values.into_iter().collect()
}
