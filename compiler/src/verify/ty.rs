// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The value types of the verification subset: fixed-width integers and bool.

use crate::ast::{IntSuffix, Type, TypeKind};

/// A type inside the subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ty {
    /// An integer of `bits` width.
    Int { bits: u8, signed: bool },
    /// `bool`.
    Bool,
}

impl Ty {
    pub const I64: Ty = Ty::Int {
        bits: 64,
        signed: true,
    };
    pub const I32: Ty = Ty::Int {
        bits: 32,
        signed: true,
    };

    /// The inclusive value range of an integer type.
    pub fn range(self) -> Option<(i128, i128)> {
        match self {
            Ty::Int { bits, signed: true } => {
                let half = 1i128 << (bits - 1);
                Some((-half, half - 1))
            }
            Ty::Int {
                bits,
                signed: false,
            } => Some((0, (1i128 << bits) - 1)),
            Ty::Bool => None,
        }
    }

    /// Whether `v` lies in this type's range.
    pub fn holds(self, v: i128) -> bool {
        match self.range() {
            Some((lo, hi)) => lo <= v && v <= hi,
            None => v == 0 || v == 1,
        }
    }

    /// Wrap `v` into this type's range (two's complement truncation).
    pub fn wrap(self, v: i128) -> i128 {
        match self {
            Ty::Int { bits, signed } => {
                let m = 1i128 << bits;
                let r = v.rem_euclid(m);
                if signed && r >= m / 2 {
                    r - m
                } else {
                    r
                }
            }
            Ty::Bool => (v != 0) as i128,
        }
    }

    pub fn bits(self) -> u8 {
        match self {
            Ty::Int { bits, .. } => bits,
            Ty::Bool => 1,
        }
    }

    pub fn is_int(self) -> bool {
        matches!(self, Ty::Int { .. })
    }

    /// The type a name such as `i64` or `bool` denotes.
    pub fn from_name(name: &str) -> Option<Ty> {
        let (signed, rest) = match name.as_bytes().first()? {
            b'i' => (true, &name[1..]),
            b'u' => (false, &name[1..]),
            _ => return (name == "bool").then_some(Ty::Bool),
        };
        let bits = match rest {
            "8" => 8,
            "16" => 16,
            "32" => 32,
            "64" => 64,
            "size" => 64,
            _ => return None,
        };
        Some(Ty::Int { bits, signed })
    }

    /// The subset type of an AST type, if it is in the subset.
    pub fn from_ast(ty: &Type) -> Option<Ty> {
        match &ty.kind {
            TypeKind::Path(p) if p.segments.len() == 1 && p.segments[0].generics.is_empty() => {
                Ty::from_name(&p.segments[0].ident.name)
            }
            _ => None,
        }
    }

    /// The type an integer literal suffix names.
    pub fn from_suffix(s: IntSuffix) -> Option<Ty> {
        let name = format!("{s:?}").to_lowercase();
        Ty::from_name(&name)
    }

    /// The C spelling of this type in the backend's output.
    pub fn c_name(self) -> String {
        match self {
            Ty::Bool => "bool".into(),
            Ty::Int { bits, signed } => {
                format!("{}int{}_t", if signed { "" } else { "u" }, bits)
            }
        }
    }
}

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::Bool => write!(f, "bool"),
            Ty::Int { bits, signed } => write!(f, "{}{}", if *signed { "i" } else { "u" }, bits),
        }
    }
}
