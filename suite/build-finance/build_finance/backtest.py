"""
Backtesting engine with walk-forward optimization and Monte Carlo analysis.

Supports any strategy that exposes a ``generate_signals(candles)`` method
returning a list of :class:`Signal` objects.
"""

from __future__ import annotations

import math
import random
from dataclasses import dataclass, field
from typing import Any, Protocol, runtime_checkable

import numpy as np

from build_finance.data import (
    BacktestPosition as Position,
)
from build_finance.data import (
    BacktestSignal,
    Candle,
    SignalType,
)
from build_finance.data import (
    BacktestSignal as Signal,
)
from build_finance.data import (
    BacktestTrade as Trade,
)
from build_finance.orderbook import ExecutionConfig, simulate_fill

# ---------------------------------------------------------------------------
# Strategy protocol
# ---------------------------------------------------------------------------


@runtime_checkable
class Strategy(Protocol):
    """Minimal interface a strategy must satisfy."""

    def generate_signals(self, candles: list[Candle]) -> list[Signal]: ...


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------


@dataclass
class BacktestConfig:
    """Parameters controlling a backtest run."""

    initial_capital: float = 100_000.0
    slippage_bps: float = 5.0
    commission_rate: float = 0.001
    risk_per_trade: float = 0.10  # 10 % of equity per position
    max_positions: int = 10
    lookback: int = 100  # bars of history fed to strategy
    risk_free_rate: float = 0.02  # annualized


# ---------------------------------------------------------------------------
# Result container
# ---------------------------------------------------------------------------


@dataclass
class BacktestResult:
    """Comprehensive backtest output."""

    initial_capital: float
    final_equity: float
    total_return: float
    annualized_return: float
    num_trades: int
    winning_trades: int
    losing_trades: int
    win_rate: float
    profit_factor: float
    sharpe_ratio: float
    sortino_ratio: float
    max_drawdown: float
    calmar_ratio: float
    var_95: float
    cvar_95: float
    avg_win: float
    avg_loss: float
    equity_curve: np.ndarray
    trades: list[Trade]
    daily_returns: np.ndarray = field(default_factory=lambda: np.array([]))

    def summary(self) -> str:
        """Human-readable performance summary."""
        lines = [
            "=== Backtest Results ===",
            f"Initial capital : ${self.initial_capital:>12,.2f}",
            f"Final equity    : ${self.final_equity:>12,.2f}",
            f"Total return    :  {self.total_return:>11.2%}",
            f"Ann. return     :  {self.annualized_return:>11.2%}",
            f"Sharpe ratio    :  {self.sharpe_ratio:>11.4f}",
            f"Sortino ratio   :  {self.sortino_ratio:>11.4f}",
            f"Max drawdown    :  {self.max_drawdown:>11.2%}",
            f"Calmar ratio    :  {self.calmar_ratio:>11.4f}",
            f"VaR (95 %)      :  {self.var_95:>11.2%}",
            f"CVaR (95 %)     :  {self.cvar_95:>11.2%}",
            f"Trades          :  {self.num_trades:>11d}",
            f"Win rate        :  {self.win_rate:>11.2%}",
            f"Profit factor   :  {self.profit_factor:>11.4f}",
            f"Avg win         : ${self.avg_win:>12,.2f}",
            f"Avg loss        : ${self.avg_loss:>12,.2f}",
        ]
        return "\n".join(lines)


# ---------------------------------------------------------------------------
# Metric helpers
# ---------------------------------------------------------------------------


def _sharpe(returns: np.ndarray, rf_daily: float) -> float:
    if len(returns) < 2:
        return 0.0
    excess = returns - rf_daily
    std = excess.std(ddof=1)
    if std < 1e-12:
        return 0.0
    return float(excess.mean() / std * math.sqrt(252))


def _sortino(returns: np.ndarray, rf_daily: float) -> float:
    if len(returns) < 2:
        return 0.0
    excess = returns - rf_daily
    downside = excess[excess < 0]
    if len(downside) < 2:
        return 0.0
    ds_std = float(np.std(downside, ddof=1))
    if ds_std < 1e-12:
        return 0.0
    return float(excess.mean() / ds_std * math.sqrt(252))


