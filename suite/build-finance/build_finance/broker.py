"""
Broker connectivity for paper trading and live trading.

Provides a unified interface for order execution with two implementations:

PaperBroker   - Simulated broker for risk-free paper trading.  Executes
                orders instantly with configurable slippage and commission.
AlpacaBroker  - REST-based broker using the Alpaca Markets API (paper +
                live).  Communicates via ``urllib`` -- no external SDK
                required.

Factory
-------
``get_broker(config)`` returns the appropriate broker instance based on
the supplied :class:`BrokerConfig`.
"""

from __future__ import annotations

import json
import logging
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------


@dataclass
class BrokerConfig:
    """Configuration for broker connectivity.

    Attributes
    ----------
    name:
        Broker backend.  ``"paper"`` (default) or ``"alpaca"``.
    api_key:
        API key / key ID (Alpaca).
    api_secret:
        API secret key (Alpaca).
    base_url:
        Override for the broker REST endpoint.  When empty, the URL is
        derived from ``paper_trading``.
    paper_trading:
        If *True* (default), the Alpaca broker targets the paper-trading
        endpoint instead of the live endpoint.
    """

    name: str = "paper"
    api_key: str = ""
    api_secret: str = ""
    base_url: str = ""
    paper_trading: bool = True


@dataclass
class AccountInfo:
    """Snapshot of account state."""

    equity: float = 0.0
    cash: float = 0.0
    buying_power: float = 0.0
    positions: dict[str, dict] = field(default_factory=dict)


# ---------------------------------------------------------------------------
# Position tracker (internal)
# ---------------------------------------------------------------------------


@dataclass
class _Position:
    """Lightweight mutable position used by :class:`PaperBroker`."""

    symbol: str
    quantity: float = 0.0
    avg_cost: float = 0.0
    market_price: float = 0.0

    @property
    def market_value(self) -> float:
        return self.quantity * self.market_price

    @property
    def unrealized_pnl(self) -> float:
        return self.quantity * (self.market_price - self.avg_cost)

    def to_dict(self) -> dict:
        return {
            "symbol": self.symbol,
            "quantity": self.quantity,
            "avg_cost": round(self.avg_cost, 4),
            "market_price": round(self.market_price, 4),
            "market_value": round(self.market_value, 2),
            "unrealized_pnl": round(self.unrealized_pnl, 2),
        }


# ---------------------------------------------------------------------------
# PaperBroker
# ---------------------------------------------------------------------------


