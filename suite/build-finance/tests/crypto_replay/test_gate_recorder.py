"""Focused regression tests for the crypto replay gate recorder."""

from pathlib import Path

from scripts.capture_crypto_replay_gate import _normalize_output


def test_normalize_output_ignores_only_pytest_summary_elapsed_time(tmp_path: Path) -> None:
    semantic_output = (
        "E assertion durations remain significant: expected 0.25s, got 0.26s\n"
        "FAILED tests/crypto_replay/test_example.py::test_example\n"
    )
    first = _normalize_output(f"{semantic_output}44 failed in 0.25s\n".encode(), tmp_path)
    second = _normalize_output(f"{semantic_output}44 failed in 0.26s\n".encode(), tmp_path)

    assert first == second
    assert "expected 0.25s, got 0.26s" in first
    assert first.endswith("44 failed in <pytest-duration>\n")
