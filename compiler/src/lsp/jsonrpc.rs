use serde_json::Value;

use super::message::CodeActionTriggerKind;
use super::types::{Diagnostic, DiagnosticSeverity, Position, Range};

#[derive(Debug, Clone)]
pub struct JsonRpcMessage {
    value: Value,
    id_json: Option<String>,
    method: Option<String>,
}

impl JsonRpcMessage {
    pub fn parse(content: &str) -> Result<Self, serde_json::Error> {
        let value = serde_json::from_str::<Value>(content)?;
        let id_json = value.get("id").map(serde_json::to_string).transpose()?;
        let method = value
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(Self {
            value,
            id_json,
            method,
        })
    }

    pub fn id_json(&self) -> Option<&str> {
        self.id_json.as_deref()
    }

    pub fn method(&self) -> Option<&str> {
        self.method.as_deref()
    }

    pub(crate) fn value_at(&self, path: &[&str]) -> Option<&Value> {
        self.at(path)
    }

    pub fn string_at(&self, path: &[&str]) -> Option<String> {
        self.at(path)?.as_str().map(str::to_string)
    }

    pub fn i64_at(&self, path: &[&str]) -> Option<i64> {
        self.at(path)?.as_i64()
    }

    pub fn text_document_uri(&self) -> Option<String> {
        self.string_at(&["params", "textDocument", "uri"])
    }

    pub fn position(&self) -> Option<Position> {
        Self::position_from_value(self.at(&["params", "position"])?)
    }

    pub fn range_at(&self, path: &[&str]) -> Option<Range> {
        Self::range_from_value(self.at(path)?)
    }

    pub fn string_vec_at(&self, path: &[&str]) -> Option<Vec<String>> {
        Some(
            self.at(path)?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        )
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let Some(diagnostics) = self
            .at(&["params", "context", "diagnostics"])
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };
        diagnostics
            .iter()
            .map(Self::diagnostic_from_value)
            .collect()
    }

    pub fn code_action_trigger_kind(&self) -> Option<CodeActionTriggerKind> {
        match self.i64_at(&["params", "context", "triggerKind"])? {
            1 => Some(CodeActionTriggerKind::Invoked),
            2 => Some(CodeActionTriggerKind::Automatic),
            _ => None,
        }
    }

    pub fn first_content_change_text(&self) -> Option<String> {
        self.value
            .get("params")?
            .get("contentChanges")?
            .as_array()?
            .first()?
            .get("text")?
            .as_str()
            .map(str::to_string)
    }

    fn at(&self, path: &[&str]) -> Option<&Value> {
        let mut current = &self.value;
        for segment in path {
            current = current.get(*segment)?;
        }
        Some(current)
    }

    fn diagnostic_from_value(value: &Value) -> Diagnostic {
        Diagnostic {
            range: Self::range_from_value(value.get("range").unwrap_or(&Value::Null))
                .unwrap_or_default(),
            severity: value
                .get("severity")
                .and_then(Value::as_i64)
                .and_then(Self::diagnostic_severity),
            code: None,
            source: value
                .get("source")
                .and_then(Value::as_str)
                .map(str::to_string),
            message: value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tags: Vec::new(),
            related_information: Vec::new(),
        }
    }

    fn diagnostic_severity(value: i64) -> Option<DiagnosticSeverity> {
        match value {
            1 => Some(DiagnosticSeverity::Error),
            2 => Some(DiagnosticSeverity::Warning),
            3 => Some(DiagnosticSeverity::Information),
            4 => Some(DiagnosticSeverity::Hint),
            _ => None,
        }
    }

    fn range_from_value(value: &Value) -> Option<Range> {
        let start = Self::position_from_value(value.get("start")?)?;
        let end = Self::position_from_value(value.get("end")?)?;
        Some(Range::new(start, end))
    }

    fn position_from_value(value: &Value) -> Option<Position> {
        let line = value.get("line")?.as_i64()? as u32;
        let character = value.get("character")?.as_i64()? as u32;
        Some(Position::new(line, character))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_rpc_parser_preserves_string_id_and_method() {
        let message = JsonRpcMessage::parse(
            r#"{
              "params": { "rootUri": "file:///workspace" },
              "method": "initialize",
              "id": "init-1",
              "jsonrpc": "2.0"
            }"#,
        )
        .expect("parse JSON-RPC message");

        assert_eq!(message.id_json(), Some("\"init-1\""));
        assert_eq!(message.method(), Some("initialize"));
        assert_eq!(
            message.string_at(&["params", "rootUri"]).as_deref(),
            Some("file:///workspace")
        );
    }

    #[test]
    fn json_rpc_parser_reads_nested_position() {
        let message = JsonRpcMessage::parse(
            r#"{
              "jsonrpc": "2.0",
              "id": 2,
              "method": "textDocument/hover",
              "params": {
                "textDocument": { "uri": "file:///workspace/main.bld" },
                "position": { "line": 3, "character": 14 }
              }
            }"#,
        )
        .expect("parse JSON-RPC message");
        let position = message.position().expect("position");

        assert_eq!(message.id_json(), Some("2"));
        assert_eq!(
            message.text_document_uri().as_deref(),
            Some("file:///workspace/main.bld")
        );
        assert_eq!(position.line, 3);
        assert_eq!(position.character, 14);
    }

    #[test]
    fn json_rpc_parser_reads_range_and_diagnostics() {
        let message = JsonRpcMessage::parse(
            r#"{
              "jsonrpc": "2.0",
              "id": 10,
              "method": "textDocument/codeAction",
              "params": {
                "range": {
                  "start": { "line": 1, "character": 13 },
                  "end": { "line": 1, "character": 14 }
                },
                "context": {
                  "diagnostics": [{
                    "range": {
                      "start": { "line": 1, "character": 13 },
                      "end": { "line": 1, "character": 14 }
                    },
                    "severity": 1,
                    "source": "buildlang",
                    "message": "expected ';'"
                  }],
                  "only": ["quickfix"],
                  "triggerKind": 1
                }
              }
            }"#,
        )
        .expect("parse JSON-RPC message");

        let range = message.range_at(&["params", "range"]).expect("range");
        let diagnostics = message.diagnostics();
        assert_eq!(range.start.line, 1);
        assert_eq!(range.end.character, 14);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "expected ';'");
        assert_eq!(
            message.string_vec_at(&["params", "context", "only"]),
            Some(vec!["quickfix".to_string()])
        );
        assert_eq!(
            message.code_action_trigger_kind(),
            Some(CodeActionTriggerKind::Invoked)
        );
    }
}
