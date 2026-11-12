"""
Persistence layer for engine state.

Saves/loads model weights, trade history, and performance metrics
to JSON files in ``~/.build-engine/``.

Files:
    - ``state.json``   -- model weights, accuracy history, config snapshot
    - ``trades.jsonl``  -- trade history (one JSON object per line, append-only)
"""

from __future__ import annotations

import json
import logging
import time
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

SAVE_DIR = Path.home() / ".build-engine"


class EngineState:
    """Persistent engine state manager.

    Reads and writes engine snapshots to disk so that model weights,
    accuracy history, and trade logs survive restarts.

    Parameters
    ----------
    save_dir:
        Override the default ``~/.build-engine/`` directory (useful
        for testing).
    """

    def __init__(self, save_dir: Path | None = None) -> None:
        self._dir = save_dir or SAVE_DIR
        self._state_path = self._dir / "state.json"
        self._trades_path = self._dir / "trades.jsonl"

    # ------------------------------------------------------------------
    # Directory bootstrap
    # ------------------------------------------------------------------

    def _ensure_dir(self) -> None:
        """Create the save directory if it does not exist."""
        self._dir.mkdir(parents=True, exist_ok=True)

    # ------------------------------------------------------------------
    # Full state save / load
    # ------------------------------------------------------------------

    def save(self, engine) -> None:
        """Save engine state to disk.

        Persists model weights, overall accuracy, cycle count,
        configuration snapshot, and a timestamp.

        Parameters
        ----------
        engine:
            An ``AdaptiveEngine`` instance (or any object with
            ``tracker``, ``cycle_count``, and ``config`` attributes).
        """
        self._ensure_dir()

        weights = {}
        accuracy = 0.0
        stats: dict[str, Any] = {}

        if hasattr(engine, "tracker"):
            weights = engine.tracker.get_model_weights()
            accuracy = engine.tracker.get_overall_accuracy()
            stats = engine.tracker.get_stats()

        cycle_count = getattr(engine, "cycle_count", 0)

        config_snapshot: dict[str, Any] = {}
        if hasattr(engine, "config"):
            cfg = engine.config
            config_snapshot = {
                "symbols": list(cfg.symbols),
                "models": list(cfg.models),
                "paper_trading": cfg.paper_trading,
                "risk_per_trade": cfg.risk_per_trade,
                "forecast_horizon": cfg.forecast_horizon,
                "min_confidence": cfg.min_confidence,
            }

        state = {
            "timestamp": time.time(),
            "cycle_count": cycle_count,
            "model_weights": weights,
            "accuracy": accuracy,
            "model_stats": stats,
            "config": config_snapshot,
        }

        tmp = self._state_path.with_suffix(".tmp")
        try:
            tmp.write_text(json.dumps(state, indent=2), encoding="utf-8")
            tmp.replace(self._state_path)
            logger.debug("Engine state saved to %s", self._state_path)
        except OSError as exc:
            logger.error("Failed to save engine state: %s", exc)
            if tmp.exists():
                tmp.unlink(missing_ok=True)
            raise

    def load(self) -> dict:
        """Load engine state from disk.

        Returns
        -------
        dict
            The saved state, or an empty dict if no state file exists
            or the file is corrupt.
        """
        if not self._state_path.exists():
            logger.debug("No state file at %s", self._state_path)
            return {}

        try:
            text = self._state_path.read_text(encoding="utf-8")
            data = json.loads(text)
            if not isinstance(data, dict):
                logger.warning("State file is not a dict, ignoring")
                return {}
            logger.debug("Engine state loaded from %s", self._state_path)
            return data
        except (json.JSONDecodeError, OSError) as exc:
            logger.warning("Corrupt state file, returning empty: %s", exc)
            return {}

    # ------------------------------------------------------------------
    # Trade history (append-only JSONL)
    # ------------------------------------------------------------------

    def save_trade(self, trade: dict) -> None:
        """Append a trade to the trade history.

        Each trade is written as a single JSON line to ``trades.jsonl``.

        Parameters
        ----------
        trade:
            A dict with trade details (symbol, side, qty, price, pnl,
            timestamp, etc.).
        """
        self._ensure_dir()

        if "timestamp" not in trade:
            trade["timestamp"] = time.time()

        try:
            line = json.dumps(trade, separators=(",", ":"))
            with open(self._trades_path, "a", encoding="utf-8") as f:
                f.write(line + "\n")
            logger.debug("Trade saved: %s", trade.get("symbol", "?"))
        except OSError as exc:
            logger.error("Failed to save trade: %s", exc)
            raise

    def load_trades(self, limit: int = 100) -> list[dict]:
        """Load recent trades from the history file.

        Reads the entire file and returns the last *limit* trades in
        chronological order (oldest first).

        Parameters
        ----------
        limit:
            Maximum number of trades to return.

        Returns
        -------
        list[dict]
            Trade dicts, newest last.
        """
        if not self._trades_path.exists():
            return []

        trades: list[dict] = []
        try:
            with open(self._trades_path, encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        trades.append(json.loads(line))
                    except json.JSONDecodeError:
                        logger.debug("Skipping corrupt trade line")
                        continue
        except OSError as exc:
            logger.warning("Failed to read trades file: %s", exc)
            return []

        if limit > 0:
            trades = trades[-limit:]

        return trades

    # ------------------------------------------------------------------
    # Utilities
    # ------------------------------------------------------------------

    def clear(self) -> None:
        """Delete all saved state (state.json and trades.jsonl)."""
        for path in (self._state_path, self._trades_path):
            if path.exists():
                path.unlink()
                logger.debug("Deleted %s", path)

    def trade_count(self) -> int:
        """Count total trades in the history file without loading all."""
        if not self._trades_path.exists():
            return 0
        count = 0
        try:
            with open(self._trades_path, encoding="utf-8") as f:
                for line in f:
                    if line.strip():
                        count += 1
        except OSError:
            return 0
        return count

    def __repr__(self) -> str:
        return f"EngineState(dir={self._dir})"
