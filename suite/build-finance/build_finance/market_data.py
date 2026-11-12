"""
Real market data fetching for crypto and traditional equities.

Provides functions to pull OHLCV data from free public APIs (Yahoo Finance,
CoinGecko) using only the Python standard library, plus CSV import/export
utilities.

Functions
---------
fetch_yahoo      - Download OHLCV from Yahoo Finance (no API key needed)
fetch_coingecko  - Download crypto OHLCV from CoinGecko (free, no key)
load_csv         - Load candles from a CSV file
save_csv         - Persist candles to a CSV file
generate_sample_data - Create synthetic candles for testing
list_available_symbols - List built-in popular symbols
"""

from __future__ import annotations

import csv
import json
import logging
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone

import numpy as np

from build_finance.data import Candle

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Data source metadata
# ---------------------------------------------------------------------------


@dataclass
class DataSource:
    """Metadata describing a market-data source."""

    name: str
    asset_type: str  # "stock" or "crypto"
    base_url: str = ""
    requires_key: bool = False


YAHOO_SOURCE = DataSource(
    name="yahoo",
    asset_type="stock",
    base_url="https://query1.finance.yahoo.com",
    requires_key=False,
)

COINGECKO_SOURCE = DataSource(
    name="coingecko",
    asset_type="crypto",
    base_url="https://api.coingecko.com/api/v3",
    requires_key=False,
)


# ---------------------------------------------------------------------------
# Popular symbols
# ---------------------------------------------------------------------------

POPULAR_STOCKS: list[str] = [
    "AAPL",
    "MSFT",
    "GOOGL",
    "AMZN",
    "NVDA",
    "META",
    "TSLA",
    "JPM",
    "V",
    "UNH",
    "XOM",
    "JNJ",
    "WMT",
    "PG",
    "MA",
    "HD",
    "BAC",
    "DIS",
]

POPULAR_CRYPTO: list[tuple] = [
    ("bitcoin", "BTC-USD"),
    ("ethereum", "ETH-USD"),
    ("solana", "SOL-USD"),
    ("cardano", "ADA-USD"),
    ("polkadot", "DOT-USD"),
    ("chainlink", "LINK-USD"),
    ("avalanche-2", "AVAX-USD"),
    ("polygon", "MATIC-USD"),
]


# ---------------------------------------------------------------------------
# Internal HTTP helper
# ---------------------------------------------------------------------------

_DEFAULT_TIMEOUT = 15  # seconds

_USER_AGENT = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
)


