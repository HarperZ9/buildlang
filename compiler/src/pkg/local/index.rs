// ===============================================================================
// THE IN-REPO REGISTRY INDEX (registry/index.json)
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! `registry/index.json`, format 2.
//!
//! Each package keeps its published versions under `versions`, each with the
//! folder that holds it (relative to the registry root), its content hash and
//! its dependency requirements. The top-level `version`, `path` and
//! `checksum` fields repeat the newest version so readers of the first
//! format keep working.

use super::{PkgError, PkgResult};
use crate::diagnostics::codes;
use crate::pkg::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One published version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionEntry {
    /// Folder of this version, relative to the registry root, with `/`.
    pub path: String,
    /// Content hash, `sha256:<hex>`.
    pub checksum: String,
    /// Dependency name to version requirement.
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

/// One package and all its published versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageEntry {
    /// Newest version (first-format field).
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    /// Content hash of the newest version (first-format field).
    #[serde(default)]
    pub checksum: String,
    /// Folder of the newest version (first-format field).
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub versions: BTreeMap<String, VersionEntry>,
}

/// The whole index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryIndex {
    #[serde(default)]
    pub format: u32,
    pub packages: BTreeMap<String, PackageEntry>,
}

/// A registry on disk: its root folder and its parsed index.
#[derive(Debug, Clone)]
pub struct Registry {
    pub root: PathBuf,
    pub index: RegistryIndex,
}

impl Registry {
    /// Load `<root>/index.json`. A first-format entry without `versions` is
    /// read as a single version at its `path`.
    pub fn load(root: &Path) -> PkgResult<Registry> {
        let path = root.join("index.json");
        let text = std::fs::read_to_string(&path).map_err(|e| PkgError::io(&path, e))?;
        let mut index: RegistryIndex = serde_json::from_str(&text).map_err(|e| {
            PkgError::new(
                codes::PKG_REGISTRY,
                format!("{} is not a valid registry index: {}", path.display(), e),
            )
        })?;
        for entry in index.packages.values_mut() {
            if entry.versions.is_empty() && !entry.version.is_empty() {
                entry.versions.insert(
                    entry.version.clone(),
                    VersionEntry {
                        path: entry.path.clone(),
                        checksum: entry.checksum.clone(),
                        dependencies: BTreeMap::new(),
                    },
                );
            }
        }
        Ok(Registry {
            root: root.to_path_buf(),
            index,
        })
    }

    /// Write the index back as format 2, pretty-printed with a final newline.
    pub fn save(&mut self) -> PkgResult<()> {
        self.index.format = 2;
        for entry in self.index.packages.values_mut() {
            refresh_latest(entry);
        }
        let mut text = serde_json::to_string_pretty(&self.index)
            .map_err(|e| PkgError::new(codes::PKG_REGISTRY, e.to_string()))?;
        text.push('\n');
        let path = self.root.join("index.json");
        std::fs::write(&path, text).map_err(|e| PkgError::io(&path, e))
    }

    /// Published versions of a package, oldest first. Unparseable version
    /// keys are skipped.
    pub fn versions(&self, name: &str) -> Vec<Version> {
        let mut out: Vec<Version> = match self.index.packages.get(name) {
            Some(entry) => entry
                .versions
                .keys()
                .filter_map(|v| Version::parse(v).ok())
                .collect(),
            None => Vec::new(),
        };
        out.sort();
        out
    }

    /// The index entry of one version.
    pub fn version_entry(&self, name: &str, version: &Version) -> Option<&VersionEntry> {
        let entry = self.index.packages.get(name)?;
        entry
            .versions
            .iter()
            .find(|(k, _)| Version::parse(k).ok().as_ref() == Some(version))
            .map(|(_, v)| v)
    }

    /// Absolute folder of one version.
    pub fn version_dir(&self, entry: &VersionEntry) -> PathBuf {
        let mut dir = self.root.clone();
        for part in entry.path.split('/').filter(|p| !p.is_empty()) {
            dir.push(part);
        }
        dir
    }
}

/// Set the first-format fields to the newest version.
fn refresh_latest(entry: &mut PackageEntry) {
    let newest = entry
        .versions
        .iter()
        .filter_map(|(k, v)| {
            Version::parse(k)
                .ok()
                .map(|ver| (ver, k.clone(), v.clone()))
        })
        .max_by(|a, b| a.0.cmp(&b.0));
    if let Some((_, key, v)) = newest {
        entry.version = key;
        entry.path = v.path;
        entry.checksum = v.checksum;
    }
}

/// Find the registry root: an explicit folder, then `BUILDLANG_REGISTRY`,
/// then the nearest `registry/index.json` above `start`.
pub fn find_registry(start: &Path, explicit: Option<&Path>) -> PkgResult<PathBuf> {
    if let Some(dir) = explicit {
        return check_root(dir.to_path_buf());
    }
    if let Ok(dir) = std::env::var("BUILDLANG_REGISTRY") {
        if !dir.is_empty() {
            return check_root(PathBuf::from(dir));
        }
    }
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let mut dir = Some(start.as_path());
    while let Some(d) = dir {
        let candidate = d.join("registry");
        if candidate.join("index.json").is_file() {
            return Ok(candidate);
        }
        if d.join("index.json").is_file() && d.join("packages").is_dir() {
            return Ok(d.to_path_buf());
        }
        dir = d.parent();
    }
    Err(PkgError::new(
        codes::PKG_REGISTRY,
        format!(
            "no registry found above {}; pass --registry DIR or set BUILDLANG_REGISTRY",
            start.display()
        ),
    ))
}

fn check_root(dir: PathBuf) -> PkgResult<PathBuf> {
    if dir.join("index.json").is_file() {
        Ok(dir)
    } else {
        Err(PkgError::new(
            codes::PKG_REGISTRY,
            format!("{} has no index.json", dir.display()),
        ))
    }
}
