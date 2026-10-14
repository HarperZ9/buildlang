//! Tests for stable codes, help wiring and suggestion classification.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::codes::{self, CODES};
use super::*;
use crate::lexer::{BytePos, LexerErrorKind, SourceFile};
use crate::parser::ParseErrorKind;
use crate::types::{Lifetime, Mutability, TraitId, Ty, TyVarId, TypeError};

fn s(x: &str) -> String {
    x.to_string()
}

/// One value of every `TypeError` variant. `type_code` is an exhaustive
/// match, so a new variant fails to compile there; this list then fails the
/// registry test below until the new variant is sampled here too.
fn type_samples() -> Vec<TypeError> {
    use TypeError as E;
    let t = Ty::bool;
    vec![
        E::TypeMismatch {
            expected: t(),
            found: Ty::unit(),
        },
        E::InfiniteType {
            var: TyVarId(0),
            ty: t(),
        },
        E::MutabilityMismatch {
            expected: Mutability::Mutable,
            found: Mutability::Immutable,
        },
        E::ArrayLengthMismatch {
            expected: 1,
            found: 2,
        },
        E::ArityMismatch {
            expected: 1,
            found: 2,
        },
        E::UnsafetyMismatch,
        E::LifetimeMismatch {
            expected: Lifetime::static_lifetime(),
            found: Lifetime::anonymous(),
        },
        E::AbiMismatch {
            expected: Arc::from("C"),
            found: Arc::from("Rust"),
        },
        E::UndefinedVariable { name: s("x") },
        E::UndefinedType { name: s("T") },
        E::UndefinedFunction { name: s("f") },
        E::UndefinedField {
            ty: t(),
            field: s("a"),
        },
        E::UndefinedMethod {
            ty: t(),
            method: s("m"),
        },
        E::UndefinedVariant {
            enum_name: s("E"),
            variant: s("V"),
        },
        E::NotCallable { ty: t() },
        E::NoMatchingMethod {
            name: s("f"),
            arg_tys: s("i32"),
            candidates: s("f(str)"),
        },
        E::AmbiguousMethod {
            name: s("f"),
            arg_tys: s("i32"),
            candidates: s("f(i32)"),
        },
        E::AmbiguousFunctionReference {
            name: s("f"),
            count: 2,
        },
        E::NotIndexable { ty: t() },
        E::NotDereferenceable { ty: t() },
        E::NotTryable { ty: t() },
        E::NotAwaitable { ty: t() },
        E::AlreadyBorrowed { variable: s("v") },
        E::DoubleMutableBorrow { variable: s("v") },
        E::UseAfterMove { variable: s("v") },
        E::ReferenceEscapesScope { variable: s("v") },
        E::LinearUseAfterMove { name: s("q") },
        E::LinearBorrowAfterMove { name: s("q") },
        E::LinearMoveOutOfBorrow { name: s("q") },
        E::LinearFieldInNonLinearType {
            container: s("C"),
            field: s("f"),
            field_type: s("Q"),
        },
        E::LinearInUnsupportedPosition {
            context: s("a tuple"),
        },
        E::InvalidBinaryOp {
            op: s("+"),
            left: t(),
            right: Ty::unit(),
        },
        E::InvalidUnaryOp {
            op: s("-"),
            ty: t(),
        },
        E::IntegerLiteralOutOfRange {
            literal: s("300"),
            ty: s("u8"),
        },
        E::IntegerOverflow {
            op: s("add"),
            ty: s("i32"),
        },
        E::InvalidAssignTarget,
        E::ImmutableAssignment { name: s("x") },
        E::PatternMismatch {
            expected: t(),
            found: Ty::unit(),
        },
        E::RefutablePattern,
        E::NonExhaustivePatterns,
        E::NonExhaustiveMatch {
            missing_variants: vec![s("B")],
        },
        E::BreakOutsideLoop,
        E::ContinueOutsideLoop,
        E::UndefinedLoopLabel { label: s("outer") },
        E::ReturnOutsideFunction,
        E::ReturnTypeMismatch {
            expected: t(),
            found: Ty::unit(),
        },
        E::TraitNotImplemented {
            ty: t(),
            trait_id: TraitId(0),
        },
        E::AmbiguousImpl {
            trait_name: s("Show"),
            ty: t(),
        },
        E::AssocTypeNotFound {
            name: s("Item"),
            trait_name: s("Iter"),
        },
        E::AssociatedTypeNotDefined {
            assoc_name: s("Item"),
        },
        E::InternalError(s("boom")),
        E::UnsupportedConstruct {
            construct: s("yield"),
            detail: s("not lowered"),
        },
        E::WrongTypeArgCount {
            expected: 1,
            found: 2,
        },
        E::BoundNotSatisfied {
            ty: t(),
            bound: s("Ord"),
        },
        E::DuplicateDefinition { name: s("f") },
        E::TypeAnnotationNeeded,
        E::UnsafeOutsideUnsafe,
        E::Internal(s("boom")),
        E::UnknownEffect { name: s("Logger") },
        E::UnhandledEffect {
            func_name: s("main"),
            effect_name: s("Console"),
        },
        E::UndeclaredEffect {
            func_name: s("main"),
            effect_name: s("FileSystem"),
            declared_effects: vec![s("Console")],
        },
        E::UnknownEffectOperation {
            effect_name: s("Logger"),
            operation: s("log"),
        },
        E::MissingHandlerClause {
            effect_name: s("Logger"),
            operation: s("log"),
        },
        E::UnitMismatch {
            expected: s("m"),
            found: s("s"),
        },
        E::UnitOperationMismatch {
            operation: "add",
            left: s("m"),
            right: s("s"),
        },
        E::ModuleImportCycle {
            path: s("a.bld"),
            module: s("b"),
        },
        E::FormatArgNotStringLiteral {
            macro_name: s("println"),
            found: s("an expression"),
        },
        E::FormatArgNotStringLiteral {
            macro_name: s("println"),
            found: s("a raw string literal"),
        },
        E::BackendRejected {
            message: s("cannot compile"),
        },
    ]
}

