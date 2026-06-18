use super::super::document::Document;
use super::classify::{classify_identifier, next_non_ws, pending_after_identifier};

const COMMENT_TYPE: u32 = 7;
const STRING_TYPE: u32 = 8;
const NUMBER_TYPE: u32 = 9;
const OPERATOR_TYPE: u32 = 10;

#[derive(Debug, Clone, Copy)]
struct Token {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    modifiers: u32,
}

pub(super) fn scan_document(doc: &Document) -> Vec<u32> {
    let mut tokens = Vec::new();
    for line in 0..doc.line_count() {
        if let Some(text) = doc.line(line as u32) {
            scan_line(line as u32, text, &mut tokens);
        }
    }
    encode_tokens(&tokens)
}

fn scan_line(line_number: u32, line: &str, tokens: &mut Vec<Token>) {
    let mut index = 0;
    let mut pending = None;
    let mut previous_was_dot = false;
    while index < line.len() {
        let Some(ch) = line[index..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            index += ch.len_utf8();
            continue;
        }
        if line[index..].starts_with("//") {
            push_token(
                line_number,
                line,
                index,
                line.len(),
                COMMENT_TYPE,
                0,
                tokens,
            );
            break;
        }
        if ch == '"' {
            let end = string_end(line, index);
            push_token(line_number, line, index, end, STRING_TYPE, 0, tokens);
            index = end;
            previous_was_dot = false;
            continue;
        }
        if ch.is_ascii_digit() {
            let end = consume_number(line, index);
            push_token(line_number, line, index, end, NUMBER_TYPE, 0, tokens);
            index = end;
            previous_was_dot = false;
            continue;
        }
        if is_identifier_start(ch) {
            let end = consume_identifier(line, index);
            let ident = &line[index..end];
            let macro_end = if next_non_ws(line, end) == Some((end, '!')) {
                end + 1
            } else {
                end
            };
            let (token_type, modifiers) =
                classify_identifier(ident, pending, previous_was_dot, line, macro_end);
            push_token(
                line_number,
                line,
                index,
                macro_end,
                token_type,
                modifiers,
                tokens,
            );
            pending = pending_after_identifier(ident);
            index = macro_end;
            previous_was_dot = false;
            continue;
        }
        if let Some(operator) = match_operator(&line[index..]) {
            let end = index + operator.len();
            push_token(line_number, line, index, end, OPERATOR_TYPE, 0, tokens);
            index = end;
            previous_was_dot = operator == ".";
            continue;
        }
        index += ch.len_utf8();
        previous_was_dot = false;
    }
}

fn push_token(
    line_number: u32,
    line: &str,
    start: usize,
    end: usize,
    token_type: u32,
    modifiers: u32,
    tokens: &mut Vec<Token>,
) {
    if end <= start {
        return;
    }
    tokens.push(Token {
        line: line_number,
        start: utf16_len(&line[..start]),
        length: utf16_len(&line[start..end]),
        token_type,
        modifiers,
    });
}

fn encode_tokens(tokens: &[Token]) -> Vec<u32> {
    let mut data = Vec::with_capacity(tokens.len() * 5);
    let mut previous_line = 0;
    let mut previous_start = 0;
    for token in tokens {
        let delta_line = token.line - previous_line;
        let delta_start = if delta_line == 0 {
            token.start - previous_start
        } else {
            token.start
        };
        data.extend([
            delta_line,
            delta_start,
            token.length,
            token.token_type,
            token.modifiers,
        ]);
        previous_line = token.line;
        previous_start = token.start;
    }
    data
}

fn string_end(line: &str, start: usize) -> usize {
    let mut escaped = false;
    for (offset, ch) in line[start + 1..].char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return start + 1 + offset + ch.len_utf8();
        }
    }
    line.len()
}

fn consume_number(line: &str, start: usize) -> usize {
    consume_while(line, start, |ch| ch.is_ascii_digit() || ch == '.')
}

fn consume_identifier(line: &str, start: usize) -> usize {
    consume_while(line, start, is_identifier_continue)
}

fn consume_while(line: &str, start: usize, predicate: fn(char) -> bool) -> usize {
    let mut end = start;
    for ch in line[start..].chars() {
        if !predicate(ch) {
            break;
        }
        end += ch.len_utf8();
    }
    end
}

fn match_operator(rest: &str) -> Option<&'static str> {
    ["==", "!=", "<=", ">=", "&&", "||", "->", "=>", "::"]
        .into_iter()
        .chain([
            "+", "-", "*", "/", "%", "=", "<", ">", "!", ".", ":", ";", ",", "|", "&",
        ])
        .find(|operator| rest.starts_with(operator))
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}

fn utf16_len(text: &str) -> u32 {
    text.chars().map(|ch| ch.len_utf16() as u32).sum()
}
