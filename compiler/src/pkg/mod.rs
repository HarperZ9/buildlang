// ===============================================================================
// BUILDLANG PACKAGE MANAGER
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Package manager for BuildLang (build-pkg).
//!
//! This module provides package management functionality including:
//! - Package manifest parsing (Build.toml)
//! - Dependency resolution with semver
//! - Registry interaction
//! - Package building and publishing
//!
//! ## Manifest Format
//!
//! ```toml
//! [package]
//! name = "my-package"
//! version = "1.0.0"
//! authors = ["Author Name <email@example.com>"]
//! edition = "2025"
//!
//! [dependencies]
//! serde = "1.0"
//! tokio = { version = "1.0", features = ["full"] }
//!
//! [dev-dependencies]
//! criterion = "0.4"
//! ```

pub mod lockfile;
pub mod manifest;
pub mod registry;
pub mod resolver;
pub mod version;

pub use lockfile::*;
pub use manifest::*;
pub use registry::*;
pub use resolver::*;
pub use version::*;
