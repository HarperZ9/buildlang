"""
CLI entry point for build-finance.

Usage:
    build-finance backtest --strategy momentum --data sample --days 252
    build-finance analyze --file trades.csv
    build-finance optimize --file returns.csv --method max_sharpe
    build-finance indicators --symbol AAPL --indicator rsi,macd
    build-finance gui          (launch GUI -- default when no command given)
"""

from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

import numpy as np


def _cmd_backtest(args: argparse.Namespace) -> None:
    """Run a backtest with the specified strategy on sample or file data."""
    from build_finance.backtest import (
        BacktestConfig,
        Backtester,
        generate_sample_data,
    )

    days = args.days or 252
    capital = args.capital or 100_000.0

    # --- load candles -------------------------------------------------------
    if args.data == "sample":
        candles = generate_sample_data(
            symbol=args.symbol or "AAPL",
            days=days,
            seed=42,
        )
    elif args.data and Path(args.data).exists():
        candles = _load_candles_csv(Path(args.data))
    else:
        print(f"Data source '{args.data}' not found. Using sample data.")
        candles = generate_sample_data(
            symbol=args.symbol or "AAPL",
            days=days,
            seed=42,
        )

    symbol = candles[0].symbol or "AAPL"

    # --- pick strategy ------------------------------------------------------
    strategy = _make_strategy(args.strategy or "momentum")

    # --- run ----------------------------------------------------------------
    cfg = BacktestConfig(initial_capital=capital)
    bt = Backtester(cfg)
    result = bt.run(strategy, {symbol: candles})

    print(result.summary())
    print(f"\nEquity curve length: {len(result.equity_curve)} points")

    if args.monte_carlo:
        mc = bt.monte_carlo(result.trades, n_simulations=1000)
        print("\n=== Monte Carlo (1 000 sims) ===")
        print(f"  Median return : {mc['median_return']:.2%}")
        print(f"  5th %-ile     : {mc['p5_return']:.2%}")
        print(f"  95th %-ile    : {mc['p95_return']:.2%}")
        print(f"  Median DD     : {mc['median_drawdown']:.2%}")
        print(f"  Worst-case DD : {mc['p95_drawdown']:.2%}")


def _cmd_analyze(args: argparse.Namespace) -> None:
    """Analyze a CSV file of completed trades."""
    from build_finance.data import BacktestTrade as Trade

    path = Path(args.file)
    if not path.exists():
        print(f"File not found: {path}")
        sys.exit(1)

    trades: list[Trade] = []
    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            trades.append(
                Trade(
                    symbol=row.get("symbol", "???"),
                    side=row.get("side", "buy"),
                    entry_price=float(row.get("entry_price", 0)),
                    exit_price=float(row.get("exit_price", 0)),
                    quantity=float(row.get("quantity", 0)),
                    entry_time=float(row.get("entry_time", 0)),
                    exit_time=float(row.get("exit_time", 0)),
                    commission=float(row.get("commission", 0)),
                    pnl=float(row.get("pnl", 0)),
                )
            )

    if not trades:
        print("No trades found in file.")
        sys.exit(1)

    pnls = np.array([t.net_pnl for t in trades])
    wins = pnls[pnls > 0]
    losses = pnls[pnls <= 0]

    print(f"Trades analyzed : {len(trades)}")
    print(f"Total P&L       : ${pnls.sum():,.2f}")
    print(f"Avg P&L         : ${pnls.mean():,.2f}")
    print(f"Win rate        : {len(wins) / len(trades):.2%}")
    if len(wins):
        print(f"Avg win         : ${wins.mean():,.2f}")
    else:
        print("Avg win         : N/A")
    if len(losses):
        print(f"Avg loss        : ${losses.mean():,.2f}")
    else:
        print("Avg loss        : N/A")
    print(f"Best trade      : ${pnls.max():,.2f}")
    print(f"Worst trade     : ${pnls.min():,.2f}")


def _cmd_optimize(args: argparse.Namespace) -> None:
    """Run portfolio optimization on a returns CSV."""
    from build_finance.portfolio import (
        hierarchical_risk_parity,
        mean_variance_optimize,
        portfolio_stats,
        risk_parity_weights,
    )

    path = Path(args.file)
    if not path.exists():
        print(f"File not found: {path}")
        sys.exit(1)

    # Load CSV: rows = dates, columns = assets
    data = np.genfromtxt(path, delimiter=",", skip_header=1)
    if data.ndim == 1:
        data = data.reshape(-1, 1)

    method = args.method or "max_sharpe"
    rf = args.risk_free or 0.02

    if method in ("max_sharpe", "min_variance", "max_return"):
        weights = mean_variance_optimize(data, target=method, risk_free=rf)
    elif method == "risk_parity":
        cov = np.cov(data, rowvar=False) * 252
        weights = risk_parity_weights(cov)
    elif method == "hrp":
        weights = hierarchical_risk_parity(data)
    else:
        print(f"Unknown method: {method}")
        sys.exit(1)

    stats = portfolio_stats(weights, data, risk_free=rf)

    print(f"=== Portfolio Optimization ({method}) ===")
    print(f"  Expected return : {stats['return']:.2%}")
    print(f"  Volatility      : {stats['volatility']:.2%}")
    print(f"  Sharpe ratio    : {stats['sharpe']:.4f}")
    print("  Weights:")
    for i, w in enumerate(weights):
        print(f"    Asset {i}: {w:.4f} ({w * 100:.1f}%)")


