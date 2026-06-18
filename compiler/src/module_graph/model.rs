#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ModuleGraphReceipt {
    pub(crate) schema: String,
    pub(crate) receipt_id: String,
    pub(crate) created_at: String,
    pub(crate) compiler: String,
    pub(crate) language: String,
    pub(crate) source_set: ModuleGraphSourceSet,
    pub(crate) module_model: ModuleGraphModel,
    pub(crate) programs: Vec<ModuleGraphProgram>,
    pub(crate) summary: ModuleGraphSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ModuleGraphSourceSet {
    pub(crate) kind: String,
    pub(crate) manifest: String,
    pub(crate) program_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ModuleGraphModel {
    pub(crate) resolver: String,
    pub(crate) input_roles: Vec<String>,
    pub(crate) digest_anchor: String,
    pub(crate) symbol_anchor: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ModuleGraphProgram {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) source_digest: ModuleGraphDigest,
    pub(crate) input_graph_digest: ModuleGraphDigest,
    pub(crate) module_graph_digest: ModuleGraphDigest,
    pub(crate) inputs: Vec<ModuleGraphInput>,
    pub(crate) edges: Vec<ModuleGraphEdge>,
    pub(crate) known_gaps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub(crate) struct ModuleGraphDigest {
    pub(crate) algorithm: String,
    pub(crate) hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub(crate) struct ModuleGraphInput {
    pub(crate) id: String,
    pub(crate) role: String,
    pub(crate) path: String,
    pub(crate) source_digest: ModuleGraphDigest,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub(crate) struct ModuleGraphEdge {
    pub(crate) kind: String,
    pub(crate) from: String,
    pub(crate) to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ModuleGraphSummary {
    pub(crate) program_count: usize,
    pub(crate) input_count: usize,
    pub(crate) input_roles: Vec<String>,
    pub(crate) edge_kinds: Vec<String>,
    pub(crate) known_gaps: Vec<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct ProgramDigestProjection<'a> {
    pub(crate) id: &'a str,
    pub(crate) path: &'a str,
    pub(crate) source_digest_hex: &'a str,
    pub(crate) input_graph_digest_hex: &'a str,
    pub(crate) inputs: &'a [ModuleGraphInput],
    pub(crate) edges: &'a [ModuleGraphEdge],
    pub(crate) known_gaps: &'a [String],
}
