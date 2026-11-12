//! Duplicate `const`, `static`, `struct` and `enum` names (G29).
//!
//! Two definitions of one name in one module passed the check and reached
//! the C compiler, which stopped with "redefinition". This pass reports the
//! second definition with BL0380 and names the line of the first.
//!
//! Constants and statics share one namespace, the module's values; structs
//! and enums share the type namespace. Functions are not checked here: two
//! functions of one name with different parameter types are overloads, and
//! dispatch picks between them. An inline
//! `mod` is its own scope. A module loaded from another file is not walked.
//! Methods inside an `impl` are not items of the module and are not checked.

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

/// The namespace and the name an item defines, with the name's span:
/// 0 values, 2 types.
fn defined_name(item: &ast::Item) -> Option<(u8, &str, Span)> {
    match &item.kind {
        ItemKind::Const(c) => Some((0, c.name.name.as_ref(), c.name.span)),
        ItemKind::Static(s) => Some((0, s.name.name.as_ref(), s.name.span)),
        ItemKind::Struct(s) => Some((2, s.name.name.as_ref(), s.name.span)),
        ItemKind::Enum(e) => Some((2, e.name.name.as_ref(), e.name.span)),
        _ => None,
    }
}

fn walk(items: &[ast::Item], source: Option<&str>, errors: &mut Vec<TypeErrorWithSpan>) {
    let mut first: BTreeMap<(u8, &str), Span> = BTreeMap::new();
    for item in items {
        if let ItemKind::Mod(m) = &item.kind {
            if let Some(content) = &m.content {
                walk(&content.items, source, errors);
            }
            continue;
        }
        let Some((space, name, span)) = defined_name(item) else {
            continue;
        };
        match first.get(&(space, name)) {
            Some(earlier) => errors.push(TypeErrorWithSpan::new(
                TypeError::DuplicateDefinition {
                    name: name.to_string(),
                    first_line: line_of(source, earlier.start.0),
                },
                span,
            )),
            None => {
                first.insert((space, name), span);
            }
        }
    }
}

/// One error for every definition whose name an earlier definition of the
/// same namespace in the same module already defines.
pub(super) fn duplicate_value_errors(
    module: &ast::Module,
    source: Option<&str>,
) -> Vec<TypeErrorWithSpan> {
    let mut errors = Vec::new();
    walk(&module.items, source, &mut errors);
    errors
}