class PaperBroker:
    """Simulated broker for paper trading.

    Executes orders instantly with configurable slippage and commission.
    Tracks positions, P&L, and equity in real time.

    Parameters
    ----------
    initial_capital:
        Starting cash balance (default $100,000).
    slippage_bps:
        Simulated slippage in basis points applied to every fill
        (default 5 bps = 0.05 %).
    commission_per_share:
        Per-share commission (default $0.00 -- most US brokers are
        commission-free for equities).
    """

    def __init__(
        self,
        initial_capital: float = 100_000.0,
        slippage_bps: float = 5.0,
        commission_per_share: float = 0.0,
    ) -> None:
        self.initial_capital = initial_capital
        self.cash = initial_capital
        self.slippage_bps = slippage_bps
        self.commission_per_share = commission_per_share

        self.positions: dict[str, _Position] = {}
        self.trades: list[dict] = []
        self.equity_history: list[tuple] = [(time.time(), initial_capital)]
        self._latest_prices: dict[str, float] = {}

        self._order_counter = 0

    # -- account info -------------------------------------------------------

    def get_account(self) -> AccountInfo:
        """Return current account snapshot."""
        position_value = sum(p.market_value for p in self.positions.values())
        equity = self.cash + position_value
        return AccountInfo(
            equity=round(equity, 2),
            cash=round(self.cash, 2),
            buying_power=round(self.cash, 2),
            positions={sym: pos.to_dict() for sym, pos in self.positions.items()},
        )

    # -- order execution ----------------------------------------------------

    def submit_order(
        self,
        symbol: str,
        side: str,
        quantity: float,
        order_type: str = "market",
        limit_price: float | None = None,
    ) -> dict:
        """Submit and immediately fill a simulated order.

        Parameters
        ----------
        symbol:
            Ticker symbol.
        side:
            ``"buy"`` or ``"sell"``.
        quantity:
            Number of shares / units (positive).
        order_type:
            ``"market"`` (default) or ``"limit"``.
        limit_price:
            Required when *order_type* is ``"limit"``.

        Returns
        -------
        dict
            Order receipt with ``order_id``, ``status``, ``filled_price``,
            ``filled_qty``, ``commission``, and ``timestamp``.
        """
        side = side.lower()
        if side not in ("buy", "sell"):
            raise ValueError(f"side must be 'buy' or 'sell', got {side!r}")
        if quantity <= 0:
            raise ValueError(f"quantity must be positive, got {quantity}")

        self._order_counter += 1
        order_id = f"paper-{self._order_counter}"
        ts = time.time()

        # Determine execution price
        if order_type == "limit" and limit_price is not None:
            base_price = limit_price
        else:
            # For market orders, use the position's last known price,
            # the latest price from update_prices(), or the limit price.
            pos = self.positions.get(symbol)
            if pos and pos.market_price > 0:
                base_price = pos.market_price
            elif symbol in self._latest_prices:
                base_price = self._latest_prices[symbol]
            elif limit_price is not None:
                base_price = limit_price
            else:
                # Cannot determine price -- record at 0
                logger.warning(
                    "No price reference for %s; fill at 0. Call update_prices() first.",
                    symbol,
                )
                base_price = 0.0

        # Apply slippage
        slip = base_price * (self.slippage_bps / 10_000.0)
        if side == "buy":
            filled_price = base_price + slip
        else:
            filled_price = base_price - slip

        filled_price = max(0.0, filled_price)

        # Commission
        commission = self.commission_per_share * quantity

        # Update cash
        notional = filled_price * quantity
        if side == "buy":
            self.cash -= notional + commission
        else:
            self.cash += notional - commission

        # Update position
        pos = self.positions.get(symbol)
        if pos is None:
            pos = _Position(symbol=symbol, market_price=filled_price)
            self.positions[symbol] = pos

        if side == "buy":
            # Update average cost
            if pos.quantity > 0:
                total_cost = pos.avg_cost * pos.quantity + filled_price * quantity
                pos.quantity += quantity
                pos.avg_cost = total_cost / pos.quantity
            else:
                pos.quantity += quantity
                pos.avg_cost = filled_price
        else:  # sell
            pos.quantity -= quantity
            if pos.quantity <= 0:
                pos.quantity = 0.0
                pos.avg_cost = 0.0

        pos.market_price = filled_price

        # Remove flat positions
        if pos.quantity == 0:
            del self.positions[symbol]

        # Record trade
        trade = {
            "order_id": order_id,
            "symbol": symbol,
            "side": side,
            "quantity": quantity,
            "filled_price": round(filled_price, 4),
            "commission": round(commission, 4),
            "notional": round(notional, 2),
            "timestamp": ts,
            "status": "filled",
        }
        self.trades.append(trade)

        # Snapshot equity
        account = self.get_account()
        self.equity_history.append((ts, account.equity))

        logger.debug(
            "Paper %s %s x%.2f @ %.4f (commission=%.4f)",
            side.upper(),
            symbol,
            quantity,
            filled_price,
            commission,
        )

        return trade

    # -- position queries ---------------------------------------------------

    def get_positions(self) -> dict[str, dict]:
        """Return all open positions as dicts keyed by symbol."""
        return {sym: pos.to_dict() for sym, pos in self.positions.items()}

    def get_trades(self) -> list[dict]:
        """Return the full trade history."""
        return list(self.trades)

    # -- mark-to-market -----------------------------------------------------

    def update_prices(self, prices: dict[str, float]) -> None:
        """Update mark-to-market prices for held positions.

        Also stores the latest price per symbol so that subsequent
        ``submit_order`` calls can determine a fill price even when no
        position exists yet.

        Parameters
        ----------
        prices:
            Mapping of ``symbol -> latest price``.
        """
        for sym, price in prices.items():
            self._latest_prices[sym] = price
            pos = self.positions.get(sym)
            if pos is not None:
                pos.market_price = price

        # Snapshot equity after re-pricing
        account = self.get_account()
        self.equity_history.append((time.time(), account.equity))

    # -- equity history -----------------------------------------------------

    def get_equity_curve(self) -> list[tuple]:
        """Return list of ``(timestamp, equity)`` snapshots."""
        return list(self.equity_history)