def _max_drawdown(equity: np.ndarray) -> float:
    peak = np.maximum.accumulate(equity)
    dd = (equity - peak) / peak
    return float(dd.min()) if len(dd) > 0 else 0.0


def _var(returns: np.ndarray, confidence: float = 0.95) -> float:
    if len(returns) == 0:
        return 0.0
    return float(np.percentile(returns, (1 - confidence) * 100))


def _cvar(returns: np.ndarray, confidence: float = 0.95) -> float:
    v = _var(returns, confidence)
    tail = returns[returns <= v]
    return float(tail.mean()) if len(tail) > 0 else v


def _compute_metrics(
    equity_curve: np.ndarray,
    trades: list[Trade],
    config: BacktestConfig,
) -> BacktestResult:
    """Derive all performance statistics from equity curve and trades."""
    initial = config.initial_capital
    final = float(equity_curve[-1]) if len(equity_curve) > 0 else initial
    total_ret = (final - initial) / initial if initial else 0.0

    n_days = max(len(equity_curve) - 1, 1)
    years = n_days / 252
    ann_ret = (1 + total_ret) ** (1 / years) - 1 if years > 0 else 0.0

    # Daily returns from equity curve
    if len(equity_curve) > 1:
        daily_returns = np.diff(equity_curve) / equity_curve[:-1]
    else:
        daily_returns = np.array([0.0])

    rf_daily = config.risk_free_rate / 252

    if trades:
        pnls = np.array([t.net_pnl for t in trades])
        wins = pnls[pnls > 0]
        losses = pnls[pnls < 0]
        gross_profit = float(wins.sum()) if len(wins) else 0.0
        gross_loss = float(abs(losses.sum())) if len(losses) else 0.0
        profit_factor = (gross_profit / gross_loss) if gross_loss > 0 else (float("inf") if gross_profit > 0 else 0.0)
        n_trades = len(trades)
        n_wins = int(len(wins))
        n_losses = int(len(losses))
        wr = n_wins / n_trades
        avg_w = float(wins.mean()) if len(wins) else 0.0
        avg_l = float(losses.mean()) if len(losses) else 0.0
    else:
        n_trades = 0
        n_wins = 0
        n_losses = 0
        wr = 0.0
        profit_factor = 0.0
        avg_w = 0.0
        avg_l = 0.0

    mdd = _max_drawdown(equity_curve)
    calmar = ann_ret / abs(mdd) if mdd != 0 else 0.0

    sharpe = _sharpe(daily_returns, rf_daily) if len(daily_returns) > 1 else 0.0
    sortino = _sortino(daily_returns, rf_daily) if len(daily_returns) > 1 else 0.0

    return BacktestResult(
        initial_capital=initial,
        final_equity=final,
        total_return=total_ret,
        annualized_return=ann_ret,
        num_trades=n_trades,
        winning_trades=n_wins,
        losing_trades=n_losses,
        win_rate=wr,
        profit_factor=profit_factor,
        sharpe_ratio=sharpe,
        sortino_ratio=sortino,
        max_drawdown=mdd,
        calmar_ratio=calmar,
        var_95=_var(daily_returns),
        cvar_95=_cvar(daily_returns),
        avg_win=avg_w,
        avg_loss=avg_l,
        equity_curve=equity_curve,
        trades=trades,
        daily_returns=daily_returns,
    )


# ---------------------------------------------------------------------------
# Main backtester
# ---------------------------------------------------------------------------


