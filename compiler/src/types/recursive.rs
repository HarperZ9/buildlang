//! Infinite-size check for recursive types (issue #83).
//!
//! A struct or enum that reaches itself through by-value fields has no finite
//! size, so the C backend cannot lay it out. Before 1.8.0 such a program passed
//! `buildc check` and then failed in the C compiler. This pass rejects it with
//! BL0384 and points to `Box<T>`, which breaks the cycle.
//!
//! By-value containment follows field types directly, through tuples and
//! arrays, and through the type arguments of `Option` and `Result` (both hold
//! their payload inline). Heap and pointer types end the walk: `Box`, `Vec`,
//! the map and set types, references and raw pointers. Arguments of other
//! generic types are not followed, so the check never rejects a type the
//! backend could lay out; it may miss a cycle through a user generic.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{self, ItemKind, StructFields};
use crate::lexer::Span;

use super::error::{TypeError, TypeErrorWithSpan};

/// Types whose storage is behind a pointer, so they end a by-value walk.
const INDIRECT: &[&str] = &[
    "Box", "Vec", "VecDeque", "HashMap", "HashSet", "BTreeMap", "BTreeSet", "Rc", "Arc", "String",
];

/// Generic types that store their type arguments inline.
const INLINE_GENERIC: &[&str] = &["Option", "Result"];

/// Errors for every user struct or enum that holds itself by value.
pub(super) fn infinite_size_errors(module: &ast::Module) -> Vec<TypeErrorWithSpan> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut spans: BTreeMap<String, Span> = BTreeMap::new();
    for item in &module.items {
        let (name, field_sets): (String, Vec<&StructFields>) = match &item.kind {
            ItemKind::Struct(s) => (s.name.name.to_string(), vec![&s.fields]),
            ItemKind::Enum(e) => (
                e.name.name.to_string(),
                e.variants.iter().map(|v| &v.fields).collect(),
            ),
            _ => continue,
        };
        let mut out = BTreeSet::new();
        for fields in field_sets {
            for ty in field_types(fields) {
                by_value_names(ty, &mut out);
            }
        }
        spans.insert(name.clone(), item.span);
        edges.insert(name, out);
    }

    let mut errors = Vec::new();
    for (name, span) in &spans {
        if let Some(path) = cycle_through(name, &edges) {
            let err = TypeErrorWithSpan::new(
                TypeError::InfiniteSizeType {
                    name: name.clone(),
                    path: path.join(" -> "),
                },
                *span,
            )
            .with_help(format!(
                "wrap the field that holds `{}` in `Box<...>`, for example `Box<{}>`",
                path[1], path[1]
            ));
            errors.push(err);
        }
    }
    errors
}

fn field_types(fields: &StructFields) -> Vec<&ast::Type> {
    match fields {
        StructFields::Named(fs) => fs.iter().map(|f| f.ty.as_ref()).collect(),
        StructFields::Tuple(fs) => fs.iter().map(|f| f.ty.as_ref()).collect(),
        StructFields::Unit => Vec::new(),
    }
}

/// Collect the names of types `ty` stores inline.
fn by_value_names(ty: &ast::Type, out: &mut BTreeSet<String>) {
    match &ty.kind {
        ast::TypeKind::Path(path) => {
            let Some(ident) = path.last_ident() else {
                return;
            };
            let name = ident.name.as_ref();
            if INDIRECT.contains(&name) {
                return;
            }
            out.insert(name.to_string());
            if INLINE_GENERIC.contains(&name) {
                for arg in path.last_generics().unwrap_or(&[]) {
                    if let ast::GenericArg::Type(inner) = arg {
                        by_value_names(inner, out);
                    }
                }
            }
        }
        ast::TypeKind::Tuple(elems) => elems.iter().for_each(|e| by_value_names(e, out)),
        ast::TypeKind::Array { elem, .. } => by_value_names(elem, out),
        _ => {}
    }
}

/// A path `start -> ... -> start` through by-value edges, if one exists.
fn cycle_through(start: &str, edges: &BTreeMap<String, BTreeSet<String>>) -> Option<Vec<String>> {
    let mut stack: Vec<(String, Vec<String>)> = vec![(start.to_string(), vec![start.to_string()])];
    let mut seen = BTreeSet::new();
    while let Some((node, path)) = stack.pop() {
        for next in edges.get(&node).into_iter().flatten() {
            if next == start {
                let mut full = path.clone();
                full.push(start.to_string());
                return Some(full);
            }
            if edges.contains_key(next) && seen.insert(next.clone()) {
                let mut p = path.clone();
                p.push(next.clone());
                stack.push((next.clone(), p));
            }
        }
    }
    None
}
