// ===============================================================================
// BUILDLANG CODE GENERATOR - LOWERING GROUNDWORK TESTS (linear annotations + spans)
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Unit tests for the MIR-phase linear checker groundwork (sub-brick 2a):
//! - `"linear"` tags stamped onto `MirLocal.annotations` for `#[linear]`-typed
//!   locals (params, `let`-bindings, and compiler temps).
//! - Source spans recorded into `MirFunction.spans` at statement/terminator
//!   emission.
//!
//! Nothing consumes these facts yet (that is sub-brick 2b); these tests only
//! assert the groundwork is present and correct.

use std::sync::Arc;

use crate::codegen::ir::MirModule;
use crate::codegen::lower::MirLowerer;
use crate::lexer::{Lexer, SourceFile};
use crate::parser::Parser;
use crate::types::{TypeChecker, TypeContext};

/// Parse, type-check, and lower `source` to MIR. Panics with a descriptive
/// message on any parse/check/lowering failure so test failures are legible.
fn lower_source(source: &str) -> MirModule {
    let source_file = SourceFile::new("lower_tests.bld", source);
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().expect("lexing should succeed");
    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse().expect("parsing should succeed");
    assert!(
        parser.errors().is_empty(),
        "unexpected parser errors: {:?}",
        parser.errors()
    );

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.check_module(&ast);
    assert!(
        !checker.has_errors(),
        "unexpected type errors: {:?}",
        checker.errors()
    );

    MirLowerer::with_source(&ctx, Arc::from(source_file.source()))
        .lower_module(&ast)
        .expect("lowering should succeed")
}

/// Find a function *definition* (not a forward-declaration stub) by name.
/// `collect_function` registers a declaration-only stub for every function
/// during the collection pass (so return types resolve before bodies are
/// lowered), and the real definition is appended later -- so `module.functions`
/// legitimately contains two entries named the same for any non-`main`
/// function. Tests must pick the definition.
fn find_fn_def<'a>(module: &'a MirModule, name: &str) -> &'a crate::codegen::ir::MirFunction {
    module
        .functions
        .iter()
        .find(|f| &*f.name == name && !f.is_declaration())
        .unwrap_or_else(|| panic!("expected a lowered definition for `{name}`"))
}

// =============================================================================
// LINEARITY ANNOTATIONS
// =============================================================================

/// A `#[linear]` struct plus helpers exercising all three local-creation
/// shapes the spec requires tagging: a parameter (`consume`'s `q`), a
/// `let`-binding (`main`'s `q`), and a compiler temp (the call-result of
/// `make()` before it is bound by the `let`).
const LINEAR_PROGRAM: &str = "#[linear]\nstruct Qubit { id: i64 }\n\
     fn make() -> Qubit { Qubit { id: 1 } }\n\
     fn consume(q: Qubit) -> i64 { q.id }\n\
     fn main() ~ Console { let q = make(); let a = consume(q); println(\"{}\", a); }\n";

#[test]
fn linear_param_local_is_tagged() {
    let module = lower_source(LINEAR_PROGRAM);
    let consume = find_fn_def(&module, "consume");
    let param = consume
        .locals
        .iter()
        .find(|l| l.is_param)
        .expect("consume should have a parameter local");
    assert!(
        param.annotations.iter().any(|a| a.as_ref() == "linear"),
        "linear parameter local must carry the \"linear\" annotation: {:?}",
        param.annotations
    );
}

#[test]
fn linear_let_binding_local_is_tagged() {
    let module = lower_source(LINEAR_PROGRAM);
    let main = find_fn_def(&module, "main");
    let q_local = main
        .locals
        .iter()
        .find(|l| l.name.as_deref() == Some("q"))
        .expect("main should have a `q` local for the let-binding");
    assert!(
        q_local.annotations.iter().any(|a| a.as_ref() == "linear"),
        "linear let-binding local must carry the \"linear\" annotation: {:?}",
        q_local.annotations
    );
}