def _cmd_indicators(args: argparse.Namespace) -> None:
    """Compute technical indicators on sample data and print."""
    from build_finance.backtest import generate_sample_data

    symbol = args.symbol or "AAPL"
    candles = generate_sample_data(symbol=symbol, days=50, seed=42)
    closes = np.array([c.close for c in candles])

    indicators = (args.indicator or "rsi,sma").split(",")

    for name in indicators:
        name = name.strip().lower()
        if name == "sma":
            period = 14
            if len(closes) >= period:
                sma = np.convolve(closes, np.ones(period) / period, mode="valid")
                print(f"SMA({period}) last 5: {sma[-5:].round(2).tolist()}")
        elif name == "ema":
            period = 14
            alpha = 2.0 / (period + 1)
            ema = np.empty_like(closes)
            ema[0] = closes[0]
            for i in range(1, len(closes)):
                ema[i] = alpha * closes[i] + (1 - alpha) * ema[i - 1]
            print(f"EMA({period}) last 5: {ema[-5:].round(2).tolist()}")
        elif name == "rsi":
            period = 14
            deltas = np.diff(closes)
            gains = np.where(deltas > 0, deltas, 0.0)
            losses_arr = np.where(deltas < 0, -deltas, 0.0)
            avg_gain = np.convolve(gains, np.ones(period) / period, mode="valid")
            avg_loss = np.convolve(losses_arr, np.ones(period) / period, mode="valid")
            rs = avg_gain / np.where(avg_loss == 0, 1e-10, avg_loss)
            rsi = 100 - 100 / (1 + rs)
            print(f"RSI({period}) last 5: {rsi[-5:].round(2).tolist()}")
        elif name == "macd":
            fast, slow, sig = 12, 26, 9
            if len(closes) >= slow:

                def _ema(arr, span):
                    a = 2.0 / (span + 1)
                    out = np.empty_like(arr)
                    out[0] = arr[0]
                    for j in range(1, len(arr)):
                        out[j] = a * arr[j] + (1 - a) * out[j - 1]
                    return out

                ema_fast = _ema(closes, fast)
                ema_slow = _ema(closes, slow)
                macd_line = ema_fast - ema_slow
                signal_line = _ema(macd_line, sig)
                histogram = macd_line - signal_line
                print(f"MACD last 5: {macd_line[-5:].round(4).tolist()}")
                print(f"Signal last 5: {signal_line[-5:].round(4).tolist()}")
                print(f"Histogram last 5: {histogram[-5:].round(4).tolist()}")
            else:
                print(f"Need at least {slow} bars for MACD.")
        else:
            print(f"Unknown indicator: {name}")


def _cmd_gui(_args: argparse.Namespace) -> None:
    """Launch the PyQt6 GUI."""
    try:
        from build_finance.gui import launch

        launch()
    except ImportError:
        print("GUI requires PyQt6. Install with: pip install 'build-finance[gui]'")
        print("Use CLI commands in the meantime: backtest, analyze, optimize, indicators")


# ---------------------------------------------------------------------------
# Strategy factory
# ---------------------------------------------------------------------------


class _MomentumStrategy:
    """Simple momentum crossover strategy for CLI demos."""

    def __init__(self, fast: int = 10, slow: int = 30) -> None:
        self.fast = fast
        self.slow = slow

    def generate_signals(self, candles):
        from build_finance.data import BacktestSignal as Signal
        from build_finance.data import SignalType

        if len(candles) < self.slow:
            return []

        closes = [c.close for c in candles]
        fast_ma = sum(closes[-self.fast :]) / self.fast
        slow_ma = sum(closes[-self.slow :]) / self.slow
        prev_closes = closes[:-1]

        signals = []
        bar = candles[-1]

        if len(prev_closes) >= self.slow:
            prev_fast = sum(prev_closes[-self.fast :]) / self.fast
            prev_slow = sum(prev_closes[-self.slow :]) / self.slow

            if fast_ma > slow_ma and prev_fast <= prev_slow:
                signals.append(
                    Signal(
                        symbol=bar.symbol,
                        signal_type=SignalType.LONG,
                        price=bar.close,
                        timestamp=bar.timestamp,
                    )
                )
            elif fast_ma < slow_ma and prev_fast >= prev_slow:
                signals.append(
                    Signal(
                        symbol=bar.symbol,
                        signal_type=SignalType.CLOSE,
                        price=bar.close,
                        timestamp=bar.timestamp,
                    )
                )

        return signals


