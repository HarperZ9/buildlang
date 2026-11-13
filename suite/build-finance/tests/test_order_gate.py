"""Order gate: kill switch, enforced stops, idempotent ids."""

from __future__ import annotations

import pytest

from build_finance.order_gate import DUPLICATE, REFUSED, SENT, OrderGate

from .order_gate_support import FakeBroker


def buy(gate, qty=10, decision="d1", **kw):
    kw.setdefault("ref_price", 100.0)
    return gate.submit("AAA", "buy", qty, decision_id=decision, **kw)


class TestKillSwitch:
    def test_halt_blocks_every_broker_method(self, tmp_path):
        broker = FakeBroker({"AAA": 5})
        gate = OrderGate(broker, tmp_path, default_stop_pct=0.05)
        gate.halt("test halt")
        assert buy(gate)["status"] == REFUSED
        sell = gate.submit("AAA", "sell", 5, decision_id="x")
        assert sell["status"] == REFUSED and "halted" in sell["reason"]
        assert gate.enforce_stops({"AAA": 1.0}) == []
        assert broker.calls == []

    def test_halt_file_survives_new_gate_and_needs_clear(self, tmp_path):
        broker = FakeBroker()
        OrderGate(broker, tmp_path).halt("file halt")
        second = OrderGate(broker, tmp_path, default_stop_pct=0.05)
        assert buy(second)["status"] == REFUSED
        second.clear_halt()
        assert buy(second)["status"] == SENT
        assert len(broker.calls) == 1


class TestStops:
    def test_buy_without_stop_is_refused(self, tmp_path):
        broker = FakeBroker()
        out = buy(OrderGate(broker, tmp_path))
        assert out["status"] == REFUSED and "stop" in out["reason"]
        assert broker.calls == []

    def test_default_stop_is_attached(self, tmp_path):
        gate = OrderGate(FakeBroker(), tmp_path, default_stop_pct=0.05)
        out = buy(gate)
        assert out["status"] == SENT and out["stop_price"] == pytest.approx(95.0)
        assert gate.stops() == {"AAA": pytest.approx(95.0)}

    def test_explicit_stop_above_price_refused(self, tmp_path):
        out = buy(OrderGate(FakeBroker(), tmp_path), stop_price=101.0)
        assert out["status"] == REFUSED

    def test_buy_needs_reference_price(self, tmp_path):
        gate = OrderGate(FakeBroker(), tmp_path, default_stop_pct=0.05)
        assert gate.submit("AAA", "buy", 1, decision_id="d")["status"] == REFUSED

    def test_breached_stop_closes_through_gate(self, tmp_path):
        broker = FakeBroker()
        gate = OrderGate(broker, tmp_path, default_stop_pct=0.05)
        buy(gate)
        assert gate.enforce_stops({"AAA": 96.0}) == []
        out = gate.enforce_stops({"AAA": 94.0})
        assert [o["status"] for o in out] == [SENT]
        assert broker.calls[-1][:3] == ("AAA", "sell", 10)
        assert gate.stops() == {}
        assert gate.enforce_stops({"AAA": 90.0}) == []

    def test_breached_stop_is_blocked_by_halt(self, tmp_path):
        broker = FakeBroker()
        gate = OrderGate(broker, tmp_path, default_stop_pct=0.05)
        buy(gate)
        gate.halt("stop test")
        out = gate.enforce_stops({"AAA": 1.0})
        assert [o["status"] for o in out] == [REFUSED]
        assert len(broker.calls) == 1

    def test_closes_only_halt_refuses_buys_and_lets_stops_close(self, tmp_path):
        broker = FakeBroker()
        gate = OrderGate(broker, tmp_path, default_stop_pct=0.05)
        buy(gate)
        gate.halt("closes test", allow_closes=True)
        assert gate.halt_mode() == "closes-only"
        refused = buy(gate, decision="d2")
        assert refused["status"] == REFUSED and "closes-only" in refused["reason"]
        out = gate.enforce_stops({"AAA": 1.0})
        assert [o["status"] for o in out] == [SENT]
        assert broker.calls[-1][:3] == ("AAA", "sell", 10) and len(broker.calls) == 2

    def test_halt_mode_persists_and_a_hand_made_file_is_a_full_stop(self, tmp_path):
        broker = FakeBroker({"AAA": 5})
        OrderGate(broker, tmp_path).halt("kept", allow_closes=True)
        second = OrderGate(broker, tmp_path)
        assert second.halt_mode() == "closes-only" and second.halted_reason() == "kept"
        assert second.submit("AAA", "sell", 5, decision_id="s1")["status"] == SENT
        second.halt("now everything")
        assert OrderGate(broker, tmp_path).halt_mode() == "full-stop"
        second.clear_halt()
        (tmp_path / "HALT").write_text("", encoding="utf-8")
        third = OrderGate(broker, tmp_path)
        assert third.halt_mode() == "full-stop"
        assert third.submit("AAA", "buy", 1, decision_id="b9", ref_price=10.0)["status"] == REFUSED
        assert len(broker.calls) == 1

    def test_sell_beyond_holdings_refused(self, tmp_path):
        broker = FakeBroker({"AAA": 3})
        out = OrderGate(broker, tmp_path).submit("AAA", "sell", 4, decision_id="d")
        assert out["status"] == REFUSED and broker.calls == []


class TestIdempotency:
    def test_repeat_returns_first_result(self, tmp_path):
        broker = FakeBroker()
        gate = OrderGate(broker, tmp_path, default_stop_pct=0.05)
        first = buy(gate)
        again = buy(gate)
        assert again["status"] == DUPLICATE
        assert again["result"] == first["result"]
        assert len(broker.calls) == 1

    def test_repeat_across_restart(self, tmp_path):
        broker = FakeBroker()
        first = buy(OrderGate(broker, tmp_path, default_stop_pct=0.05))
        again = buy(OrderGate(broker, tmp_path, default_stop_pct=0.05))
        assert again["status"] == DUPLICATE and again["result"] == first["result"]
        assert len(broker.calls) == 1

    def test_different_decision_gets_different_id(self, tmp_path):
        broker = FakeBroker()
        gate = OrderGate(broker, tmp_path, default_stop_pct=0.05)
        a, b = buy(gate, decision="d1"), buy(gate, decision="d2")
        assert a["client_order_id"] != b["client_order_id"]
        assert len(broker.calls) == 2

    def test_id_is_deterministic(self, tmp_path):
        ids = []
        for sub in ("a", "b"):
            gate = OrderGate(FakeBroker(), tmp_path / sub, default_stop_pct=0.05)
            ids.append(buy(gate)["client_order_id"])
        assert ids[0] == ids[1]

    def test_broker_error_blocks_blind_resend(self, tmp_path):
        broker = FakeBroker()
        broker.fail = True
        gate = OrderGate(broker, tmp_path, default_stop_pct=0.05)
        with pytest.raises(ConnectionError):
            buy(gate)
        broker.fail = False
        out = buy(OrderGate(broker, tmp_path, default_stop_pct=0.05))
        assert out["status"] == REFUSED and "unknown outcome" in out["reason"]
        assert len(broker.calls) == 1
