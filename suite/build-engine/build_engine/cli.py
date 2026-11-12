"""
Command-line interface for Build Engine.

Commands::

    build-engine run --symbols AAPL,BTC-USD --paper --cycles 10
    build-engine status
    build-engine backtest --symbols AAPL --days 252
    build-engine gui           (default when invoked with no arguments)
"""

from __future__ import annotations

import argparse
import logging
import sys


def _cmd_run(args: argparse.Namespace) -> None:
    """Run the adaptive engine for N cycles."""
    from build_engine.adaptive_engine import AdaptiveEngine
    from build_engine.config import EngineConfig

    symbols = [s.strip() for s in args.symbols.split(",")]
    models = [m.strip() for m in args.models.split(",")]

    config = EngineConfig(
        symbols=symbols,
        models=models,
        paper_trading=args.paper,
        risk_per_trade=args.risk,
        max_positions=args.max_positions,
        forecast_horizon=args.horizon,
        live_trading_ack=args.live_ack,
    )

    engine = AdaptiveEngine(config)

    print(f"Build Engine starting ({'paper' if args.paper else 'LIVE'})")
    print(f"  Symbols : {symbols}")
    print(f"  Models  : {models}")
    print(f"  Cycles  : {args.cycles or 'unlimited'}")
    print(f"  Interval: {args.interval}s")
    print()

    try:
        engine.run_loop(
            max_cycles=args.cycles,
            interval_seconds=args.interval,
        )
    except KeyboardInterrupt:
        engine.stop()
        print("\nStopped by user.")

    # Final report
    status = engine.get_status()
    print()
    print("=== Final Status ===")
    print(f"  Cycles completed : {status['cycles']}")
    print(f"  Equity           : ${status['equity']:,.2f}")
    print(f"  Open positions   : {status['positions']}")
    print(f"  Total trades     : {status['trades']}")
    print(f"  Model accuracy   : {status['accuracy']:.0%}")
    print(f"  Models           : {status['models']}")
    print(f"  Model weights    : {status['model_weights']}")


def _cmd_backtest(args: argparse.Namespace) -> None:
    """Run a backtest using PredictionStrategy on synthetic or real data."""
    from build_finance.backtest import BacktestConfig, Backtester, generate_sample_data

    from build_engine.config import EngineConfig
    from build_engine.prediction_strategy import PredictionStrategy

    symbols = [s.strip() for s in args.symbols.split(",")]
    models = [m.strip() for m in args.models.split(",")]

    engine_config = EngineConfig(
        symbols=symbols,
        models=models,
        forecast_horizon=args.horizon,
        min_confidence=0.3,  # Lower threshold for backtest to generate more signals
    )

    strategy = PredictionStrategy(engine_config)

    # Generate sample data for each symbol
    candle_data: dict = {}
    for symbol in symbols:
        candles = generate_sample_data(
            symbol=symbol,
            days=args.days,
            start_price=100.0,
            volatility=0.02,
            seed=42,
        )
        candle_data[symbol] = candles

    bt_config = BacktestConfig(
        initial_capital=args.capital,
        risk_per_trade=0.10,
        max_positions=5,
    )

    print(f"Backtesting PredictionStrategy on {symbols} for {args.days} days...")
    print(f"  Models  : {models}")
    print(f"  Capital : ${args.capital:,.0f}")
    print()

    bt = Backtester(bt_config)
    result = bt.run(strategy, candle_data)
    print(result.summary())

    if args.monte_carlo:
        print()
        print("Running Monte Carlo analysis (1000 simulations)...")
        mc = bt.monte_carlo(result.trades, n_simulations=1000)
        print(f"  Median return   : {mc['median_return']:.2%}")
        print(f"  5th percentile  : {mc['p5_return']:.2%}")
        print(f"  95th percentile : {mc['p95_return']:.2%}")
        print(f"  Median drawdown : {mc['median_drawdown']:.2%}")


def _cmd_status(args: argparse.Namespace) -> None:
    """Display engine status (placeholder for a persistent daemon)."""
    print("Build Engine status")
    print("  No running engine detected.")
    print("  Use 'build-engine run' to start a new session.")


