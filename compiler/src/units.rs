// ===============================================================================
// BUILDLANG COMPILER - DIMENSIONAL ANALYSIS CORE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Compile-time dimensional analysis: first-class typed physical units.
//!
//! This module is the PURE, dependency-free core of BuildLang's dimensional
//! analysis. It represents a physical dimension as a fixed vector of rational
//! (here: integer) exponents over the seven SI base dimensions, and provides the
//! algebra a type checker needs:
//!
//! - MULTIPLY / DIVIDE combine dimensions by ADDING / SUBTRACTING exponents.
//! - POWER scales every exponent.
//! - ADD / SUBTRACT / COMPARE require EQUAL dimensions; a mismatch is an error
//!   (this is the rule a dimensional bug trips: `metre + second` has no meaning).
//!
//! It also parses a compact unit grammar (`m`, `s`, `kg`, `m/s`, `kg*m/s^2`,
//! `1` for dimensionless) into a canonical [`Dimension`], and formats a
//! [`Dimension`] back to a canonical string. The canonical form is what the
//! scientific-runtime receipt seals in its `measurement.units` field, so a
//! receipt records a CHECKED, normalized unit rather than an arbitrary
//! free-text string.
//!
//! # Maturity
//!
//! This is a real, tested core. It is NOT yet wired into the full
//! Hindley-Milner type checker (`f64<m/s>` unit-annotated types), which is a
//! separate multi-pass build specced in `docs/DIMENSIONAL-ANALYSIS.md`. Nothing
//! here changes codegen or the C backend, and it makes no claim that a compiled
//! program's runtime numbers carry units: it checks and canonicalizes unit
//! ANNOTATIONS and the receipt measurement label. Honest scope.

use std::fmt;

/// The number of SI base dimensions.
pub const BASE_DIMENSION_COUNT: usize = 7;

/// The seven SI base dimensions, in the fixed order used by [`Dimension`].
///
/// The order is the canonical SI order and is load-bearing: the exponent at
/// index `i` in a [`Dimension`] is the exponent of `BASE_DIMENSIONS[i]`.
pub const BASE_DIMENSIONS: [BaseDimension; BASE_DIMENSION_COUNT] = [
    BaseDimension::Length,
    BaseDimension::Mass,
    BaseDimension::Time,
    BaseDimension::ElectricCurrent,
    BaseDimension::Temperature,
    BaseDimension::AmountOfSubstance,
    BaseDimension::LuminousIntensity,
];

/// One of the seven SI base dimensions. Each carries its base UNIT symbol (the
/// SI base unit) and a human-readable name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseDimension {
    /// Length, base unit metre (`m`).
    Length,
    /// Mass, base unit kilogram (`kg`).
    Mass,
    /// Time, base unit second (`s`).
    Time,
    /// Electric current, base unit ampere (`A`).
    ElectricCurrent,
    /// Thermodynamic temperature, base unit kelvin (`K`).
    Temperature,
    /// Amount of substance, base unit mole (`mol`).
    AmountOfSubstance,
    /// Luminous intensity, base unit candela (`cd`).
    LuminousIntensity,
}

impl BaseDimension {
    /// The SI base-unit symbol for this dimension.
    pub fn symbol(self) -> &'static str {
        match self {
            BaseDimension::Length => "m",
            BaseDimension::Mass => "kg",
            BaseDimension::Time => "s",
            BaseDimension::ElectricCurrent => "A",
            BaseDimension::Temperature => "K",
            BaseDimension::AmountOfSubstance => "mol",
            BaseDimension::LuminousIntensity => "cd",
        }
    }

    /// The index of this dimension in a [`Dimension`] exponent vector.
    pub fn index(self) -> usize {
        match self {
            BaseDimension::Length => 0,
            BaseDimension::Mass => 1,
            BaseDimension::Time => 2,
            BaseDimension::ElectricCurrent => 3,
            BaseDimension::Temperature => 4,
            BaseDimension::AmountOfSubstance => 5,
            BaseDimension::LuminousIntensity => 6,
        }
    }
}

