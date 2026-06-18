use crate::mir_representation::{MirRepresentationDigest, MirRepresentationSymbols};

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolGraphReceipt {
    pub(crate) schema: String,
    pub(crate) receipt_id: String,
    pub(crate) created_at: String,
    pub(crate) compiler: String,
    pub(crate) language: String,
    pub(crate) source_set: SymbolGraphSourceSet,
    pub(crate) symbol_model: SymbolGraphModel,
    pub(crate) programs: Vec<SymbolGraphProgram>,
    pub(crate) summary: SymbolGraphSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolGraphSourceSet {
    pub(crate) kind: String,
    pub(crate) manifest: String,
    pub(crate) program_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolGraphModel {
    pub(crate) source: String,
    pub(crate) representation: String,
    pub(crate) semantic_anchor: String,
    pub(crate) lowering_pipeline: String,
    pub(crate) representation_anchor: String,
    pub(crate) memory_anchor: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolGraphProgram {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) source_digest: MirRepresentationDigest,
    pub(crate) input_graph_digest: MirRepresentationDigest,
    pub(crate) mir_digest: MirRepresentationDigest,
    pub(crate) symbol_graph_digest: MirRepresentationDigest,
    pub(crate) source_symbols: Vec<SymbolSourceSymbol>,
    pub(crate) mir_symbols: MirRepresentationSymbols,
    pub(crate) effect_symbols: Vec<SymbolGraphEffectSymbol>,
    pub(crate) edges: Vec<SymbolGraphEdge>,
    pub(crate) known_gaps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolSourceSymbol {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) visibility: String,
    pub(crate) span: SymbolSourceSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) signature: Option<SymbolSourceSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolSourceSpan {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolSourceSignature {
    pub(crate) parameters: usize,
    pub(crate) has_return: bool,
    pub(crate) is_async: bool,
    pub(crate) is_unsafe: bool,
    pub(crate) declared_effects: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolGraphEffectSymbol {
    pub(crate) function: String,
    pub(crate) declared_effects: Vec<String>,
    pub(crate) observed_capabilities: Vec<String>,
    pub(crate) propagated_effect_sources: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolGraphEdge {
    pub(crate) kind: String,
    pub(crate) from: String,
    pub(crate) to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct SymbolGraphSummary {
    pub(crate) program_count: usize,
    pub(crate) source_symbol_kinds: Vec<String>,
    pub(crate) mir_symbol_kinds: Vec<String>,
    pub(crate) effect_names: Vec<String>,
    pub(crate) edge_kinds: Vec<String>,
    pub(crate) known_gaps: Vec<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct ProgramDigestProjection<'a> {
    pub(crate) id: &'a str,
    pub(crate) path: &'a str,
    pub(crate) source_digest_hex: &'a str,
    pub(crate) input_graph_digest_hex: &'a str,
    pub(crate) mir_digest_hex: &'a str,
    pub(crate) source_symbols: &'a [SymbolSourceSymbol],
    pub(crate) mir_symbols: &'a MirRepresentationSymbols,
    pub(crate) effect_symbols: &'a [SymbolGraphEffectSymbol],
    pub(crate) edges: &'a [SymbolGraphEdge],
    pub(crate) known_gaps: &'a [String],
}
