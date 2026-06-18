use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use super::model::ModuleGraphDigest;

pub(super) fn sorted(values: BTreeSet<String>) -> Vec<String> {
    values.into_iter().collect()
}

pub(super) fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write digest hex");
    }
    hex
}

pub(super) fn module_digest(hex: String) -> ModuleGraphDigest {
    ModuleGraphDigest {
        algorithm: "sha256".to_string(),
        hex,
    }
}

fn is_lexically_invalid_relative_path(relative: &str) -> bool {
    relative.starts_with('\\')
        || relative
            .as_bytes()
            .get(0..2)
            .is_some_and(|bytes| bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn validate_corpus_relative_path(
    root: &Path,
    relative: &str,
    field: &str,
) -> Result<PathBuf, String> {
    if relative.trim().is_empty() {
        return Err(format!("module graph {field} must not be empty"));
    }
    let relative_path = Path::new(relative);
    if is_lexically_invalid_relative_path(relative)
        || relative_path.is_absolute()
        || relative_path.has_root()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "module graph {field} must stay within corpus root: {relative}"
        ));
    }
    let canonical_root = root.canonicalize().map_err(|err| {
        format!(
            "module graph {field} failed to canonicalize corpus root {}: {err}",
            root.display()
        )
    })?;
    let path = root.join(relative_path);
    if !path.is_file() {
        return Err(format!(
            "module graph {field} path not found: {}",
            path.display()
        ));
    }
    let canonical_path = path.canonicalize().map_err(|err| {
        format!(
            "module graph {field} failed to canonicalize path {}: {err}",
            path.display()
        )
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "module graph {field} must stay within corpus root: {relative}"
        ));
    }
    Ok(canonical_path)
}

pub(super) fn corpus_relative_path(
    root: &Path,
    path: &Path,
    field: &str,
) -> Result<String, String> {
    let canonical_root = root.canonicalize().map_err(|err| {
        format!(
            "module graph {field} failed to canonicalize corpus root {}: {err}",
            root.display()
        )
    })?;
    let canonical_path = path.canonicalize().map_err(|err| {
        format!(
            "module graph {field} failed to canonicalize path {}: {err}",
            path.display()
        )
    })?;
    let relative = canonical_path.strip_prefix(&canonical_root).map_err(|_| {
        format!(
            "module graph {field} must stay within corpus root: {}",
            path.display()
        )
    })?;
    Ok(slash_path(relative))
}
