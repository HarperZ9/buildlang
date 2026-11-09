// ===============================================================================
// PACKAGE CONTENT: FILE SET, LINE ENDINGS AND THE CONTENT HASH
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! What a package is made of, and how its hash is computed.
//!
//! The file set is every regular file at the top of the package directory
//! plus everything under `src/` and `tests/`. Other subdirectories are not
//! part of the package, so a registry may keep later versions in sibling
//! folders without changing an earlier version's hash. Hidden files and
//! `target/` are skipped.
//!
//! The hash is SHA-256 over, for each file in byte order of its `/`-joined
//! relative path: `file <path> <length>\n`, the bytes, then `\n`. The result
//! is written `sha256:<64 hex digits>`. The bytes are hashed as stored.
//! `publish` writes text files outside `tests/` with LF line endings, and
//! the repository keeps git from converting `registry/`, so a checkout on any
//! platform hashes the same.

use super::{PkgError, PkgResult};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Directories below the package root that belong to the package.
const PACKAGE_SUBDIRS: &[&str] = &["src", "tests"];

/// Extensions treated as text: published with LF line endings. The empty
/// extension covers `LICENSE` and `NOTICE`.
const TEXT_EXTENSIONS: &[&str] = &[
    "bld", "toml", "md", "txt", "json", "jsonl", "csv", "tsv", "svg", "py", "",
];

/// Relative path (with `/`) and absolute path of every package file, sorted
/// by the bytes of the relative path.
pub fn package_files(root: &Path) -> PkgResult<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for path in read_dir_sorted(root)? {
        let name = file_name(&path);
        if name.starts_with('.') {
            continue;
        }
        if path.is_file() {
            out.push((name, path));
        } else if path.is_dir() && PACKAGE_SUBDIRS.contains(&name.as_str()) {
            walk(&path, &name, &mut out)?;
        }
    }
    out.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> PkgResult<()> {
    for path in read_dir_sorted(dir)? {
        let name = file_name(&path);
        if name.starts_with('.') || name == "target" {
            continue;
        }
        let rel = format!("{}/{}", prefix, name);
        if path.is_dir() {
            walk(&path, &rel, out)?;
        } else if path.is_file() {
            out.push((rel, path));
        }
    }
    Ok(())
}

fn read_dir_sorted(dir: &Path) -> PkgResult<Vec<PathBuf>> {
    let rd = std::fs::read_dir(dir).map_err(|e| PkgError::io(dir, e))?;
    let mut paths = Vec::new();
    for entry in rd {
        paths.push(entry.map_err(|e| PkgError::io(dir, e))?.path());
    }
    paths.sort();
    Ok(paths)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The content hash of a package directory, `sha256:<hex>`.
pub fn package_checksum(root: &Path) -> PkgResult<String> {
    let files = package_files(root)?;
    let mut hasher = Sha256::new();
    for (rel, path) in &files {
        let bytes = std::fs::read(path).map_err(|e| PkgError::io(path, e))?;
        hasher.update(format!("file {} {}\n", rel, bytes.len()).as_bytes());
        hasher.update(&bytes);
        hasher.update(b"\n");
    }
    Ok(format!("sha256:{}", hex(&hasher.finalize())))
}

/// SHA-256 of a byte string as lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Whether a file is published as text (LF line endings).
pub fn is_text(rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    let ext = match base.rfind('.') {
        Some(i) => &base[i + 1..],
        None => "",
    };
    TEXT_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

/// Copy a package's files into `dest`, writing text files with LF endings.
/// Files under `tests/` are copied byte for byte: a test input may hold CR LF
/// on purpose.
/// Returns the relative paths copied.
pub fn copy_package(src: &Path, dest: &Path) -> PkgResult<Vec<String>> {
    let files = package_files(src)?;
    let mut copied = Vec::new();
    for (rel, path) in &files {
        let mut bytes = std::fs::read(path).map_err(|e| PkgError::io(path, e))?;
        if is_text(rel) && !rel.starts_with("tests/") {
            bytes = crlf_to_lf(&bytes);
        }
        let target = dest.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PkgError::io(parent, e))?;
        }
        std::fs::write(&target, &bytes).map_err(|e| PkgError::io(&target, e))?;
        copied.push(rel.clone());
    }
    Ok(copied)
}

/// Replace every CR LF pair with LF; a lone CR stays.
pub fn crlf_to_lf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            continue;
        }
        out.push(b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_pairs_become_lf_and_lone_cr_stays() {
        assert_eq!(crlf_to_lf(b"a\r\nb\rc\r\n"), b"a\nb\rc\n".to_vec());
    }

    #[test]
    fn text_detection_uses_the_extension() {
        assert!(is_text("src/lib.bld"));
        assert!(is_text("LICENSE"));
        assert!(!is_text("tests/blob.bin"));
    }
}