def _http_get_json(url: str, timeout: int = _DEFAULT_TIMEOUT) -> dict:
    """Perform an HTTP GET and return parsed JSON.

    Uses urllib from the standard library -- no external dependencies.
    A browser-like User-Agent header is sent to avoid 403 errors from
    Yahoo Finance.

    Raises
    ------
    urllib.error.URLError
        On network-level failures.
    json.JSONDecodeError
        When the response body is not valid JSON.
    """
    req = urllib.request.Request(url, headers={"User-Agent": _USER_AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        raw = resp.read()
    return json.loads(raw)


# ---------------------------------------------------------------------------
# Yahoo Finance fetcher
# ---------------------------------------------------------------------------

_YAHOO_VALID_PERIODS = {
    "1d",
    "5d",
    "1mo",
    "3mo",
    "6mo",
    "1y",
    "2y",
    "5y",
    "10y",
    "ytd",
    "max",
}
_YAHOO_VALID_INTERVALS = {
    "1m",
    "2m",
    "5m",
    "15m",
    "30m",
    "60m",
    "90m",
    "1h",
    "1d",
    "5d",
    "1wk",
    "1mo",
    "3mo",
}


def fetch_yahoo(
    symbol: str,
    period: str = "1y",
    interval: str = "1d",
    timeout: int = _DEFAULT_TIMEOUT,
) -> list[Candle]:
    """Fetch OHLCV data from Yahoo Finance (no API key needed).

    Uses the Yahoo Finance v8 chart endpoint via ``urllib``.

    Parameters
    ----------
    symbol:
        Ticker symbol.  Examples: ``"AAPL"``, ``"BTC-USD"``, ``"ETH-USD"``.
    period:
        Data range.  One of ``"1d"``, ``"5d"``, ``"1mo"``, ``"3mo"``,
        ``"6mo"``, ``"1y"``, ``"2y"``, ``"5y"``, ``"10y"``, ``"ytd"``,
        ``"max"``.
    interval:
        Bar interval.  One of ``"1m"``, ``"2m"``, ``"5m"``, ``"15m"``,
        ``"30m"``, ``"60m"``, ``"90m"``, ``"1h"``, ``"1d"``, ``"5d"``,
        ``"1wk"``, ``"1mo"``, ``"3mo"``.
    timeout:
        HTTP timeout in seconds.

    Returns
    -------
    list[Candle]
        Chronologically ordered candles.  Returns an empty list on failure.
    """
    if period not in _YAHOO_VALID_PERIODS:
        logger.warning(
            "Invalid Yahoo period %r; falling back to '1y'. Valid: %s",
            period,
            _YAHOO_VALID_PERIODS,
        )
        period = "1y"

    if interval not in _YAHOO_VALID_INTERVALS:
        logger.warning(
            "Invalid Yahoo interval %r; falling back to '1d'. Valid: %s",
            interval,
            _YAHOO_VALID_INTERVALS,
        )
        interval = "1d"

    url = f"https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?range={period}&interval={interval}"

    try:
        data = _http_get_json(url, timeout=timeout)
    except urllib.error.HTTPError as exc:
        logger.warning(
            "Yahoo Finance HTTP %s for %s: %s",
            exc.code,
            symbol,
            exc.reason,
        )
        return []
    except (urllib.error.URLError, OSError) as exc:
        logger.warning("Yahoo Finance network error for %s: %s", symbol, exc)
        return []
    except json.JSONDecodeError as exc:
        logger.warning("Yahoo Finance JSON parse error for %s: %s", symbol, exc)
        return []

    return _parse_yahoo_response(data, symbol)


def _parse_yahoo_response(data: dict, symbol: str) -> list[Candle]:
    """Convert Yahoo v8 JSON into a list of :class:`Candle` objects."""
    try:
        result = data["chart"]["result"][0]
        timestamps = result["timestamp"]
        quote = result["indicators"]["quote"][0]
        opens = quote["open"]
        highs = quote["high"]
        lows = quote["low"]
        closes = quote["close"]
        volumes = quote["volume"]
    except (KeyError, IndexError, TypeError) as exc:
        logger.warning("Unexpected Yahoo response structure: %s", exc)
        return []

    candles: list[Candle] = []
    for i, ts in enumerate(timestamps):
        o = opens[i]
        h = highs[i]
        lo = lows[i]
        c = closes[i]
        v = volumes[i]

        # Yahoo sometimes returns None for individual bars
        if any(x is None for x in (o, h, lo, c)):
            continue

        candles.append(
            Candle(
                timestamp=float(ts),
                open=float(o),
                high=float(h),
                low=float(lo),
                close=float(c),
                volume=float(v if v is not None else 0),
                symbol=symbol,
            )
        )

    return candles


# ---------------------------------------------------------------------------
# CoinGecko fetcher
# ---------------------------------------------------------------------------

_COINGECKO_VALID_DAYS = {1, 7, 14, 30, 90, 180, 365}


def fetch_coingecko(
    coin_id: str,
    vs_currency: str = "usd",
    days: int = 365,
    timeout: int = _DEFAULT_TIMEOUT,
) -> list[Candle]:
    """Fetch crypto OHLC from CoinGecko (free, no API key).

    Parameters
    ----------
    coin_id:
        CoinGecko coin identifier.  Examples: ``"bitcoin"``,
        ``"ethereum"``, ``"solana"``.
    vs_currency:
        Quote currency.  ``"usd"``, ``"eur"``, ``"btc"``, etc.
    days:
        Number of days of history.  CoinGecko allows ``1``, ``7``, ``14``,
        ``30``, ``90``, ``180``, ``365``.  Other values are clamped to the
        nearest valid value.
    timeout:
        HTTP timeout in seconds.

    Returns
    -------
    list[Candle]
        Chronologically ordered candles.  Returns an empty list on failure.

    Notes
    -----
    The CoinGecko free OHLC endpoint returns candles whose granularity
    depends on the *days* parameter:

    * 1-2 days   -> 30-minute candles
    * 3-30 days  -> 4-hour candles
    * 31+ days   -> 4-day candles
    """
    # Clamp to nearest valid days value
    if days not in _COINGECKO_VALID_DAYS:
        closest = min(_COINGECKO_VALID_DAYS, key=lambda d: abs(d - days))
        logger.info(
            "CoinGecko days=%d clamped to nearest valid value %d",
            days,
            closest,
        )
        days = closest

    url = f"https://api.coingecko.com/api/v3/coins/{coin_id}/ohlc?vs_currency={vs_currency}&days={days}"

    try:
        data = _http_get_json(url, timeout=timeout)
    except urllib.error.HTTPError as exc:
        if exc.code == 429:
            logger.warning(
                "CoinGecko rate limit hit for %s. Try again later.",
                coin_id,
            )
        else:
            logger.warning(
                "CoinGecko HTTP %s for %s: %s",
                exc.code,
                coin_id,
                exc.reason,
            )
        return []
    except (urllib.error.URLError, OSError) as exc:
        logger.warning("CoinGecko network error for %s: %s", coin_id, exc)
        return []
    except json.JSONDecodeError as exc:
        logger.warning("CoinGecko JSON parse error for %s: %s", coin_id, exc)
        return []

    return _parse_coingecko_response(data, coin_id)


def _parse_coingecko_response(data, coin_id: str) -> list[Candle]:
    """Convert CoinGecko OHLC JSON into :class:`Candle` objects.

    CoinGecko returns a list of ``[timestamp_ms, open, high, low, close]``.
    Volume is not provided by this endpoint so it is set to zero.
    """
    if not isinstance(data, list):
        logger.warning("Unexpected CoinGecko response type: %s", type(data).__name__)
        return []

    # child safety assessment coin_id to a Yahoo-like ticker symbol for the Candle.symbol field
    ticker_map = {cg_id: ticker for cg_id, ticker in POPULAR_CRYPTO}
    symbol = ticker_map.get(coin_id, coin_id.upper())

    candles: list[Candle] = []
    for row in data:
        if not isinstance(row, list) or len(row) < 5:
            continue
        ts_ms, o, h, lo, c = row[:5]
        if any(x is None for x in (o, h, lo, c)):
            continue
        candles.append(
            Candle(
                timestamp=float(ts_ms) / 1000.0,  # ms -> seconds
                open=float(o),
                high=float(h),
                low=float(lo),
                close=float(c),
                volume=0.0,
                symbol=symbol,
            )
        )

    return candles


# ---------------------------------------------------------------------------
# CSV import / export
# ---------------------------------------------------------------------------

# Common column-name aliases (lowered)
_COL_ALIASES = {
    "date": "timestamp",
    "datetime": "timestamp",
    "time": "timestamp",
    "ts": "timestamp",
    "timestamp": "timestamp",
    "open": "open",
    "o": "open",
    "high": "high",
    "h": "high",
    "low": "low",
    "l": "low",
    "close": "close",
    "c": "close",
    "adj close": "close",
    "adj_close": "close",
    "volume": "volume",
    "vol": "volume",
    "v": "volume",
}


def _resolve_columns(header: list[str], overrides: dict | None = None) -> dict:
    """child safety assessment CSV column names to canonical OHLCV names.

    Returns a dict mapping canonical name -> column index.
    """
    mapping: dict[str, int] = {}
    for idx, raw in enumerate(header):
        key = raw.strip().lower()
        canonical = _COL_ALIASES.get(key)
        if canonical:
            mapping[canonical] = idx

    if overrides:
        lower_header = [h.strip().lower() for h in header]
        for canon, col_name in overrides.items():
            col_lower = col_name.strip().lower()
            if col_lower in lower_header:
                mapping[canon] = lower_header.index(col_lower)

    return mapping


def _parse_timestamp(value: str) -> float:
    """Best-effort timestamp parsing from a string value."""
    # Try numeric (epoch seconds or ms)
    try:
        ts = float(value)
        # If > year 2100 in seconds, assume milliseconds
        if ts > 4_102_444_800:
            ts /= 1000.0
        return ts
    except ValueError:
        pass

    # Try ISO-like date/datetime strings
    for fmt in (
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%m/%d/%Y",
        "%d-%b-%Y",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%SZ",
    ):
        try:
            dt = datetime.strptime(value.strip(), fmt)
            return dt.replace(tzinfo=timezone.utc).timestamp()
        except ValueError:
            continue

    raise ValueError(f"Cannot parse timestamp: {value!r}")


def load_csv(
    path: str,
    date_col: str = "Date",
    ohlcv_cols: dict | None = None,
) -> list[Candle]:
    """Load OHLCV data from a CSV file.

    Auto-detects column names from the header row using common aliases
    (Yahoo Finance downloads, TradingView exports, generic OHLCV).

    Parameters
    ----------
    path:
        Filesystem path to the CSV file.
    date_col:
        Name of the date/timestamp column.  Only used when auto-detection
        fails for the timestamp field.
    ohlcv_cols:
        Optional dict overriding column mapping, e.g.
        ``{"open": "Open", "close": "Adj Close"}``.

    Returns
    -------
    list[Candle]
        Chronologically ordered candles.
    """
    candles: list[Candle] = []

    try:
        with open(path, newline="", encoding="utf-8-sig") as fh:
            reader = csv.reader(fh)
            header = next(reader, None)
            if header is None:
                logger.warning("CSV file %s is empty", path)
                return []

            col_map = _resolve_columns(header, overrides=ohlcv_cols)

            # If timestamp column was not auto-detected, try the explicit name
            if "timestamp" not in col_map:
                lower_header = [h.strip().lower() for h in header]
                dc = date_col.strip().lower()
                if dc in lower_header:
                    col_map["timestamp"] = lower_header.index(dc)

            required = {"timestamp", "open", "high", "low", "close"}
            missing = required - set(col_map.keys())
            if missing:
                logger.warning(
                    "CSV %s missing required columns: %s (header: %s)",
                    path,
                    missing,
                    header,
                )
                return []

            has_volume = "volume" in col_map

            for row_num, row in enumerate(reader, start=2):
                try:
                    ts = _parse_timestamp(row[col_map["timestamp"]])
                    o = float(row[col_map["open"]])
                    h = float(row[col_map["high"]])
                    lo = float(row[col_map["low"]])
                    c = float(row[col_map["close"]])
                    v = float(row[col_map["volume"]]) if has_volume else 0.0

                    candles.append(
                        Candle(
                            timestamp=ts,
                            open=o,
                            high=h,
                            low=lo,
                            close=c,
                            volume=v,
                        )
                    )
                except (IndexError, ValueError) as exc:
                    logger.debug("Skipping CSV row %d: %s", row_num, exc)
                    continue

    except FileNotFoundError:
        logger.warning("CSV file not found: %s", path)
    except OSError as exc:
        logger.warning("Error reading CSV %s: %s", path, exc)

    return candles


def save_csv(candles: list[Candle], path: str) -> None:
    """Write candles to a CSV file.

    Produces a file with columns:
    ``Date, Open, High, Low, Close, Volume, Symbol``

    The ``Date`` column is written as ISO-8601 UTC for readability.
    """
    if not candles:
        logger.warning("No candles to save")
        return

    with open(path, "w", newline="", encoding="utf-8") as fh:
        writer = csv.writer(fh)
        writer.writerow(["Date", "Open", "High", "Low", "Close", "Volume", "Symbol"])
        for c in candles:
            dt_str = datetime.fromtimestamp(c.timestamp, tz=timezone.utc).strftime("%Y-%m-%d %H:%M:%S")
            writer.writerow(
                [
                    dt_str,
                    c.open,
                    c.high,
                    c.low,
                    c.close,
                    c.volume,
                    c.symbol,
                ]
            )


# ---------------------------------------------------------------------------
# Sample data generation
# ---------------------------------------------------------------------------


def generate_sample_data(
    symbol: str = "SAMPLE",
    days: int = 252,
    start_price: float = 100.0,
    volatility: float = 0.02,
    seed: int | None = 42,
    trend: float = 0.005,
    regime_period: int = 60,
) -> list[Candle]:
    """Generate synthetic daily OHLCV candles via geometric Brownian motion.

    Useful for testing and demos when live data is unavailable.

    Parameters
    ----------
    symbol:
        Ticker label attached to each candle.
    days:
        Number of trading days to generate.
    start_price:
        Opening price on day 1.
    volatility:
        Daily volatility (standard deviation of log returns).
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
    list[Candle]
        *days* candles with realistic OHLCV structure.
    """
    rng = np.random.default_rng(seed)
    base_ts = time.time() - days * 86400

    candles: list[Candle] = []
    price = start_price
    current_trend = trend

    for day in range(days):
        # Flip trend direction every regime_period days
        if regime_period > 0 and day > 0 and day % regime_period == 0:
            current_trend = -current_trend

        # Daily log return
        ret = rng.normal(current_trend, volatility)
        close = price * np.exp(ret)

        # Intraday noise for open/high/low
        intraday = rng.uniform(0.002, 0.015)
        open_price = price * (1 + rng.uniform(-intraday, intraday))
        high_price = max(open_price, close) * (1 + rng.uniform(0, intraday))
        low_price = min(open_price, close) * (1 - rng.uniform(0, intraday))

        # Volume: random with a loose correlation to price movement
        base_vol = rng.uniform(500_000, 5_000_000)
        vol_multiplier = 1.0 + abs(ret) * 20  # bigger moves -> higher volume
        volume = base_vol * vol_multiplier

        candles.append(
            Candle(
                timestamp=base_ts + day * 86400,
                open=round(open_price, 4),
                high=round(high_price, 4),
                low=round(low_price, 4),
                close=round(close, 4),
                volume=round(volume, 0),
                symbol=symbol,
            )
        )

        price = close

    return candles


# ---------------------------------------------------------------------------
# Symbol catalogue
# ---------------------------------------------------------------------------


def list_available_symbols(asset_type: str = "all") -> list[dict]:
    """List available symbols with metadata.

    Parameters
    ----------
    asset_type:
        Filter by asset class.  ``"stock"``, ``"crypto"``, or ``"all"``
        (default).

    Returns
    -------
    list[dict]
        Each dict contains ``symbol``, ``name``, ``asset_type``, and
        ``source``.
    """
    results: list[dict] = []

    if asset_type in ("all", "stock"):
        for sym in POPULAR_STOCKS:
            results.append(
                {
                    "symbol": sym,
                    "name": sym,
                    "asset_type": "stock",
                    "source": "yahoo",
                }
            )

    if asset_type in ("all", "crypto"):
        for cg_id, ticker in POPULAR_CRYPTO:
            results.append(
                {
                    "symbol": ticker,
                    "name": cg_id,
                    "asset_type": "crypto",
                    "source": "coingecko",
                }
            )

    return results