class Backtester:
    """Event-driven backtesting engine.

    Usage::

        bt = Backtester(BacktestConfig(initial_capital=50_000))
        result = bt.run(my_strategy, {"AAPL": candles})
        print(result.summary())
    """

    def __init__(self, config: BacktestConfig | None = None) -> None:
        self.config = config or BacktestConfig()

    # -----------------------------------------------------------------------
    # Core run
    # -----------------------------------------------------------------------

    def run(
        self,
        strategy: Strategy,
        candles: dict[str, list[Candle]],
    ) -> BacktestResult:
        """Run a full backtest over *candles*.

        Parameters
        ----------
        strategy:
            Object with ``generate_signals(candles) -> list[Signal]``.
        candles:
            ``{symbol: [Candle, ...]}`` sorted ascending by timestamp.
        """
        cfg = self.config
        exec_cfg = ExecutionConfig(
            slippage_bps=cfg.slippage_bps,
            commission_per_share=cfg.commission_rate,
        )

        equity = cfg.initial_capital
        positions: dict[str, Position] = {}
        completed_trades: list[Trade] = []
        equity_history: list[float] = [equity]

        # Build a unified timeline: list of (timestamp, symbol, candle_index)
        timeline: list[tuple[float, str, int]] = []
        for sym, bars in candles.items():
            for idx, bar in enumerate(bars):
                timeline.append((bar.timestamp, sym, idx))
        timeline.sort(key=lambda t: t[0])

        # Track how many bars we have seen per symbol
        bar_counts: dict[str, int] = {sym: 0 for sym in candles}

        prev_ts: float | None = None

        for ts, sym, bar_idx in timeline:
            bar_counts[sym] = bar_idx + 1

            # Build lookback window for this symbol
            start = max(0, bar_idx + 1 - cfg.lookback)
            window = candles[sym][start : bar_idx + 1]

            # Generate signals
            raw_signals = strategy.generate_signals(window)

            # Adapt Signal objects to BacktestSignal if needed
            signals = []
            for s in raw_signals:
                if hasattr(s, "signal_type"):
                    signals.append(s)
                else:
                    # Convert data.Signal to BacktestSignal
                    # Only consider signals from the current bar (last bar
                    # in the window) to avoid replaying old signals.
                    sig_ts = getattr(s, "timestamp", 0.0)
                    if sig_ts != window[-1].timestamp:
                        continue

                    side = getattr(s, "side", "buy")
                    # Always use the actual candle symbol so positions
                    # are keyed consistently with the candle data.
                    sig_symbol = sym

                    # If we already hold a position in this symbol and the
                    # signal is in the opposite direction, emit a CLOSE
                    # instead of opening a new position.
                    if side == "buy":
                        if sig_symbol in positions and positions[sig_symbol].side == "sell":
                            st = SignalType.CLOSE
                        else:
                            st = SignalType.LONG
                    else:  # sell
                        if sig_symbol in positions and positions[sig_symbol].side == "buy":
                            st = SignalType.CLOSE
                        else:
                            st = SignalType.SHORT

                    signals.append(
                        BacktestSignal(
                            symbol=sig_symbol,
                            signal_type=st,
                            price=window[-1].close,
                            timestamp=window[-1].timestamp,
                            strength=getattr(s, "strength", 0.5),
                        )
                    )

            for sig in signals:
                if sig.signal_type == SignalType.CLOSE:
                    # Close existing position
                    pos = positions.pop(sig.symbol, None)
                    if pos is None:
                        continue
                    exit_side = "sell" if pos.side == "buy" else "buy"
                    fill, comm = simulate_fill(
                        sig.price,
                        pos.quantity,
                        exit_side,
                        exec_cfg,
                    )
                    if pos.side == "buy":
                        pnl = (fill - pos.entry_price) * pos.quantity
                    else:
                        pnl = (pos.entry_price - fill) * pos.quantity
                    total_comm = comm + pos.commission
                    completed_trades.append(
                        Trade(
                            symbol=sig.symbol,
                            side=pos.side,
                            entry_price=pos.entry_price,
                            exit_price=fill,
                            quantity=pos.quantity,
                            entry_time=pos.entry_time,
                            exit_time=ts,
                            commission=total_comm,
                            pnl=pnl,
                        )
                    )

                elif sig.signal_type in (SignalType.LONG, SignalType.SHORT):
                    if sig.symbol in positions:
                        continue  # already have a position in this symbol
                    if len(positions) >= cfg.max_positions:
                        continue

                    # Position sizing: risk_per_trade * equity
                    risk_amount = cfg.risk_per_trade * equity
                    side_str = "buy" if sig.signal_type == SignalType.LONG else "sell"
                    fill, comm = simulate_fill(sig.price, 1, side_str, exec_cfg)

                    if fill <= 0:
                        continue
                    qty = max(1.0, math.floor(risk_amount / fill))

                    # Re-simulate with actual quantity
                    fill, comm = simulate_fill(sig.price, qty, side_str, exec_cfg)

                    positions[sig.symbol] = Position(
                        symbol=sig.symbol,
                        side=side_str,
                        quantity=qty,
                        entry_price=fill,
                        entry_time=ts,
                        commission=comm,
                    )

            # Mark-to-market equity
            pos_value = 0.0
            for pos_sym, pos in positions.items():
                cnt = bar_counts.get(pos_sym, 0)
                if cnt > 0:
                    cp = candles[pos_sym][cnt - 1].close
                else:
                    cp = pos.entry_price
                pos_value += pos.unrealized_pnl(cp)

            # Equity = initial + realized P&L + unrealized P&L
            realized = sum(t.net_pnl for t in completed_trades)
            equity = cfg.initial_capital + realized + pos_value

            # Only record equity once per unique timestamp
            if ts != prev_ts:
                equity_history.append(equity)
                prev_ts = ts

        # Close any remaining positions at last price
        for sym, pos in list(positions.items()):
            cnt = bar_counts.get(sym, 0)
            last_price = candles[sym][cnt - 1].close if cnt > 0 else pos.entry_price
            exit_side = "sell" if pos.side == "buy" else "buy"
            fill, comm = simulate_fill(
                last_price,
                pos.quantity,
                exit_side,
                exec_cfg,
            )
            if pos.side == "buy":
                pnl = (fill - pos.entry_price) * pos.quantity
            else:
                pnl = (pos.entry_price - fill) * pos.quantity
            completed_trades.append(
                Trade(
                    symbol=sym,
                    side=pos.side,
                    entry_price=pos.entry_price,
                    exit_price=fill,
                    quantity=pos.quantity,
                    entry_time=pos.entry_time,
                    exit_time=timeline[-1][0] if timeline else 0.0,
                    commission=comm + pos.commission,
                    pnl=pnl,
                )
            )
        positions.clear()

        eq_arr = np.array(equity_history, dtype=np.float64)
        return _compute_metrics(eq_arr, completed_trades, self.config)

    # -----------------------------------------------------------------------
    # Walk-forward
    # -----------------------------------------------------------------------

    def walk_forward(
        self,
        strategy_class: type,
        candles: dict[str, list[Candle]],
        n_splits: int = 5,
        in_sample_ratio: float = 0.7,
    ) -> BacktestResult:
        """Walk-forward optimization.

        Splits the data into *n_splits* folds.  For each fold the strategy is
        instantiated on the in-sample portion and evaluated on the
        out-of-sample portion.  Results from all OOS folds are concatenated.
        """
        first_sym = next(iter(candles))
        total_bars = len(candles[first_sym])

        fold_size = total_bars // n_splits
        all_trades: list[Trade] = []
        all_equity: list[float] = [self.config.initial_capital]

        for fold_idx in range(n_splits):
            fold_start = fold_idx * fold_size
            fold_end = min(fold_start + fold_size, total_bars)
            split_point = fold_start + int((fold_end - fold_start) * in_sample_ratio)

            # Build OOS candles
            oos_candles: dict[str, list[Candle]] = {}
            for sym, bars in candles.items():
                oos_candles[sym] = bars[split_point:fold_end]

            if not oos_candles[first_sym]:
                continue

            strategy = strategy_class()
            result = self.run(strategy, oos_candles)
            all_trades.extend(result.trades)

            # Chain equity: scale OOS equity relative to where we left off
            if len(result.equity_curve) > 1:
                scale = all_equity[-1] / result.equity_curve[0] if result.equity_curve[0] != 0 else 1.0
                for val in result.equity_curve[1:]:
                    all_equity.append(val * scale)

        eq_arr = np.array(all_equity, dtype=np.float64)
        return _compute_metrics(eq_arr, all_trades, self.config)

    # -----------------------------------------------------------------------
    # Monte Carlo
    # -----------------------------------------------------------------------

    def monte_carlo(
        self,
        trades: list[Trade],
        n_simulations: int = 1000,
    ) -> dict[str, Any]:
        """Monte Carlo analysis by shuffling trade order.

        Returns a dict with keys:
        - ``median_return``: median total return across simulations
        - ``p5_return``: 5th-percentile return (worst case)
        - ``p95_return``: 95th-percentile return
        - ``median_drawdown``: median max drawdown
        - ``p95_drawdown``: 95th-percentile max drawdown (worst case)
        - ``returns``: array of all simulated total returns
        - ``drawdowns``: array of all simulated max drawdowns
        """
        pnls = [t.net_pnl for t in trades]
        if not pnls:
            return {
                "median_return": 0.0,
                "p5_return": 0.0,
                "p95_return": 0.0,
                "median_drawdown": 0.0,
                "p95_drawdown": 0.0,
                "returns": np.zeros(n_simulations),
                "drawdowns": np.zeros(n_simulations),
            }

        initial = self.config.initial_capital
        sim_returns = np.empty(n_simulations)
        sim_drawdowns = np.empty(n_simulations)

        for i in range(n_simulations):
            shuffled = pnls.copy()
            random.shuffle(shuffled)

            equity = initial
            peak = equity
            worst_dd = 0.0

            for pnl in shuffled:
                equity += pnl
                if equity > peak:
                    peak = equity
                dd = (equity - peak) / peak if peak > 0 else 0.0
                if dd < worst_dd:
                    worst_dd = dd

            sim_returns[i] = (equity - initial) / initial
            sim_drawdowns[i] = worst_dd

        return {
            "median_return": float(np.median(sim_returns)),
            "p5_return": float(np.percentile(sim_returns, 5)),
            "p95_return": float(np.percentile(sim_returns, 95)),
            "median_drawdown": float(np.median(sim_drawdowns)),
            "p95_drawdown": float(np.percentile(sim_drawdowns, 5)),
            "returns": sim_returns,
            "drawdowns": sim_drawdowns,
        }


