"""Static check: only the gate may call a broker's order method."""

from __future__ import annotations

import ast
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
ORDER_METHODS = {"submit_order", "place_order", "create_order", "send_order"}
ALLOWED = {"order_gate.py"}


def find_violations(root: Path) -> list[str]:
    """Return 'file:line' for each order-method call outside the gate."""
    found: list[str] = []
    for path in sorted(root.rglob("*.py")):
        parts = set(path.parts)
        if path.name in ALLOWED or "tests" in parts or "node_modules" in parts:
            continue
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if (
                isinstance(node, ast.Call)
                and isinstance(node.func, ast.Attribute)
                and node.func.attr in ORDER_METHODS
            ):
                found.append(f"{path.relative_to(root).as_posix()}:{node.lineno}")
    return found


def test_no_direct_broker_order_calls_in_suite():
    assert find_violations(SUITE) == []


def test_scanner_catches_planted_violation(tmp_path):
    pkg = tmp_path / "pkg"
    pkg.mkdir()
    (pkg / "rogue.py").write_text("def go(b):\n    return b.submit_order('A', 'buy', 1)\n")
    (pkg / "order_gate.py").write_text("def ok(b):\n    return b.submit_order('A', 'buy', 1)\n")
    assert find_violations(tmp_path) == ["pkg/rogue.py:2"]
