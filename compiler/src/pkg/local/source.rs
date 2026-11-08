// ===============================================================================
// OFFLINE BUILD INPUT: VERIFY THE LOCK, THEN JOIN THE SOURCES
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Turns a project, its `Build.lock` and a registry into one BuildLang source.
//!
//! The lock must exist and match the manifest's requirements (`BL0703`).
//! Every locked package must be in the registry at its locked version
//! (`BL0704`) and its files must hash to the locked checksum (`BL0702`).
//! Then each package's library files are joined in build order, followed by
//! the project's own library files and `src/main.bld`. Lines that import a
//! locked package by name (`use name;` or `// import name`) are dropped,
//! because the joined text already holds that package. The joined text holds
//! no absolute paths, so two clones build the same bytes.

use super::content::package_checksum;
use super::index::Registry;
use super::lock::{Lock, LOCK_FILE};
use super::project::{library_files, read_project, ProjectInfo};
use super::resolve::build_order;
use super::resolve::{Picked, Picks};
use super::{PkgError, PkgResult};
use crate::diagnostics::codes;
use std::collections::BTreeSet;
use std::path::Path;

/// A verified, joined build input.
#[derive(Debug, Clone)]
pub struct BuildInput {
    pub project: ProjectInfo,
    pub lock: Lock,
    /// The lock file's bytes, for the receipt.
    pub lock_text: String,
    /// The joined BuildLang source.
    pub source: String,
    /// Locked packages in the order they were joined.
    pub order: Vec<String>,
}

/// Read the lock of `dir` and check it against the manifest.
pub fn read_lock(dir: &Path, project: &ProjectInfo) -> PkgResult<(Lock, String)> {
    let path = dir.join(LOCK_FILE);
    if !path.is_file() {
        return Err(PkgError::new(
            codes::PKG_LOCK,
            format!(
                "{} has no {}; run `buildc pkg resolve` first",
                dir.display(),
                LOCK_FILE
            ),
        ));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| PkgError::io(&path, e))?;
    let lock = Lock::parse(&text)?;
    if lock.root_name != project.name || lock.requires != project.dependencies {
        return Err(PkgError::new(
            codes::PKG_LOCK,
            format!(
                "{} is out of date: Build.toml changed since it was written; run `buildc pkg resolve`",
                LOCK_FILE
            ),
        ));
    }
    Ok((lock, text))
}

/// Check every locked package against the registry: present at its locked
/// version, the index agrees on the checksum, and the files hash to it.
pub fn verify_locked(registry: &Registry, lock: &Lock) -> PkgResult<()> {
    for (name, locked) in &lock.packages {
        let entry = registry
            .version_entry(name, &locked.version)
            .ok_or_else(|| {
                PkgError::new(
                    codes::PKG_MISSING,
                    format!(
                        "{} {} is locked but not published in {}",
                        name,
                        locked.version,
                        registry.root.display()
                    ),
                )
            })?;
        let dir = registry.version_dir(entry);
        if !dir.is_dir() {
            return Err(PkgError::new(
                codes::PKG_MISSING,
                format!(
                    "{} {}: folder {} is missing",
                    name, locked.version, entry.path
                ),
            ));
        }
        let actual = package_checksum(&dir)?;
        if actual != locked.checksum || entry.checksum != locked.checksum {
            return Err(PkgError::new(
                codes::PKG_CHECKSUM,
                format!(
                    "{} {} does not match its lock: locked {}, index {}, files {}",
                    name, locked.version, locked.checksum, entry.checksum, actual
                ),
            ));
        }
    }
    Ok(())
}

/// Verify the lock of the project at `dir` and join its sources.
pub fn build_input(dir: &Path, registry: &Registry) -> PkgResult<BuildInput> {
    let project = read_project(dir)?;
    let (lock, lock_text) = read_lock(dir, &project)?;
    verify_locked(registry, &lock)?;

    let picks: Picks = lock
        .packages
        .iter()
        .map(|(name, l)| {
            let deps = l
                .dependencies
                .iter()
                .map(|(d, v)| (d.clone(), v.to_string()))
                .collect();
            let picked = Picked {
                version: l.version.clone(),
                checksum: l.checksum.clone(),
                dependencies: deps,
            };
            (name.clone(), picked)
        })
        .collect();
    let order = build_order(&picks);
    let mut names: BTreeSet<String> = picks.keys().cloned().collect();
    names.insert(project.name.clone());

    let mut source = String::new();
    for name in &order {
        let locked = &lock.packages[name];
        let entry = registry
            .version_entry(name, &locked.version)
            .expect("verified above");
        let header = format!("package {} {} ({})", name, locked.version, locked.checksum);
        append_library(&mut source, &registry.version_dir(entry), &header, &names)?;
    }
    let header = format!("package {} {} (the project)", project.name, project.version);
    append_library(&mut source, dir, &header, &names)?;
    let main = dir.join("src").join("main.bld");
    if !main.is_file() {
        return Err(PkgError::new(
            codes::PKG_MANIFEST,
            format!("{} has no src/main.bld to build", dir.display()),
        ));
    }
    append_file(&mut source, &main, "src/main.bld", &names)?;
    Ok(BuildInput {
        project,
        lock,
        lock_text,
        source,
        order,
    })
}

fn append_library(
    out: &mut String,
    dir: &Path,
    header: &str,
    names: &BTreeSet<String>,
) -> PkgResult<()> {
    out.push_str(&format!("// ==== {} ====\n", header));
    for file in library_files(dir)? {
        let rel = format!(
            "src/{}",
            file.file_name().unwrap_or_default().to_string_lossy()
        );
        append_file(out, &file, &rel, names)?;
    }
    Ok(())
}

fn append_file(
    out: &mut String,
    file: &Path,
    rel: &str,
    names: &BTreeSet<String>,
) -> PkgResult<()> {
    let text = std::fs::read_to_string(file).map_err(|e| PkgError::io(file, e))?;
    out.push_str(&format!("// ---- {} ----\n", rel));
    for line in text.lines() {
        if imports_known_package(line, names) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
    Ok(())
}

/// Whether a line is `use name;` or `// import name` for a locked package
/// (or the project itself). `_` and `-` are the same in package names here.
fn imports_known_package(line: &str, names: &BTreeSet<String>) -> bool {
    let t = line.trim();
    let name = if let Some(rest) = t.strip_prefix("// import ") {
        rest.trim()
    } else if let Some(rest) = t.strip_prefix("use ").and_then(|r| r.strip_suffix(';')) {
        rest.trim()
    } else {
        return false;
    };
    names.contains(&name.replace('_', "-"))
}
