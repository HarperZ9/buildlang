use serde_json::Value;

use super::types::Position;

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
        let line = self.i64_at(&["params", "position", "line"])? as u32;
        let character = self.i64_at(&["params", "position", "character"])? as u32;
        Some(Position::new(line, character))
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
                "textDocument": { "uri": "file:///workspace/main.quanta" },
                "position": { "line": 3, "character": 14 }
              }
            }"#,
        )
        .expect("parse JSON-RPC message");
        let position = message.position().expect("position");

        assert_eq!(message.id_json(), Some("2"));
        assert_eq!(
            message.text_document_uri().as_deref(),
            Some("file:///workspace/main.quanta")
        );
        assert_eq!(position.line, 3);
        assert_eq!(position.character, 14);
    }
}
