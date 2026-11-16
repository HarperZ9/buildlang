// ===============================================================================
// BUILDLANG COMPILER - UNIT RULES FOR FLOAT METHODS AND POWERS
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! The dimension a float method or `**` gives when a unit is involved.
//!
//! Checker slice one refused `**`, `.sqrt()`, `.cbrt()`, `.powi()` and
//! `.powf()` on a unit-carrying value, and let every other float method keep
//! the receiver's unit unchanged. That was wrong for several of them:
//! `d.recip()` on metres stayed metres, and `d.sin()` on metres checked. This
//! module gives each method its dimension rule:
//!
//! - roots and integer powers scale the exponents (`area.sqrt()` on `m^2` is
//!   `m`); a root that leaves a fractional exponent is refused;
//! - `.recip()` inverts the dimension;
//! - trigonometric, exponential and logarithmic methods need a dimensionless
//!   value and give a dimensionless one;
//! - `.min()`, `.max()`, `.clamp()`, `.hypot()`, `.atan2()` and
//!   `.rem_euclid()` need their arguments in the receiver's dimension.
//!
//! Weak mode is kept: a float with no annotation (`None`) is compatible with
//! any dimension, exactly as at every other boundary. Every rule here runs only
//! when at least one operand carries a unit, so an unannotated program is
//! checked exactly as before.

use crate::ast;
use crate::units::Dimension;

use super::error::TypeError;

/// The dimension a unit-aware float operation produces. `Ok(None)` means the
/// result carries no unit (both inputs were unannotated in weak mode).
pub type UnitResult = Result<Option<Dimension>, TypeError>;

/// Methods whose value changes but whose unit does not.
const IDENTITY: &[&str] = &["abs", "ceil", "floor", "round", "trunc", "fract"];

/// Methods that need a dimensionless value and give a dimensionless one.
const DIMENSIONLESS_ONLY: &[&str] = &[
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "sinh",
    "cosh",
    "tanh",
    "asinh",
    "acosh",
    "atanh",
    "exp",
    "exp2",
    "ln",
    "log2",
    "log10",
    "to_radians",
    "to_degrees",
    "sin_cos",
];

/// The integer value of a literal exponent: `2`, `2.0`, `-1`, `(3)`. `None`
/// for anything else, including `2.5` and a variable.
pub fn literal_integer_exponent(expr: &ast::Expr) -> Option<i32> {
    match &expr.kind {
        ast::ExprKind::Literal(ast::Literal::Int { value, .. }) => i32::try_from(*value).ok(),
        ast::ExprKind::Literal(ast::Literal::Float { value, .. }) => {
            let v = *value;
            (v.fract() == 0.0 && v.abs() <= i32::MAX as f64).then_some(v as i32)
        }
        ast::ExprKind::Unary {
            op: ast::UnaryOp::Neg,
            expr,
        } => literal_integer_exponent(expr).map(|n| -n),
        ast::ExprKind::Paren(inner) => literal_integer_exponent(inner),
        _ => None,
    }
}

/// The dimension of `base ** exponent` (and `.powi(n)`, `.powf(n)`) when the
/// base carries a unit.
pub fn power(base: Dimension, exponent: Option<i32>, what: &str) -> UnitResult {
    match exponent {
        Some(n) => Ok(Some(base.powi(n))),
        None => Err(TypeError::UnsupportedConstruct {
            construct: format!("{what} on a unit-carrying value with a non-literal exponent"),
            detail: format!(
                "the unit of the result depends on the exponent, so it must be an integer \
                 literal (`2`, `-1`, `3.0`); the base has unit `{}`",
                base.to_canonical_string()
            ),
        }),
    }
}

/// The n-th root of a dimension, or an error when an exponent does not divide.
pub fn root(dim: Dimension, n: i32, method: &str) -> UnitResult {
    let exps: Vec<i32> = crate::units::BASE_DIMENSIONS
        .iter()
        .map(|b| dim.exponent(*b))
        .collect();
    if exps.iter().any(|e| e % n != 0) {
        return Err(TypeError::UnitRootNotExact {
            method: method.to_string(),
            found: dim.to_canonical_string(),
        });
    }
    let mut out = [0i32; crate::units::BASE_DIMENSION_COUNT];
    for (slot, e) in out.iter_mut().zip(exps) {
        *slot = e / n;
    }
    Ok(Some(Dimension::from_exponents(out)))
}

fn require_dimensionless(dim: Option<Dimension>, operation: &str) -> Result<(), TypeError> {
    match dim {
        Some(d) if !d.is_dimensionless() => Err(TypeError::UnitNotDimensionless {
            operation: operation.to_string(),
            found: d.to_canonical_string(),
        }),
        _ => Ok(()),
    }
}

/// Weak-mode agreement of two optional dimensions: `None` matches anything.
/// Returns the known dimension, if either side has one.
fn same(
    a: Option<Dimension>,
    b: Option<Dimension>,
    operation: &'static str,
) -> Result<Option<Dimension>, TypeError> {
    match (a, b) {
        (Some(x), Some(y)) if x != y => Err(TypeError::UnitOperationMismatch {
            operation,
            left: x.to_canonical_string(),
            right: y.to_canonical_string(),
        }),
        (Some(x), _) | (_, Some(x)) => Ok(Some(x)),
        (None, None) => Ok(None),
    }
}

