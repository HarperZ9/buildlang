// ===============================================================================
// VERSION RESOLUTION AGAINST THE LOCAL REGISTRY
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Pick one version of every package the root needs.
//!
//! Each round gathers the requirements of the root and of every package
//! picked so far, then picks for each name the newest published version that
//! meets all of its requirements. Rounds repeat until the picks stop
//! changing. One version per name: two requirements with no common version
//! fail with `BL0701` and name both sides. There is no backtracking over
//! older versions of a dependent package; a conflict that only an older
//! dependent would avoid is reported, not searched for.

use super::index::Registry;
use super::{PkgError, PkgResult};
use crate::diagnostics::codes;
use crate::pkg::{parse_version_req, Version, VersionReq};
use std::collections::BTreeMap;

/// One picked package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub version: Version,
    pub checksum: String,
    /// Dependency name to requirement, as published.
    pub dependencies: BTreeMap<String, String>,
}

/// Name to picked package, in name order.
pub type Picks = BTreeMap<String, Picked>;

const MAX_ROUNDS: usize = 64;

/// Resolve `root` requirements (name to requirement text) against `registry`.
pub fn resolve(registry: &Registry, root: &BTreeMap<String, String>) -> PkgResult<Picks> {
    let mut picks = Picks::new();
    for _ in 0..MAX_ROUNDS {
        let wanted = gather(root, &picks)?;
        let mut next = Picks::new();
        for (name, reqs) in &wanted {
            next.insert(name.clone(), pick(registry, name, reqs)?);
        }
        if next == picks {
            return Ok(picks);
        }
        picks = next;
    }
    Err(PkgError::new(
        codes::PKG_UNRESOLVABLE,
        format!("resolution did not settle after {} rounds", MAX_ROUNDS),
    ))
}

/// Requirements per name: (requirement, who asked).
type Wanted = BTreeMap<String, Vec<(VersionReq, String)>>;

fn gather(root: &BTreeMap<String, String>, picks: &Picks) -> PkgResult<Wanted> {
    let mut wanted = Wanted::new();
    for (name, req) in root {
        add(&mut wanted, name, req, "the root manifest")?;
    }
    for (parent, picked) in picks {
        let who = format!("{} {}", parent, picked.version);
        for (name, req) in &picked.dependencies {
            add(&mut wanted, name, req, &who)?;
        }
    }
    Ok(wanted)
}

fn add(wanted: &mut Wanted, name: &str, req: &str, who: &str) -> PkgResult<()> {
    let parsed = parse_version_req(req).map_err(|e| {
        PkgError::new(
            codes::PKG_UNRESOLVABLE,
            format!(
                "{} asks for {} \"{}\", which is not a version requirement: {}",
                who, name, req, e
            ),
        )
    })?;
    wanted
        .entry(name.to_string())
        .or_default()
        .push((parsed, who.to_string()));
    Ok(())
}

fn pick(registry: &Registry, name: &str, reqs: &[(VersionReq, String)]) -> PkgResult<Picked> {
    let versions = registry.versions(name);
    let asked = reqs
        .iter()
        .map(|(r, who)| format!("{} {} (from {})", name, r, who))
        .collect::<Vec<_>>()
        .join("; ");
    if versions.is_empty() {
        return Err(PkgError::new(
            codes::PKG_UNRESOLVABLE,
            format!(
                "no package named {} in the registry; asked: {}",
                name, asked
            ),
        ));
    }
    let best = versions
        .iter()
        .rev()
        .find(|v| !v.is_prerelease() && reqs.iter().all(|(r, _)| r.matches(v)))
        .or_else(|| {
            versions
                .iter()
                .rev()
                .find(|v| reqs.iter().all(|(r, _)| r.matches(v)))
        });
    let Some(version) = best else {
        let have = versions
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(PkgError::new(
            codes::PKG_UNRESOLVABLE,
            format!(
                "no published version of {} meets every requirement; asked: {}; published: {}",
                name, asked, have
            ),
        ));
    };
    let entry = registry
        .version_entry(name, version)
        .expect("a listed version has an entry");
    Ok(Picked {
        version: version.clone(),
        checksum: entry.checksum.clone(),
        dependencies: entry.dependencies.clone(),
    })
}

/// Names in build order: every package after all of its dependencies, ties
/// broken by name.
pub fn build_order(picks: &Picks) -> Vec<String> {
    let mut done: Vec<String> = Vec::new();
    let mut left: Vec<String> = picks.keys().cloned().collect();
    while !left.is_empty() {
        let ready = left.iter().position(|name| {
            picks[name]
                .dependencies
                .keys()
                .all(|d| done.contains(d) || !picks.contains_key(d))
        });
        // A cycle cannot be ordered; fall back to name order for the rest.
        let i = ready.unwrap_or(0);
        done.push(left.remove(i));
    }
    done
}
