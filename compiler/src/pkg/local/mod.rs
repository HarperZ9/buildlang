// ===============================================================================
// BUILDLANG PACKAGE MANAGER - LOCAL REGISTRY, LOCKFILE AND OFFLINE BUILD
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! The working package flow: publish into an in-repo registry, resolve
//! versions, write `Build.lock` with content hashes, and build offline from it.
//!
//! The registry is a directory: `index.json` plus one folder per published
//! version. Nothing here opens a network connection. A package's identity is
//! its name, its version and the SHA-256 of its files (see [`content`]); a
//! build re-hashes every locked package and refuses one whose files changed.
//!
//! Every failure carries a stable `BL07xx` code (see `diagnostics::codes`).

pub mod content;
pub mod index;
pub mod lock;
pub mod ops;
pub mod project;
pub mod resolve;
pub mod source;

use std::fmt;

/// A package-manager failure with its stable diagnostic code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgError {
    /// Stable code, one of the `BL07xx` constants in `diagnostics::codes`.
    pub code: &'static str,
    /// What went wrong, in one or more lines.
    pub message: String,
}

impl PkgError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        PkgError {
            code,
            message: message.into(),
        }
    }

    /// An I/O failure on a named path.
    pub fn io(path: &std::path::Path, err: impl fmt::Display) -> Self {
        PkgError::new(
            crate::diagnostics::codes::PKG_IO,
            format!("{}: {}", path.display(), err),
        )
    }
}

impl fmt::Display for PkgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error[{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for PkgError {}

pub type PkgResult<T> = Result<T, PkgError>;
