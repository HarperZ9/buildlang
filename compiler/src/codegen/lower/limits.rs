//! Integer limit constants (`i8::MAX`, `u64::MAX`, `usize::MAX`, ...).
//!
//! Lowering emits the C limit macros (`INT8_MAX`, `UINT64_MAX`, `SIZE_MAX`)
//! as globals, which keeps `usize` and `isize` correct on every target width.
//! A global has no module entry, so `type_of_value` asks `c_limit_type` for
//! its MIR type; without it a temporary such as `u64::MAX - 1` defaulted to
//! `int32_t` and truncated (#215).

use crate::codegen::ir::{IntSize, MirType};

/// The type of `-literal` when the unsuffixed literal is one above a signed
/// maximum, so the negation is exactly a signed minimum (G32). The literal
/// alone does not fit the hinted type and was widened: `-128` under an `i8`
/// hint was an `i32` constant, and `-9223372036854775808` under an `i64`
/// hint an `i128` constant. `hint` is the expected type; without a signed
/// hint the 32-bit minimum is an `i32`, the default integer type.
pub(super) fn negated_min_type(negated: i128, hint: Option<&MirType>) -> Option<MirType> {
    let min_of = |size: &IntSize| match size {
        IntSize::I8 => i128::from(i8::MIN),
        IntSize::I16 => i128::from(i16::MIN),
        IntSize::I32 => i128::from(i32::MIN),
        IntSize::I64 | IntSize::ISize => i128::from(i64::MIN),
        IntSize::I128 => i128::MIN,
    };
    match hint {
        Some(MirType::Int(size, true)) if negated == min_of(size) => {
            Some(MirType::Int(*size, true))
        }
        Some(MirType::Int(..)) => None,
        _ if negated == i128::from(i32::MIN) => Some(MirType::i32()),
        _ => None,
    }
}

/// The integer type named by a BuildLang primitive (`"i8"`, `"usize"`, ...).
pub(super) fn int_type_by_name(name: &str) -> MirType {
    match name {
        "i8" => MirType::i8(),
        "i16" => MirType::i16(),
        "i32" => MirType::i32(),
        "i64" => MirType::i64(),
        "isize" => MirType::isize(),
        "u8" => MirType::u8(),
        "u16" => MirType::u16(),
        "u32" => MirType::u32(),
        "u64" => MirType::u64(),
        "usize" => MirType::usize(),
        _ => MirType::i32(),
    }
}

/// The C macro for `ty::bound`, for the signed minimums and every maximum.
pub(super) fn c_limit_macro(ty: &str, bound: &str) -> Option<&'static str> {
    Some(match (ty, bound) {
        ("i8", "MIN") => "INT8_MIN",
        ("i8", "MAX") => "INT8_MAX",
        ("i16", "MIN") => "INT16_MIN",
        ("i16", "MAX") => "INT16_MAX",
        ("i32", "MIN") => "INT32_MIN",
        ("i32", "MAX") => "INT32_MAX",
        ("i64", "MIN") => "INT64_MIN",
        ("i64", "MAX") => "INT64_MAX",
        ("isize", "MIN") => "INTPTR_MIN",
        ("isize", "MAX") => "INTPTR_MAX",
        ("u8", "MAX") => "UINT8_MAX",
        ("u16", "MAX") => "UINT16_MAX",
        ("u32", "MAX") => "UINT32_MAX",
        ("u64", "MAX") => "UINT64_MAX",
        ("usize", "MAX") => "SIZE_MAX",
        _ => return None,
    })
}

/// The MIR type of a C limit macro emitted by `c_limit_macro`.
pub(super) fn c_limit_type(name: &str) -> Option<MirType> {
    let ty = match name {
        "INT8_MIN" | "INT8_MAX" => "i8",
        "INT16_MIN" | "INT16_MAX" => "i16",
        "INT32_MIN" | "INT32_MAX" => "i32",
        "INT64_MIN" | "INT64_MAX" => "i64",
        "INTPTR_MIN" | "INTPTR_MAX" => "isize",
        "UINT8_MAX" => "u8",
        "UINT16_MAX" => "u16",
        "UINT32_MAX" => "u32",
        "UINT64_MAX" => "u64",
        "SIZE_MAX" => "usize",
        _ => return None,
    };
    Some(int_type_by_name(ty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_negated_minimum_takes_the_hinted_type() {
        let i8t = MirType::i8();
        let i64t = MirType::i64();
        assert_eq!(negated_min_type(-128, Some(&i8t)), Some(MirType::i8()));
        assert_eq!(negated_min_type(-127, Some(&i8t)), None);
        assert_eq!(negated_min_type(-128, Some(&i64t)), None);
        assert_eq!(
            negated_min_type(i128::from(i64::MIN), Some(&i64t)),
            Some(MirType::i64())
        );
        assert_eq!(negated_min_type(-128, Some(&MirType::u8())), None);
        assert_eq!(
            negated_min_type(i128::from(i32::MIN), None),
            Some(MirType::i32())
        );
        assert_eq!(negated_min_type(i128::from(i64::MIN), None), None);
    }

    #[test]
    fn every_macro_round_trips_to_its_type() {
        for ty in [
            "i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize",
        ] {
            for bound in ["MIN", "MAX"] {
                if let Some(m) = c_limit_macro(ty, bound) {
                    assert_eq!(c_limit_type(m), Some(int_type_by_name(ty)), "{ty}::{bound}");
                }
            }
        }
    }
}
