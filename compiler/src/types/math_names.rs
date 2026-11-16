//! The maths functions and constants the compiler provides with no
//! declaration: `sqrt(x)`, `min(a, b)`, `PI`.
//!
//! One list, read from two places. The checker accepts a bare use of one of
//! these names (`infer_ident`), and it accepts `use std::math::sqrt;` or
//! `use core::math::{min, max, PI};` as an import of the same built-in
//! (`unresolved_use_reason`). The import binds nothing: with it or without
//! it, `sqrt` is the built-in. A program's own `fn sqrt` still takes the
//! name, as before.
//!
//! `stdlib/math.bld` must not declare a function with one of these names:
//! the declared function would replace the built-in for every importer.
//! `compiler/tests/math_imports.rs` checks that.

/// Functions on numbers, on vectors, and the constants.
pub(crate) const MATH_BUILTINS: &[&str] = &[
    // Functions on numbers.
    "sqrt",
    "sin",
    "cos",
    "tan",
    "pow",
    "abs",
    "sinh",
    "cosh",
    "tanh",
    "asin",
    "acos",
    "atan",
    "log",
    "log2",
    "log10",
    "exp",
    "exp2",
    "atan2",
    "floor",
    "ceil",
    "round",
    "min",
    "max",
    "clamp",
    "smoothstep",
    "mix",
    "fract",
    "step",
    // Functions on the built-in vectors; `lerp` also takes numbers.
    "dot",
    "cross",
    "normalize",
    "length",
    "reflect",
    "lerp",
    // Constants.
    "PI",
    "E",
    "TAU",
];

/// Whether `name` is a maths built-in: usable with no declaration, and
/// importable from `std::math` and `core::math`.
pub(crate) fn is_math_builtin(name: &str) -> bool {
    MATH_BUILTINS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_the_universe_imports_are_built_ins() {
        for name in [
            "min",
            "max",
            "pow",
            "abs",
            "exp",
            "sqrt",
            "sin",
            "cos",
            "log",
            "clamp",
            "PI",
            "floor",
            "atan2",
            "ceil",
            "lerp",
            "tan",
            "tanh",
            "smoothstep",
        ] {
            assert!(is_math_builtin(name), "{name}");
        }
    }

    #[test]
    fn a_name_the_compiler_does_not_provide_is_not_a_built_in() {
        for name in ["ln", "cbrt", "TWO_PI", "Quaternion", "sqrtf", ""] {
            assert!(!is_math_builtin(name), "{name}");
        }
    }

    #[test]
    fn no_name_is_listed_twice() {
        let mut seen = std::collections::HashSet::new();
        for name in MATH_BUILTINS {
            assert!(seen.insert(*name), "{name} is listed twice");
        }
    }
}
