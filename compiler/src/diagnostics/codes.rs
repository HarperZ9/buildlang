// ===============================================================================
// BUILDLANG DIAGNOSTICS - STABLE ERROR CODES
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Stable diagnostic codes.
//!
//! Every lexer, parser and type-checker error kind maps to one code here. The
//! matches are exhaustive, so a new error kind does not compile until it has a
//! code. A code, once published, never changes meaning; a retired code is never
//! reused. Ranges:
//!
//! - `BL0001`..`BL0099`: input and driver errors
//! - `BL0100`..`BL0199`: lexer
//! - `BL0200`..`BL0299`: parser
//! - `BL0300`..`BL0399`: types, lookup, expressions, patterns, control flow, traits
//! - `BL0400`..`BL0499`: effects and capabilities
//! - `BL0500`..`BL0599`: ownership, borrows and linear values
//! - `BL0600`..`BL0699`: units, modules and macros
//! - `BL0900`..`BL0999`: backend

use crate::lexer::LexerErrorKind;
use crate::parser::ParseErrorKind;
use crate::types::TypeError;

/// The input file could not be read.
pub const IO_ERROR: &str = "BL0001";

/// An import, include or `mod` file of the input could not be loaded.
pub const INPUT_GRAPH_ERROR: &str = "BL0002";

/// A check policy profile rejected the program.
pub const POLICY_VIOLATION: &str = "BL0090";

/// Code for a lexer error kind.
pub fn lexer_code(kind: &LexerErrorKind) -> &'static str {
    use LexerErrorKind as K;
    match kind {
        K::UnexpectedChar(_) => "BL0101",
        K::UnexpectedEof => "BL0102",
        K::UnterminatedString => "BL0103",
        K::UnterminatedChar => "BL0104",
        K::UnterminatedByteString => "BL0105",
        K::UnterminatedRawString { .. } => "BL0106",
        K::EmptyCharLiteral => "BL0107",
        K::MultipleCharsInCharLiteral => "BL0108",
        K::UnknownEscape(_) => "BL0109",
        K::InvalidUnicodeEscape => "BL0110",
        K::UnicodeEscapeMissingBrace => "BL0111",
        K::UnicodeEscapeUnclosed => "BL0112",
        K::UnicodeEscapeTooLong => "BL0113",
        K::InvalidUnicodeCodepoint(_) => "BL0114",
        K::InvalidHexEscape => "BL0115",
        K::HexEscapeOutOfRange(_) => "BL0116",
        K::EscapeAtEof => "BL0117",
        K::InvalidNumber => "BL0118",
        K::IntegerOverflow => "BL0119",
        K::FloatOverflow => "BL0120",
        K::EmptyExponent => "BL0121",
        K::NoDigitsAfterPrefix(_) => "BL0122",
        K::InvalidDigit(_, _) => "BL0123",
        K::FloatWithBase(_) => "BL0124",
        K::InvalidNumericSuffix(_) => "BL0125",
        K::IntSuffixOnFloat(_) => "BL0126",
        K::FloatSuffixOnNonDecimal(_) => "BL0127",
        K::LeadingUnderscore => "BL0128",
        K::TrailingUnderscore => "BL0129",
        K::ConsecutiveUnderscores => "BL0130",
        K::UnterminatedBlockComment { .. } => "BL0131",
        K::InvalidDocComment => "BL0132",
        K::InvalidIdentChar(_) => "BL0133",
        K::ExpectedRawIdent => "BL0134",
        K::CannotBeRawIdent(_) => "BL0135",
        K::ExpectedLifetime => "BL0136",
        K::InvalidLifetime => "BL0137",
        K::MismatchedDslDelimiters => "BL0138",
        K::UnterminatedDslBlock => "BL0139",
        K::UnterminatedFormatString => "BL0140",
        K::UnclosedInterpolation => "BL0141",
        K::EmptyInterpolation => "BL0142",
        K::InterpolationTooDeep(_) => "BL0143",
        K::InvalidFormatSpecifier(_) => "BL0144",
        K::ShebangNotAtStart => "BL0145",
        K::InvalidUtf8 => "BL0146",
        K::NulInSource => "BL0147",
        K::NonAsciiInByteLiteral => "BL0148",
        K::Internal(_) => "BL0199",
    }
}