def _cmd_gui(args: argparse.Namespace) -> None:
    """Launch the GUI (requires PyQt6)."""
    try:
        from build_engine.gui import launch
    except ImportError:
        print("GUI is private/license-gated. Install a compatible Qt binding in your local environment.")
        sys.exit(1)

    sys.exit(launch())


def main(argv: list[str] | None = None) -> None:
    """Entry point for ``build-engine`` CLI."""
    parser = argparse.ArgumentParser(
        prog="build-engine",
        description="Self-improving prediction and trading engine",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Enable debug logging",
    )
    subparsers = parser.add_subparsers(dest="command")

    # -- run ---------------------------------------------------------------
    p_run = subparsers.add_parser("run", help="Run the adaptive engine")
    p_run.add_argument(
        "--symbols",
        default="AAPL,BTC-USD",
        help="Comma-separated ticker symbols (default: AAPL,BTC-USD)",
    )
    p_run.add_argument(
        "--models",
        default="arima,prophet",
        help="Comma-separated model names (default: arima,prophet)",
    )
    p_run.add_argument(
        "--paper",
        action="store_true",
        default=True,
        help="Use paper trading (default)",
    )
    p_run.add_argument(
        "--live",
        action="store_true",
        help="Use live trading (overrides --paper)",
    )
    p_run.add_argument(
        "--live-ack",
        default="",
        help="Required live-mode acknowledgement value; also accepted from BUILD_ENGINE_LIVE_ACK.",
    )
    p_run.add_argument(
        "--cycles",
        type=int,
        default=None,
        help="Max cycles to run (default: unlimited)",
    )
    p_run.add_argument(
        "--interval",
        type=int,
        default=300,
        help="Seconds between cycles (default: 300)",
    )
    p_run.add_argument(
        "--risk",
        type=float,
        default=0.02,
        help="Risk per trade as fraction (default: 0.02)",
    )
    p_run.add_argument(
        "--max-positions",
        type=int,
        default=5,
        help="Maximum simultaneous positions (default: 5)",
    )
    p_run.add_argument(
        "--horizon",
        type=int,
        default=5,
        help="Forecast horizon in steps (default: 5)",
    )
    p_run.set_defaults(func=_cmd_run)

    # -- backtest ----------------------------------------------------------
    p_bt = subparsers.add_parser("backtest", help="Backtest the prediction strategy")
    p_bt.add_argument(
        "--symbols",
        default="AAPL",
        help="Comma-separated ticker symbols (default: AAPL)",
    )
    p_bt.add_argument(
        "--models",
        default="arima,prophet",
        help="Comma-separated model names (default: arima,prophet)",
    )
    p_bt.add_argument(
        "--days",
        type=int,
        default=252,
        help="Number of trading days (default: 252)",
    )
    p_bt.add_argument(
        "--capital",
        type=float,
        default=100_000,
        help="Initial capital (default: 100000)",
    )
    p_bt.add_argument(
        "--horizon",
        type=int,
        default=5,
        help="Forecast horizon in steps (default: 5)",
    )
    p_bt.add_argument(
        "--monte-carlo",
        action="store_true",
        help="Run Monte Carlo analysis after backtest",
    )
    p_bt.set_defaults(func=_cmd_backtest)

    # -- status ------------------------------------------------------------
    p_status = subparsers.add_parser("status", help="Show engine status")
    p_status.set_defaults(func=_cmd_status)

    # -- gui ---------------------------------------------------------------
    p_gui = subparsers.add_parser("gui", help="Launch the GUI")
    p_gui.set_defaults(func=_cmd_gui)

    # -- parse & dispatch --------------------------------------------------
    args = parser.parse_args(argv)

    # Logging setup
    level = logging.DEBUG if args.verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        datefmt="%H:%M:%S",
    )

    if hasattr(args, "func"):
        # Handle --live flag overriding --paper
        if hasattr(args, "live") and args.live:
            args.paper = False
        args.func(args)
    else:
        # Default to GUI when no subcommand is given
        _cmd_gui(args)


if __name__ == "__main__":
    main()
