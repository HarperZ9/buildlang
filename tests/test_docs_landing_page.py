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

    assert '<link rel="stylesheet" href="styles.css?v=20260617e">' in source
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
    assert "Compiler research you can run" in source
    assert "QuantaLang is where my language, graphics, and evidence instincts meet" in source
    assert "Open the repo and run <code>quantac</code>" in source
    assert "<code>.quanta</code> to C99 is the supported native path" in source
    assert "HLSL/GLSL emit shader source" in source
    assert "the other backends are explicitly research targets" in source
    assert "What you can run today" in source
    assert "Run the compiler path" in source
    assert "The root README, STATUS, and TEST_RESULTS files are the factual anchors" in source
    assert "Start there, then decide for yourself what the evidence supports." in source
    assert "Where it is now" in source
    assert "Where it is trying to go" in source
    assert "Today, QuantaLang is a Rust-built effects compiler with a verified C execution path, working HLSL/GLSL shader-source output, typed effect receipts, policy gates, SourceId provenance, and semantic-corpus checks." in source
    assert "Long term, QuantaLang is meant to become a live-state-aware language substrate: one source shape that can coordinate CPU and GPU outputs, declare machine and model boundaries, and emit receipts that WARDEN-style membrane tooling can inspect." in source
    assert "A compiler you can run, with the receipts close by" not in source
    assert "A compiler with receipts" not in source
    assert "The strongest path today is concrete" not in source
    assert "What you can trust today" not in source


def test_immediate_user_value_is_explicit() -> None:
    source = index_source()

    assert "Evidence" in source
    assert "How to judge it" in source
    assert "Build <code>quantac</code>, run <code>hello.quanta</code>, <code>ledger.quanta</code>, and <code>effects_greeting.quanta</code>, then verify the 8-program semantic corpus. The point is not a perfect language ecosystem; it is a real compiler path with receipts nearby." in source
    assert "Compile <code>vignette_shader.quanta</code> to HLSL or GLSL when you want readable shader output from Quanta source." in source
    assert "<code>STATUS.md</code> says the self-hosted compiler and standard library exist as <code>.quanta</code> source, but cannot be compiled or executed today." in source
    assert "Immediate value as of June 15, 2026" not in source
    assert "Who uses it" not in source
    assert "What it does not claim" not in source
    assert "Public signal" not in source
    assert "Why it matters now" not in source


def test_compiler_workflow_and_capabilities_are_plainly_explained() -> None:
    source = index_source()

    assert "How the compiler works" in source
    assert "source moves through lexer, parser, type checker, MIR, and backends" in source
    assert "ambition, but with the current state kept visible" in source
    assert "Algebraic effects" in source
    assert "vignette_shader.quanta" in source
    assert "C99" in source
    assert "VS Code extension" in source
    assert "Quanta Universe" in source
    assert "part sketchbook, part research map" in source


def test_live_state_provenance_aspiration_is_explicit_without_overclaiming() -> None:
    source = index_source()

    assert "Live-state provenance substrate" in source
    assert "Machines need senses before they need more freedom." in source
    assert "Safety, transparency, and creativity can evolve on the same surface." in source
    assert "AI does not become more creative by escaping accountability." in source
    assert "Many creatives are hesitant to use AI tools" in source
    assert "That resistance is not irrational" in source
    assert "The deeper technical problem is state" in source
    assert "The same gap that makes AI dangerous also makes it limited" in source
    assert "programmatic sensory organs" in source
    assert "Accountability is not the cage around capability" in source
    assert "LLMs can hallucinate the state they describe; compilers and machines can report the state they actually touched." in source
    assert "QuantaLang points toward code that declares effects, records ambient capability use, emits CPU and shader artifacts with maturity labels, and hands those receipts to WARDEN-style live-state tooling." in source
    assert "creative tools, research pipelines, and security workflows" in source
    assert "This is an aspiration, not a finished platform claim." in source
    assert "C remains the verified execution path today; HLSL/GLSL are working shader-source outputs; simultaneous CPU/GPU orchestration with WARDEN is the direction, not the current release promise." in source
    for overclaim in [
        "simultaneous CPU/GPU orchestration is production-ready",
        "one source shape already coordinates CPU and GPU outputs under WARDEN",
        "WARDEN-integrated CPU/GPU emission is complete",
        "models always have live-state ground truth",
        "AI tools are ethically neutral",
        "machines now sense the world natively",
    ]:
        assert overclaim not in source
