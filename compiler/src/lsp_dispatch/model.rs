#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct LspDispatchReceipt {
    pub(crate) schema: String,
    pub(crate) receipt_id: String,
    pub(crate) created_at: String,
    pub(crate) compiler: String,
    pub(crate) language: String,
    pub(crate) source_set: LspDispatchSourceSet,
    pub(crate) lsp_model: LspDispatchModel,
    pub(crate) fixtures: Vec<LspDispatchFixture>,
    pub(crate) summary: LspDispatchSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct LspDispatchSourceSet {
    pub(crate) kind: String,
    pub(crate) manifest: String,
    pub(crate) program_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct LspDispatchModel {
    pub(crate) protocol: String,
    pub(crate) dispatch: String,
    pub(crate) request_parser: String,
    pub(crate) semantic_anchor: String,
    pub(crate) symbol_anchor: String,
    pub(crate) module_anchor: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct LspDispatchFixture {
    pub(crate) id: String,
    pub(crate) method: String,
    pub(crate) response_kind: String,
    pub(crate) result_digest: LspDispatchDigest,
    pub(crate) observed: LspDispatchObserved,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct LspDispatchObserved {
    pub(crate) has_result: bool,
    pub(crate) diagnostics: usize,
    pub(crate) completion_items: usize,
    pub(crate) document_symbols: usize,
    pub(crate) locations: usize,
    pub(crate) text_edits: usize,
    pub(crate) folding_ranges: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct LspDispatchDigest {
    pub(crate) algorithm: String,
    pub(crate) hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct LspDispatchSummary {
    pub(crate) fixture_count: usize,
    pub(crate) methods: Vec<String>,
    pub(crate) response_kinds: Vec<String>,
    pub(crate) known_gaps: Vec<String>,
}
