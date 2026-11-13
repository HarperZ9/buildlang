"""One order gate in front of every broker call.

Every order path (AutoTrader, the build-engine adapter, the GUI flatten
action) submits through :class:`OrderGate`.  The gate enforces three rules
before any broker method is reached:

1. Kill switch: a halt flag held in process and in a file under the state
   folder.  While set, every submit is refused.  Only :meth:`OrderGate.clear_halt`
   (a person) unsets it.
2. Enforced stops: a buy must carry a stop below its reference price, or the
   gate attaches ``default_stop_pct``.  A buy with neither is refused.
   :meth:`OrderGate.enforce_stops` turns a breached stop into a closing sell
   that goes through the same path.  Sells may only reduce a held position.
3. Idempotency: each order gets a client order id, a hash of its content and
   the decision id.  A repeat submit returns the first result and does not
   reach the broker again.  Ids persist in the state folder, so the rule holds
   across a restart.  An id written before the broker call and never completed
   (crash or broker error) is refused on repeat, because the outcome is
   unknown; a person clears it with :meth:`OrderGate.forget_order`.

The gate uses float arithmetic.  The G2 paper kernel's risk code works on
integer Q18 records and a different input schema, so it is not reused here.
"""

from __future__ import annotations

import hashlib
import json
import logging
import os
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

SENT = "sent"
REFUSED = "refused"
DUPLICATE = "duplicate"


def client_order_id(
    symbol: str,
    side: str,
    quantity: float,
    order_type: str,
    limit_price: float | None,
    stop_price: float | None,
    decision_id: str,
) -> str:
    """Deterministic id from order content and the producing decision."""
    body = json.dumps(
        [symbol, side, repr(float(quantity)), order_type, limit_price, stop_price, decision_id],
        separators=(",", ":"),
    )
    return hashlib.sha256(body.encode("utf-8")).hexdigest()[:32]


class _JsonStore:
    """Tiny JSON-file dict; in-memory when no path is given."""

    def __init__(self, path: Path | None) -> None:
        self._path = path
        self.data: dict[str, Any] = {}
        if path is not None and path.exists():
            self.data = json.loads(path.read_text(encoding="utf-8"))

    def save(self) -> None:
        if self._path is None:
            return
        tmp = self._path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.data, sort_keys=True), encoding="utf-8")
        os.replace(tmp, self._path)


