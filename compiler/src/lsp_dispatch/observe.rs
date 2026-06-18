use serde_json::Value;

use super::model::LspDispatchObserved;

const LEXER_SOURCE: &str = "quantalang/lexer";
const PARSER_SOURCE: &str = "quantalang/parser";
const TYPE_CHECKER_SOURCE: &str = "quantalang/type-checker";

pub(super) fn observe_response(method: &str, response: Option<&Value>) -> LspDispatchObserved {
    let mut observed = LspDispatchObserved {
        has_result: response.is_some_and(|value| value.get("result").is_some()),
        diagnostics: 0,
        compiler_diagnostics: 0,
        type_errors: 0,
        completion_items: 0,
        document_symbols: 0,
        locations: 0,
        text_edits: 0,
        folding_ranges: 0,
        code_actions: 0,
        workspace_edits: 0,
        semantic_tokens: 0,
    };
    let Some(value) = response else {
        return observed;
    };
    if value.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics") {
        let diagnostics = value
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        observed.diagnostics = diagnostics.len();
        observed.compiler_diagnostics = diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic["source"].as_str(),
                    Some(LEXER_SOURCE | PARSER_SOURCE | TYPE_CHECKER_SOURCE)
                )
            })
            .count();
        observed.type_errors = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic["source"].as_str() == Some(TYPE_CHECKER_SOURCE))
            .count();
        return observed;
    }
    match method {
        "textDocument/completion" => {
            observed.completion_items = value
                .pointer("/result/items")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
        }
        "textDocument/documentSymbol" => observed.document_symbols = result_array_len(value),
        "textDocument/definition" | "textDocument/references" => {
            observed.locations = result_array_len(value)
        }
        "textDocument/formatting" => observed.text_edits = result_array_len(value),
        "textDocument/foldingRange" => observed.folding_ranges = result_array_len(value),
        "textDocument/codeAction" => observed.code_actions = result_array_len(value),
        "textDocument/rename" => observed.workspace_edits = workspace_edit_count(value),
        "textDocument/semanticTokens/full" => {
            observed.semantic_tokens = value
                .pointer("/result/data")
                .and_then(Value::as_array)
                .map_or(0, |data| data.len() / 5);
        }
        _ => {}
    }
    observed
}

fn result_array_len(value: &Value) -> usize {
    value
        .get("result")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn workspace_edit_count(value: &Value) -> usize {
    value
        .pointer("/result/changes")
        .and_then(Value::as_object)
        .map(|changes| {
            changes
                .values()
                .filter_map(Value::as_array)
                .map(Vec::len)
                .sum()
        })
        .unwrap_or(0)
}