# ---------------------------------------------------------------------------
# AlpacaBroker
# ---------------------------------------------------------------------------


class AlpacaBroker:
    """Alpaca Markets broker (paper + live trading).

    Communicates with the Alpaca REST API v2 using ``urllib`` from the
    standard library -- no ``alpaca-trade-api`` dependency required.

    Parameters
    ----------
    config:
        A :class:`BrokerConfig` with ``name="alpaca"`` and valid
        ``api_key`` / ``api_secret``.
    """

    def __init__(self, config: BrokerConfig) -> None:
        if not config.api_key or not config.api_secret:
            raise ValueError("AlpacaBroker requires api_key and api_secret in BrokerConfig")

        self.base_url = config.base_url or (
            "https://paper-api.alpaca.markets" if config.paper_trading else "https://api.alpaca.markets"
        )
        self.headers = {
            "APCA-API-KEY-ID": config.api_key,
            "APCA-API-SECRET-KEY": config.api_secret,
            "Content-Type": "application/json",
        }
        self.trades: list[dict] = []

    # -- low-level request --------------------------------------------------

    def _request(
        self,
        method: str,
        endpoint: str,
        data: dict | None = None,
        timeout: int = 15,
    ) -> dict:
        """Make an authenticated API request using ``urllib``.

        Parameters
        ----------
        method:
            HTTP method (``"GET"``, ``"POST"``, ``"DELETE"``).
        endpoint:
            API path, e.g. ``"/v2/account"``.
        data:
            JSON body for POST requests.
        timeout:
            HTTP timeout in seconds.

        Returns
        -------
        dict
            Parsed JSON response body.
        """
        url = f"{self.base_url}{endpoint}"
        body = json.dumps(data).encode("utf-8") if data else None

        req = urllib.request.Request(url, data=body, method=method)
        for key, val in self.headers.items():
            req.add_header(key, val)

        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                raw = resp.read()
                if not raw:
                    return {}
                return json.loads(raw)
        except urllib.error.HTTPError as exc:
            body_text = exc.read().decode("utf-8", errors="replace")
            logger.error(
                "Alpaca HTTP %s %s -> %s: %s",
                method,
                endpoint,
                exc.code,
                body_text,
            )
            raise
        except (urllib.error.URLError, OSError) as exc:
            logger.error("Alpaca network error %s %s: %s", method, endpoint, exc)
            raise

    # -- account info -------------------------------------------------------

    def get_account(self) -> AccountInfo:
        """Fetch account details from Alpaca."""
        data = self._request("GET", "/v2/account")
        return AccountInfo(
            equity=float(data.get("equity", 0)),
            cash=float(data.get("cash", 0)),
            buying_power=float(data.get("buying_power", 0)),
            positions={},  # populated separately via get_positions()
        )

    # -- order execution ----------------------------------------------------

    def submit_order(
        self,
        symbol: str,
        side: str,
        quantity: float,
        order_type: str = "market",
        limit_price: float | None = None,
        time_in_force: str = "day",
    ) -> dict:
        """Submit an order via the Alpaca REST API.

        Parameters
        ----------
        symbol:
            Ticker symbol (e.g. ``"AAPL"``).
        side:
            ``"buy"`` or ``"sell"``.
        quantity:
            Number of shares.
        order_type:
            ``"market"`` (default) or ``"limit"``.
        limit_price:
            Required when *order_type* is ``"limit"``.
        time_in_force:
            ``"day"`` (default), ``"gtc"``, ``"ioc"``, ``"fok"``.

        Returns
        -------
        dict
            Raw Alpaca order response.
        """
        payload = {
            "symbol": symbol,
            "qty": str(quantity),
            "side": side.lower(),
            "type": order_type.lower(),
            "time_in_force": time_in_force,
        }
        if order_type.lower() == "limit" and limit_price is not None:
            payload["limit_price"] = str(limit_price)

        result = self._request("POST", "/v2/orders", data=payload)

        trade_record = {
            "order_id": result.get("id", ""),
            "symbol": symbol,
            "side": side.lower(),
            "quantity": quantity,
            "status": result.get("status", "unknown"),
            "timestamp": time.time(),
        }
        self.trades.append(trade_record)

        return result

    # -- position queries ---------------------------------------------------

    def get_positions(self) -> dict[str, dict]:
        """Fetch all open positions from Alpaca."""
        data = self._request("GET", "/v2/positions")
        positions: dict[str, dict] = {}
        if isinstance(data, list):
            for p in data:
                sym = p.get("symbol", "")
                positions[sym] = {
                    "symbol": sym,
                    "quantity": float(p.get("qty", 0)),
                    "avg_cost": float(p.get("avg_entry_price", 0)),
                    "market_price": float(p.get("current_price", 0)),
                    "market_value": float(p.get("market_value", 0)),
                    "unrealized_pnl": float(p.get("unrealized_pl", 0)),
                }
        return positions

    # -- order management ---------------------------------------------------

    def cancel_order(self, order_id: str) -> bool:
        """Cancel an open order.

        Returns *True* on success, *False* on failure.
        """
        try:
            self._request("DELETE", f"/v2/orders/{order_id}")
            return True
        except (urllib.error.HTTPError, urllib.error.URLError, OSError) as exc:
            logger.warning("Failed to cancel order %s: %s", order_id, exc)
            return False

    # -- market data (convenience) ------------------------------------------

    def get_quote(self, symbol: str) -> dict:
        """Fetch the latest quote for *symbol* from Alpaca data API.

        Uses the Alpaca data endpoint (v2).
        """
        # Alpaca market data lives on a different host
        data_url = "https://data.alpaca.markets"
        url = f"{data_url}/v2/stocks/{symbol}/quotes/latest"

        req = urllib.request.Request(url, method="GET")
        for key, val in self.headers.items():
            req.add_header(key, val)

        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                raw = resp.read()
                result = json.loads(raw)
                quote = result.get("quote", {})
                return {
                    "symbol": symbol,
                    "bid": float(quote.get("bp", 0)),
                    "ask": float(quote.get("ap", 0)),
                    "bid_size": float(quote.get("bs", 0)),
                    "ask_size": float(quote.get("as", 0)),
                    "timestamp": quote.get("t", ""),
                }
        except (urllib.error.HTTPError, urllib.error.URLError, OSError, json.JSONDecodeError, ValueError) as exc:
            logger.warning("Alpaca quote fetch failed for %s: %s", symbol, exc)
            return {"symbol": symbol, "bid": 0, "ask": 0}

    def get_trades(self) -> list[dict]:
        """Return the local trade log (orders submitted this session)."""
        return list(self.trades)


# ---------------------------------------------------------------------------
# Factory
# ---------------------------------------------------------------------------


def get_broker(config: BrokerConfig | None = None):
    """Create and return a broker instance.

    Parameters
    ----------
    config:
        A :class:`BrokerConfig`.  When *None* or when ``config.name``
        is ``"paper"``, a :class:`PaperBroker` is returned.  When
        ``config.name`` is ``"alpaca"``, an :class:`AlpacaBroker` is
        returned.

    Raises
    ------
    ValueError
        If *config.name* is not recognised.
    """
    if config is None or config.name == "paper":
        return PaperBroker()
    if config.name == "alpaca":
        return AlpacaBroker(config)
    raise ValueError(f"Unknown broker: {config.name!r}")