/// A physical dimension: a vector of integer exponents over the seven SI base
/// dimensions. `Dimension::DIMENSIONLESS` is the all-zero vector (a pure
/// number). Velocity is length^1 time^-1, force is mass^1 length^1 time^-2, and
/// so on.
///
/// Exponents are integers here (not full rationals). That covers every derived
/// unit BuildLang's receipt and its numeric kernels use; fractional exponents
/// (e.g. `sqrt(Hz)`) are out of scope for this core and documented as such in
/// the spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dimension {
    /// Exponent of each base dimension, indexed by [`BaseDimension::index`].
    exps: [i32; BASE_DIMENSION_COUNT],
}

impl Dimension {
    /// The dimensionless dimension (a pure number): all exponents zero.
    pub const DIMENSIONLESS: Dimension = Dimension {
        exps: [0; BASE_DIMENSION_COUNT],
    };

    /// Construct a dimension from an explicit exponent vector.
    pub const fn from_exponents(exps: [i32; BASE_DIMENSION_COUNT]) -> Self {
        Dimension { exps }
    }

    /// A base dimension raised to exponent `1` (e.g. `Dimension::base(Length)`
    /// is length).
    pub fn base(base: BaseDimension) -> Self {
        let mut exps = [0; BASE_DIMENSION_COUNT];
        exps[base.index()] = 1;
        Dimension { exps }
    }

    /// The exponent of a given base dimension.
    pub fn exponent(&self, base: BaseDimension) -> i32 {
        self.exps[base.index()]
    }

    /// Whether this is the dimensionless dimension (a pure number).
    pub fn is_dimensionless(&self) -> bool {
        self.exps.iter().all(|&e| e == 0)
    }

    /// Multiply two dimensions: ADD exponents component-wise. This is the
    /// dimension of a product `a * b`.
    pub fn multiply(&self, other: &Dimension) -> Dimension {
        let mut exps = self.exps;
        for i in 0..BASE_DIMENSION_COUNT {
            exps[i] += other.exps[i];
        }
        Dimension { exps }
    }

    /// Divide two dimensions: SUBTRACT exponents component-wise. This is the
    /// dimension of a quotient `a / b`.
    pub fn divide(&self, other: &Dimension) -> Dimension {
        let mut exps = self.exps;
        for i in 0..BASE_DIMENSION_COUNT {
            exps[i] -= other.exps[i];
        }
        Dimension { exps }
    }

    /// Raise a dimension to an integer power: SCALE every exponent by `n`.
    pub fn powi(&self, n: i32) -> Dimension {
        let mut exps = self.exps;
        for e in exps.iter_mut() {
            *e *= n;
        }
        Dimension { exps }
    }

    /// The reciprocal dimension (`1 / self`): NEGATE every exponent.
    pub fn reciprocal(&self) -> Dimension {
        self.powi(-1)
    }

    /// Checked ADD: two quantities may be added only when their dimensions are
    /// EQUAL. Returns the common dimension on success, or [`UnitError::Mismatch`]
    /// otherwise. This is the rule that turns a dimensional bug into an error.
    pub fn checked_add(&self, other: &Dimension) -> Result<Dimension, UnitError> {
        if self == other {
            Ok(*self)
        } else {
            Err(UnitError::Mismatch {
                left: *self,
                right: *other,
                operation: "add",
            })
        }
    }

    /// Checked SUBTRACT: identical rule to [`checked_add`](Self::checked_add).
    pub fn checked_sub(&self, other: &Dimension) -> Result<Dimension, UnitError> {
        if self == other {
            Ok(*self)
        } else {
            Err(UnitError::Mismatch {
                left: *self,
                right: *other,
                operation: "subtract",
            })
        }
    }

    /// Checked COMPARE (`<`, `>`, `==`, ...): comparands must share a dimension.
    /// Returns `Ok(())` when comparable.
    pub fn checked_compare(&self, other: &Dimension) -> Result<(), UnitError> {
        if self == other {
            Ok(())
        } else {
            Err(UnitError::Mismatch {
                left: *self,
                right: *other,
                operation: "compare",
            })
        }
    }

