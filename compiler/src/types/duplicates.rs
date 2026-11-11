//! Duplicate `const` and `static` names (G29).
//!
//! Two constants or statics of one name in one module passed the check and
//! reached the C compiler, which stopped with "redefinition". This pass
//! reports the second definition with BL0380 and names the line of the first.
//!
//! Constants and statics share one namespace, the module's values. An inline
//! `mod` is its own scope. A module loaded from another file is not walked.

use std::collections::BTreeMap;

use crate::ast::{self, ItemKind};
use crate::lexer::Span;

use super::error::{TypeError, TypeErrorWithSpan};

/// The 1-based line of byte `pos` in `source`, or `None` without the text.
fn line_of(source: Option<&str>, pos: u32) -> Option<u32> {
    let text = source?.as_bytes();
    let end = (pos as usize).min(text.len());
    let breaks = text[..end].iter().filter(|b| **b == b'\n').count();
    u32::try_from(breaks + 1).ok()
}

/// The name a `const` or `static` item defines, with the name's span.
fn value_name(item: &ast::Item) -> Option<(&str, Span)> {
    match &item.kind {
        ItemKind::Const(c) => Some((c.name.name.as_ref(), c.name.span)),
        ItemKind::Static(s) => Some((s.name.name.as_ref(), s.name.span)),
        _ => None,
    }
}

fn walk(items: &[ast::Item], source: Option<&str>, errors: &mut Vec<TypeErrorWithSpan>) {
    let mut first: BTreeMap<&str, Span> = BTreeMap::new();
    for item in items {
        if let ItemKind::Mod(m) = &item.kind {
            if let Some(content) = &m.content {
                walk(&content.items, source, errors);
            }
            continue;
        }
        let Some((name, span)) = value_name(item) else {
            continue;
        };
        match first.get(name) {
            Some(earlier) => errors.push(TypeErrorWithSpan::new(
                TypeError::DuplicateDefinition {
                    name: name.to_string(),
                    first_line: line_of(source, earlier.start.0),
                },
                span,
            )),
            None => {
                first.insert(name, span);
            }
        }
    }
}

/// One error for every `const` or `static` whose name an earlier `const` or
/// `static` of the same module already defines.
pub(super) fn duplicate_value_errors(
    module: &ast::Module,
    source: Option<&str>,
) -> Vec<TypeErrorWithSpan> {
    let mut errors = Vec::new();
    walk(&module.items, source, &mut errors);
    errors
}
