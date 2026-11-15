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
use super::index::{find_registry, Registry};
use super::linemap::{shown_path, LineMap};
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
    /// The file and line each line of `source` was copied from.
    pub line_map: LineMap,
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
    join(dir, project, lock, lock_text, Some(registry), true)
}

/// The joined text of the package at `dir` for `buildc check <dir>` and
/// `buildc lint <dir>`. Differences from [`build_input`]: `src/main.bld` is
/// optional (a library package is checked too), and a package with no
/// dependencies needs neither a `Build.lock` nor a registry. A package with
/// dependencies is held to the same lock and checksum rules as a build;
/// `explicit_registry` overrides the registry search.
pub fn check_input(dir: &Path, explicit_registry: Option<&Path>) -> PkgResult<BuildInput> {
    let project = read_project(dir)?;
    let has_lock = dir.join(LOCK_FILE).is_file();
    let (lock, lock_text) = if project.dependencies.is_empty() && !has_lock {
        let lock = Lock::new(
            &project.name,
            &project.version,
            &project.dependencies,
            &Picks::new(),
        );
        (lock, String::new())
    } else {
        read_lock(dir, &project)?
    };
    if lock.packages.is_empty() {
        return join(dir, project, lock, lock_text, None, false);
    }
    let registry = Registry::load(&find_registry(dir, explicit_registry)?)?;
    verify_locked(&registry, &lock)?;
    join(dir, project, lock, lock_text, Some(&registry), false)
}

fn join(
    dir: &Path,
    project: ProjectInfo,
    lock: Lock,
    lock_text: String,
    registry: Option<&Registry>,
    need_main: bool,
) -> PkgResult<BuildInput> {
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

    let mut out = Joined::default();
    for name in &order {
        let locked = &lock.packages[name];
        let entry = registry
            .and_then(|r| r.version_entry(name, &locked.version))
            .expect("verified above");
        let pkg_dir = registry.expect("verified above").version_dir(entry);
        let header = format!("package {} {} ({})", name, locked.version, locked.checksum);
        append_library(&mut out, &pkg_dir, &header, &names, false)?;
    }
    let header = format!("package {} {} (the project)", project.name, project.version);
    let library = append_library(&mut out, dir, &header, &names, true)?;
    let main = dir.join("src").join("main.bld");
    if main.is_file() {
        append_file(&mut out, &main, dir, "src/main.bld", &names, true)?;
    } else if need_main {
        return Err(PkgError::new(
            codes::PKG_MANIFEST,
            format!("{} has no src/main.bld to build", dir.display()),
        ));
    } else if library == 0 {
        return Err(PkgError::new(
            codes::PKG_MANIFEST,
            format!("{} has no .bld file in src/ to check", dir.display()),
        ));
    }
    Ok(BuildInput {
        project,
        lock,
        lock_text,
        source: out.source,
        line_map: out.map,
        order,
    })
}

/// The joined text and the origin of each of its lines, kept in step.
#[derive(Default)]
struct Joined {
    source: String,
    map: LineMap,
}

impl Joined {
    /// Write a line of the join's own.
    fn written(&mut self, line: &str) {
        self.source.push_str(line);
        self.source.push('\n');
        self.map.synthetic();
    }
}

/// Join the library files of the package at `dir`; return how many there were.
fn append_library(
    out: &mut Joined,
    dir: &Path,
    header: &str,
    names: &BTreeSet<String>,
    own: bool,
) -> PkgResult<usize> {
    out.written(&format!("// ==== {} ====", header));
    let files = library_files(dir)?;
    for file in &files {
        let rel = format!(
            "src/{}",
            file.file_name().unwrap_or_default().to_string_lossy()
        );
        append_file(out, file, dir, &rel, names, own)?;
    }
    Ok(files.len())
}

fn append_file(
    out: &mut Joined,
    file: &Path,
    dir: &Path,
    rel: &str,
    names: &BTreeSet<String>,
    own: bool,
) -> PkgResult<()> {
    let text = std::fs::read_to_string(file).map_err(|e| PkgError::io(file, e))?;
    out.written(&format!("// ---- {} ----", rel));
    out.map.begin_file(shown_path(dir, rel), own);
    let mut byte_start = 0;
    for (index, raw) in text.split_inclusive('\n').enumerate() {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !imports_known_package(line, names) {
            out.source.push_str(line);
            out.source.push('\n');
            out.map.copied(index + 1, byte_start);
        }
        byte_start += raw.len();
    }
    out.written("");
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
