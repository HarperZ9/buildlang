"""
Build Oracle — Command-Line Interface

Commands:
    build-oracle forecast    --data sample --model arima --horizon 30
    build-oracle decompose   --data sample --period 7
    build-oracle changepoints --data sample --penalty bic
    build-oracle features    --data sample
    build-oracle gui         (launch GUI, default if no command)
"""

import argparse
import math
import sys
from typing import Any

# =============================================================================
# Sample Data Generation
# =============================================================================


def generate_sample_series(
    n: int = 365,
    trend: float = 0.01,
    seasonality: bool = True,
    noise: float = 0.1,
    changepoints: int = 2,
) -> list[float]:
    """
    Create a realistic time series with trend, seasonality, noise,
    and optional structural changepoints.

    Parameters
    ----------
    n : int
        Number of data points.
    trend : float
        Linear trend slope per time step.
    seasonality : bool
        If True, add a weekly-period sine wave.
    noise : float
        Standard deviation of Gaussian noise.
    changepoints : int
        Number of level shifts to inject.

    Returns
    -------
    list[float]
        Generated time series values.
    """
    import random

    random.seed(42)
    values: list[float] = []

    # Pre-compute changepoint positions and magnitudes
    cp_positions: list[int] = []
    cp_shifts: list[float] = []
    if changepoints > 0:
        segment = n // (changepoints + 1)
        for i in range(changepoints):
            pos = segment * (i + 1) + random.randint(-segment // 4, segment // 4)
            cp_positions.append(max(1, min(pos, n - 1)))
            cp_shifts.append(random.uniform(-2.0, 2.0))
        cp_positions.sort()

    level = 10.0  # base level
    cp_idx = 0

    for t in range(n):
        # Apply changepoint shifts
        if cp_idx < len(cp_positions) and t == cp_positions[cp_idx]:
            level += cp_shifts[cp_idx]
            cp_idx += 1

        value = level + trend * t

        # Weekly seasonality (period = 7)
        if seasonality:
            value += 1.5 * math.sin(2 * math.pi * t / 7.0)
            value += 0.5 * math.cos(2 * math.pi * t / 30.0)

        # Gaussian noise
        value += random.gauss(0, noise)
        values.append(value)

    return values


# =============================================================================
# Forecast Command
# =============================================================================


def _cmd_forecast(args: argparse.Namespace) -> None:
    """Run ARIMA or Prophet forecast and print results."""
    import numpy as np

    model: Any  # holds whichever forecaster type this command selects

    # ---- Load from saved model file ------------------------------------
    if args.load:
        import json

        print(f"  Loading model from: {args.load}")
        with open(args.load) as f:
            state = json.load(f)
        model_type = state.get("model_type", "")

        if model_type == "arima":
            from build_oracle.arima import ARIMA

            model = ARIMA._from_state(state)
            model_name = "arima"
        elif model_type == "prophet":
            from build_oracle.prophet import Prophet

            model = Prophet._from_state(state)
            model_name = "prophet"
        elif model_type == "neural":
            from build_oracle.neural import SimpleForecaster

            model = SimpleForecaster._from_state(state)
            model_name = "neural"
        elif model_type == "ensemble":
            print("  Note: ensemble models cannot be loaded from file (re-fit required).")
            sys.exit(1)
        else:
            print(f"  Error: unknown model_type '{model_type}' in saved file.")
            sys.exit(1)

        horizon = args.horizon
        print(f"  Model       : {model_name.upper()} (loaded)")
        print(f"  Horizon     : {horizon} steps")
        print()

        # For loaded models we generate sample data just for evaluation
        if args.data == "sample":
            series = generate_sample_series(n=365)
            arr = np.array(series, dtype=np.float64)
            actual = arr[-horizon:]
        else:
            actual = None

        if model_name == "neural":
            series_data = np.array(generate_sample_series(n=365), dtype=np.float64)
            forecast = model.predict(series_data)
            forecast = np.asarray(forecast, dtype=np.float64)[:horizon]
        elif model_name == "prophet":
            result = model.predict(np.arange(len(arr) - horizon, len(arr), dtype=np.float64))
            forecast = np.asarray(result["yhat"], dtype=np.float64)[:horizon]
        else:
            forecast = model.predict(horizon)
            forecast = np.asarray(forecast, dtype=np.float64)[:horizon]

        if actual is not None:
            errors = actual - forecast[: len(actual)]
            mae = float(np.mean(np.abs(errors)))
            rmse = float(np.sqrt(np.mean(errors**2)))
            nonzero = actual[actual != 0]
            if len(nonzero) > 0:
                mape = float(np.mean(np.abs(errors[actual != 0] / nonzero)) * 100)
            else:
                mape = 0.0
            print("  --- Metrics ---")
            print(f"  MAE  : {mae:.4f}")
            print(f"  RMSE : {rmse:.4f}")
            print(f"  MAPE : {mape:.2f}%")
            print()

        print("  --- Forecast (first 5 / last 5) ---")
        fc = forecast.tolist()
        for i, v in enumerate(fc[:5]):
            print(f"    t+{i + 1:3d}: {v:.4f}")
        if len(fc) > 10:
            print(f"    {'...':>8}")
        for i, v in enumerate(fc[-5:]):
            idx = len(fc) - 5 + i + 1
            print(f"    t+{idx:3d}: {v:.4f}")
        return

    # ---- Multivariate (VAR) path ----------------------------------------
    if getattr(args, "multivariate", False):
        import numpy as np

        horizon = args.horizon

        # Generate a 2-column sample series
        series1 = generate_sample_series(n=365, trend=0.01, noise=0.1)
        series2 = generate_sample_series(n=365, trend=0.02, noise=0.15, changepoints=1)
        data = np.column_stack([series1, series2])
        print(f"  Data source : multivariate sample ({data.shape[0]} x {data.shape[1]})")
        print("  Model       : VAR")
        print(f"  Horizon     : {horizon} steps")
        print()

        from build_oracle.var import VAR as VARModel

        model = VARModel(p=2)
        model.fit(data)
        forecast = model.predict(horizon)

        if args.save:
            model.save(args.save)
            print(f"  Model saved to: {args.save}")
            print()

        print("  --- Forecast (first 5 / last 5) — Variable 1 ---")
        fc = forecast[:, 0].tolist()
        for i, v in enumerate(fc[:5]):
            print(f"    t+{i + 1:3d}: {v:.4f}")
        if len(fc) > 10:
            print(f"    {'...':>8}")
        for i, v in enumerate(fc[-5:]):
            idx = len(fc) - 5 + i + 1
            print(f"    t+{idx:3d}: {v:.4f}")
        return

    # ---- Normal fit-and-forecast path ----------------------------------
    # Load data
    if args.data == "sample":
        series = generate_sample_series(n=365)
        print(f"  Data source : sample ({len(series)} points)")
    else:
        print(f"  Error: unknown data source '{args.data}'")
        sys.exit(1)

    horizon = args.horizon
    model_name = args.model.lower()
    print(f"  Model       : {model_name.upper()}")
    print(f"  Horizon     : {horizon} steps")
    print()

    arr = np.array(series, dtype=np.float64)
    train = arr[:-horizon]
    actual = arr[-horizon:]

    if model_name == "arima":
        try:
            from build_oracle.arima import ARIMA

            model = ARIMA(p=2, d=1, q=2)
            model.fit(train)
            forecast = model.predict(horizon)
        except ImportError:
            # Fallback: simple linear extrapolation
            x = np.arange(len(train))
            slope = np.polyfit(x, train, 1)[0]
            last_val = train[-1]
            forecast = np.array([last_val + slope * (i + 1) for i in range(horizon)])
    elif model_name == "prophet":
        try:
            from build_oracle.prophet import Prophet

            t_train = np.arange(len(train), dtype=np.float64)
            model = Prophet()
            model.fit(t_train, train)
            t_future = np.arange(len(train), len(train) + horizon, dtype=np.float64)
            result = model.predict(t_future)
            forecast = result["yhat"]
        except ImportError:
            # Fallback: seasonal naive
            period = 7
            forecast = np.array([train[-(period - i % period)] for i in range(horizon)])
    elif model_name == "ensemble":
        try:
            from build_oracle.ensemble import EnsembleForecaster

            model = EnsembleForecaster()
            model.fit(train)
            forecast = model.predict(horizon)
            # Print ensemble weights
            print("  --- Ensemble Weights ---")
            for name, w in model.weights.items():
                print(f"    {name:>8s}: {w:.4f}")
            print()
        except ImportError:
            print("  Error: ensemble module not available.")
            sys.exit(1)
    else:
        print(f"  Error: unknown model '{model_name}'. Choose arima, prophet, or ensemble.")
        sys.exit(1)

    forecast = np.asarray(forecast, dtype=np.float64)[:horizon]

    # ---- Optionally save the fitted model ------------------------------
    if args.save:
        model.save(args.save)
        print(f"  Model saved to: {args.save}")
        print()

    # Metrics
    errors = actual - forecast[: len(actual)]
    mae = float(np.mean(np.abs(errors)))
    rmse = float(np.sqrt(np.mean(errors**2)))
    nonzero = actual[actual != 0]
    if len(nonzero) > 0:
        mape = float(np.mean(np.abs(errors[actual != 0] / nonzero)) * 100)
    else:
        mape = 0.0

    print("  --- Metrics ---")
    print(f"  MAE  : {mae:.4f}")
    print(f"  RMSE : {rmse:.4f}")
    print(f"  MAPE : {mape:.2f}%")
    print()

    # Print first/last forecast values
    print("  --- Forecast (first 5 / last 5) ---")
    fc = forecast.tolist()
    for i, v in enumerate(fc[:5]):
        print(f"    t+{i + 1:3d}: {v:.4f}")
    if len(fc) > 10:
        print(f"    {'...':>8}")
    for i, v in enumerate(fc[-5:]):
        idx = len(fc) - 5 + i + 1
        print(f"    t+{idx:3d}: {v:.4f}")


# =============================================================================
# Decompose Command
# =============================================================================


def _cmd_decompose(args: argparse.Namespace) -> None:
    """Decompose a time series and report component strengths."""
    import numpy as np

    if args.data == "sample":
        series = generate_sample_series(n=365)
        print(f"  Data source : sample ({len(series)} points)")
    else:
        print(f"  Error: unknown data source '{args.data}'")
        sys.exit(1)

    period = args.period
    model = args.model
    print(f"  Period      : {period}")
    print(f"  Model       : {model}")
    print()

    arr = np.array(series, dtype=np.float64)

    try:
        from build_oracle.decompose import classical_decompose

        result = classical_decompose(arr, period=period, model=model)
        trend_comp = result["trend"]
        seasonal_comp = result["seasonal"]
        residual_comp = result["residual"]
    except (ImportError, KeyError):
        # Fallback: simple moving-average decomposition
        kernel = np.ones(period) / period
        trend_comp = np.convolve(arr, kernel, mode="same")
        detrended = arr - trend_comp
        # Average seasonal pattern
        seasonal_comp = np.zeros_like(arr)
        for i in range(period):
            indices = list(range(i, len(arr), period))
            mean_val = np.mean(detrended[indices])
            for idx in indices:
                seasonal_comp[idx] = mean_val
        residual_comp = arr - trend_comp - seasonal_comp

    # Filter NaN values for strength calculation
    mask = ~(np.isnan(trend_comp) | np.isnan(seasonal_comp) | np.isnan(residual_comp))
    arr_clean = arr[mask]
    trend_clean = trend_comp[mask]
    seasonal_clean = seasonal_comp[mask]
    residual_clean = residual_comp[mask]

    # Strength measures (based on variance ratios)
    var_remainder = float(np.var(residual_clean)) if len(residual_clean) > 0 else 0
    var_detrended = float(np.var(arr_clean - trend_clean)) if len(arr_clean) > 0 else 0
    var_deseasoned = float(np.var(arr_clean - seasonal_clean)) if len(arr_clean) > 0 else 0

    trend_strength = max(0, 1 - var_remainder / var_deseasoned) if var_deseasoned > 0 else 0
    seasonal_strength = max(0, 1 - var_remainder / var_detrended) if var_detrended > 0 else 0

    print("  --- Component Strengths ---")
    print(f"  Trend      : {trend_strength:.4f}")
    print(f"  Seasonal   : {seasonal_strength:.4f}")
    print()
    print(f"  Trend range    : [{float(np.nanmin(trend_comp)):.2f}, {float(np.nanmax(trend_comp)):.2f}]")
    print(f"  Seasonal range : [{float(np.nanmin(seasonal_comp)):.2f}, {float(np.nanmax(seasonal_comp)):.2f}]")
    print(f"  Residual std   : {float(np.nanstd(residual_comp)):.4f}")


# =============================================================================
# Changepoints Command
# =============================================================================


def _cmd_changepoints(args: argparse.Namespace) -> None:
    """Detect changepoints in a time series."""
    import numpy as np

    if args.data == "sample":
        series = generate_sample_series(n=365, changepoints=3)
        print(f"  Data source : sample ({len(series)} points)")
    else:
        print(f"  Error: unknown data source '{args.data}'")
        sys.exit(1)

    penalty = args.penalty.upper()
    print(f"  Penalty     : {penalty}")
    print()

    arr = np.array(series, dtype=np.float64)

    try:
        from build_oracle.changepoint import pelt as pelt_detect

        cps = pelt_detect(arr, penalty=penalty.lower())
    except ImportError:
        # Fallback: CUSUM-like detection
        cps = []
        mean_val = float(np.mean(arr))
        cumsum = 0.0
        threshold = float(np.std(arr)) * 3.0
        for i in range(1, len(arr)):
            cumsum += arr[i] - mean_val
            if abs(cumsum) > threshold:
                cps.append(i)
                cumsum = 0.0
                mean_val = float(np.mean(arr[i : min(i + 30, len(arr))]))

    if not cps:
        print("  No changepoints detected.")
        return

    print(f"  Detected {len(cps)} changepoint(s):")
    print()
    print(f"  {'Index':>8}  {'Confidence':>10}  {'Left Mean':>10}  {'Right Mean':>10}")
    print(f"  {'-----':>8}  {'----------':>10}  {'---------':>10}  {'----------':>10}")

    for cp in cps:
        left_seg = arr[max(0, cp - 20) : cp]
        right_seg = arr[cp : min(len(arr), cp + 20)]
        left_mean = float(np.mean(left_seg)) if len(left_seg) > 0 else 0.0
        right_mean = float(np.mean(right_seg)) if len(right_seg) > 0 else 0.0
        shift = abs(right_mean - left_mean)
        confidence = min(1.0, shift / (float(np.std(arr)) + 1e-8))
        print(f"  {cp:>8d}  {confidence:>10.4f}  {left_mean:>10.4f}  {right_mean:>10.4f}")


# =============================================================================
# Features Command
# =============================================================================


def _cmd_features(args: argparse.Namespace) -> None:
    """Extract and display features from a time series."""
    import numpy as np

    if args.data == "sample":
        series = generate_sample_series(n=365)
        print(f"  Data source : sample ({len(series)} points)")
    else:
        print(f"  Error: unknown data source '{args.data}'")
        sys.exit(1)

    print()
    arr = np.array(series, dtype=np.float64)

    # Statistical features
    print("  --- Statistical Features ---")
    print(f"  Mean            : {float(np.mean(arr)):.4f}")
    print(f"  Std Dev         : {float(np.std(arr)):.4f}")
    print(f"  Min             : {float(np.min(arr)):.4f}")
    print(f"  Max             : {float(np.max(arr)):.4f}")
    print(f"  Skewness        : {float(_skewness(arr)):.4f}")
    print(f"  Kurtosis        : {float(_kurtosis(arr)):.4f}")
    print()

    # Temporal features
    diffs = np.diff(arr)
    print("  --- Temporal Features ---")
    print(f"  Trend slope     : {float(np.polyfit(np.arange(len(arr)), arr, 1)[0]):.6f}")
    print(f"  Mean |diff|     : {float(np.mean(np.abs(diffs))):.4f}")
    print(f"  Zero-cross rate : {float(_zero_crossing_rate(diffs)):.4f}")
    print(f"  Autocorr lag-1  : {float(_autocorrelation(arr, 1)):.4f}")
    print(f"  Autocorr lag-7  : {float(_autocorrelation(arr, 7)):.4f}")
    print()

    # Frequency features
    print("  --- Frequency Features ---")
    fft_vals = np.abs(np.fft.rfft(arr - np.mean(arr)))
    dominant_freq = int(np.argmax(fft_vals[1:])) + 1
    spectral_entropy = _spectral_entropy(fft_vals)
    print(f"  Dominant freq   : {dominant_freq} (period ~ {len(arr) // dominant_freq})")
    print(f"  Spectral energy : {float(np.sum(fft_vals**2)):.2f}")
    print(f"  Spectral entropy: {spectral_entropy:.4f}")


def _skewness(arr) -> float:
    import numpy as np

    len(arr)
    mean = np.mean(arr)
    std = np.std(arr)
    if std == 0:
        return 0.0
    return float(np.mean(((arr - mean) / std) ** 3))


def _kurtosis(arr) -> float:
    import numpy as np

    mean = np.mean(arr)
    std = np.std(arr)
    if std == 0:
        return 0.0
    return float(np.mean(((arr - mean) / std) ** 4) - 3.0)


def _zero_crossing_rate(arr) -> float:
    import numpy as np

    signs = np.sign(arr)
    crossings = np.sum(np.abs(np.diff(signs)) > 0)
    return float(crossings / len(arr))


def _autocorrelation(arr, lag: int) -> float:
    import numpy as np

    if lag >= len(arr):
        return 0.0
    mean = np.mean(arr)
    var = np.var(arr)
    if var == 0:
        return 0.0
    return float(np.mean((arr[:-lag] - mean) * (arr[lag:] - mean)) / var)


def _spectral_entropy(fft_vals) -> float:
    import numpy as np

    power = fft_vals**2
    total = np.sum(power)
    if total == 0:
        return 0.0
    probs = power / total
    probs = probs[probs > 0]
    return float(-np.sum(probs * np.log2(probs)))


# =============================================================================
# GUI Launch
# =============================================================================


def _cmd_gui(args: argparse.Namespace) -> None:
    """Launch the Build Oracle GUI."""
    try:
        from build_oracle.gui import launch

        sys.exit(launch())
    except ImportError as e:
        print("  Error: GUI requires PyQt6.  pip install PyQt6")
        print(f"  ({e})")
        sys.exit(1)


# =============================================================================
# Entry Point
# =============================================================================


def main(argv: list[str] | None = None) -> None:
    """Parse arguments and dispatch to the appropriate command."""
    parser = argparse.ArgumentParser(
        prog="build-oracle",
        description="Build Oracle -- Time Series Forecasting & Anomaly Detection",
    )
    sub = parser.add_subparsers(dest="command")

    # forecast
    p_fc = sub.add_parser("forecast", help="Run a forecast model")
    p_fc.add_argument("--data", default="sample", help="Data source (default: sample)")
    p_fc.add_argument(
        "--model", default="arima", choices=["arima", "prophet", "ensemble"], help="Forecast model (default: arima)"
    )
    p_fc.add_argument("--horizon", type=int, default=30, help="Forecast horizon (default: 30)")
    p_fc.add_argument("--save", default=None, metavar="PATH", help="Save fitted model to PATH after training")
    p_fc.add_argument("--load", default=None, metavar="PATH", help="Load a saved model from PATH instead of fitting")
    p_fc.add_argument(
        "--multivariate", action="store_true", default=False, help="Use VAR model on a 2-column sample series"
    )

    # decompose
    p_dc = sub.add_parser("decompose", help="Decompose time series")
    p_dc.add_argument("--data", default="sample", help="Data source (default: sample)")
    p_dc.add_argument("--period", type=int, default=7, help="Seasonal period (default: 7)")
    p_dc.add_argument(
        "--model",
        default="additive",
        choices=["additive", "multiplicative"],
        help="Decomposition model (default: additive)",
    )

    # changepoints
    p_cp = sub.add_parser("changepoints", help="Detect changepoints")
    p_cp.add_argument("--data", default="sample", help="Data source (default: sample)")
    p_cp.add_argument(
        "--penalty", default="bic", choices=["bic", "aic", "mbic"], help="Penalty criterion (default: bic)"
    )

    # features
    p_ft = sub.add_parser("features", help="Extract time series features")
    p_ft.add_argument("--data", default="sample", help="Data source (default: sample)")

    # gui
    sub.add_parser("gui", help="Launch the GUI")

    args = parser.parse_args(argv)

    print()
    print("  Build Oracle v1.0.0")
    print("  " + "=" * 40)
    print()

    dispatch = {
        "forecast": _cmd_forecast,
        "decompose": _cmd_decompose,
        "changepoints": _cmd_changepoints,
        "features": _cmd_features,
        "gui": _cmd_gui,
    }

    if args.command is None:
        # Default: launch GUI
        _cmd_gui(args)
    elif args.command in dispatch:
        dispatch[args.command](args)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
