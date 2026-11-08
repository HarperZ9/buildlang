// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Translation validation for the C backend.
//!
//! [`cert`] emits a per-function certificate for the C the backend wrote;
//! [`check`] is the independent checker that re-derives each certificate
//! from the source and the C text alone. See `docs/TRANSLATION-VALIDATION.md`.

pub mod cert;
pub mod check;
