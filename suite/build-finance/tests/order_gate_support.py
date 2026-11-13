"""In-memory fake broker for order gate tests.  No network, no keys."""

from __future__ import annotations


class FakeBroker:
    def __init__(self, positions: dict[str, float] | None = None) -> None:
        self.calls: list[tuple] = []
        self.positions = {s: {"quantity": q} for s, q in (positions or {}).items()}
        self.fail = False

    def get_positions(self) -> dict[str, dict]:
        return {s: dict(p) for s, p in self.positions.items()}

    def submit_order(self, symbol, side, quantity, order_type="market", limit_price=None):
        self.calls.append((symbol, side, quantity, order_type, limit_price))
        if self.fail:
            raise ConnectionError("simulated outage")
        held = self.positions.get(symbol, {}).get("quantity", 0)
        new = held + quantity if side == "buy" else held - quantity
        if new > 0:
            self.positions[symbol] = {"quantity": new}
        else:
            self.positions.pop(symbol, None)
        return {"order_id": f"fake-{len(self.calls)}", "status": "filled"}
