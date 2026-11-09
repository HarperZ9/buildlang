// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The value types of the verification subset: fixed-width integers and bool.
//! Translation validation also admits `f32` and `f64`, as opaque IEEE values.

use crate::ast::{IntSuffix, Type, TypeKind};

/// A type inside the subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ty {
    /// An integer of `bits` width.
    Int { bits: u8, signed: bool },
    /// `bool`.
    Bool,
    /// An IEEE 754 float of `bits` width (translation validation only).
    Float { bits: u8 },
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
            Ty::Bool | Ty::Float { .. } => None,
        }
    }

    /// Whether `v` lies in this type's range.
    pub fn holds(self, v: i128) -> bool {
        match self.range() {
            Some((lo, hi)) => lo <= v && v <= hi,
            None if self == Ty::Bool => v == 0 || v == 1,
            None => false,
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
            Ty::Float { .. } => v,
        }
    }

    pub fn bits(self) -> u8 {
        match self {
            Ty::Int { bits, .. } | Ty::Float { bits } => bits,
            Ty::Bool => 1,
        }
    }

    pub fn is_int(self) -> bool {
        matches!(self, Ty::Int { .. })
    }

    pub fn is_float(self) -> bool {
        matches!(self, Ty::Float { .. })
    }

    pub const F64: Ty = Ty::Float { bits: 64 };
    pub const F32: Ty = Ty::Float { bits: 32 };

    /// `from_name`, plus `f32` and `f64` (translation validation).
    pub fn from_name_tv(name: &str) -> Option<Ty> {
        match name {
            "f64" => Some(Ty::F64),
            "f32" => Some(Ty::F32),
            _ => Ty::from_name(name),
        }
    }

    /// `from_ast`, plus `f32` and `f64` (translation validation).
    pub fn from_ast_tv(ty: &Type) -> Option<Ty> {
        match &ty.kind {
            TypeKind::Path(p) if p.segments.len() == 1 && p.segments[0].generics.is_empty() => {
                Ty::from_name_tv(&p.segments[0].ident.name)
            }
            _ => None,
        }
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
            Ty::Float { bits: 32 } => "float".into(),
            Ty::Float { .. } => "double".into(),
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
            Ty::Float { bits } => write!(f, "f{bits}"),
            Ty::Int { bits, signed } => write!(f, "{}{}", if *signed { "i" } else { "u" }, bits),
        }
    }
}