    /// Format this dimension as a canonical unit string over SI base-unit
    /// symbols, e.g. `m/s`, `kg*m/s^2`, or `1` for dimensionless.
    ///
    /// Canonical form: positive-exponent factors first in fixed SI base order,
    /// joined by `*`; then, if any negative exponents exist, a `/` followed by
    /// the negative-exponent factors (with their absolute exponents). A factor
    /// with exponent 1 omits the `^1`. Dimensionless is the literal `1`.
    pub fn to_canonical_string(&self) -> String {
        let mut numerator: Vec<String> = Vec::new();
        let mut denominator: Vec<String> = Vec::new();
        for base in BASE_DIMENSIONS {
            let e = self.exps[base.index()];
            if e > 0 {
                numerator.push(factor_string(base.symbol(), e));
            } else if e < 0 {
                denominator.push(factor_string(base.symbol(), -e));
            }
        }
        if numerator.is_empty() && denominator.is_empty() {
            return "1".to_string();
        }
        let num = if numerator.is_empty() {
            "1".to_string()
        } else {
            numerator.join("*")
        };
        if denominator.is_empty() {
            num
        } else {
            format!("{}/{}", num, denominator.join("*"))
        }
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_canonical_string())
    }
}

/// Format one `symbol^exp` factor, omitting `^1`.
fn factor_string(symbol: &str, exp: i32) -> String {
    if exp == 1 {
        symbol.to_string()
    } else {
        format!("{}^{}", symbol, exp)
    }
}

/// An error from dimensional analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitError {
    /// Two dimensions that had to be equal were not (add / subtract / compare).
    Mismatch {
        /// The left operand's dimension.
        left: Dimension,
        /// The right operand's dimension.
        right: Dimension,
        /// The operation that required equality (`"add"`, `"subtract"`,
        /// `"compare"`).
        operation: &'static str,
    },
    /// A unit token was not a recognised base or derived unit symbol.
    UnknownUnit(String),
    /// The unit annotation could not be parsed (bad grammar).
    ParseError(String),
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnitError::Mismatch {
                left,
                right,
                operation,
            } => write!(
                f,
                "unit mismatch: cannot {} `{}` and `{}` (dimensions differ)",
                operation, left, right
            ),
            UnitError::UnknownUnit(u) => write!(f, "unknown unit `{}`", u),
            UnitError::ParseError(m) => write!(f, "malformed unit annotation: {}", m),
        }
    }
}

impl std::error::Error for UnitError {}

/// Look up a single unit token, returning its dimension. Recognises the seven
/// SI base-unit symbols and a curated set of named derived units.
///
/// The named derived units are the ones BuildLang's scientific kernels and
/// receipt actually reach for; the list is deliberately small and documented,
/// not a full SI/CODATA table.
pub fn lookup_unit(token: &str) -> Result<Dimension, UnitError> {
    use BaseDimension::*;
    let d = |exps: [i32; BASE_DIMENSION_COUNT]| Dimension::from_exponents(exps);
    let dim = match token {
        // Dimensionless.
        "1" => Dimension::DIMENSIONLESS,

        // SI base units.
        "m" => Dimension::base(Length),
        "kg" => Dimension::base(Mass),
        "s" => Dimension::base(Time),
        "A" => Dimension::base(ElectricCurrent),
        "K" => Dimension::base(Temperature),
        "mol" => Dimension::base(AmountOfSubstance),
        "cd" => Dimension::base(LuminousIntensity),

        // Named derived units (subset). Exponent order: [m, kg, s, A, K, mol, cd].
        // Frequency: hertz = 1/s.
        "Hz" => d([0, 0, -1, 0, 0, 0, 0]),
        // Force: newton = kg*m/s^2.
        "N" => d([1, 1, -2, 0, 0, 0, 0]),
        // Pressure: pascal = kg/(m*s^2) = N/m^2.
        "Pa" => d([-1, 1, -2, 0, 0, 0, 0]),
        // Energy: joule = kg*m^2/s^2 = N*m.
        "J" => d([2, 1, -2, 0, 0, 0, 0]),
        // Power: watt = kg*m^2/s^3 = J/s.
        "W" => d([2, 1, -3, 0, 0, 0, 0]),
        // Electric charge: coulomb = A*s.
        "C" => d([0, 0, 1, 1, 0, 0, 0]),
        // Electric potential: volt = kg*m^2/(s^3*A) = W/A.
        "V" => d([2, 1, -3, -1, 0, 0, 0]),

        other => return Err(UnitError::UnknownUnit(other.to_string())),
    };
    Ok(dim)
}

