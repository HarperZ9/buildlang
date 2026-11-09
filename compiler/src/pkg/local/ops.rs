// ===============================================================================
// PUBLISH AND LOCK
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! `publish` copies a package into the registry and records its hash.
//! `lock_project` resolves a project and writes its `Build.lock`.

use super::content::{copy_package, package_checksum};
use super::index::{PackageEntry, Registry, VersionEntry};
use super::lock::{Lock, LOCK_FILE};
use super::project::{read_project, ProjectInfo};
use super::resolve::resolve;
use super::{PkgError, PkgResult};
use crate::diagnostics::codes;
use std::collections::BTreeMap;
use std::path::Path;

/// What `publish` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub name: String,
    pub version: String,
    pub path: String,
    pub checksum: String,
    /// False when the same version with the same contents was already there.
    pub added: bool,
    pub files: Vec<String>,
}

/// Publish the package at `dir` into `registry`.
///
/// A version is immutable: publishing it again with the same contents is a
/// no-op, and with different contents fails with `BL0707`.
pub fn publish(dir: &Path, registry: &mut Registry) -> PkgResult<Published> {
    let project = read_project(dir)?;
    check_publishable(dir, &project)?;
    // Every dependency must already be published and resolvable.
    resolve(registry, &project.dependencies)?;

    let rel = format!("packages/{}/{}", project.name, project.version);
    let existing = registry
        .index
        .packages
        .get(&project.name)
        .and_then(|p| p.versions.get(&project.version))
        .cloned();
    let staging = registry
        .root
        .join(format!(".staging-{}-{}", project.name, project.version));
    let _ = std::fs::remove_dir_all(&staging);
    let files = copy_package(dir, &staging)?;
    let checksum = package_checksum(&staging)?;

    if let Some(prev) = existing {
        let _ = std::fs::remove_dir_all(&staging);
        if prev.checksum == checksum {
            return Ok(Published {
                name: project.name,
                version: project.version,
                path: prev.path,
                checksum,
                added: false,
                files,
            });
        }
        return Err(PkgError::new(
            codes::PKG_PUBLISHED,
            format!(
                "{} {} is already published as {} and these files hash to {}; publish a new version",
                project.name, project.version, prev.checksum, checksum
            ),
        ));
    }

    let dest = registry
        .root
        .join("packages")
        .join(&project.name)
        .join(&project.version);
    if dest.exists() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(PkgError::new(
            codes::PKG_PUBLISHED,
            format!(
                "{} exists but the index does not list it; remove it or restore the index",
                rel
            ),
        ));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PkgError::io(parent, e))?;
    }
    std::fs::rename(&staging, &dest).map_err(|e| PkgError::io(&dest, e))?;

    let entry = registry
        .index
        .packages
        .entry(project.name.clone())
        .or_insert_with(|| PackageEntry {
            version: String::new(),
            description: String::new(),
            author: String::new(),
            license: String::new(),
            checksum: String::new(),
            path: String::new(),
            versions: BTreeMap::new(),
        });
    entry.description = project.description.clone();
    entry.author = project.author.clone();
    entry.license = project.license.clone();
    entry.versions.insert(
        project.version.clone(),
        VersionEntry {
            path: rel.clone(),
            checksum: checksum.clone(),
            dependencies: project.dependencies.clone(),
        },
    );
    registry.save()?;
    Ok(Published {
        name: project.name,
        version: project.version,
        path: rel,
        checksum,
        added: true,
        files,
    })
}

fn check_publishable(dir: &Path, project: &ProjectInfo) -> PkgResult<()> {
    let manifest = dir.join("Build.toml");
    if project.license.is_empty() {
        return Err(PkgError::new(
            codes::PKG_MANIFEST,
            format!(
                "{}: a published package needs `license` (an SPDX expression)",
                manifest.display()
            ),
        ));
    }
    let src = dir.join("src");
    if !src.join("lib.bld").is_file() && !src.join("main.bld").is_file() {
        return Err(PkgError::new(
            codes::PKG_MANIFEST,
            format!("{} has neither src/lib.bld nor src/main.bld", dir.display()),
        ));
    }
    Ok(())
}

/// Resolve the project at `dir` and write its `Build.lock`. Returns the lock
/// and whether the file changed.
pub fn lock_project(dir: &Path, registry: &Registry) -> PkgResult<(Lock, bool)> {
    let project = read_project(dir)?;
    let picks = resolve(registry, &project.dependencies)?;
    let lock = Lock::new(
        &project.name,
        &project.version,
        &project.dependencies,
        &picks,
    );
    let text = lock.to_text();
    let path = dir.join(LOCK_FILE);
    let old = std::fs::read_to_string(&path).ok();
    let changed = old.as_deref() != Some(text.as_str());
    if changed {
        std::fs::write(&path, &text).map_err(|e| PkgError::io(&path, e))?;
    }
    Ok((lock, changed))
}
