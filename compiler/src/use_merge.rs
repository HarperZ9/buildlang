//! A `use` of a module file loads that file.
//!
//! `use shapes::Square;` with `shapes.bld` beside the program used to bind
//! `Square` as a value of unknown type: the check accepted any use of it and
//! the C build then had no struct or function behind the name. The driver now
//! merges the module file into the program, the way `mod shapes;` does, so
//! the import brings the real type or signature.
//!
//! This file finds the module file behind each `use` leaf. The merge itself
//! is `resolve_modules_with_prefix` in `main.rs`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use buildlang::ast::{self, ItemKind, Module};
use buildlang::types::{clean_module_export_names, find_use_module_file, is_builtin_std_module};

/// One module file that `use` items of a file ask for.
pub(crate) struct UseRequest {
    /// The module path as written (`shapes`, `std::math`), for messages.
    pub name: String,
    pub file: PathBuf,
    /// Where the module file's own `mod` and `use` lines resolve.
    pub sub_source_dir: PathBuf,
    /// The prefix its functions get: the path joined with `_`, which is what
    /// a path call (`shapes::area`) lowers to.
    pub full_prefix: String,
    /// The prefix handed to the module file's own `mod` declarations.
    pub child_prefix: String,
    /// `use m::*`: every function of the module is callable by its bare name.
    pub glob: bool,
    /// `(item, local name)` for each name imported one by one.
    pub names: Vec<(String, String)>,
}

/// The file-backed imports of one file.
pub(crate) struct UseRequests {
    pub requests: Vec<UseRequest>,
    /// Indexes into `ast.items` of the `use` items whose every leaf names a
    /// module file found here.
    pub satisfied_items: Vec<usize>,
}

struct Leaf {
    segs: Vec<Arc<str>>,
    glob: bool,
    rename: Option<Arc<str>>,
}

fn leaves(tree: &ast::UseTree, prefix: &[Arc<str>], out: &mut Vec<Leaf>) {
    let join = |path: &ast::Path| {
        let mut segs = prefix.to_vec();
        segs.extend(path.segments.iter().map(|s| s.ident.name.clone()));
        segs
    };
    match &tree.kind {
        ast::UseTreeKind::Simple { path, rename } => out.push(Leaf {
            segs: join(path),
            glob: false,
            rename: rename.as_ref().map(|r| r.name.clone()),
        }),
        ast::UseTreeKind::Glob(path) => out.push(Leaf {
            segs: join(path),
            glob: true,
            rename: None,
        }),
        ast::UseTreeKind::Nested { path, trees } => {
            let inner = join(path);
            for t in trees {
                leaves(t, &inner, out);
            }
        }
    }
}

struct Found {
    name: String,
    file: PathBuf,
    full_prefix: String,
    child_prefix: String,
    /// The imported item, or `None` for a glob.
    item: Option<String>,
}

fn join(segs: &[Arc<str>], sep: &str) -> String {
    segs.iter()
        .map(|s| s.as_ref())
        .collect::<Vec<_>>()
        .join(sep)
}

/// The module file behind one leaf, when the leaf names a file that parses
/// with no error and declares the imported name. Anything else is left to
/// the checker, which reports an import that names nothing (BL0611).
fn find(
    leaf: &Leaf,
    source_dir: &Path,
    stdlib: Option<&Path>,
    local: &HashSet<Arc<str>>,
) -> Option<Found> {
    let segs = &leaf.segs;
    let root = segs.first()?.as_ref();
    if matches!(root, "crate" | "self" | "super") {
        return None;
    }
    if matches!(root, "std" | "core") {
        let module = segs.get(1)?;
        if is_builtin_std_module(module) {
            return None;
        }
        let file = stdlib?.join(format!("{}.bld", module));
        if !file.is_file() {
            return None;
        }
        let names = clean_module_export_names(&file)?;
        let item = if leaf.glob {
            if segs.len() != 2 {
                return None;
            }
            None
        } else {
            if segs.len() != 3 || !names.contains(&segs[2]) {
                return None;
            }
            Some(segs[2].to_string())
        };
        return Some(Found {
            name: join(&segs[..2], "::"),
            file,
            full_prefix: module.to_string(),
            child_prefix: String::new(),
            item,
        });
    }
    if local.contains(root) {
        return None;
    }
    let module_segs: &[Arc<str>] = if leaf.glob {
        &segs[..]
    } else {
        if segs.len() < 2 || segs[segs.len() - 1].as_ref() == "self" {
            return None;
        }
        &segs[..segs.len() - 1]
    };
    let file = find_use_module_file(source_dir, module_segs)?;
    let names = clean_module_export_names(&file)?;
    let item = if leaf.glob {
        None
    } else {
        let last = &segs[segs.len() - 1];
        if !names.contains(last) {
            return None;
        }
        Some(last.to_string())
    };
    let last_seg = module_segs[module_segs.len() - 1].as_ref();
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let directory_module = matches!(stem, "lib" | "mod") && stem != last_seg;
    let full_prefix = join(module_segs, "_");
    let child_prefix = if directory_module {
        full_prefix.clone()
    } else {
        join(&module_segs[..module_segs.len() - 1], "_")
    };
    Some(Found {
        name: join(module_segs, "::"),
        file,
        full_prefix,
        child_prefix,
        item,
    })
}

/// Every module file the top-level `use` items of `ast` name.
pub(crate) fn collect(ast: &Module, source_dir: &Path, stdlib: Option<&Path>) -> UseRequests {
    let local: HashSet<Arc<str>> = ast
        .items
        .iter()
        .filter_map(|item| item.name().map(|n| n.name.clone()))
        .collect();
    let mut requests: Vec<UseRequest> = Vec::new();
    let mut satisfied_items = Vec::new();
    for (index, item) in ast.items.iter().enumerate() {
        let ItemKind::Use(use_def) = &item.kind else {
            continue;
        };
        let mut found_leaves = Vec::new();
        leaves(&use_def.tree, &[], &mut found_leaves);
        let mut all_found = !found_leaves.is_empty();
        for leaf in &found_leaves {
            let Some(found) = find(leaf, source_dir, stdlib, &local) else {
                all_found = false;
                continue;
            };
            let position = match requests.iter().position(|r| r.file == found.file) {
                Some(position) => position,
                None => {
                    let sub_source_dir =
                        found.file.parent().unwrap_or(Path::new(".")).to_path_buf();
                    requests.push(UseRequest {
                        name: found.name,
                        file: found.file,
                        sub_source_dir,
                        full_prefix: found.full_prefix,
                        child_prefix: found.child_prefix,
                        glob: false,
                        names: Vec::new(),
                    });
                    requests.len() - 1
                }
            };
            match found.item {
                None => requests[position].glob = true,
                Some(item) => {
                    let local_name = leaf
                        .rename
                        .as_ref()
                        .map(|r| r.to_string())
                        .unwrap_or_else(|| item.clone());
                    requests[position].names.push((item, local_name));
                }
            }
        }
        if all_found {
            satisfied_items.push(index);
        }
    }
    UseRequests {
        requests,
        satisfied_items,
    }
}
