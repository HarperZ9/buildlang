from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "docs" / "index.html"
STYLES = ROOT / "docs" / "styles.css"


def page_source() -> str:
    return INDEX.read_text(encoding="utf-8") + "\n" + STYLES.read_text(encoding="utf-8")


def index_source() -> str:
    return INDEX.read_text(encoding="utf-8")


def test_docs_page_uses_portfolio_glass_language() -> None:
    source = page_source()

    assert '<link rel="stylesheet" href="styles.css?v=20260615">' in source
    assert "--olive-wash:#c9d6a3" in source
    assert "--glass-blur:saturate(170%) blur(30px)" in source
    assert ".grain{display:block" in source
    assert ".glass{background:var(--gloss), var(--glass-base)" in source
    assert "scroll-margin-top:5rem" in source


def test_backend_maturity_claims_are_precise() -> None:
    source = index_source()

    assert "C is the test-backed execution path" in source
    assert "HLSL and GLSL shader output are working" in source
    assert "Other backends are research surfaces" in source
    assert "Rust, LLVM IR, WebAssembly, SPIR-V, x86-64, and ARM64 are wired" in source
    assert "verified by corpus and tests" in source
    assert "eight production targets" not in source.lower()
    assert "one source, eight compile targets" not in source.lower()
    assert "same verified promise as C today" not in source


def test_current_progress_evidence_is_visible() -> None:
    source = index_source()

    assert "868 passing compiler tests" in source
    assert "192 CLI tests" in source
    assert "8-program semantic corpus" in source
    assert "quantac corpus verify" in source
    assert "quantac doctor" in source
    assert "quantac run examples/quickstart/effects_greeting.quanta" in source
    assert "QuantaLang is a compiler project" in source
    assert "Today, the C path verified by corpus and tests compiles .quanta programs to C" in source
    assert "runs them through a native compiler" in source
    assert "also emits HLSL/GLSL shader code" in source
    assert "Other backends exist as research surfaces" in source
    assert "What works today" in source
    assert "Run the compiler path" in source
    assert "The root README, STATUS, and TEST_RESULTS files are the factual anchors" in source
    assert "A compiler you can run, with the receipts close by" not in source
    assert "A compiler with receipts" not in source
    assert "The strongest path today is concrete" not in source
    assert "What you can trust today" not in source


def test_immediate_user_value_is_explicit() -> None:
    source = index_source()

    assert "Immediate value as of June 15, 2026" in source
    assert "Use it today to run compiler examples, inspect source-to-C lowering, emit shader code, and see exactly which backend claims are backed by tests." in source
    assert "Who uses it" in source
    assert "language designers, compiler engineers, graphics programmers, and technical reviewers" in source
    assert "What it does not claim" in source
    assert "It is not presented as a production language ecosystem or as eight equally mature backends." in source


def test_compiler_workflow_and_capabilities_are_plainly_explained() -> None:
    source = index_source()

    assert "How the compiler works" in source
    assert "source moves through lexer, parser, type checker, MIR, and backends" in source
    assert "Algebraic effects" in source
    assert "vignette_shader.quanta" in source
    assert "C99" in source
    assert "VS Code extension" in source
    assert "Quanta Universe" in source