#[test]
fn linear_temp_local_is_tagged() {
    let module = lower_source(LINEAR_PROGRAM);
    let main = find_fn_def(&module, "main");
    // Every unnamed local of type Qubit is a compiler temp; at least one must
    // exist (the call-result of `make()`) and must carry the annotation.
    let linear_temps: Vec<_> = main
        .locals
        .iter()
        .filter(|l| {
            l.name.is_none()
                && matches!(&l.ty, crate::codegen::ir::MirType::Struct(n) if n.as_ref() == "Qubit")
        })
        .collect();
    assert!(
        !linear_temps.is_empty(),
        "expected at least one unnamed Qubit temp in main: {:?}",
        main.locals
    );
    assert!(
        linear_temps
            .iter()
            .all(|l| l.annotations.iter().any(|a| a.as_ref() == "linear")),
        "every linear temp local must carry the \"linear\" annotation: {:?}",
        linear_temps
    );
}

#[test]
fn non_linear_local_is_not_tagged() {
    let module = lower_source(LINEAR_PROGRAM);
    let consume = find_fn_def(&module, "consume");
    // `consume`'s return value (i64) must never be tagged "linear".
    for local in &consume.locals {
        if matches!(local.ty, crate::codegen::ir::MirType::Int(_, _)) {
            assert!(
                !local.annotations.iter().any(|a| a.as_ref() == "linear"),
                "a non-linear (i64) local must not carry the \"linear\" annotation: {:?}",
                local
            );
        }
    }
}

#[test]
fn ordinary_struct_local_is_not_tagged() {
    let module = lower_source(
        "struct Coin { value: i64 }\n\
         fn spend(c: Coin) -> i64 { c.value }\n\
         fn main() ~ Console { let coin = Coin { value: 1 }; \
         let a = spend(coin); println(\"{}\", a); }\n",
    );
    let main = find_fn_def(&module, "main");
    let coin_local = main
        .locals
        .iter()
        .find(|l| l.name.as_deref() == Some("coin"))
        .expect("main should have a `coin` local");
    assert!(
        !coin_local
            .annotations
            .iter()
            .any(|a| a.as_ref() == "linear"),
        "an ordinary (non-#[linear]) struct local must not be tagged: {:?}",
        coin_local.annotations
    );
}

// =============================================================================
// SPAN SIDE-TABLE
// =============================================================================

#[test]
fn statement_spans_match_source_text() {
    let source = "fn main() ~ Console { let x = 42; println(\"{}\", x); }\n";
    let module = lower_source(source);
    let main = find_fn_def(&module, "main");

    assert!(
        !main.spans.stmt.is_empty(),
        "expected at least one recorded statement span"
    );

    // Every recorded statement span must slice back to non-empty, in-bounds
    // source text (proves spans are real source ranges, not placeholders).
    for (&(block, idx), span) in main.spans.stmt.iter() {
        let start = span.start.to_usize();
        let end = span.end.to_usize();
        assert!(
            end <= source.len() && start <= end,
            "stmt span at block {block} idx {idx} out of bounds: {start}..{end} (source len {})",
            source.len()
        );
        assert!(
            start < end,
            "stmt span at block {block} idx {idx} must be non-empty: {start}..{end}"
        );
    }

    // At least one statement span should cover the `42` literal init.
    let covers_literal = main.spans.stmt.values().any(|span| {
        let start = span.start.to_usize();
        let end = span.end.to_usize();
        end <= source.len() && source[start..end].contains("42")
    });
    assert!(
        covers_literal,
        "expected a statement span covering the `let x = 42` initializer; spans: {:?}",
        main.spans
            .stmt
            .values()
            .map(|s| (s.start.to_usize(), s.end.to_usize()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn terminator_spans_are_recorded_for_return() {
    let source = "fn answer() -> i32 { return 42; }\n";
    let module = lower_source(source);
    let answer = find_fn_def(&module, "answer");

    assert!(
        !answer.spans.terminator.is_empty(),
        "expected at least one recorded terminator span"
    );

    for (&block, span) in answer.spans.terminator.iter() {
        let start = span.start.to_usize();
        let end = span.end.to_usize();
        assert!(
            end <= source.len() && start < end,
            "terminator span at block {block} out of bounds/empty: {start}..{end}"
        );
    }
}

#[test]
fn spans_table_is_not_populated_when_absent_by_construction() {
    // A freshly-built MirFunction (not produced by the lowerer) must have an
    // empty span table -- proves `spans` is additive/in-memory-only and not
    // silently defaulted to something non-empty.
    use crate::codegen::ir::{MirFnSig, MirFunction, MirType};
    let func = MirFunction::new("empty", MirFnSig::new(vec![], MirType::Void));
    assert!(func.spans.stmt.is_empty());
    assert!(func.spans.terminator.is_empty());
}
