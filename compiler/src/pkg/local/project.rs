// ===============================================================================
// PACKAGE MANIFEST FIELDS THE PACKAGE FLOW NEEDS
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Reads a package's `Build.toml` into the fields that publish, resolve and
//! build use, and checks them.

use super::{PkgError, PkgResult};
use crate::diagnostics::codes;
use crate::pkg::Manifest;
use std::collections::BTreeMap;
use std::path::Path;

/// The manifest fields the package flow uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    /// Registry dependency name to its requirement, written in normal form.
    pub dependencies: BTreeMap<String, String>,
}

/// Read and check `<dir>/Build.toml`.
pub fn read_project(dir: &Path) -> PkgResult<ProjectInfo> {
    let path = dir.join("Build.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| PkgError::io(&path, e))?;
    let manifest = Manifest::from_str(&text)
        .map_err(|e| PkgError::new(codes::PKG_MANIFEST, format!("{}: {}", path.display(), e)))?;
    let pkg = &manifest.package;
    check_name(&pkg.name, &path)?;
    let mut dependencies = BTreeMap::new();
    for (name, dep) in &manifest.dependencies {
        check_name(name, &path)?;
        if dep.path.is_some() || dep.git.is_some() {
            return Err(PkgError::new(
                codes::PKG_MANIFEST,
                format!(
                    "{}: dependency {} uses a path or git source; only registry versions are supported",
                    path.display(),
                    name
                ),
            ));
        }
        let req = dep
            .version
            .as_ref()
            .map(|r| r.to_string())
            .unwrap_or_else(|| "*".to_string());
        dependencies.insert(name.clone(), req);
    }
    let author = if pkg.authors.is_empty() {
        single_field(&text, "author").unwrap_or_default()
    } else {
        pkg.authors.join(", ")
    };
    Ok(ProjectInfo {
        name: pkg.name.clone(),
        version: pkg.version.to_string(),
        description: pkg.description.clone().unwrap_or_default(),
        author,
        license: pkg.license.clone().unwrap_or_default(),
        dependencies,
    })
}

/// A package name: lowercase ASCII letters, digits and `-`, starting with a
/// letter.
fn check_name(name: &str, path: &Path) -> PkgResult<()> {
    let ok = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(PkgError::new(
            codes::PKG_MANIFEST,
            format!(
                "{}: `{}` is not a package name (lowercase letters, digits and `-`, starting with a letter)",
                path.display(),
                name
            ),
        ))
    }
}

/// Value of a `key = "value"` line in the `[package]` table.
fn single_field(text: &str, key: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// The library sources of a package: every `.bld` file directly in `src/`
/// except `main.bld`, in name order.
pub fn library_files(dir: &Path) -> PkgResult<Vec<std::path::PathBuf>> {
    let src = dir.join("src");
    if !src.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&src).map_err(|e| PkgError::io(&src, e))? {
        let path = entry.map_err(|e| PkgError::io(&src, e))?.path();
        let is_bld = path.extension().is_some_and(|e| e == "bld");
        let is_main = path.file_name().is_some_and(|n| n == "main.bld");
        if path.is_file() && is_bld && !is_main {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}
