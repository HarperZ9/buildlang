// ===============================================================================
// Build.lock: THE RESOLVED SET WITH CONTENT HASHES
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! `Build.lock`, lock format 1.
//!
//! ```toml
//! lock-version = 1
//!
//! [root]
//! name = "demo"
//! version = "0.1.0"
//! requires = ["merkle ^1.0"]
//!
//! [[package]]
//! name = "merkle"
//! version = "1.0.0"
//! checksum = "sha256:..."
//! dependencies = ["sha256 1.0.0"]
//! ```
//!
//! `requires` repeats the root manifest's requirements, so a build can tell
//! when the manifest changed after the lock was written. Packages are in name
//! order and the text has LF endings, so the same resolution always writes
//! the same bytes.

use super::resolve::{Picked, Picks};
use super::{PkgError, PkgResult};
use crate::diagnostics::codes;
use crate::pkg::Version;
use std::collections::BTreeMap;

pub const LOCK_FILE: &str = "Build.lock";

/// A parsed lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    pub root_name: String,
    pub root_version: String,
    /// Root requirement name to requirement text.
    pub requires: BTreeMap<String, String>,
    pub packages: BTreeMap<String, LockedPkg>,
}

/// One locked package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedPkg {
    pub version: Version,
    pub checksum: String,
    /// Dependency name to the exact locked version.
    pub dependencies: BTreeMap<String, Version>,
}

impl Lock {
    /// Build the lock for a resolution.
    pub fn new(
        root_name: &str,
        root_version: &str,
        requires: &BTreeMap<String, String>,
        picks: &Picks,
    ) -> Lock {
        let packages = picks
            .iter()
            .map(|(name, p)| (name.clone(), locked(p, picks)))
            .collect();
        Lock {
            root_name: root_name.to_string(),
            root_version: root_version.to_string(),
            requires: requires.clone(),
            packages,
        }
    }

    /// The lock as text.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("# Build.lock: written by `buildc pkg resolve`. Do not edit by hand.\n");
        out.push_str("lock-version = 1\n\n[root]\n");
        out.push_str(&format!("name = {}\n", quote(&self.root_name)));
        out.push_str(&format!("version = {}\n", quote(&self.root_version)));
        let reqs = self.requires.iter().map(|(n, r)| format!("{} {}", n, r));
        out.push_str(&format!("requires = {}\n", list(reqs)));
        for (name, p) in &self.packages {
            out.push_str("\n[[package]]\n");
            out.push_str(&format!("name = {}\n", quote(name)));
            out.push_str(&format!("version = {}\n", quote(&p.version.to_string())));
            out.push_str(&format!("checksum = {}\n", quote(&p.checksum)));
            let deps = p.dependencies.iter().map(|(n, v)| format!("{} {}", n, v));
            out.push_str(&format!("dependencies = {}\n", list(deps)));
        }
        out
    }

    /// Parse lock text.
    pub fn parse(text: &str) -> PkgResult<Lock> {
        let mut lock = Lock {
            root_name: String::new(),
            root_version: String::new(),
            requires: BTreeMap::new(),
            packages: BTreeMap::new(),
        };
        let mut section = "";
        let mut current: Option<(String, BTreeMap<String, String>)> = None;
        let mut saw_version = false;
        for (n, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[root]" || line == "[[package]]" {
                flush(&mut current, &mut lock)?;
                section = if line == "[root]" { "root" } else { "package" };
                if section == "package" {
                    current = Some((String::new(), BTreeMap::new()));
                }
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| bad(n, "expected `key = value`"))?;
            let (key, value) = (key.trim(), value.trim());
            match (section, key) {
                ("", "lock-version") => {
                    if value != "1" {
                        return Err(bad(n, "unknown lock-version"));
                    }
                    saw_version = true;
                }
                ("root", "name") => lock.root_name = unquote(value, n)?,
                ("root", "version") => lock.root_version = unquote(value, n)?,
                ("root", "requires") => {
                    for item in unlist(value, n)? {
                        let (name, req) = split_pair(&item, n)?;
                        lock.requires.insert(name, req);
                    }
                }
                ("package", _) => {
                    let fields = &mut current.as_mut().expect("in a package").1;
                    fields.insert(key.to_string(), value.to_string());
                }
                _ => return Err(bad(n, &format!("unexpected key `{}`", key))),
            }
        }
        flush(&mut current, &mut lock)?;
        if !saw_version {
            return Err(bad(0, "missing `lock-version = 1`"));
        }
        Ok(lock)
    }
}