/// The unit rule for `receiver.method(args)` on a float.
///
/// Returns `None` when no operand carries a unit, or when the method has no
/// unit rule here (predicates such as `is_nan`, conversions such as
/// `to_bits`): the caller then types the call as it always has.
/// `exponent` is the literal integer value of the first argument, if it is one.
pub fn float_method(
    method: &str,
    receiver: Option<Dimension>,
    args: &[Option<Dimension>],
    exponent: Option<i32>,
) -> Option<UnitResult> {
    if receiver.is_none() && args.iter().all(Option::is_none) {
        return None;
    }
    let arg = |i: usize| args.get(i).copied().flatten();
    let result = if IDENTITY.contains(&method) {
        Ok(receiver)
    } else if DIMENSIONLESS_ONLY.contains(&method) {
        require_dimensionless(receiver, &format!(".{method}()"))
            .map(|_| Some(Dimension::DIMENSIONLESS))
    } else {
        match method {
            "sqrt" => receiver.map_or(Ok(None), |d| root(d, 2, "sqrt")),
            "cbrt" => receiver.map_or(Ok(None), |d| root(d, 3, "cbrt")),
            "powi" | "powf" => match receiver {
                Some(d) => power(d, exponent, &format!("`.{method}()`")),
                None => Ok(None),
            },
            "recip" => Ok(receiver.map(|d| d.reciprocal())),
            "signum" => Ok(Some(Dimension::DIMENSIONLESS)),
            "log" => require_dimensionless(receiver, ".log()")
                .and_then(|_| require_dimensionless(arg(0), "the base of .log()"))
                .map(|_| Some(Dimension::DIMENSIONLESS)),
            "atan2" => {
                same(receiver, arg(0), "take atan2 of").map(|_| Some(Dimension::DIMENSIONLESS))
            }
            "hypot" => same(receiver, arg(0), "take the hypotenuse of"),
            "min" | "max" => same(receiver, arg(0), "compare"),
            "clamp" => same(receiver, arg(0), "compare").and_then(|d| same(d, arg(1), "compare")),
            "rem_euclid" => same(receiver, arg(0), "take the remainder of"),
            "copysign" => Ok(receiver),
            "div_euclid" => Ok(match (receiver, arg(0)) {
                (None, None) => None,
                (r, a) => Some(
                    r.unwrap_or(Dimension::DIMENSIONLESS)
                        .divide(&a.unwrap_or(Dimension::DIMENSIONLESS)),
                ),
            }),
            "mul_add" => {
                let product = match (receiver, arg(0)) {
                    (None, None) => None,
                    (r, a) => Some(
                        r.unwrap_or(Dimension::DIMENSIONLESS)
                            .multiply(&a.unwrap_or(Dimension::DIMENSIONLESS)),
                    ),
                };
                same(product, arg(1), "add")
            }
            _ => return None,
        }
    };
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::parse_unit;

    fn u(s: &str) -> Option<Dimension> {
        Some(parse_unit(s).unwrap())
    }

    #[test]
    fn unannotated_operands_have_no_rule() {
        assert!(float_method("sqrt", None, &[], None).is_none());
        assert!(float_method("hypot", None, &[None], None).is_none());
    }

    #[test]
    fn roots_scale_and_refuse_fractions() {
        assert_eq!(
            float_method("sqrt", u("m^2"), &[], None).unwrap().unwrap(),
            u("m")
        );
        assert_eq!(
            float_method("cbrt", u("m^3"), &[], None).unwrap().unwrap(),
            u("m")
        );
        assert!(matches!(
            float_method("sqrt", u("m"), &[], None).unwrap(),
            Err(TypeError::UnitRootNotExact { .. })
        ));
    }

    #[test]
    fn powers_need_a_literal_exponent() {
        assert_eq!(
            float_method("powi", u("m"), &[None], Some(2))
                .unwrap()
                .unwrap(),
            u("m^2")
        );
        assert_eq!(
            float_method("powf", u("s"), &[None], Some(-1))
                .unwrap()
                .unwrap(),
            u("Hz")
        );
        assert!(float_method("powi", u("m"), &[None], None)
            .unwrap()
            .is_err());
    }

    #[test]
    fn recip_inverts() {
        assert_eq!(
            float_method("recip", u("s"), &[], None).unwrap().unwrap(),
            u("Hz")
        );
    }

    #[test]
    fn transcendental_methods_need_dimensionless() {
        assert!(matches!(
            float_method("sin", u("m"), &[], None).unwrap(),
            Err(TypeError::UnitNotDimensionless { .. })
        ));
        assert_eq!(
            float_method("exp", u("1"), &[], None).unwrap().unwrap(),
            Some(Dimension::DIMENSIONLESS)
        );
        assert!(float_method("log", u("1"), &[u("m")], None)
            .unwrap()
            .is_err());
    }

    #[test]
    fn binary_methods_need_matching_units() {
        assert!(float_method("hypot", u("m"), &[u("s")], None)
            .unwrap()
            .is_err());
        assert_eq!(
            float_method("hypot", u("m"), &[None], None)
                .unwrap()
                .unwrap(),
            u("m")
        );
        assert_eq!(
            float_method("min", None, &[u("Pa")], None)
                .unwrap()
                .unwrap(),
            u("Pa")
        );
        assert!(float_method("clamp", u("K"), &[u("K"), u("s")], None)
            .unwrap()
            .is_err());
        assert_eq!(
            float_method("atan2", u("m"), &[u("m")], None)
                .unwrap()
                .unwrap(),
            Some(Dimension::DIMENSIONLESS)
        );
    }

    #[test]
    fn mul_add_checks_the_sum() {
        // a * t + d: m/s times s plus m is m.
        assert_eq!(
            float_method("mul_add", u("m/s"), &[u("s"), u("m")], None)
                .unwrap()
                .unwrap(),
            u("m")
        );
        assert!(float_method("mul_add", u("m/s"), &[u("s"), u("s")], None)
            .unwrap()
            .is_err());
    }
}