# ---------------------------------------------------------------------------
# Sample data generator
# ---------------------------------------------------------------------------


def generate_sample_data(
    symbol: str = "AAPL",
    days: int = 252,
    start_price: float = 100.0,
    volatility: float = 0.02,
    seed: int | None = 42,
    trend: float = 0.005,
    regime_period: int = 60,
) -> list[Candle]:
    """Generate synthetic daily OHLCV candles via geometric Brownian motion.

    Parameters
    ----------
    symbol:
        Ticker label attached to each candle.
    days:
        Number of trading days to generate.
    start_price:
        Opening price on day 1.
    volatility:
        Daily return standard deviation.
    seed:
        Optional RNG seed for reproducibility.
    trend:
        Daily drift added to returns.  The sign flips every
        *regime_period* days to create alternating up/down trends that
        give strategies clear trading opportunities.
    regime_period:
        Number of days between trend-direction flips.

    Returns
    -------
    list[Candle]:
        *days* candles sorted ascending by timestamp.
    """
    rng = np.random.default_rng(seed)
    candles: list[Candle] = []
    price = start_price
    base_ts = 1_700_000_000.0  # arbitrary epoch
    current_trend = trend

    for day in range(days):
        # Flip trend direction every regime_period days
        if regime_period > 0 and day > 0 and day % regime_period == 0:
            current_trend = -current_trend

        ts = base_ts + day * 86_400
        daily_return = rng.normal(current_trend, volatility)
        close = price * math.exp(daily_return)

        # Intraday high/low
        intraday_vol = abs(rng.normal(0, volatility * price))
        high = max(price, close) + intraday_vol * 0.5
        low = min(price, close) - intraday_vol * 0.5
        low = max(low, 0.01)  # floor

        volume = float(rng.integers(100_000, 10_000_000))

        candles.append(
            Candle(
                timestamp=ts,
                open=round(price, 4),
                high=round(high, 4),
                low=round(low, 4),
                close=round(close, 4),
                volume=volume,
                symbol=symbol,
            )
        )
        price = close

    return candles
