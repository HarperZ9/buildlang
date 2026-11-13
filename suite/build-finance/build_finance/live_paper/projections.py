"""Derived immutable read models for the offline G2 paper kernel."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, fields, is_dataclass
from typing import TYPE_CHECKING, Any, cast

from build_finance.crypto_replay.canonical import canonical_json_bytes, parse_canonical_record

if TYPE_CHECKING:
    from build_finance.live_paper.kernel import PaperKernelClosure, PaperKernelResult


@dataclass(frozen=True, slots=True)
class PositionProjection:
    market_id: str
    base_mint: str
    quantity_base_atoms: str
    market_value_quote_atoms: str
    unrealized_pnl_quote_atoms: str


@dataclass(frozen=True, slots=True)
class BreakerStateProjection:
    status: str
    kill_latched: bool
    reason_codes: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class ModelValidationProjection:
    decision_sequence: str
    disposition: str
    reason_code: str


@dataclass(frozen=True, slots=True)
class DecisionProjection:
    decision_sequence: str
    fused_action: str
    risk_verdict: str
    reason_codes: tuple[str, ...]
    intent_id: str | None


@dataclass(frozen=True, slots=True)
class FillProjection:
    fill_receipt_id: str
    intent_id: str
    status: str
    reason_codes: tuple[str, ...]
    filled_base_atoms: str
    gross_quote_atoms: str


@dataclass(frozen=True, slots=True)
class ReconciliationStateProjection:
    status: str
    reason_codes: tuple[str, ...]
    receipt_ids: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class ReceiptIdsProjection:
    run_receipt_id: str | None
    reconciliation_receipt_ids: tuple[str, ...]
    ledger_head_id: str | None
    final_portfolio_state_id: str | None
    closure_id: str


@dataclass(frozen=True, slots=True)
class PaperKernelProjection:
    positions: tuple[PositionProjection, ...]
    cash_quote_atoms: str
    equity_quote_atoms: str
    breaker_state: BreakerStateProjection
    model_validation: tuple[ModelValidationProjection, ...]
    decisions: tuple[DecisionProjection, ...]
    fills: tuple[FillProjection, ...]
    reconciliation_state: ReconciliationStateProjection
    receipt_ids: ReceiptIdsProjection


def _record(record: bytes | None) -> dict[str, Any] | None:
    if record is None:
        return None
    try:
        return cast(dict[str, Any], parse_canonical_record(bytes(record)))
    except (TypeError, ValueError):
        return None


def _strings(value: object) -> tuple[str, ...]:
    if not isinstance(value, (list, tuple)):
        return ()
    return tuple(item for item in value if isinstance(item, str))


def _content_id(document: Mapping[str, Any]) -> str | None:
    for field in (
        "reconciliation_receipt_id",
        "fill_receipt_id",
        "ledger_record_id",
        "portfolio_state_id",
    ):
        value = document.get(field)
        if isinstance(value, str):
            return value
    return None


def build_kernel_projection(
    *,
    closure: PaperKernelClosure,
    run_receipt_id: str | None,
    final_portfolio_state_record: bytes | None,
    model_dispositions: Sequence[tuple[str, str, str]],
    fusion_decision_records: Sequence[bytes],
    risk_decision_records: Sequence[bytes],
    simulated_order_intent_records: Sequence[bytes],
    simulated_fill_receipt_records: Sequence[bytes],
    reconciliation_receipt_records: Sequence[bytes],
) -> PaperKernelProjection:
    """Build read-only views solely from retained kernel evidence."""
    state = _record(final_portfolio_state_record) or {}
    balances = state.get("balances")
    quote_mint = state.get("quote_mint")
    cash = "0"
    if isinstance(balances, list):
        for row in balances:
            if isinstance(row, Mapping) and row.get("mint") == quote_mint:
                value = row.get("total_atoms")
                if isinstance(value, str):
                    cash = value
                break
    summary = state.get("summary")
    equity = "0"
    if isinstance(summary, Mapping) and isinstance(summary.get("equity_quote_atoms"), str):
        equity = cast(str, summary["equity_quote_atoms"])

    positions: list[PositionProjection] = []
    state_positions = state.get("positions")
    if isinstance(state_positions, list):
        for row in state_positions:
            if not isinstance(row, Mapping):
                continue
            positions.append(
                PositionProjection(
                    market_id=str(row.get("market_id", "")),
                    base_mint=str(row.get("base_mint", "")),
                    quantity_base_atoms=str(row.get("quantity_base_atoms", "0")),
                    market_value_quote_atoms=str(row.get("market_value_quote_atoms", "0")),
                    unrealized_pnl_quote_atoms=str(row.get("unrealized_pnl_quote_atoms", "0")),
                )
            )

    intents: dict[str, str] = {}
    for record in simulated_order_intent_records:
        document = _record(record)
        if document is not None and isinstance(document.get("decision_sequence"), str):
            intent_id = document.get("intent_id")
            if isinstance(intent_id, str):
                intents[cast(str, document["decision_sequence"])] = intent_id
    risk_by_sequence: dict[str, dict[str, Any]] = {}
    for record in risk_decision_records:
        document = _record(record)
        if document is not None and isinstance(document.get("decision_sequence"), str):
            risk_by_sequence[cast(str, document["decision_sequence"])] = document
    decisions: list[DecisionProjection] = []
    for record in fusion_decision_records:
        fusion = _record(record)
        if fusion is None:
            continue
        sequence = fusion.get("decision_sequence")
        if not isinstance(sequence, str):
            continue
        risk = risk_by_sequence.get(sequence, {})
        decisions.append(
            DecisionProjection(
                decision_sequence=sequence,
                fused_action=str(fusion.get("fused_action", "HOLD")),
                risk_verdict=str(risk.get("verdict", "UNAVAILABLE")),
                reason_codes=_strings(risk.get("reason_codes")),
                intent_id=intents.get(sequence),
            )
        )

    fills: list[FillProjection] = []
    for record in simulated_fill_receipt_records:
        document = _record(record)
        if document is None:
            continue
        receipt_id = document.get("fill_receipt_id")
        intent_id = document.get("intent_id")
        if not isinstance(receipt_id, str) or not isinstance(intent_id, str):
            continue
        fills.append(
            FillProjection(
                fill_receipt_id=receipt_id,
                intent_id=intent_id,
                status=str(document.get("status", "UNKNOWN")),
                reason_codes=_strings(document.get("reason_codes")),
                filled_base_atoms=str(document.get("filled_base_atoms", "0")),
                gross_quote_atoms=str(document.get("gross_quote_atoms", "0")),
            )
        )

    reconciliation_documents = tuple(
        document
        for document in (_record(record) for record in reconciliation_receipt_records)
        if document is not None
    )
    reconciliation_ids = tuple(
        content_id
        for content_id in (_content_id(document) for document in reconciliation_documents)
        if content_id is not None
    )
    terminal_reconciliation = reconciliation_documents[-1] if reconciliation_documents else None
    reconciliation_reasons = (
        () if terminal_reconciliation is None else _strings(terminal_reconciliation.get("reason_codes"))
    )
    reconciliation_status = (
        "NOT_RUN" if terminal_reconciliation is None else str(terminal_reconciliation.get("status", "UNKNOWN"))
    )
    final_state_id = state.get("portfolio_state_id")
    return PaperKernelProjection(
        positions=tuple(positions),
        cash_quote_atoms=cash,
        equity_quote_atoms=equity,
        breaker_state=BreakerStateProjection(
            status=closure.status,
            kill_latched=state.get("kill_latched") is True,
            reason_codes=closure.reason_codes or _strings(state.get("kill_reason_codes")),
        ),
        model_validation=tuple(
            ModelValidationProjection(sequence, disposition, reason)
            for sequence, disposition, reason in model_dispositions
        ),
        decisions=tuple(decisions),
        fills=tuple(fills),
        reconciliation_state=ReconciliationStateProjection(
            status=reconciliation_status,
            reason_codes=reconciliation_reasons,
            receipt_ids=reconciliation_ids,
        ),
        receipt_ids=ReceiptIdsProjection(
            run_receipt_id=run_receipt_id,
            reconciliation_receipt_ids=reconciliation_ids,
            ledger_head_id=closure.ledger_head_id,
            final_portfolio_state_id=final_state_id if isinstance(final_state_id, str) else None,
            closure_id=closure.closure_id,
        ),
    )


def _canonical_value(value: object) -> Any:
    if isinstance(value, bytes):
        return value.hex()
    if is_dataclass(value) and not isinstance(value, type):
        return {item.name: _canonical_value(getattr(value, item.name)) for item in fields(value)}
    if isinstance(value, Mapping):
        return {str(key): _canonical_value(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_canonical_value(item) for item in value]
    if value is None or type(value) in {bool, int, str}:
        return value
    raise TypeError(f"kernel result contains unsupported canonical value: {type(value).__name__}")


def kernel_result_canonical_bytes(result: PaperKernelResult) -> bytes:
    """Canonicalize the complete immutable result without consulting ambient state."""
    return canonical_json_bytes(
        {
            "schema": "build-finance.live-paper.kernel-result/v1",
            "result": _canonical_value(result),
        }
    )


__all__ = [
    "BreakerStateProjection",
    "DecisionProjection",
    "FillProjection",
    "ModelValidationProjection",
    "PaperKernelProjection",
    "PositionProjection",
    "ReceiptIdsProjection",
    "ReconciliationStateProjection",
    "build_kernel_projection",
    "kernel_result_canonical_bytes",
]
