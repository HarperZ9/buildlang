"""AutoTrader routes its orders through the gate."""

from __future__ import annotations

from build_finance.autotrader import AutoTrader, AutoTraderConfig
from build_finance.data import Candle, Signal

from .order_gate_support import FakeBroker


class _Account:
    equity = 100_000.0
    cash = 100_000.0


class _Broker(FakeBroker):
    def get_account(self):
        return _Account()


def _candles(price=100.0):
    return [Candle(float(i), price, price, price, price, 1.0) for i in range(60)]


def _trader(tmp_path, broker):
    return AutoTrader(AutoTraderConfig(symbols=["AAA"]), broker=broker, state_dir=tmp_path)


def test_buy_goes_through_gate_with_default_stop(tmp_path):
    broker = _Broker()
    trader = _trader(tmp_path, broker)
    actions = trader._process_signals("AAA", [Signal("AAA", "buy", 0.9, timestamp=1.0)], _candles())
    assert len(actions) == 1 and len(broker.calls) == 1
    assert trader.gate.stops() == {"AAA": 95.0}


def test_halt_stops_autotrader_orders(tmp_path):
    broker = _Broker()
    trader = _trader(tmp_path, broker)
    trader.gate.halt("test")
    actions = trader._process_signals("AAA", [Signal("AAA", "buy", 0.9, timestamp=1.0)], _candles())
    assert actions == [] and broker.calls == []


def test_same_decision_replayed_sends_once(tmp_path):
    broker = _Broker()
    trader = _trader(tmp_path, broker)
    sig = [Signal("AAA", "buy", 0.9, timestamp=1.0)]
    trader._process_signals("AAA", sig, _candles())
    broker.positions.clear()
    trader._process_signals("AAA", sig, _candles())
    assert len(broker.calls) == 1