/// Code for a parser error kind.
pub fn parse_code(kind: &ParseErrorKind) -> &'static str {
    use ParseErrorKind as K;
    match kind {
        K::UnexpectedToken { .. } => "BL0201",
        K::Expected { .. } => "BL0202",
        K::UnexpectedEof => "BL0203",
        K::InvalidExpression => "BL0204",
        K::IntegerLiteralTooLarge { .. } => "BL0205",
        K::InvalidAssignTarget => "BL0206",
        K::MissingOperand => "BL0207",
        K::UnclosedDelimiter { .. } => "BL0208",
        K::MismatchedDelimiter { .. } => "BL0209",
        K::InvalidStatement => "BL0210",
        K::ExpectedSemicolon => "BL0211",
        K::InvalidItem => "BL0212",
        K::DuplicateModifier { .. } => "BL0213",
        K::ConflictingModifiers { .. } => "BL0214",
        K::InvalidType => "BL0215",
        K::ExpectedType => "BL0216",
        K::InvalidUnitAnnotation(_) => "BL0217",
        K::InvalidPattern => "BL0218",
        K::ExpectedPattern => "BL0219",
        K::RestPatternNotLast => "BL0220",
        K::InvalidGenericParam => "BL0221",
        K::InvalidWhereClause => "BL0222",
        K::InvalidAttribute => "BL0223",
        K::InnerAttributeNotAllowed => "BL0224",
        K::InvalidMacroInvocation => "BL0225",
        K::LexerError(_) => "BL0226",
        K::Internal(_) => "BL0299",
    }
}

/// Code for a type-checker error.
pub fn type_code(error: &TypeError) -> &'static str {
    use TypeError as E;
    match error {
        E::TypeMismatch { .. } => "BL0301",
        E::InfiniteType { .. } => "BL0302",
        E::MutabilityMismatch { .. } => "BL0303",
        E::ArrayLengthMismatch { .. } => "BL0304",
        E::ArityMismatch { .. } => "BL0305",
        E::UnsafetyMismatch => "BL0306",
        E::LifetimeMismatch { .. } => "BL0307",
        E::AbiMismatch { .. } => "BL0308",
        E::UndefinedVariable { .. } => "BL0310",
        E::UndefinedType { .. } => "BL0311",
        E::UndefinedFunction { .. } => "BL0312",
        E::UndefinedField { .. } => "BL0313",
        E::UndefinedMethod { .. } => "BL0314",
        E::UndefinedVariant { .. } => "BL0315",
        E::NotCallable { .. } => "BL0320",
        E::NoMatchingMethod { .. } => "BL0321",
        E::AmbiguousMethod { .. } => "BL0322",
        E::AmbiguousFunctionReference { .. } => "BL0323",
        E::NotIndexable { .. } => "BL0324",
        E::NotDereferenceable { .. } => "BL0325",
        E::NotTryable { .. } => "BL0326",
        E::NotAwaitable { .. } => "BL0327",
        E::InvalidBinaryOp { .. } => "BL0328",
        E::InvalidUnaryOp { .. } => "BL0329",
        E::IntegerLiteralOutOfRange { .. } => "BL0330",
        E::IntegerOverflow { .. } => "BL0331",
        E::InvalidAssignTarget => "BL0332",
        E::ImmutableAssignment { .. } => "BL0333",
        E::PatternMismatch { .. } => "BL0340",
        E::RefutablePattern => "BL0341",
        E::NonExhaustivePatterns => "BL0342",
        E::NonExhaustiveMatch { .. } => "BL0343",
        E::BreakOutsideLoop => "BL0350",
        E::ContinueOutsideLoop => "BL0351",
        E::UndefinedLoopLabel { .. } => "BL0352",
        E::ReturnOutsideFunction => "BL0353",
        E::ReturnTypeMismatch { .. } => "BL0354",
        E::TraitNotImplemented { .. } => "BL0360",
        E::AmbiguousImpl { .. } => "BL0361",
        E::AssocTypeNotFound { .. } => "BL0362",
        E::AssociatedTypeNotDefined { .. } => "BL0363",
        E::WrongTypeArgCount { .. } => "BL0370",
        E::BoundNotSatisfied { .. } => "BL0371",
        E::DuplicateDefinition { .. } => "BL0380",
        E::TypeAnnotationNeeded => "BL0381",
        E::UnsafeOutsideUnsafe => "BL0382",
        E::UnsupportedConstruct { .. } => "BL0383",
        E::InfiniteSizeType { .. } => "BL0384",
        E::InternalError(_) => "BL0398",
        E::Internal(_) => "BL0399",
        E::UnknownEffect { .. } => "BL0401",
        E::UnhandledEffect { .. } => "BL0402",
        E::UndeclaredEffect { .. } => "BL0403",
        E::UnknownEffectOperation { .. } => "BL0404",
        E::MissingHandlerClause { .. } => "BL0405",
        E::AlreadyBorrowed { .. } => "BL0501",
        E::DoubleMutableBorrow { .. } => "BL0502",
        E::UseAfterMove { .. } => "BL0503",
        E::ReferenceEscapesScope { .. } => "BL0504",
        E::LinearUseAfterMove { .. } => "BL0510",
        E::LinearBorrowAfterMove { .. } => "BL0511",
        E::LinearMoveOutOfBorrow { .. } => "BL0512",
        E::LinearFieldInNonLinearType { .. } => "BL0513",
        E::LinearInUnsupportedPosition { .. } => "BL0514",
        E::UnitMismatch { .. } => "BL0601",
        E::UnitOperationMismatch { .. } => "BL0602",
        E::ModuleImportCycle { .. } => "BL0610",
        E::FormatArgNotStringLiteral { .. } => "BL0620",
        E::BackendRejected { .. } => "BL0901",
    }
}

