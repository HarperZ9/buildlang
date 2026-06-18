const TYPE_TYPE: u32 = 1;
const FUNCTION_TYPE: u32 = 2;
const VARIABLE_TYPE: u32 = 3;
const PROPERTY_TYPE: u32 = 5;
const KEYWORD_TYPE: u32 = 6;
const MACRO_TYPE: u32 = 11;

const DECLARATION_MODIFIER: u32 = 1 << 0;
const DEFINITION_MODIFIER: u32 = 1 << 1;
const READONLY_MODIFIER: u32 = 1 << 2;

#[derive(Debug, Clone, Copy)]
pub(super) enum PendingDeclaration {
    Function,
    Type,
    Variable { readonly: bool },
}

pub(super) fn classify_identifier(
    ident: &str,
    pending: Option<PendingDeclaration>,
    previous_was_dot: bool,
    line: &str,
    end: usize,
) -> (u32, u32) {
    if is_keyword(ident) {
        return (KEYWORD_TYPE, 0);
    }
    if end > ident.len() && line[..end].ends_with('!') {
        return (MACRO_TYPE, 0);
    }
    if let Some(pending) = pending {
        return match pending {
            PendingDeclaration::Function => {
                (FUNCTION_TYPE, DECLARATION_MODIFIER | DEFINITION_MODIFIER)
            }
            PendingDeclaration::Type => (TYPE_TYPE, DECLARATION_MODIFIER | DEFINITION_MODIFIER),
            PendingDeclaration::Variable { readonly } => {
                let modifiers = DECLARATION_MODIFIER
                    | DEFINITION_MODIFIER
                    | if readonly { READONLY_MODIFIER } else { 0 };
                (VARIABLE_TYPE, modifiers)
            }
        };
    }
    if previous_was_dot {
        return (PROPERTY_TYPE, 0);
    }
    if is_type_like(ident) {
        return (TYPE_TYPE, 0);
    }
    if next_non_ws(line, end).is_some_and(|(_, ch)| ch == '(') {
        return (FUNCTION_TYPE, 0);
    }
    (VARIABLE_TYPE, 0)
}

pub(super) fn pending_after_identifier(ident: &str) -> Option<PendingDeclaration> {
    match ident {
        "fn" => Some(PendingDeclaration::Function),
        "struct" | "enum" | "trait" | "impl" | "type" => Some(PendingDeclaration::Type),
        "let" => Some(PendingDeclaration::Variable { readonly: false }),
        "const" => Some(PendingDeclaration::Variable { readonly: true }),
        _ => None,
    }
}

pub(super) fn next_non_ws(line: &str, start: usize) -> Option<(usize, char)> {
    let mut index = start;
    while index < line.len() {
        let ch = line[index..].chars().next()?;
        if !ch.is_whitespace() {
            return Some((index, ch));
        }
        index += ch.len_utf8();
    }
    None
}

fn is_keyword(ident: &str) -> bool {
    matches!(
        ident,
        "fn" | "let"
            | "const"
            | "struct"
            | "enum"
            | "trait"
            | "impl"
            | "type"
            | "use"
            | "mod"
            | "pub"
            | "return"
            | "if"
            | "else"
            | "match"
            | "for"
            | "while"
            | "loop"
            | "break"
            | "continue"
            | "async"
            | "await"
            | "effect"
            | "perform"
            | "handle"
            | "true"
            | "false"
    )
}

fn is_type_like(ident: &str) -> bool {
    matches!(
        ident,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "f32"
            | "f64"
            | "bool"
            | "str"
            | "String"
    ) || ident.chars().next().is_some_and(char::is_uppercase)
}
