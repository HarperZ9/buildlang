//! Duplicate definitions across the files of one package (BL0612).
//!
//! A package is checked as one joined text, so every file in `src/` shares
//! one namespace. Two files that each define `struct Sheet` used to be
//! reported, if at all, as a duplicate "already defined on line N" where N
//! was a line of the joined text, a file that does not exist. A function
//! defined in two files with the same parameter types was not reported at
//! its definitions at all: a call to it failed with BL0901, and with no call
//! the check passed. A trait or a type alias defined twice passed silently.
//!
//! [`rewrite`] runs on the errors of a package check, with the join's line
//! map. It reports each cross-file duplicate once, at the later definition,
//! naming both files and lines (BL0612). A duplicate inside one file keeps
//! its BL0380, with the line of that file.
//!
//! Checked across files: constants and statics (one namespace), structs,
//! enums, traits and type aliases (one namespace), and functions that have a
//! body and the same parameter types (two functions of one name with
//! different parameter types are overloads). Items of a module loaded with
//! `mod NAME;` and methods inside an `impl` are not walked.

use std::collections::BTreeMap;

use crate::ast::{self, ItemKind};
use crate::lexer::{SourceFile, Span};
use crate::pkg::local::linemap::{LineMap, Origin};

use super::error::{TypeError, TypeErrorWithSpan};

/// The origin of the line `span` starts on, when the span belongs to `file`.
fn origin_of<'a>(span: Span, file: &SourceFile, map: &'a LineMap) -> Option<Origin<'a>> {
    if span.source_id != file.id || span.end.0 as usize > file.source().len() {
        return None;
    }
    map.origin(file.lookup_line(span.start) + 1)
}

/// The text of `span` with every whitespace character removed.
fn squeezed(span: Span, file: &SourceFile) -> Option<String> {
    let text = file
        .source()
        .get(span.start.0 as usize..span.end.0 as usize)?;
    Some(text.chars().filter(|c| !c.is_whitespace()).collect())
}

/// The namespace an item defines a name in, the name and the name's span.
/// Values are `v`, types `t`, and a function is `f(<parameter types>)`.
fn defined(item: &ast::Item, file: &SourceFile) -> Option<(String, String, Span)> {
    let (space, name) = match &item.kind {
        ItemKind::Const(c) => ("v".to_string(), &c.name),
        ItemKind::Static(s) => ("v".to_string(), &s.name),
        ItemKind::Struct(s) => ("t".to_string(), &s.name),
        ItemKind::Enum(e) => ("t".to_string(), &e.name),
        ItemKind::Trait(t) => ("t".to_string(), &t.name),
        ItemKind::TypeAlias(a) => ("t".to_string(), &a.name),
        ItemKind::Function(f) => {
            f.body.as_ref()?;
            let mut types = Vec::new();
            for param in &f.sig.params {
                types.push(squeezed(param.ty.span, file)?);
            }
            (format!("f({})", types.join(",")), &f.name)
        }
        _ => return None,
    };
    Some((space, name.name.to_string(), name.span))
}

/// Rewrite the duplicate-definition errors of a package check and add the
/// cross-file duplicates. `module` is the parsed joined text, `file` its
/// source and `map` the origin of each of its lines.
pub fn rewrite(
    module: &ast::Module,
    file: &SourceFile,
    map: &LineMap,
    errors: &mut Vec<TypeErrorWithSpan>,
) {
    // A BL0380 names the line of the first definition. Inside one file that
    // line becomes the file's own line. Across files the error goes: the walk
    // below reports the pair with both places.
    errors.retain_mut(|e| {
        let here = origin_of(e.span, file, map);
        let TypeError::DuplicateDefinition {
            first_line: Some(first),
            ..
        } = &mut e.error
        else {
            return true;
        };
        match (here, map.origin(*first as usize)) {
            (Some(h), Some(t)) if h.file == t.file => {
                *first = t.line as u32;
                true
            }
            (Some(_), Some(_)) => false,
            _ => true,
        }
    });

    let mut first: BTreeMap<(String, String), Span> = BTreeMap::new();
    for item in &module.items {
        let Some((space, name, span)) = defined(item, file) else {
            continue;
        };
        let Some(here) = origin_of(span, file, map) else {
            continue;
        };
        let key = (space, name);
        let Some(earlier) = first.get(&key) else {
            first.insert(key, span);
            continue;
        };
        let Some(there) = origin_of(*earlier, file, map) else {
            continue;
        };
        if there.file == here.file {
            continue;
        }
        errors.push(
            TypeErrorWithSpan::new(
                TypeError::DuplicateAcrossFiles {
                    name: key.1,
                    first: format!("{}:{}", there.file, there.line),
                    second: format!("{}:{}", here.file, here.line),
                },
                span,
            )
            .with_help(
                "the files of a package share one namespace: rename one of the two, or delete the copy",
            ),
        );
    }
}
