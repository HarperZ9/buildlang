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

use crate::codegen::ir::{
    LocalId, MirFunction, MirModule, MirRValue, MirStmtKind, MirTerminator, MirType, MirValue,
};
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

fn named_local(func: &MirFunction, name: &str) -> LocalId {
    func.locals
        .iter()
        .find(|local| local.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("expected `{name}` local in {}", func.name))
        .id
}

fn local_type(func: &MirFunction, id: LocalId) -> MirType {
    func.locals
        .iter()
        .find(|local| local.id == id)
        .unwrap_or_else(|| panic!("expected local {id} in {}", func.name))
        .ty
        .clone()
}

fn assert_named_local_assigned_from_local_of_type(
    func: &MirFunction,
    name: &str,
    expected_ty: MirType,
) {
    let dest = named_local(func, name);
    let mut saw_assignment = false;

    for block in func.blocks.as_ref().expect("function should have blocks") {
        for stmt in &block.stmts {
            if let MirStmtKind::Assign {
                dest: stmt_dest,
                value: MirRValue::Use(MirValue::Local(rhs)),
            } = &stmt.kind
            {
                if *stmt_dest == dest {
                    let rhs_ty = local_type(func, *rhs);
                    assert_eq!(
                        rhs_ty, expected_ty,
                        "assignment into `{name}` must use the match expression's arm value type"
                    );
                    saw_assignment = true;
                }
            }
        }
    }

    assert!(
        saw_assignment,
        "expected `{name}` to be assigned from a lowered match result local"
    );
}

fn assert_tail_return_local_of_type(func: &MirFunction, expected_ty: MirType) {
    let mut saw_return = false;

    for block in func.blocks.as_ref().expect("function should have blocks") {
        if let Some(MirTerminator::Return(Some(MirValue::Local(local)))) = &block.terminator {
            let return_ty = local_type(func, *local);
            assert_eq!(
                return_ty, expected_ty,
                "tail match return must use the function result type"
            );
            saw_return = true;
        }
    }

    assert!(
        saw_return,
        "expected function to return a lowered match result local"
    );
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

// =============================================================================
// RUNTIME SUM-TYPE MATCH RESULT TYPES
// =============================================================================

#[test]
fn runtime_option_match_assignment_rhs_uses_payload_type_inside_option_returning_fn() {
    let module = lower_source(
        r#"
fn probe(input: u64) -> Option<u64> {
    let mut value = 0u64;
    value = match input.checked_mul(10u64) {
        Some(v) => v,
        None => { return None; 0u64 },
    };
    Some(value)
}
"#,
    );
    let probe = find_fn_def(&module, "probe");

    assert_named_local_assigned_from_local_of_type(probe, "value", MirType::u64());
}

#[test]
fn runtime_option_match_reversed_arms_uses_match_value_type_not_payload_type() {
    let module = lower_source(
        r#"
fn probe(input: u64) -> Option<u64> {
    let mut flag = false;
    flag = match input.checked_mul(10u64) {
        None => { return None; false },
        Some(v) => v > 0u64,
    };
    if flag {
        Some(input)
    } else {
        None
    }
}
"#,
    );
    let probe = find_fn_def(&module, "probe");

    assert_named_local_assigned_from_local_of_type(probe, "flag", MirType::Bool);
}

#[test]
fn runtime_option_match_retypes_from_nondiverging_arm_after_diverging_arm() {
    let module = lower_source(
        r#"
fn probe(input: u64) -> Option<u64> {
    let mut value = 0u64;
    value = match input.checked_mul(10u64) {
        None => return None,
        Some(v) => v,
    };
    Some(value)
}
"#,
    );
    let probe = find_fn_def(&module, "probe");

    assert_named_local_assigned_from_local_of_type(probe, "value", MirType::u64());
}

#[test]
fn runtime_option_tail_match_keeps_option_result_type() {
    let module = lower_source(
        r#"
fn probe(input: u64) -> Option<u64> {
    match input.checked_mul(10u64) {
        Some(v) => Some(v),
        None => None,
    }
}
"#,
    );
    let probe = find_fn_def(&module, "probe");

    assert_tail_return_local_of_type(probe, MirType::Option(Box::new(MirType::u64())));
}

#[test]
fn runtime_result_match_assignment_rhs_uses_ok_payload_type_inside_result_returning_fn() {
    let module = lower_source(
        r#"
enum Result {
    Ok(u64),
    Err(u64),
}

fn maybe(input: u64) -> Result {
    if input > 0u64 {
        Result::Ok(input)
    } else {
        Result::Err(1u64)
    }
}

fn probe(input: u64) -> Result {
    let mut value = 0u64;
    value = match maybe(input) {
        Result::Ok(v) => v,
        Result::Err(e) => { return Result::Err(e); 0u64 },
    };
    Result::Ok(value)
}
"#,
    );
    let probe = find_fn_def(&module, "probe");

    assert_named_local_assigned_from_local_of_type(probe, "value", MirType::u64());
}

#[test]
fn runtime_result_match_retypes_from_nondiverging_arm_after_diverging_arm() {
    let module = lower_source(
        r#"
enum Result {
    Ok(u64),
    Err(u64),
}

fn maybe(input: u64) -> Result {
    if input > 0u64 {
        Result::Ok(input)
    } else {
        Result::Err(1u64)
    }
}

fn probe(input: u64) -> Result {
    let mut value = 0u64;
    value = match maybe(input) {
        Result::Err(e) => return Result::Err(e),
        Result::Ok(v) => v,
    };
    Result::Ok(value)
}
"#,
    );
    let probe = find_fn_def(&module, "probe");

    assert_named_local_assigned_from_local_of_type(probe, "value", MirType::u64());
}

#[test]
fn runtime_result_tail_match_keeps_result_type() {
    let module = lower_source(
        r#"
enum Result {
    Ok(u64),
    Err(u64),
}

fn maybe(input: u64) -> Result {
    if input > 0u64 {
        Result::Ok(input)
    } else {
        Result::Err(1u64)
    }
}

fn probe(input: u64) -> Result {
    match maybe(input) {
        Result::Ok(v) => Result::Ok(v),
        Result::Err(e) => Result::Err(e),
    }
}
"#,
    );
    let probe = find_fn_def(&module, "probe");

    assert_tail_return_local_of_type(probe, MirType::Struct("Result".into()));
}

#[test]
fn runtime_result_match_reversed_arms_uses_match_value_type_not_result_type() {
    let module = lower_source(
        r#"
enum Result {
    Ok(u64),
    Err(u64),
}

fn maybe(input: u64) -> Result {
    if input > 0u64 {
        Result::Ok(input)
    } else {
        Result::Err(1u64)
    }
}

fn probe(input: u64) -> Result {
    let mut flag = false;
    flag = match maybe(input) {
        Result::Err(e) => { return Result::Err(e); false },
        Result::Ok(v) => v > 0u64,
    };
    if flag {
        Result::Ok(input)
    } else {
        Result::Err(0u64)
    }
}
"#,
    );
    let probe = find_fn_def(&module, "probe");

    assert_named_local_assigned_from_local_of_type(probe, "flag", MirType::Bool);
}