/// Parse a compact unit annotation into a canonical [`Dimension`].
///
/// Grammar (whitespace is insignificant):
///
/// ```text
/// unit      := factor ( ('*' | '/') factor )*
/// factor    := token ( '^' signed-int )?
/// token     := an identifier looked up by `lookup_unit` (base or named derived)
/// ```
///
/// The leftmost factor is in the numerator. Each `*` keeps the following factor
/// in the numerator; each `/` places the following factor in the denominator
/// (i.e. `/` binds one factor, matching the canonical formatter: `kg*m/s^2` is
/// `kg*m*(s^-2)`, not `kg*m/(s^2*...)` beyond the single factor). A `^`
/// exponent scales that one factor. `1` is the dimensionless literal.
///
/// Examples: `"m/s"`, `"kg*m/s^2"`, `"1/s"`, `"J"`, `"1"`.
pub fn parse_unit(input: &str) -> Result<Dimension, UnitError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(UnitError::ParseError("empty unit annotation".to_string()));
    }

    let tokens = tokenize(trimmed)?;
    if tokens.is_empty() {
        return Err(UnitError::ParseError("no unit factors found".to_string()));
    }

    // The first token must be a factor (unit, optionally `^ exp`).
    let mut idx = 0usize;
    let mut result = Dimension::DIMENSIONLESS;

    // Parse the first factor into the numerator.
    let (first, next) = parse_factor(&tokens, idx)?;
    result = result.multiply(&first);
    idx = next;

    // Then a sequence of ('*' | '/') factor.
    while idx < tokens.len() {
        let op = &tokens[idx];
        let sign = match op.as_str() {
            "*" => 1,
            "/" => -1,
            other => {
                return Err(UnitError::ParseError(format!(
                    "expected `*` or `/`, found `{}`",
                    other
                )))
            }
        };
        idx += 1;
        let (factor, next) = parse_factor(&tokens, idx)?;
        result = if sign == 1 {
            result.multiply(&factor)
        } else {
            result.divide(&factor)
        };
        idx = next;
    }

    Ok(result)
}

/// Parse one factor starting at `tokens[idx]`: a unit token optionally followed
/// by `^ signed-int`. Returns the factor's dimension and the index after it.
fn parse_factor(tokens: &[String], idx: usize) -> Result<(Dimension, usize), UnitError> {
    let token = tokens.get(idx).ok_or_else(|| {
        UnitError::ParseError("expected a unit factor but reached end of input".to_string())
    })?;
    if token == "*" || token == "/" || token == "^" {
        return Err(UnitError::ParseError(format!(
            "expected a unit name, found `{}`",
            token
        )));
    }
    let mut dim = lookup_unit(token)?;
    let mut next = idx + 1;
    // Optional `^ exp`.
    if next < tokens.len() && tokens[next] == "^" {
        let exp_tok = tokens.get(next + 1).ok_or_else(|| {
            UnitError::ParseError("expected an exponent after `^`".to_string())
        })?;
        let exp: i32 = exp_tok
            .parse()
            .map_err(|_| UnitError::ParseError(format!("invalid exponent `{}`", exp_tok)))?;
        dim = dim.powi(exp);
        next += 2;
    }
    Ok((dim, next))
}

