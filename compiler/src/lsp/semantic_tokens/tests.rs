use super::*;
use crate::lsp::document::Document;

fn doc(source: &str) -> Document {
    Document::new(
        "file:///main.quanta".into(),
        "quanta".into(),
        1,
        source.into(),
    )
}

#[test]
fn semantic_tokens_encode_core_quantalang_surface() {
    let tokens = SemanticTokensProvider::new().full(&doc(
        "// comment\nfn helper() -> i32 { 42 }\nfn main() { helper(\"x\"); }\n",
    ));

    assert_eq!(tokens.data.len() % 5, 0);
    let decoded = decode_absolute(&tokens.data);
    assert!(decoded.iter().any(|t| t.line == 0 && t.token_type == 7));
    assert!(decoded
        .iter()
        .any(|t| t.line == 1 && t.start == 0 && t.token_type == 6));
    assert!(decoded
        .iter()
        .any(|t| t.line == 1 && t.start == 3 && t.token_type == 2 && t.modifiers == 3));
    assert!(decoded.iter().any(|t| t.line == 1 && t.token_type == 9));
    assert!(decoded.iter().any(|t| t.line == 2 && t.token_type == 8));
}

#[test]
fn semantic_tokens_return_best_effort_for_malformed_source() {
    let tokens = SemanticTokensProvider::new().full(&doc("fn broken(\"unterminated\nlet x = 1\n"));

    assert!(!tokens.data.is_empty());
    assert_eq!(tokens.data.len() % 5, 0);
}

struct AbsoluteToken {
    line: u32,
    start: u32,
    token_type: u32,
    modifiers: u32,
}

fn decode_absolute(data: &[u32]) -> Vec<AbsoluteToken> {
    let (mut line, mut start) = (0, 0);
    data.chunks_exact(5)
        .map(|chunk| {
            line += chunk[0];
            start = if chunk[0] == 0 {
                start + chunk[1]
            } else {
                chunk[1]
            };
            AbsoluteToken {
                line,
                start,
                token_type: chunk[3],
                modifiers: chunk[4],
            }
        })
        .collect()
}
