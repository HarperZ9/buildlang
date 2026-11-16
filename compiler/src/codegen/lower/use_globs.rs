//! `use` inside a program: names brought in by a glob or by name.
//!
//! The lowerer names a function or constant of an inline module after the
//! module's path (`transfer::srgb_eotf` is `transfer_srgb_eotf`). A bare
//! name inside a module was tried with the module's own prefix and then as
//! written, so `use super::transfer::*;` followed by `srgb_eotf(x)` named a
//! function that does not exist and the C backend refused the call (BL0901).
//! The `use` items are now recorded while items are collected, and a bare
//! name that resolves nowhere else is tried through them: each glob's
//! module, then a name imported by itself (`use super::k::ONE;`). Only
//! names that exist as a function or a global are taken.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ast;

use super::MirLowerer;

/// What the `use` items of one module bring in: the prefix of each glob's
/// module (`transfer_`, or `` for the program's root), and each name
/// imported by itself with the C name it stands for.
#[derive(Default)]
pub(crate) struct ModuleUses {
    globs: Vec<String>,
    names: HashMap<Arc<str>, String>,
}

impl<'ctx> MirLowerer<'ctx> {
    /// The current module's path, as the key of `module_uses`.
    fn module_key(&self) -> String {
        self.module_prefix
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join("::")
    }

    /// Record a `use` item of the current module.
    pub(super) fn note_use(&mut self, u: &ast::UseDef) {
        let here: Vec<Arc<str>> = self.module_prefix.clone();
        let key = self.module_key();
        let mut uses = self.module_uses.remove(&key).unwrap_or_default();
        Self::note_use_tree(&here, &[], &u.tree, &mut uses);
        self.module_uses.insert(key, uses);
    }

    fn note_use_tree(
        here: &[Arc<str>],
        outer: &[Arc<str>],
        tree: &ast::UseTree,
        uses: &mut ModuleUses,
    ) {
        let joined = |path: &ast::Path| -> Vec<Arc<str>> {
            let mut all: Vec<Arc<str>> = outer.to_vec();
            all.extend(path.segments.iter().map(|s| s.ident.name.clone()));
            all
        };
        match &tree.kind {
            ast::UseTreeKind::Glob(path) => {
                if let Some(target) = Self::module_path_from(here, &joined(path)) {
                    uses.globs.push(Self::prefix_of(&target));
                }
            }
            ast::UseTreeKind::Simple { path, rename } => {
                let full = joined(path);
                let Some((last, module)) = full.split_last() else {
                    return;
                };
                if let Some(target) = Self::module_path_from(here, module) {
                    let local = rename.as_ref().map_or(last.clone(), |r| r.name.clone());
                    uses.names
                        .insert(local, format!("{}{}", Self::prefix_of(&target), last));
                }
            }
            ast::UseTreeKind::Nested { path, trees } => {
                let base = joined(path);
                for t in trees {
                    Self::note_use_tree(here, &base, t, uses);
                }
            }
        }
    }

    /// The module a `use` path names, from the program's root. `super`,
    /// `self` and `crate` are read against the current module; a path that
    /// starts with `std` or `core` is not a module of the program.
    fn module_path_from(here: &[Arc<str>], path: &[Arc<str>]) -> Option<Vec<Arc<str>>> {
        let mut out: Vec<Arc<str>> = here.to_vec();
        let mut rest = path;
        match path.first().map(|s| s.as_ref()) {
            Some("std") | Some("core") => return None,
            Some("crate") => {
                out.clear();
                rest = &path[1..];
            }
            Some("self") => rest = &path[1..],
            Some("super") => {
                while let Some(("super", tail)) = rest.split_first().map(|(h, t)| (h.as_ref(), t)) {
                    out.pop()?;
                    rest = tail;
                }
            }
            _ => out.clear(),
        }
        out.extend(rest.iter().cloned());
        Some(out)
    }

    fn prefix_of(module: &[Arc<str>]) -> String {
        let mut s = String::new();
        for seg in module {
            s.push_str(seg);
            s.push('_');
        }
        s
    }

    /// The C name a bare value name stands for through the current module's
    /// `use` items, if one of them names a function or a global that exists.
    pub(super) fn use_resolved(&self, bare: &str) -> Option<Arc<str>> {
        let uses = self.module_uses.get(&self.module_key())?;
        let exists = |name: &str| {
            self.module.find_function(name).is_some() || self.module.find_global(name).is_some()
        };
        if let Some(name) = uses.names.get(bare) {
            if name != bare && exists(name) {
                return Some(Arc::from(name.as_str()));
            }
        }
        for prefix in &uses.globs {
            if prefix.is_empty() {
                continue;
            }
            let candidate = format!("{prefix}{bare}");
            if exists(&candidate) {
                return Some(Arc::from(candidate));
            }
        }
        None
    }
}
