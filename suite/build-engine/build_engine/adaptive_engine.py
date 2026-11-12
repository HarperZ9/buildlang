"""
Self-improving prediction and trading engine.

The feedback loop::

    Market Data -> Models Train -> Predictions -> Trades -> Results -> Models Improve

:class:`AdaptiveEngine` wires together:
    - :class:`ModelTrainer`        (build-oracle model management)
    - :class:`PredictionStrategy`  (oracle-to-signal bridge)
    - :class:`PerformanceTracker`  (accuracy tracking & weight adjustment)
    - ``AutoTrader``               (build-finance execution layer)
    - ``PaperBroker`` / ``AlpacaBroker``

Each *cycle* fetches data, generates predictions, executes trades, and
feeds the results back to adjust model weights.
"""

from __future__ import annotations

import logging
import os
import time

from build_engine.config import LIVE_TRADING_ACK, EngineConfig
from build_engine.model_trainer import ModelTrainer
from build_engine.performance_tracker import PerformanceTracker
from build_engine.prediction_strategy import PredictionStrategy

logger = logging.getLogger(__name__)


class AdaptiveEngine:
    """Self-improving prediction and trading engine.

    Parameters
    ----------
    config:
        An :class:`EngineConfig` controlling symbols, models, risk, etc.
    """

    def __init__(self, config: EngineConfig | None = None) -> None:
        self.config = config or EngineConfig()

        # Core components
        self.trainer = ModelTrainer(self.config)
        self.tracker = PerformanceTracker(self.config)
        self.strategy = PredictionStrategy(self.config, self.trainer)

        # Broker + AutoTrader
        self._setup_broker()
        self._setup_trader()

        # State
        self.running: bool = False
        self.cycle_count: int = 0

    # ------------------------------------------------------------------
    # Setup helpers
    # ------------------------------------------------------------------

    def _setup_broker(self) -> None:
        """Create the appropriate broker (paper or live)."""
        from build_finance.broker import PaperBroker

        if self.config.paper_trading:
            self.broker = PaperBroker(initial_capital=100_000)
        else:
            from build_finance.broker import AlpacaBroker, BrokerConfig

            ack = self.config.live_trading_ack or os.environ.get("BUILD_ENGINE_LIVE_ACK", "")
            if ack != LIVE_TRADING_ACK:
                raise ValueError(
                    "Live Alpaca mode requires BUILD_ENGINE_LIVE_ACK=I_UNDERSTAND_LIVE_RISK "
                    "or EngineConfig.live_trading_ack with the same value."
                )

            api_key = self.config.broker_api_key or os.environ.get("APCA_API_KEY_ID", "")
            api_secret = self.config.broker_api_secret or os.environ.get("APCA_API_SECRET_KEY", "")
            if not api_key or not api_secret:
                raise ValueError(
                    "Live Alpaca mode requires broker_api_key/broker_api_secret or APCA_API_KEY_ID/APCA_API_SECRET_KEY."
                )

            broker_cfg = BrokerConfig(
                name="alpaca",
                api_key=api_key,
                api_secret=api_secret,
                base_url=self.config.broker_base_url,
                paper_trading=False,
            )
            self.broker = AlpacaBroker(broker_cfg)

    def _setup_trader(self) -> None:
        """Create an AutoTrader wired to the prediction strategy."""
        from build_finance.autotrader import AutoTrader, AutoTraderConfig

        trader_config = AutoTraderConfig(
            symbols=self.config.symbols,
            risk_per_trade=self.config.risk_per_trade,
            max_positions=self.config.max_positions,
            paper_trading=self.config.paper_trading,
            interval_seconds=300,
        )
        self.trader = AutoTrader(trader_config, broker=self.broker)
        # Inject the prediction strategy in place of the default one
        self.trader.strategy = self.strategy

        # Prediction models need more history than the default 100 bars.
        # Store reference so run_once passes all available data.
        self._lookback = max(200, self.config.lookback_days)

    # ------------------------------------------------------------------
    # Single cycle
    # ------------------------------------------------------------------

    def run_cycle(self) -> dict:
        """Run one prediction-trade-evaluate cycle.

        The engine fetches data itself (with extended lookback for prediction
        models), generates signals via the PredictionStrategy, and executes
        through the AutoTrader's broker.

        Returns
        -------
        dict with keys ``cycle``, ``actions``, ``equity``, ``cash``,
        ``positions``, ``model_weights``, ``accuracy``.
        """
        self.cycle_count += 1
        all_actions = []

        for symbol in self.config.symbols:
            # 1. Fetch data with extended lookback
            candles = self._fetch_data(symbol)
            if len(candles) < 50:
                logger.debug("Insufficient data for %s (%d candles)", symbol, len(candles))
                continue

            # 2. Update broker price
            current_price = candles[-1].close
            if hasattr(self.broker, "update_prices"):
                self.broker.update_prices({symbol: current_price})

            # 3. Generate prediction signals
            signals = self.strategy.generate_signals(candles)

            # 4. Execute via AutoTrader's signal processor
            if signals:
                actions = self.trader._process_signals(symbol, signals, candles)
                all_actions.extend(actions)

        # 5. Update model weights from performance
        weights = self.tracker.get_model_weights()
        self.strategy.set_model_weights(weights)

        account = self.broker.get_account()

        return {
            "cycle": self.cycle_count,
            "actions": all_actions,
            "equity": account.equity,
            "cash": account.cash,
            "positions": len(account.positions),
            "model_weights": weights,
            "accuracy": self.tracker.get_overall_accuracy(),
        }

    def _fetch_data(self, symbol: str):
        """Fetch market data with extended lookback for prediction models."""
        lookback = self._lookback
        try:
            from build_finance.market_data import fetch_yahoo

            candles = fetch_yahoo(symbol, period="6mo", interval="1d")
            if candles:
                self.trader.history[symbol] = candles
                return candles[-lookback:]
        except (ConnectionError, TimeoutError, OSError):
            pass
        # Fallback to cached history
        cached = self.trader.history.get(symbol, [])
        return cached[-lookback:] if len(cached) > lookback else cached

    # ------------------------------------------------------------------
    # Continuous loop
    # ------------------------------------------------------------------

    def run_loop(
        self,
        max_cycles: int | None = None,
        interval_seconds: int = 300,
    ) -> None:
        """Run continuously until stopped or *max_cycles* is reached.

        Parameters
        ----------
        max_cycles:
            Stop after this many cycles.  ``None`` means run until
            :meth:`stop` is called.
        interval_seconds:
            Seconds to sleep between cycles.
        """
        self.running = True
        logger.info(
            "AdaptiveEngine started: symbols=%s, models=%s",
            self.config.symbols,
            self.config.models,
        )

        try:
            while self.running:
                try:
                    result = self.run_cycle()
                    logger.info(
                        "Cycle %d: equity=$%.2f, actions=%d, accuracy=%.0f%%",
                        result["cycle"],
                        result["equity"],
                        len(result["actions"]),
                        result["accuracy"] * 100,
                    )
                except Exception as exc:
                    logger.error("Cycle error: %s", exc)

                if max_cycles and self.cycle_count >= max_cycles:
                    break

                time.sleep(interval_seconds)
        finally:
            self.running = False
            logger.info("AdaptiveEngine stopped after %d cycles.", self.cycle_count)

    # ------------------------------------------------------------------
    # Control
    # ------------------------------------------------------------------

    def stop(self) -> None:
        """Signal the run loop to stop after the current cycle."""
        self.running = False
        logger.info("AdaptiveEngine stop requested.")

    # ------------------------------------------------------------------
    # Status
    # ------------------------------------------------------------------

    def get_status(self) -> dict:
        """Return a comprehensive status snapshot.

        Returns
        -------
        dict with keys ``running``, ``cycles``, ``equity``, ``positions``,
        ``trades``, ``models``, ``accuracy``, ``model_weights``.
        """
        account = self.broker.get_account()
        return {
            "running": self.running,
            "cycles": self.cycle_count,
            "equity": account.equity,
            "positions": len(account.positions),
            "trades": len(self.broker.trades),
            "models": self.trainer.get_model_info(),
            "accuracy": self.tracker.get_overall_accuracy(),
            "model_weights": self.tracker.get_model_weights(),
        }

    def __repr__(self) -> str:
        return (
            f"AdaptiveEngine(symbols={self.config.symbols}, "
            f"models={self.config.models}, "
            f"paper={self.config.paper_trading})"
        )