fn locked(p: &Picked, picks: &Picks) -> LockedPkg {
    let dependencies = p
        .dependencies
        .keys()
        .filter_map(|d| picks.get(d).map(|q| (d.clone(), q.version.clone())))
        .collect();
    LockedPkg {
        version: p.version.clone(),
        checksum: p.checksum.clone(),
        dependencies,
    }
}

fn flush(
    current: &mut Option<(String, BTreeMap<String, String>)>,
    lock: &mut Lock,
) -> PkgResult<()> {
    let Some((_, fields)) = current.take() else {
        return Ok(());
    };
    let get = |k: &str| -> PkgResult<String> {
        let v = fields
            .get(k)
            .ok_or_else(|| bad(0, &format!("a [[package]] has no `{}`", k)))?;
        unquote(v, 0)
    };
    let name = get("name")?;
    let version = parse_ver(&get("version")?)?;
    let checksum = get("checksum")?;
    let mut dependencies = BTreeMap::new();
    if let Some(v) = fields.get("dependencies") {
        for item in unlist(v, 0)? {
            let (dep, ver) = split_pair(&item, 0)?;
            dependencies.insert(dep, parse_ver(&ver)?);
        }
    }
    lock.packages.insert(
        name,
        LockedPkg {
            version,
            checksum,
            dependencies,
        },
    );
    Ok(())
}

fn parse_ver(s: &str) -> PkgResult<Version> {
    Version::parse(s).map_err(|e| bad(0, &format!("bad version `{}`: {}", s, e)))
}

fn bad(line: usize, what: &str) -> PkgError {
    let at = if line == 0 {
        String::new()
    } else {
        format!(" (line {})", line + 1)
    };
    PkgError::new(
        codes::PKG_LOCK,
        format!("{} is malformed{}: {}", LOCK_FILE, at, what),
    )
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn list(items: impl Iterator<Item = String>) -> String {
    let parts: Vec<String> = items.map(|s| quote(&s)).collect();
    format!("[{}]", parts.join(", "))
}

fn unquote(v: &str, line: usize) -> PkgResult<String> {
    let inner = v
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| bad(line, "expected a quoted string"))?;
    Ok(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn unlist(v: &str, line: usize) -> PkgResult<Vec<String>> {
    let inner = v
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| bad(line, "expected a list"))?
        .trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner.split(',').map(|s| unquote(s.trim(), line)).collect()
}

fn split_pair(item: &str, line: usize) -> PkgResult<(String, String)> {
    let (a, b) = item
        .split_once(' ')
        .ok_or_else(|| bad(line, &format!("expected `name version`, got `{}`", item)))?;
    Ok((a.to_string(), b.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_text_round_trips() {
        let mut picks = Picks::new();
        let mut deps = BTreeMap::new();
        deps.insert("b".to_string(), "^1.0".to_string());
        picks.insert(
            "a".into(),
            Picked {
                version: Version::new(1, 2, 0),
                checksum: "sha256:aa".into(),
                dependencies: deps,
            },
        );
        picks.insert(
            "b".into(),
            Picked {
                version: Version::new(1, 0, 3),
                checksum: "sha256:bb".into(),
                dependencies: BTreeMap::new(),
            },
        );
        let mut req = BTreeMap::new();
        req.insert("a".to_string(), "^1.0".to_string());
        let lock = Lock::new("demo", "0.1.0", &req, &picks);
        let text = lock.to_text();
        assert_eq!(Lock::parse(&text).unwrap(), lock);
        assert!(text.contains("dependencies = [\"b 1.0.3\"]"));
    }

    #[test]
    fn missing_lock_version_is_rejected_with_its_code() {
        let err = Lock::parse("[root]\nname = \"x\"\n").unwrap_err();
        assert_eq!(err.code, codes::PKG_LOCK);
    }
}