class OrderGate:
    """Kill switch, stop enforcement and idempotency in front of a broker."""

    def __init__(
        self,
        broker: Any,
        state_dir: str | os.PathLike[str] | None = None,
        default_stop_pct: float | None = None,
    ) -> None:
        self.broker = broker
        self.default_stop_pct = default_stop_pct
        self._halted_reason: str | None = None
        self._halt_file: Path | None = None
        ids_path = stops_path = None
        if state_dir is not None:
            folder = Path(state_dir)
            folder.mkdir(parents=True, exist_ok=True)
            self._halt_file = folder / "HALT"
            ids_path = folder / "order_ids.json"
            stops_path = folder / "stops.json"
        self._ids = _JsonStore(ids_path)
        self._stops = _JsonStore(stops_path)

    # -- kill switch --------------------------------------------------------

    def halt(self, reason: str = "manual halt") -> None:
        """Set the halt flag in process and, if a state folder exists, on disk."""
        self._halted_reason = reason
        if self._halt_file is not None:
            self._halt_file.write_text(reason, encoding="utf-8")
        logger.error("Order gate halted: %s", reason)

    def clear_halt(self) -> None:
        """Clear the halt.  Call this only on a person's decision."""
        self._halted_reason = None
        if self._halt_file is not None and self._halt_file.exists():
            self._halt_file.unlink()

    def halted_reason(self) -> str | None:
        if self._halted_reason is not None:
            return self._halted_reason
        if self._halt_file is not None and self._halt_file.exists():
            return self._halt_file.read_text(encoding="utf-8") or "halt file present"
        return None

    # -- submit -------------------------------------------------------------

    def submit(
        self,
        symbol: str,
        side: str,
        quantity: float,
        *,
        decision_id: str,
        ref_price: float | None = None,
        stop_price: float | None = None,
        order_type: str = "market",
        limit_price: float | None = None,
    ) -> dict[str, Any]:
        """Submit one order through every check.  A refusal does not raise."""
        reason = self.halted_reason()
        if reason is not None:
            return self._refuse(f"halted: {reason}")
        if side not in ("buy", "sell") or not quantity or quantity <= 0:
            return self._refuse("invalid side or quantity")
        stop = stop_price
        if side == "buy":
            stop, problem = self._resolve_stop(ref_price, limit_price, stop_price)
        else:
            problem = self._check_reduce_only(symbol, quantity)
        if problem:
            return self._refuse(problem)
        cid = client_order_id(symbol, side, quantity, order_type, limit_price, stop, decision_id)
        seen = self._ids.data.get(cid)
        if seen is not None:
            return self._repeat(cid, seen)
        return self._send(cid, symbol, side, quantity, stop, order_type, limit_price)

    def _resolve_stop(
        self, ref: float | None, limit: float | None, stop: float | None
    ) -> tuple[float | None, str | None]:
        price = ref if ref is not None else limit
        if price is None or price <= 0:
            return None, "buy needs a positive reference price to validate its stop"
        if stop is None and self.default_stop_pct:
            stop = round(price * (1.0 - self.default_stop_pct), 6)
        if stop is None:
            return None, "buy without a stop"
        if not 0 < stop < price:
            return None, "stop must be positive and below the reference price"
        return stop, None

    def _check_reduce_only(self, symbol: str, quantity: float) -> str | None:
        held = self.broker.get_positions().get(symbol, {}).get("quantity", 0)
        if quantity > held:
            return f"sell of {quantity} exceeds held {held}; shorting is not allowed"
        return None

    def _refuse(self, reason: str) -> dict[str, Any]:
        logger.warning("Order refused: %s", reason)
        return {"status": REFUSED, "reason": reason}

    def _repeat(self, cid: str, seen: dict[str, Any]) -> dict[str, Any]:
        if seen.get("state") == "done":
            return {"status": DUPLICATE, "client_order_id": cid, "result": seen["result"]}
        return self._refuse(f"order {cid} has an unknown outcome ({seen.get('state')}); repeat blocked")

    def _send(
        self,
        cid: str,
        symbol: str,
        side: str,
        quantity: float,
        stop: float | None,
        order_type: str,
        limit_price: float | None,
    ) -> dict[str, Any]:
        self._ids.data[cid] = {"state": "pending"}
        self._ids.save()
        try:
            result = self.broker.submit_order(
                symbol, side, quantity, order_type=order_type, limit_price=limit_price
            )
        except Exception as exc:
            self._ids.data[cid] = {"state": "error", "error": str(exc)}
            self._ids.save()
            logger.error("Broker error on %s: %s", cid, exc)
            raise
        self._ids.data[cid] = {"state": "done", "result": result}
        self._ids.save()
        self._track_stop(symbol, side, stop)
        return {"status": SENT, "client_order_id": cid, "stop_price": stop, "result": result}

    def _track_stop(self, symbol: str, side: str, stop: float | None) -> None:
        if side == "buy" and stop is not None:
            self._stops.data[symbol] = stop
        elif side == "sell" and symbol not in self.broker.get_positions():
            self._stops.data.pop(symbol, None)
        self._stops.save()

    # -- stop enforcement ---------------------------------------------------

    def stops(self) -> dict[str, float]:
        return dict(self._stops.data)

    def enforce_stops(self, prices: dict[str, float]) -> list[dict[str, Any]]:
        """Close every held position whose price is at or below its stop."""
        out: list[dict[str, Any]] = []
        positions = self.broker.get_positions()
        for symbol, stop in sorted(self._stops.data.items()):
            price = prices.get(symbol)
            qty = positions.get(symbol, {}).get("quantity", 0)
            if price is None or qty <= 0 or price > stop:
                continue
            out.append(self.submit(symbol, "sell", qty, decision_id=f"stop:{symbol}:{stop}:{qty}"))
        return out

    # -- operator tools -----------------------------------------------------

    def forget_order(self, cid: str) -> None:
        """Clear an unknown-outcome id after a person checked the broker."""
        self._ids.data.pop(cid, None)
        self._ids.save()
