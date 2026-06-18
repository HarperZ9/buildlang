use serde_json::{json, Map, Value};

use super::semantic_tokens::{SemanticTokenLegendSpec, SemanticTokens};
use super::types::{
    CodeAction, Location, Position, Range, SymbolInformation, TextEdit, WorkspaceEdit,
};

pub fn build_code_actions_json(actions: &[CodeAction]) -> String {
    serde_json::to_string(&actions.iter().map(code_action_json).collect::<Vec<Value>>())
        .expect("serialize code actions")
}

pub fn build_workspace_edit_json(edit: &WorkspaceEdit) -> String {
    serde_json::to_string(&workspace_edit_json(edit)).expect("serialize workspace edit")
}

pub fn build_semantic_tokens_json(tokens: &SemanticTokens) -> String {
    serde_json::to_string(&json!({ "data": tokens.data })).expect("serialize semantic tokens")
}

pub fn build_semantic_tokens_options_json(legend: &SemanticTokenLegendSpec) -> String {
    serde_json::to_string(&json!({
        "legend": {
            "tokenTypes": legend.token_types,
            "tokenModifiers": legend.token_modifiers,
        },
        "range": false,
        "full": true,
    }))
    .expect("serialize semantic token options")
}

pub fn build_symbol_information_json(symbols: &[SymbolInformation]) -> String {
    serde_json::to_string(
        &symbols
            .iter()
            .map(symbol_information_json)
            .collect::<Vec<Value>>(),
    )
    .expect("serialize workspace symbols")
}

fn code_action_json(action: &CodeAction) -> Value {
    let mut value = Map::new();
    value.insert("title".to_string(), json!(action.title));
    if let Some(kind) = &action.kind {
        value.insert("kind".to_string(), json!(kind.0));
    }
    value.insert("isPreferred".to_string(), json!(action.is_preferred));
    if let Some(edit) = &action.edit {
        value.insert("edit".to_string(), workspace_edit_json(edit));
    }
    Value::Object(value)
}

fn symbol_information_json(symbol: &SymbolInformation) -> Value {
    let mut value = Map::new();
    value.insert("name".to_string(), json!(symbol.name));
    value.insert("kind".to_string(), json!(symbol.kind as u8));
    value.insert("location".to_string(), location_json(&symbol.location));
    if !symbol.tags.is_empty() {
        value.insert(
            "tags".to_string(),
            Value::Array(symbol.tags.iter().map(|tag| json!(*tag as u8)).collect()),
        );
    }
    if let Some(container) = &symbol.container_name {
        value.insert("containerName".to_string(), json!(container));
    }
    Value::Object(value)
}

fn location_json(location: &Location) -> Value {
    json!({
        "uri": location.uri,
        "range": range_json(&location.range),
    })
}

fn workspace_edit_json(edit: &WorkspaceEdit) -> Value {
    let mut changes = Map::new();
    let mut uris = edit.changes.keys().collect::<Vec<_>>();
    uris.sort();
    for uri in uris {
        let edits = edit
            .changes
            .get(uri)
            .expect("workspace edit uri should exist")
            .iter()
            .map(text_edit_json)
            .collect::<Vec<_>>();
        changes.insert(uri.clone(), Value::Array(edits));
    }
    json!({ "changes": Value::Object(changes) })
}

fn text_edit_json(edit: &TextEdit) -> Value {
    json!({
        "range": range_json(&edit.range),
        "newText": edit.new_text,
    })
}

fn range_json(range: &Range) -> Value {
    json!({
        "start": position_json(&range.start),
        "end": position_json(&range.end),
    })
}

fn position_json(position: &Position) -> Value {
    json!({
        "line": position.line,
        "character": position.character,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::types::{CodeAction, CodeActionKind, TextEdit};

    #[test]
    fn workspace_edit_json_sorts_changes_by_uri() {
        let mut edit = WorkspaceEdit::new();
        edit.add_edit(
            "file:///b.quanta".to_string(),
            TextEdit::insert(Position::new(0, 0), "b".to_string()),
        );
        edit.add_edit(
            "file:///a.quanta".to_string(),
            TextEdit::insert(Position::new(0, 0), "a".to_string()),
        );

        let json: Value =
            serde_json::from_str(&build_workspace_edit_json(&edit)).expect("workspace edit json");
        let keys = json["changes"]
            .as_object()
            .expect("changes object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["file:///a.quanta", "file:///b.quanta"]);
    }

    #[test]
    fn code_action_json_includes_edit() {
        let mut edit = WorkspaceEdit::new();
        edit.add_edit(
            "file:///main.quanta".to_string(),
            TextEdit::insert(Position::new(1, 13), ";".to_string()),
        );
        let action = CodeAction::new("Add missing semicolon")
            .with_kind(CodeActionKind::quick_fix())
            .with_edit(edit);

        let json: Value =
            serde_json::from_str(&build_code_actions_json(&[action])).expect("code actions json");

        assert_eq!(json[0]["title"], "Add missing semicolon");
        assert_eq!(json[0]["kind"], "quickfix");
        assert!(json[0]["edit"]["changes"].is_object());
    }
}