fn parse_samples() -> Vec<ParseErrorKind> {
    use ParseErrorKind as K;
    vec![
        K::UnexpectedToken { found: s("+") },
        K::Expected {
            expected: s("`;`"),
            found: s("`}`"),
        },
        K::UnexpectedEof,
        K::InvalidExpression,
        K::IntegerLiteralTooLarge { literal: s("9") },
        K::InvalidAssignTarget,
        K::MissingOperand,
        K::UnclosedDelimiter {
            delimiter: s("`(`"),
        },
        K::MismatchedDelimiter {
            expected: s("`)`"),
            found: s("`]`"),
        },
        K::InvalidStatement,
        K::ExpectedSemicolon,
        K::InvalidItem,
        K::DuplicateModifier { modifier: s("pub") },
        K::ConflictingModifiers {
            first: s("a"),
            second: s("b"),
        },
        K::InvalidType,
        K::ExpectedType,
        K::InvalidUnitAnnotation(s("bad unit")),
        K::InvalidPattern,
        K::ExpectedPattern,
        K::RestPatternNotLast,
        K::InvalidGenericParam,
        K::InvalidWhereClause,
        K::InvalidAttribute,
        K::InnerAttributeNotAllowed,
        K::InvalidMacroInvocation,
        K::LexerError(s("bad")),
        K::Internal(s("boom")),
    ]
}

fn lexer_samples() -> Vec<LexerErrorKind> {
    use LexerErrorKind as K;
    vec![
        K::UnexpectedChar('$'),
        K::UnexpectedEof,
        K::UnterminatedString,
        K::UnterminatedChar,
        K::UnterminatedByteString,
        K::UnterminatedRawString { expected_hashes: 1 },
        K::EmptyCharLiteral,
        K::MultipleCharsInCharLiteral,
        K::UnknownEscape('q'),
        K::InvalidUnicodeEscape,
        K::UnicodeEscapeMissingBrace,
        K::UnicodeEscapeUnclosed,
        K::UnicodeEscapeTooLong,
        K::InvalidUnicodeCodepoint(0xD800),
        K::InvalidHexEscape,
        K::HexEscapeOutOfRange(0x80),
        K::EscapeAtEof,
        K::InvalidNumber,
        K::IntegerOverflow,
        K::FloatOverflow,
        K::EmptyExponent,
        K::NoDigitsAfterPrefix(s("0x")),
        K::InvalidDigit('9', 8),
        K::FloatWithBase(16),
        K::InvalidNumericSuffix(s("q")),
        K::IntSuffixOnFloat(s("i32")),
        K::FloatSuffixOnNonDecimal(s("f64")),
        K::LeadingUnderscore,
        K::TrailingUnderscore,
        K::ConsecutiveUnderscores,
        K::UnterminatedBlockComment { depth: 1 },
        K::InvalidDocComment,
        K::InvalidIdentChar('@'),
        K::ExpectedRawIdent,
        K::CannotBeRawIdent(s("self")),
        K::ExpectedLifetime,
        K::InvalidLifetime,
        K::MismatchedDslDelimiters,
        K::UnterminatedDslBlock,
        K::UnterminatedFormatString,
        K::UnclosedInterpolation,
        K::EmptyInterpolation,
        K::InterpolationTooDeep(8),
        K::InvalidFormatSpecifier(s("q")),
        K::ShebangNotAtStart,
        K::InvalidUtf8,
        K::NulInSource,
        K::NonAsciiInByteLiteral,
        K::Internal(s("boom")),
    ]
}