class _MeanReversionStrategy:
    """Bollinger-band mean-reversion strategy for CLI demos."""

    def __init__(self, period: int = 20, num_std: float = 2.0) -> None:
        self.period = period
        self.num_std = num_std

    def generate_signals(self, candles):
        from build_finance.data import BacktestSignal as Signal
        from build_finance.data import SignalType

        if len(candles) < self.period:
            return []

        closes = np.array([c.close for c in candles[-self.period :]])
        mean = closes.mean()
        std = closes.std()
        bar = candles[-1]

        signals = []
        if bar.close < mean - self.num_std * std:
            signals.append(
                Signal(
                    symbol=bar.symbol,
                    signal_type=SignalType.LONG,
                    price=bar.close,
                    timestamp=bar.timestamp,
                )
            )
        elif bar.close > mean + self.num_std * std:
            signals.append(
                Signal(
                    symbol=bar.symbol,
                    signal_type=SignalType.CLOSE,
                    price=bar.close,
                    timestamp=bar.timestamp,
                )
            )

        return signals


def _make_strategy(name: str):
    """Create a strategy instance by name."""
    strategies = {
        "momentum": _MomentumStrategy,
        "mean_reversion": _MeanReversionStrategy,
    }
    cls = strategies.get(name)
    if cls is None:
        print(f"Unknown strategy: {name}")
        print(f"Available: {', '.join(strategies)}")
        sys.exit(1)
    return cls()


# ---------------------------------------------------------------------------
# CSV loader
# ---------------------------------------------------------------------------


def _load_candles_csv(path: Path):
    """Load candles from CSV with columns: timestamp,open,high,low,close,volume."""
    from build_finance.data import Candle

    candles = []
    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            candles.append(
                Candle(
                    timestamp=float(row.get("timestamp", 0)),
                    open=float(row.get("open", 0)),
                    high=float(row.get("high", 0)),
                    low=float(row.get("low", 0)),
                    close=float(row.get("close", 0)),
                    volume=float(row.get("volume", 0)),
                    symbol=row.get("symbol", ""),
                )
            )
    return candles


# ---------------------------------------------------------------------------
# Argument parser
# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="build-finance",
        description="Algorithmic trading toolkit -- strategies, backtesting, portfolio optimization",
    )
    sub = parser.add_subparsers(dest="command")

    # backtest
    bt = sub.add_parser("backtest", help="Run a backtest")
    bt.add_argument("--strategy", default="momentum", help="Strategy name (momentum, mean_reversion)")
    bt.add_argument("--data", default="sample", help="'sample' or path to CSV")
    bt.add_argument("--symbol", default="AAPL", help="Symbol for sample data")
    bt.add_argument("--days", type=int, default=252, help="Trading days for sample data")
    bt.add_argument("--capital", type=float, default=100_000, help="Initial capital")
    bt.add_argument("--monte-carlo", action="store_true", help="Run Monte Carlo analysis")

    # analyze
    az = sub.add_parser("analyze", help="Analyze trade log CSV")
    az.add_argument("--file", required=True, help="Path to trades CSV")

    # optimize
    op = sub.add_parser("optimize", help="Portfolio optimization")
    op.add_argument("--file", required=True, help="Path to returns CSV")
    op.add_argument("--method", default="max_sharpe", help="max_sharpe, min_variance, max_return, risk_parity, hrp")
    op.add_argument("--risk-free", type=float, default=0.02, help="Risk-free rate")

    # indicators
    ind = sub.add_parser("indicators", help="Compute technical indicators")
    ind.add_argument("--symbol", default="AAPL", help="Symbol")
    ind.add_argument("--indicator", default="rsi,sma", help="Comma-separated: rsi,sma,ema,macd")

    # gui
    sub.add_parser("gui", help="Launch GUI (default)")

    return parser


def main(argv: list[str] | None = None) -> None:
    """CLI entry point."""
    parser = build_parser()
    args = parser.parse_args(argv)

    dispatch = {
        "backtest": _cmd_backtest,
        "analyze": _cmd_analyze,
        "optimize": _cmd_optimize,
        "indicators": _cmd_indicators,
        "gui": _cmd_gui,
    }

    handler = dispatch.get(args.command)
    if handler is None:
        # Default to gui when no command given
        _cmd_gui(args)
    else:
        handler(args)


if __name__ == "__main__":
    main()
