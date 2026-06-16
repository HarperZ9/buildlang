from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / "docs" / "index.html"
STYLES = ROOT / "docs" / "styles.css"


def page_source() -> str:
    return INDEX.read_text(encoding="utf-8") + "\n" + STYLES.read_text(encoding="utf-8")


def test_docs_page_uses_portfolio_glass_language() -> None:
    source = page_source()

    assert '<link rel="stylesheet" href="styles.css?v=20260615">' in source
    assert "--olive-wash:#c9d6a3" in source
    assert "--glass-blur:saturate(170%) blur(30px)" in source
    assert ".grain{display:block" in source
    assert ".glass{background:var(--gloss), var(--glass-base)" in source
    assert "scroll-margin-top:5rem" in source


def test_backend_maturity_claims_are_precise() -> None:
    source = page_source()

    assert "C is the adoption path" in source
    assert "HLSL and GLSL shader output are working" in source
    assert "Research targets are labeled" in source
    assert "Rust, LLVM IR, WebAssembly, SPIR-V, x86-64, and ARM64 are wired" in source
    assert "eight production targets" not in source.lower()
    assert "one source, eight compile targets" not in source.lower()


def test_current_progress_evidence_is_visible() -> None:
    source = page_source()

    assert "868 passing compiler tests" in source
    assert "192 CLI tests" in source
    assert "8-program semantic corpus" in source
    assert "quantac corpus verify" in source
    assert "quantac doctor" in source
    assert "quantac run examples/quickstart/effects_greeting.quanta" in source
    assert "A compiler with receipts" in source
    assert "What works today" in source
    assert "Run the compiler path" in source
    assert "while new backends mature" in source
    assert "same verified promise as C today" in source
    assert "A compiler you can run, with the receipts close by" not in source
    assert "The strongest path today is concrete" not in source
    assert "What you can trust today" not in source


def test_capability_showcase_includes_real_surfaces() -> None:
    source = page_source()

    assert "Algebraic effects" in source
    assert "vignette_shader.quanta" in source
    assert "C99" in source
    assert "VS Code extension" in source
    assert "Quanta Universe" in source