/// Every code produced by a sample, plus the driver codes.
fn produced_codes() -> Vec<&'static str> {
    let mut out = vec![
        codes::IO_ERROR,
        codes::INPUT_GRAPH_ERROR,
        codes::POLICY_VIOLATION,
    ];
    let mut seen_types = std::collections::HashSet::new();
    for e in type_samples() {
        // FormatArgNotStringLiteral is sampled twice (two suggestion texts).
        if seen_types.insert(std::mem::discriminant(&e)) {
            out.push(codes::type_code(&e));
        }
    }
    out.extend(parse_samples().iter().map(codes::parse_code));
    out.extend(lexer_samples().iter().map(codes::lexer_code));
    out
}

#[test]
fn every_error_kind_has_a_distinct_registered_code() {
    let produced = produced_codes();
    let unique: BTreeSet<_> = produced.iter().copied().collect();
    assert_eq!(unique.len(), produced.len(), "two error kinds share a code");

    let registered: BTreeSet<_> = CODES.iter().map(|i| i.code).collect();
    assert_eq!(
        registered.len(),
        CODES.len(),
        "the code table lists a code twice"
    );
    assert_eq!(
        unique, registered,
        "the code table and the error kinds disagree"
    );

    let mut sorted = CODES.iter().map(|i| i.code).collect::<Vec<_>>();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        CODES.iter().map(|i| i.code).collect::<Vec<_>>(),
        "table not sorted"
    );
    for info in CODES {
        assert!(
            info.code.len() == 6 && info.code.starts_with("BL"),
            "{}",
            info.code
        );
        assert!(
            info.code[2..].chars().all(|c| c.is_ascii_digit()),
            "{}",
            info.code
        );
        assert!(!info.title.is_empty());
    }
}

#[test]
fn codes_are_pinned() {
    // A published code never changes meaning. Spot-check one per range.
    assert_eq!(codes::type_code(&type_samples()[0]), "BL0301");
    assert_eq!(
        codes::parse_code(&ParseErrorKind::ExpectedSemicolon),
        "BL0211"
    );
    assert_eq!(
        codes::lexer_code(&LexerErrorKind::UnterminatedString),
        "BL0103"
    );
    let undeclared = type_samples()
        .into_iter()
        .find(|e| matches!(e, TypeError::UndeclaredEffect { .. }))
        .unwrap();
    assert_eq!(codes::type_code(&undeclared), "BL0403");
    assert_eq!(codes::lookup("bl0403").map(|i| i.code), Some("BL0403"));
    assert!(codes::lookup("BL9999").is_none());
}

#[test]
fn error_codes_doc_lists_the_table() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/ERROR_CODES.md");
    let doc = std::fs::read_to_string(path).expect("docs/ERROR_CODES.md exists");
    for info in CODES {
        let row = format!("| {} | {} |", info.code, info.title);
        assert!(
            doc.contains(&row),
            "docs/ERROR_CODES.md is missing the row `{row}`"
        );
    }
    let rows = doc.lines().filter(|l| l.starts_with("| BL")).count();
    assert_eq!(
        rows,
        CODES.len(),
        "docs/ERROR_CODES.md has rows for codes that do not exist"
    );
}

fn file() -> SourceFile {
    SourceFile::new("t.bld", "fn main() {\n    let x = 1;\n}\n".to_string())
}

fn span() -> Span {
    Span::new(BytePos(16), BytePos(21), file().id)
}