/// Split a unit annotation into tokens: identifiers, the operators `* / ^`, and
/// signed integer literals (an exponent may be negative: `s^-2`).
fn tokenize(input: &str) -> Result<Vec<String>, UnitError> {
    let mut tokens: Vec<String> = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '*' | '/' | '^' => {
                tokens.push(c.to_string());
                i += 1;
            }
            _ if c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                tokens.push(chars[start..i].iter().collect());
            }
            _ if c.is_ascii_digit() || c == '-' => {
                // A numeric literal: either the dimensionless `1`, or an
                // exponent (possibly negative) after a `^`.
                let start = i;
                if c == '-' {
                    i += 1;
                }
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let lit: String = chars[start..i].iter().collect();
                if lit == "-" {
                    return Err(UnitError::ParseError(
                        "stray `-` with no digits".to_string(),
                    ));
                }
                tokens.push(lit);
            }
            other => {
                return Err(UnitError::ParseError(format!(
                    "unexpected character `{}`",
                    other
                )))
            }
        }
    }
    Ok(tokens)
}

/// Parse a unit annotation and return its CANONICAL string form. This is the
/// helper the receipt path uses: it both VALIDATES the annotation (a malformed
/// or unknown unit is a hard error, not a silently-passed free-text string) and
/// NORMALIZES it, so two spellings of the same unit (`m/s` and `m*s^-1`) seal to
/// the same bytes.
pub fn canonicalize_unit(input: &str) -> Result<String, UnitError> {
    Ok(parse_unit(input)?.to_canonical_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use BaseDimension::*;

    #[test]
    fn dimensionless_is_all_zero() {
        assert!(Dimension::DIMENSIONLESS.is_dimensionless());
        assert_eq!(Dimension::DIMENSIONLESS.to_canonical_string(), "1");
    }

    #[test]
    fn base_units_round_trip() {
        assert_eq!(Dimension::base(Length).to_canonical_string(), "m");
        assert_eq!(Dimension::base(Mass).to_canonical_string(), "kg");
        assert_eq!(Dimension::base(Time).to_canonical_string(), "s");
        assert_eq!(Dimension::base(ElectricCurrent).to_canonical_string(), "A");
        assert_eq!(Dimension::base(Temperature).to_canonical_string(), "K");
        assert_eq!(
            Dimension::base(AmountOfSubstance).to_canonical_string(),
            "mol"
        );
        assert_eq!(
            Dimension::base(LuminousIntensity).to_canonical_string(),
            "cd"
        );
    }

    #[test]
    fn multiply_adds_exponents() {
        // m * m = m^2 (area).
        let area = Dimension::base(Length).multiply(&Dimension::base(Length));
        assert_eq!(area.exponent(Length), 2);
        assert_eq!(area.to_canonical_string(), "m^2");
    }

    #[test]
    fn divide_subtracts_exponents() {
        // m / s = m/s (velocity).
        let velocity = Dimension::base(Length).divide(&Dimension::base(Time));
        assert_eq!(velocity.exponent(Length), 1);
        assert_eq!(velocity.exponent(Time), -1);
        assert_eq!(velocity.to_canonical_string(), "m/s");
    }

    #[test]
    fn acceleration_and_force() {
        // acceleration = m / s^2.
        let accel = Dimension::base(Length).divide(&Dimension::base(Time).powi(2));
        assert_eq!(accel.to_canonical_string(), "m/s^2");
        // force = mass * acceleration = newton. Canonical form follows the
        // fixed SI base order (length before mass), so it renders `m*kg/s^2`
        // regardless of the multiplication order that built it.
        let force = Dimension::base(Mass).multiply(&accel);
        assert_eq!(force, lookup_unit("N").unwrap());
        assert_eq!(force.to_canonical_string(), "m*kg/s^2");
    }

    #[test]
    fn checked_add_same_dimension_ok() {
        let a = Dimension::base(Length);
        let b = Dimension::base(Length);
        assert_eq!(a.checked_add(&b), Ok(Dimension::base(Length)));
    }

    #[test]
    fn checked_add_mismatch_is_error() {
        // metre + second must be an error: this is the dimensional bug.
        let metre = Dimension::base(Length);
        let second = Dimension::base(Time);
        let err = metre.checked_add(&second).unwrap_err();
        match err {
            UnitError::Mismatch {
                left,
                right,
                operation,
            } => {
                assert_eq!(left, metre);
                assert_eq!(right, second);
                assert_eq!(operation, "add");
            }
            other => panic!("expected Mismatch, got {:?}", other),
        }
    }

    #[test]
    fn checked_sub_and_compare_mismatch() {
        let energy = lookup_unit("J").unwrap();
        let power = lookup_unit("W").unwrap();
        assert!(energy.checked_sub(&power).is_err());
        assert!(energy.checked_compare(&power).is_err());
        // Same dimension compares fine.
        assert!(energy.checked_compare(&lookup_unit("J").unwrap()).is_ok());
    }

    #[test]
    fn parse_base_and_derived() {
        assert_eq!(parse_unit("m").unwrap(), Dimension::base(Length));
        assert_eq!(parse_unit("1").unwrap(), Dimension::DIMENSIONLESS);
        assert_eq!(parse_unit("N").unwrap(), lookup_unit("N").unwrap());
    }

    #[test]
    fn parse_composite_velocity() {
        let v = parse_unit("m/s").unwrap();
        assert_eq!(v, Dimension::base(Length).divide(&Dimension::base(Time)));
        assert_eq!(v.to_canonical_string(), "m/s");
    }

    #[test]
    fn parse_force_expression() {
        let f = parse_unit("kg*m/s^2").unwrap();
        assert_eq!(f, lookup_unit("N").unwrap());
    }

    #[test]
    fn parse_negative_exponent_form_equals_slash_form() {
        // m*s^-1 and m/s must canonicalize identically.
        let a = parse_unit("m*s^-1").unwrap();
        let b = parse_unit("m/s").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_canonical_string(), b.to_canonical_string());
    }

    #[test]
    fn parse_inverse_time() {
        let hz = parse_unit("1/s").unwrap();
        assert_eq!(hz, lookup_unit("Hz").unwrap());
        assert_eq!(hz.to_canonical_string(), "1/s");
    }

    #[test]
    fn canonicalize_normalizes_spellings() {
        assert_eq!(canonicalize_unit("m*s^-1").unwrap(), "m/s");
        // Canonical order is fixed SI base order (length before mass), so both
        // `kg*m/s^2` and `m*kg/s^2` normalize to the same `m*kg/s^2`.
        assert_eq!(canonicalize_unit("kg*m/s^2").unwrap(), "m*kg/s^2");
        assert_eq!(canonicalize_unit("m*kg/s^2").unwrap(), "m*kg/s^2");
        assert_eq!(canonicalize_unit("  m / s  ").unwrap(), "m/s");
    }

    #[test]
    fn unknown_unit_is_error() {
        let err = parse_unit("furlong").unwrap_err();
        assert!(matches!(err, UnitError::UnknownUnit(u) if u == "furlong"));
    }

    #[test]
    fn malformed_annotations_are_errors() {
        assert!(parse_unit("").is_err());
        assert!(parse_unit("m/").is_err());
        assert!(parse_unit("m^").is_err());
        assert!(parse_unit("*s").is_err());
        assert!(parse_unit("m s").is_err()); // two factors with no operator
    }

    #[test]
    fn powi_and_reciprocal() {
        let area = Dimension::base(Length).powi(2);
        assert_eq!(area.to_canonical_string(), "m^2");
        let inv_time = Dimension::base(Time).reciprocal();
        assert_eq!(inv_time.to_canonical_string(), "1/s");
        assert_eq!(inv_time, lookup_unit("Hz").unwrap());
    }

    #[test]
    fn energy_from_named_units_matches_base_composition() {
        // J == N*m.
        let joule = lookup_unit("J").unwrap();
        let n_times_m = lookup_unit("N").unwrap().multiply(&Dimension::base(Length));
        assert_eq!(joule, n_times_m);
        // W == J/s.
        let watt = lookup_unit("W").unwrap();
        assert_eq!(watt, joule.divide(&Dimension::base(Time)));
    }
}
