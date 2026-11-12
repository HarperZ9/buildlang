"""
Automated trading engine that connects strategies to brokers.

The :class:`AutoTrader` periodically:

1. Fetches the latest market data for each configured symbol.
2. Runs the selected strategy to generate trading signals.
3. Sizes positions according to a fixed-risk-per-trade rule.
4. Executes orders through a broker (paper or live).
5. Logs all activity.

Usage
-----
::

    from build_finance.autotrader import AutoTrader, AutoTraderConfig
    from build_finance.broker import PaperBroker

    config = AutoTraderConfig(symbols=["AAPL", "BTC-USD"])
    trader = AutoTrader(config)
    trader.run_once()          # single iteration
    trader.run_loop(max_iterations=10)   # loop with sleep
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass, field

from build_finance.broker import PaperBroker
from build_finance.data import Candle, Signal

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------


@dataclass
class AutoTraderConfig:
    """Parameters controlling the auto-trading loop.

    Attributes
    ----------
    symbols:
        Ticker symbols to trade.
    strategy_name:
        Name of the strategy to use.  One of ``"momentum"``,
        ``"mean_reversion"``, ``"trend"``, ``"breakout"``, or
        ``"ensemble"`` (default).
    interval_seconds:
        Seconds between iterations (default 300 = 5 minutes).
    risk_per_trade:
        Fraction of equity risked on each new position (default 0.02 = 2 %).
    max_positions:
        Maximum number of simultaneous open positions.
    paper_trading:
        Whether to use paper trading (default *True*).
    asset_type:
        ``"stock"``, ``"crypto"``, or ``"mixed"`` (default).
    """

    symbols: list[str] = field(default_factory=lambda: ["AAPL", "BTC-USD"])
    strategy_name: str = "ensemble"
    interval_seconds: int = 300
    risk_per_trade: float = 0.02
    max_positions: int = 5
    paper_trading: bool = True
    asset_type: str = "mixed"


# ---------------------------------------------------------------------------
# Strategy loader
# ---------------------------------------------------------------------------


def _create_strategy(name: str):
    """Instantiate a strategy by name.

    Imports are deferred so that ``autotrader`` can be imported without
    pulling in NumPy unless a strategy is actually constructed.
    """
    from build_finance.strategies import (
        BreakoutStrategy,
        EnsembleStrategy,
        MeanReversionStrategy,
        MomentumStrategy,
        TrendFollowingStrategy,
    )

    strategies = {
        "momentum": MomentumStrategy,
        "mean_reversion": MeanReversionStrategy,
        "trend": TrendFollowingStrategy,
        "breakout": BreakoutStrategy,
        "ensemble": EnsembleStrategy,
    }
    cls = strategies.get(name)
    if cls is None:
        logger.warning(
            "Unknown strategy %r; falling back to EnsembleStrategy",
            name,
        )
        cls = EnsembleStrategy
    return cls()


# ---------------------------------------------------------------------------
# AutoTrader
# ---------------------------------------------------------------------------


class AutoTrader:
    """Automated trading engine.

    Connects a strategy to a broker with periodic execution.

    Parameters
    ----------
    config:
        An :class:`AutoTraderConfig` instance.
    broker:
        A broker object (must implement ``get_account``, ``get_positions``,
        ``submit_order``).  Defaults to a :class:`PaperBroker` if not
        provided.
    """

    def __init__(self, config: AutoTraderConfig, broker=None) -> None:
        self.config = config
        self.broker = broker if broker is not None else PaperBroker()
        self.strategy = _create_strategy(config.strategy_name)
        self.running: bool = False
        self.iteration: int = 0
        self.history: dict[str, list[Candle]] = {}
        self._actions_log: list[dict] = []

    # -- data fetching ------------------------------------------------------

    def _fetch_recent(
        self,
        symbol: str,
        lookback: int = 100,
    ) -> list[Candle]:
        """Fetch recent candles for *symbol*.

        Uses :func:`build_finance.market_data.fetch_yahoo` with a
        3-month daily window and trims to the most recent *lookback*
        bars.  On failure, falls back to any previously cached history.
        """
        try:
            from build_finance.market_data import fetch_yahoo

            candles = fetch_yahoo(symbol, period="3mo", interval="1d")
            if candles:
                self.history[symbol] = candles
            return candles[-lookback:] if len(candles) > lookback else candles
        except (ImportError, ConnectionError, TimeoutError, OSError) as exc:
            logger.warning("Data fetch failed for %s: %s", symbol, exc)
            cached = self.history.get(symbol, [])
            return cached[-lookback:] if len(cached) > lookback else cached

    # -- signal -> order logic ----------------------------------------------

    def _position_size(
        self,
        price: float,
        equity: float,
    ) -> int:
        """Calculate the number of shares to buy given risk budget.

        Uses the ``risk_per_trade`` fraction of current equity divided
        by the asset price, floored to at least 1 share.
        """
        if price <= 0:
            return 0
        risk_amount = equity * self.config.risk_per_trade
        qty = int(risk_amount / price)
        return max(1, qty)

    def _process_signals(
        self,
        symbol: str,
        signals: list[Signal],
        candles: list[Candle],
    ) -> list[dict]:
        """Convert signals into broker orders, respecting position limits."""
        if not signals:
            return []

        actions: list[dict] = []
        account = self.broker.get_account()
        positions = self.broker.get_positions()
        current_price = candles[-1].close

        # Ensure broker knows the current price before any order
        if hasattr(self.broker, "update_prices"):
            self.broker.update_prices({symbol: current_price})

        # Only act on the most recent signal (last in the list)
        signal = signals[-1]

        # BUY logic
        if signal.side == "buy" and symbol not in positions:
            # Check max positions constraint
            if len(positions) >= self.config.max_positions:
                logger.info(
                    "Max positions (%d) reached; skipping BUY %s",
                    self.config.max_positions,
                    symbol,
                )
                return []

            qty = self._position_size(current_price, account.equity)
            if qty <= 0:
                return []

            # Check if we have enough cash
            cost = current_price * qty
            if cost > account.cash:
                # Reduce quantity to fit available cash
                qty = max(1, int(account.cash / current_price))
                if current_price * qty > account.cash:
                    logger.info(
                        "Insufficient cash for %s (need %.2f, have %.2f)",
                        symbol,
                        current_price,
                        account.cash,
                    )
                    return []

            try:
                result = self.broker.submit_order(symbol, "buy", qty)
                action = {
                    "action": "BUY",
                    "symbol": symbol,
                    "qty": qty,
                    "price": current_price,
                    "signal_strength": signal.strength,
                    "timestamp": time.time(),
                    **result,
                }
                actions.append(action)
                logger.info(
                    "BUY %d %s @ %.2f (strength=%.2f)",
                    qty,
                    symbol,
                    current_price,
                    signal.strength,
                )
            except (ValueError, ConnectionError, TimeoutError, OSError) as exc:
                logger.error("Order failed for BUY %s: %s", symbol, exc)

        # SELL logic
        elif signal.side == "sell" and symbol in positions:
            pos = positions[symbol]
            qty = pos.get("quantity", 0)
            if qty > 0:
                try:
                    result = self.broker.submit_order(symbol, "sell", qty)
                    action = {
                        "action": "SELL",
                        "symbol": symbol,
                        "qty": qty,
                        "price": current_price,
                        "signal_strength": signal.strength,
                        "timestamp": time.time(),
                        **result,
                    }
                    actions.append(action)
                    logger.info(
                        "SELL %d %s @ %.2f (strength=%.2f)",
                        qty,
                        symbol,
                        current_price,
                        signal.strength,
                    )
                except (ValueError, ConnectionError, TimeoutError, OSError) as exc:
                    logger.error("Order failed for SELL %s: %s", symbol, exc)

        return actions

    # -- main loop ----------------------------------------------------------

    def run_once(self) -> list[dict]:
        """Run one iteration: fetch data, generate signals, execute.

        Returns
        -------
        list[dict]
            A list of action dicts describing orders placed this iteration.
        """
        all_actions: list[dict] = []

        for symbol in self.config.symbols:
            # 1. Fetch recent data
            candles = self._fetch_recent(symbol)
            if len(candles) < 50:
                logger.debug(
                    "Insufficient data for %s (%d candles, need 50)",
                    symbol,
                    len(candles),
                )
                continue

            # 2. Update broker prices for mark-to-market
            if hasattr(self.broker, "update_prices"):
                self.broker.update_prices({symbol: candles[-1].close})

            # 3. Generate signals
            try:
                signals = self.strategy.generate_signals(candles)
            except (ValueError, ZeroDivisionError) as exc:
                logger.error("Strategy error for %s: %s", symbol, exc)
                continue

            # 4. Execute
            actions = self._process_signals(symbol, signals, candles)
            all_actions.extend(actions)

        self._actions_log.extend(all_actions)
        self.iteration += 1

        return all_actions

    def run_loop(self, max_iterations: int | None = None) -> None:
        """Run continuously with *interval_seconds* between iterations.

        Parameters
        ----------
        max_iterations:
            Stop after this many iterations.  *None* means run forever
            (until :meth:`stop` is called).
        """
        self.running = True
        logger.info(
            "AutoTrader started: symbols=%s, strategy=%s, interval=%ds",
            self.config.symbols,
            self.config.strategy_name,
            self.config.interval_seconds,
        )

        try:
            while self.running:
                try:
                    actions = self.run_once()
                    account = self.broker.get_account()
                    logger.info(
                        "Iteration %d: equity=$%.2f, cash=$%.2f, positions=%d, actions=%d",
                        self.iteration,
                        account.equity,
                        account.cash,
                        len(account.positions),
                        len(actions),
                    )
                except Exception as exc:
                    logger.error("AutoTrader error in iteration %d: %s", self.iteration, exc)

                if max_iterations is not None and self.iteration >= max_iterations:
                    logger.info(
                        "Reached max_iterations=%d; stopping.",
                        max_iterations,
                    )
                    break

                time.sleep(self.config.interval_seconds)
        finally:
            self.running = False
            logger.info("AutoTrader stopped after %d iterations.", self.iteration)

    def stop(self) -> None:
        """Signal the run loop to stop after the current iteration."""
        self.running = False
        logger.info("AutoTrader stop requested.")

    # -- status / introspection ---------------------------------------------

    def get_status(self) -> dict:
        """Return a summary of the trader's current state.

        Returns
        -------
        dict
            Keys: ``running``, ``iteration``, ``equity``, ``cash``,
            ``positions``, ``total_trades``, ``symbols``, ``strategy``.
        """
        account = self.broker.get_account()
        return {
            "running": self.running,
            "iteration": self.iteration,
            "equity": account.equity,
            "cash": account.cash,
            "positions": account.positions,
            "total_trades": len(self.broker.trades),
            "symbols": self.config.symbols,
            "strategy": self.config.strategy_name,
        }

    def get_actions_log(self) -> list[dict]:
        """Return the full list of actions taken across all iterations."""
        return list(self._actions_log)

    # -- convenience --------------------------------------------------------

    def add_symbol(self, symbol: str) -> None:
        """Add a symbol to the watch list (if not already present)."""
        if symbol not in self.config.symbols:
            self.config.symbols.append(symbol)
            logger.info("Added symbol %s to AutoTrader", symbol)

    def remove_symbol(self, symbol: str) -> None:
        """Remove a symbol from the watch list."""
        if symbol in self.config.symbols:
            self.config.symbols.remove(symbol)
            logger.info("Removed symbol %s from AutoTrader", symbol)

    def set_strategy(self, name: str) -> None:
        """Switch to a different strategy by name."""
        self.strategy = _create_strategy(name)
        self.config.strategy_name = name
        logger.info("Switched strategy to %s", name)
