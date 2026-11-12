"""
Tests for the persistence layer.

Covers state save/load roundtrip, trade history append/retrieve,
and graceful handling of missing or corrupt files.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from build_engine.persistence import EngineState

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def state_dir(tmp_path: Path) -> Path:
    """Return a fresh temporary directory for state files."""
    d = tmp_path / "build-engine-test"
    return d  # EngineState._ensure_dir creates it


@pytest.fixture
def es(state_dir: Path) -> EngineState:
    """Return an EngineState pointed at a temp directory."""
    return EngineState(save_dir=state_dir)


# ---------------------------------------------------------------------------
# Fake engine for save() testing
# ---------------------------------------------------------------------------


class _FakeTracker:
    def get_model_weights(self):
        return {"arima": 0.38, "prophet": 0.30, "neural": 0.23}

    def get_overall_accuracy(self):
        return 0.57

    def get_stats(self):
        return {
            "arima": {"total": 98, "correct": 61, "accuracy": 0.622},
            "prophet": {"total": 87, "correct": 48, "accuracy": 0.551},
            "neural": {"total": 61, "correct": 29, "accuracy": 0.475},
        }


class _FakeConfig:
    symbols = ["AAPL", "BTC-USD"]
    models = ["arima", "prophet"]
    paper_trading = True
    risk_per_trade = 0.02
    forecast_horizon = 5
    min_confidence = 0.3
    broker_api_key = "test-key-should-not-persist"
    broker_api_secret = "test-secret-should-not-persist"
    broker_base_url = "https://example.invalid"
    live_trading_ack = "I_UNDERSTAND_LIVE_RISK"


class _FakeEngine:
    def __init__(self):
        self.tracker = _FakeTracker()
        self.config = _FakeConfig()
        self.cycle_count = 42


# ---------------------------------------------------------------------------
# State save/load roundtrip
# ---------------------------------------------------------------------------


class TestStateSaveLoad:
    """Test state.json save and load."""

    def test_save_creates_directory(self, es: EngineState, state_dir: Path):
        """save() must create the save directory if missing."""
        assert not state_dir.exists()
        engine = _FakeEngine()
        es.save(engine)
        assert state_dir.exists()
        assert (state_dir / "state.json").exists()

    def test_save_load_roundtrip(self, es: EngineState):
        """Saved state must be loadable and contain all expected keys."""
        engine = _FakeEngine()
        es.save(engine)

        loaded = es.load()
        assert loaded["cycle_count"] == 42
        assert loaded["accuracy"] == pytest.approx(0.57)
        assert loaded["model_weights"]["arima"] == pytest.approx(0.38)
        assert loaded["model_weights"]["prophet"] == pytest.approx(0.30)
        assert loaded["config"]["symbols"] == ["AAPL", "BTC-USD"]
        assert loaded["config"]["paper_trading"] is True
        assert "timestamp" in loaded

    def test_save_does_not_persist_broker_credentials(self, es: EngineState):
        """State snapshots must not include live broker secrets."""
        engine = _FakeEngine()
        es.save(engine)

        loaded = es.load()
        config = loaded["config"]
        text = str(loaded)

        assert "broker_api_key" not in config
        assert "broker_api_secret" not in config
        assert "broker_base_url" not in config
        assert "live_trading_ack" not in config
        assert "test-key-should-not-persist" not in text
        assert "test-secret-should-not-persist" not in text
        assert "I_UNDERSTAND_LIVE_RISK" not in text

    def test_save_overwrites_previous(self, es: EngineState):
        """A second save replaces the first cleanly."""
        engine = _FakeEngine()
        es.save(engine)

        engine.cycle_count = 100
        es.save(engine)

        loaded = es.load()
        assert loaded["cycle_count"] == 100

    def test_load_returns_empty_when_missing(self, es: EngineState):
        """load() returns {} when no state file exists."""
        result = es.load()
        assert result == {}

    def test_load_returns_empty_on_corrupt_json(
        self,
        es: EngineState,
        state_dir: Path,
    ):
        """load() returns {} when state.json is invalid JSON."""
        state_dir.mkdir(parents=True, exist_ok=True)
        (state_dir / "state.json").write_text("not valid json {{{", encoding="utf-8")

        result = es.load()
        assert result == {}

    def test_load_returns_empty_on_non_dict(
        self,
        es: EngineState,
        state_dir: Path,
    ):
        """load() returns {} when state.json contains a non-dict value."""
        state_dir.mkdir(parents=True, exist_ok=True)
        (state_dir / "state.json").write_text('"just a string"', encoding="utf-8")

        result = es.load()
        assert result == {}

    def test_save_with_minimal_engine(self, es: EngineState):
        """save() works with an engine that has no tracker or config."""

        class _Bare:
            cycle_count = 0

        es.save(_Bare())
        loaded = es.load()
        assert loaded["cycle_count"] == 0
        assert loaded["model_weights"] == {}


# ---------------------------------------------------------------------------
# Trade history
# ---------------------------------------------------------------------------


class TestTradeHistory:
    """Test trades.jsonl append-only log."""

    def test_save_trade_creates_file(self, es: EngineState, state_dir: Path):
        """First save_trade call must create the trades file."""
        trade = {"symbol": "AAPL", "side": "buy", "qty": 50, "price": 178.23}
        es.save_trade(trade)
        assert (state_dir / "trades.jsonl").exists()

    def test_save_and_load_single_trade(self, es: EngineState):
        """A single saved trade is retrievable."""
        trade = {"symbol": "AAPL", "side": "buy", "qty": 50, "price": 178.23}
        es.save_trade(trade)

        trades = es.load_trades()
        assert len(trades) == 1
        assert trades[0]["symbol"] == "AAPL"
        assert trades[0]["side"] == "buy"
        assert trades[0]["qty"] == 50

    def test_save_trade_adds_timestamp(self, es: EngineState):
        """Trades without a timestamp get one auto-added."""
        trade = {"symbol": "BTC-USD", "side": "sell"}
        es.save_trade(trade)

        trades = es.load_trades()
        assert "timestamp" in trades[0]

    def test_save_trade_preserves_existing_timestamp(self, es: EngineState):
        """Trades with a timestamp keep it unchanged."""
        ts = 1700000000.0
        trade = {"symbol": "ETH-USD", "side": "buy", "timestamp": ts}
        es.save_trade(trade)

        trades = es.load_trades()
        assert trades[0]["timestamp"] == ts

    def test_multiple_trades_append(self, es: EngineState):
        """Multiple save_trade calls append to the same file."""
        for i in range(5):
            es.save_trade({"symbol": f"SYM{i}", "side": "buy"})

        trades = es.load_trades()
        assert len(trades) == 5
        assert trades[0]["symbol"] == "SYM0"
        assert trades[4]["symbol"] == "SYM4"

    def test_load_trades_limit(self, es: EngineState):
        """load_trades(limit=N) returns only the last N trades."""
        for i in range(20):
            es.save_trade({"symbol": f"SYM{i}", "side": "buy"})

        trades = es.load_trades(limit=5)
        assert len(trades) == 5
        # Should be the last 5 trades (SYM15..SYM19)
        assert trades[0]["symbol"] == "SYM15"
        assert trades[4]["symbol"] == "SYM19"

    def test_load_trades_empty_when_missing(self, es: EngineState):
        """load_trades() returns [] when no trades file exists."""
        assert es.load_trades() == []

    def test_load_trades_skips_corrupt_lines(
        self,
        es: EngineState,
        state_dir: Path,
    ):
        """Corrupt lines in trades.jsonl are skipped, valid ones kept."""
        state_dir.mkdir(parents=True, exist_ok=True)
        trades_path = state_dir / "trades.jsonl"
        trades_path.write_text(
            '{"symbol":"AAPL","side":"buy"}\nnot valid json\n{"symbol":"BTC-USD","side":"sell"}\n',
            encoding="utf-8",
        )

        trades = es.load_trades()
        assert len(trades) == 2
        assert trades[0]["symbol"] == "AAPL"
        assert trades[1]["symbol"] == "BTC-USD"

    def test_trade_count(self, es: EngineState):
        """trade_count() returns the number of trades without loading."""
        for i in range(7):
            es.save_trade({"symbol": f"SYM{i}", "side": "buy"})

        assert es.trade_count() == 7

    def test_trade_count_zero_when_missing(self, es: EngineState):
        """trade_count() returns 0 when no trades file exists."""
        assert es.trade_count() == 0


# ---------------------------------------------------------------------------
# Clear and repr
# ---------------------------------------------------------------------------


class TestClearAndRepr:
    """Test utility methods."""

    def test_clear_removes_both_files(self, es: EngineState):
        """clear() deletes state.json and trades.jsonl."""
        engine = _FakeEngine()
        es.save(engine)
        es.save_trade({"symbol": "AAPL"})

        es.clear()
        assert es.load() == {}
        assert es.load_trades() == []

    def test_clear_safe_when_no_files(self, es: EngineState):
        """clear() does not raise when files do not exist."""
        es.clear()  # Should not raise

    def test_repr(self, es: EngineState, state_dir: Path):
        """repr() includes the directory path."""
        r = repr(es)
        assert "EngineState" in r
        assert str(state_dir) in r
