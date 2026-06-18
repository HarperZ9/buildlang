use super::document::Document;

mod classify;
mod scan;

#[cfg(test)]
mod tests;

pub const TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "function",
    "variable",
    "parameter",
    "property",
    "keyword",
    "comment",
    "string",
    "number",
    "operator",
    "macro",
];

pub const TOKEN_MODIFIERS: &[&str] = &[
    "declaration",
    "definition",
    "readonly",
    "static",
    "deprecated",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticTokens {
    pub data: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticTokenLegendSpec {
    pub token_types: Vec<&'static str>,
    pub token_modifiers: Vec<&'static str>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SemanticTokensProvider;

impl SemanticTokensProvider {
    pub fn new() -> Self {
        Self
    }

    pub fn legend() -> SemanticTokenLegendSpec {
        SemanticTokenLegendSpec {
            token_types: TOKEN_TYPES.to_vec(),
            token_modifiers: TOKEN_MODIFIERS.to_vec(),
        }
    }

    pub fn full(&self, doc: &Document) -> SemanticTokens {
        SemanticTokens {
            data: scan::scan_document(doc),
        }
    }
}