/// One entry of the published code table: the code and a one-line title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeInfo {
    pub code: &'static str,
    pub title: &'static str,
}

const fn c(code: &'static str, title: &'static str) -> CodeInfo {
    CodeInfo { code, title }
}

/// Every published code, sorted. `buildc explain` reads this table, and
/// `docs/ERROR_CODES.md` is checked against it by a test.
pub const CODES: &[CodeInfo] = &[
    c("BL0001", "the input file could not be read"),
    c(
        "BL0002",
        "an import, include or module file could not be loaded",
    ),
    c("BL0090", "the check policy profile rejected the program"),
    c("BL0101", "unexpected character"),
    c("BL0102", "unexpected end of file while lexing"),
    c("BL0103", "unterminated string literal"),
    c("BL0104", "unterminated character literal"),
    c("BL0105", "unterminated byte string literal"),
    c("BL0106", "unterminated raw string literal"),
    c("BL0107", "empty character literal"),
    c("BL0108", "character literal holds more than one codepoint"),
    c("BL0109", "unknown escape sequence"),
    c("BL0110", "invalid Unicode escape"),
    c("BL0111", "Unicode escape missing its opening brace"),
    c("BL0112", "Unicode escape missing its closing brace"),
    c("BL0113", "Unicode escape has more than six digits"),
    c("BL0114", "invalid Unicode codepoint"),
    c("BL0115", "invalid hex escape"),
    c("BL0116", "hex escape out of range"),
    c("BL0117", "escape character at end of file"),
    c("BL0118", "invalid numeric literal"),
    c("BL0119", "integer literal too large"),
    c("BL0120", "float literal out of range"),
    c("BL0121", "exponent has no digits"),
    c("BL0122", "radix prefix has no digits"),
    c("BL0123", "digit not valid for the base"),
    c("BL0124", "float literal with a non-decimal base"),
    c("BL0125", "invalid numeric suffix"),
    c("BL0126", "integer suffix on a float literal"),
    c("BL0127", "float suffix on a non-decimal integer"),
    c("BL0128", "numeric literal starts with an underscore"),
    c("BL0129", "numeric literal ends with an underscore"),
    c("BL0130", "consecutive underscores in a numeric literal"),
    c("BL0131", "unterminated block comment"),
    c("BL0132", "invalid documentation comment"),
    c("BL0133", "invalid character in an identifier"),
    c("BL0134", "expected an identifier after `r#`"),
    c("BL0135", "name cannot be a raw identifier"),
    c("BL0136", "expected a lifetime name"),
    c("BL0137", "invalid lifetime name"),
    c("BL0138", "mismatched DSL block delimiters"),
    c("BL0139", "unterminated DSL block"),
    c("BL0140", "unterminated format string"),
    c("BL0141", "unclosed interpolation in a format string"),
    c("BL0142", "empty interpolation in a format string"),
    c("BL0143", "interpolation nested too deep"),
    c("BL0144", "invalid format specifier"),
    c("BL0145", "shebang not at the start of the file"),
    c("BL0146", "invalid UTF-8 in source"),
    c("BL0147", "NUL character in source"),
    c("BL0148", "non-ASCII character in a byte literal"),
    c("BL0199", "internal lexer error"),
    c("BL0201", "unexpected token"),
    c("BL0202", "expected a different token"),
    c("BL0203", "unexpected end of file while parsing"),
    c("BL0204", "invalid expression"),
    c("BL0205", "integer literal too large for the parser"),
    c("BL0206", "invalid left-hand side of an assignment"),
    c("BL0207", "missing operand"),
    c("BL0208", "unclosed delimiter"),
    c("BL0209", "mismatched delimiter"),
    c("BL0210", "invalid statement"),
    c("BL0211", "missing semicolon"),
    c("BL0212", "invalid item declaration"),
    c("BL0213", "duplicate modifier"),
    c("BL0214", "conflicting modifiers"),
    c("BL0215", "invalid type"),
    c("BL0216", "expected a type"),
    c("BL0217", "invalid unit annotation"),
    c("BL0218", "invalid pattern"),
    c("BL0219", "expected a pattern"),
    c("BL0220", "rest pattern not last"),
    c("BL0221", "invalid generic parameter"),
    c("BL0222", "invalid where clause"),
    c("BL0223", "invalid attribute"),
    c("BL0224", "inner attribute not allowed here"),
    c("BL0225", "invalid macro invocation"),
    c("BL0226", "lexer error reported by the parser"),
    c("BL0299", "internal parser error"),
    c("BL0301", "type mismatch"),
    c("BL0302", "infinite type"),
    c("BL0303", "mutability mismatch"),
    c("BL0304", "array length mismatch"),
    c("BL0305", "wrong number of arguments"),
    c("BL0306", "unsafety mismatch"),
    c("BL0307", "lifetime mismatch"),
    c("BL0308", "ABI mismatch"),
    c("BL0310", "undefined variable"),
    c("BL0311", "undefined type"),
    c("BL0312", "undefined function"),
    c("BL0313", "undefined field"),
    c("BL0314", "undefined method"),
    c("BL0315", "undefined enum variant"),
    c("BL0320", "value is not callable"),
    c("BL0321", "no overload matches the argument types"),
    c("BL0322", "ambiguous overloaded call"),
    c("BL0323", "overloaded function used as a value"),
    c("BL0324", "value cannot be indexed"),
    c("BL0325", "value cannot be dereferenced"),
    c("BL0326", "value cannot be used with `?`"),
    c("BL0327", "value cannot be awaited"),
    c("BL0328", "invalid binary operator for these types"),
    c("BL0329", "invalid unary operator for this type"),
    c("BL0330", "integer literal out of range for its type"),
    c("BL0331", "integer overflow in a constant expression"),
    c("BL0332", "invalid assignment target"),
    c("BL0333", "assignment to an immutable variable"),
    c("BL0340", "pattern type mismatch"),
    c("BL0341", "refutable pattern in an irrefutable position"),
    c("BL0342", "non-exhaustive patterns"),
    c("BL0343", "non-exhaustive match: missing enum variants"),
    c("BL0350", "`break` outside a loop"),
    c("BL0351", "`continue` outside a loop"),
    c("BL0352", "undefined loop label"),
    c("BL0353", "`return` outside a function"),
    c("BL0354", "return type mismatch"),
    c("BL0360", "trait not implemented"),
    c("BL0361", "ambiguous trait implementation"),
    c("BL0362", "associated type not found in the trait"),
    c("BL0363", "associated type not defined in the impl"),
    c("BL0370", "wrong number of type arguments"),
    c("BL0371", "trait bound not satisfied"),
    c("BL0380", "duplicate definition"),
    c("BL0381", "type annotation needed"),
    c("BL0382", "unsafe operation outside an `unsafe` block"),
    c("BL0383", "construct not supported yet"),
    c("BL0384", "recursive type has infinite size"),
    c("BL0398", "internal checker error"),
    c("BL0399", "internal type error"),
    c("BL0401", "unknown effect"),
    c("BL0402", "effect performed but not declared"),
    c("BL0403", "undeclared effect in the function body"),
    c("BL0404", "unknown effect operation"),
    c("BL0405", "handler missing an operation"),
    c("BL0501", "mutable borrow while already borrowed"),
    c("BL0502", "two mutable borrows at once"),
    c("BL0503", "use of a moved value"),
    c("BL0504", "reference to a local escapes its scope"),
    c("BL0510", "linear value used after it was consumed"),
    c("BL0511", "linear value borrowed after it was consumed"),
    c("BL0512", "linear value moved out of a shared borrow"),
    c("BL0513", "linear field in a non-linear type"),
    c("BL0514", "linear value in an untracked position"),
    c("BL0601", "unit mismatch"),
    c("BL0602", "unit mismatch in an operation"),
    c("BL0610", "module import cycle"),
    c("BL0620", "formatting macro needs a string literal first"),
    c("BL0901", "the C backend cannot compile the program"),
];

/// Look up a code in the published table.
pub fn lookup(code: &str) -> Option<&'static CodeInfo> {
    let wanted = code.trim().to_ascii_uppercase();
    CODES.iter().find(|info| info.code == wanted)
}