#[test]
fn every_type_suggestion_reaches_human_and_json_output() {
    let f = file();
    let mut with_suggestion = 0;
    for e in type_samples() {
        let Some(text) = e.suggestion() else { continue };
        with_suggestion += 1;
        let err = TypeErrorWithSpan::new(e, span());
        let human = err.to_string();
        assert!(
            human.contains(&format!("help: {text}")),
            "human output lacks: {text}\n{human}"
        );
        let d = Diagnostic::from_type(&err, &f);
        assert!(d.help.contains(&text), "JSON help lacks: {text}");
        assert!(
            d.suggestions.iter().any(|s| s.message == text),
            "JSON suggestion lacks: {text}"
        );
    }
    assert!(
        with_suggestion >= 12,
        "suggestion texts went missing: {with_suggestion}"
    );
}

#[test]
fn every_parse_and_lexer_suggestion_reaches_human_and_json_output() {
    let f = file();
    for kind in parse_samples() {
        let Some(text) = kind.suggestion() else {
            continue;
        };
        let err = crate::parser::ParseError::new(kind, span());
        assert!(err.to_string().contains(&format!("help: {text}")));
        assert!(Diagnostic::from_parse(&err, &f)
            .help
            .iter()
            .any(|h| h == text));
    }
    for kind in lexer_samples() {
        let Some(text) = kind.help() else { continue };
        let err = crate::lexer::LexerError::new(kind, span());
        assert!(err.to_string().contains(&format!("help: {text}")));
        assert!(Diagnostic::from_lexer(&err, &f)
            .help
            .iter()
            .any(|h| h == text));
    }
}

#[test]
fn explicit_help_and_identical_suggestion_print_once() {
    let e = TypeError::NonExhaustivePatterns;
    let text = e.suggestion().unwrap();
    let err = TypeErrorWithSpan::new(e, span()).with_help(text.clone());
    assert_eq!(err.to_string().matches(&text).count(), 1);
    assert_eq!(help_lines(Some("a"), Some("a")), vec![s("a")]);
    assert_eq!(help_lines(Some("a"), Some("b")), vec![s("a"), s("b")]);
    assert!(help_lines(None, None).is_empty());
}

#[test]
fn authority_widening_fix_is_never_machine_applicable() {
    let widened = Suggestion::new("add `~ Network`", Applicability::MachineApplicable, true);
    assert_eq!(widened.applicability, Applicability::MaybeIncorrect);
    assert!(widened.widens_authority);
    let plain = Suggestion::new("add `;`", Applicability::MachineApplicable, false);
    assert_eq!(plain.applicability, Applicability::MachineApplicable);

    let f = file();
    for e in type_samples() {
        let code = codes::type_code(&e);
        let err = TypeErrorWithSpan::new(e, span());
        for sug in Diagnostic::from_type(&err, &f).suggestions {
            if matches!(code, "BL0401" | "BL0402" | "BL0403" | "BL0382") {
                assert!(sug.widens_authority, "{code}: {}", sug.message);
            }
            if sug.widens_authority {
                assert_ne!(sug.applicability, Applicability::MachineApplicable);
            }
        }
    }
}

#[test]
fn effect_row_suggestion_uses_plus() {
    let e = TypeError::UndeclaredEffect {
        func_name: s("main"),
        effect_name: s("FileSystem"),
        declared_effects: vec![s("Console"), s("Clock")],
    };
    let text = e.suggestion().unwrap();
    assert!(text.contains("`~ Console + Clock + FileSystem`"), "{text}");
    assert!(!text.contains(", "), "{text}");
}

#[test]
fn span_resolves_to_line_and_byte_column() {
    let f = file();
    let d = DiagnosticSpan::resolve(span(), &f).unwrap();
    assert_eq!((d.line, d.column, d.end_line, d.end_column), (2, 5, 2, 10));
    assert_eq!((d.byte_start, d.byte_end), (16, 21));
    assert!(DiagnosticSpan::resolve(Span::new(BytePos(0), BytePos(0), f.id), &f).is_none());
}

#[test]
fn report_shape() {
    let r = Report::new(
        "t.bld",
        vec![Diagnostic::bare(codes::IO_ERROR, "t.bld", "gone")],
    );
    let v: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
    assert_eq!(v["schema"], SCHEMA_ID);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error_count"], 1);
    assert_eq!(v["diagnostics"][0]["code"], "BL0001");
    assert_eq!(v["diagnostics"][0]["severity"], "error");
    assert!(v["diagnostics"][0]["span"].is_null());
    let ok = Report::new("t.bld", Vec::new());
    assert!(ok.ok && ok.error_count == 0);
}
