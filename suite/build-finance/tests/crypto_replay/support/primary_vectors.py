"""Bottom-up synthetic primary vectors for the T01 cryptographic graph gate."""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from typing import Final, Literal

from build_finance.crypto_replay.canonical import JsonObject, JsonValue
from build_finance.crypto_replay.content_ids import compute_content_id
from build_finance.crypto_replay.schema_definitions import SELF_ID_FIELDS
from build_finance.crypto_replay.schema_model import ResolvedContent
from tests.crypto_replay.support.builders import (
    InMemoryResolver,
    build_digest_only_supporting_document,
    build_primary_document,
    synthetic_payload,
)

_CLASSIFICATION: Final[Literal["SYNTHETIC_CONTRACT_VECTOR"]] = "SYNTHETIC_CONTRACT_VECTOR"
_MARKET_ID = "synthetic-base-mint/synthetic-quote-mint:local"
_BASE_MINT = "synthetic-base-mint"
_QUOTE_MINT = "synthetic-quote-mint"
_SOURCE_ID = "synthetic-local-contract-vector"
_SOURCE_KIND = "SYNTHETIC_ROUTE_QUOTE"
_SOURCE_REVISION = "synthetic-revision-v1"
_MISSING_FEATURES = [
    "atr_14_price_q18",
    "breakout_high_20_price_q18",
    "breakout_low_20_price_q18",
    "ema_fast_price_q18",
    "ema_slow_price_q18",
    "liquidity_quote_atoms",
    "mid_price_q18",
    "return_1_q18",
    "route_impact_bps",
    "rsi_14_q18",
    "stale_age_ns",
    "volume_20_base_atoms",
]


@dataclass(frozen=True, slots=True)
class PrimaryVectorScenario:
    """One explicitly non-production collection of linked primary records."""

    name: str
    evidence_classification: Literal["SYNTHETIC_CONTRACT_VECTOR"]
    authority: Literal["NONE", "CONTRACT_ONLY"]
    documents: tuple[JsonObject, ...]


@dataclass(frozen=True, slots=True)
class PrimaryVectorSet:
    """The three disjoint T01 scenarios and all retained cryptographic evidence."""

    resolver: InMemoryResolver
    disabled_primary_vectors: PrimaryVectorScenario
    contract_only_model_vector: PrimaryVectorScenario
    contract_only_execution_vector: PrimaryVectorScenario
    supporting_content_ids: tuple[str, ...]

    @property
    def documents(self) -> tuple[JsonObject, ...]:
        return (
            *self.disabled_primary_vectors.documents,
            *self.contract_only_model_vector.documents,
            *self.contract_only_execution_vector.documents,
        )

    @property
    def supporting_contents(self) -> tuple[ResolvedContent, ...]:
        contents: list[ResolvedContent] = []
        for content_id in self.supporting_content_ids:
            resolved = self.resolver.resolve_object(content_id)
            if resolved is None:
                raise AssertionError(f"missing retained supporting preimage: {content_id}")
            contents.append(resolved)
        return tuple(contents)


def _self_id(document: JsonObject) -> str:
    content_id = compute_content_id(document)
    schema_id = document["schema"]
    if not isinstance(schema_id, str) or document.get(SELF_ID_FIELDS[schema_id]) != content_id:
        raise AssertionError("sealed document does not contain its computed self-ID")
    return content_id


def _retain_payload(resolver: InMemoryResolver, label: str) -> str:
    return resolver.retain_payload(synthetic_payload(label))


def _supporting(
    resolver: InMemoryResolver,
    schema: str,
    label: str,
    **links: JsonValue,
) -> JsonObject:
    document: JsonObject = {
        "schema": schema,
        "t01_vector_classification": "SYNTHETIC_DIGEST_ONLY",
        "t01_vector_label": label,
        **links,
    }
    return build_digest_only_supporting_document(document, resolver)


def _merkle_root(event_ids: tuple[str, ...]) -> str:
    if not event_ids:
        raise ValueError("a non-empty event set is required")
    level = [hashlib.sha256(b"\x00" + bytes.fromhex(event_id)).digest() for event_id in event_ids]
    while len(level) > 1:
        if len(level) % 2:
            level.append(level[-1])
        level = [
            hashlib.sha256(b"\x01" + level[index] + level[index + 1]).digest() for index in range(0, len(level), 2)
        ]
    return level[0].hex()


def _raw_event(
    resolver: InMemoryResolver,
    *,
    label: str,
    fixture_manifest_id: str,
    source_receipt_id: str,
    sequence: int,
    slot: int,
    equal_time_group: int,
    replay_clock_ns: int,
) -> JsonObject:
    raw_payload_sha256 = _retain_payload(resolver, f"{label}-raw-payload")
    second = sequence - 1
    event_time = f"2026-01-01T00:00:{second:02d}.000000000Z"
    observed_at = f"2026-01-02T00:00:{second:02d}.000000000Z"
    ingested_at = f"2026-01-02T00:00:{second:02d}.000000001Z"
    return build_primary_document(
        {
            "schema": "trading.raw-event/v1",
            "fixture_manifest_sha256": fixture_manifest_id,
            "raw_payload_sha256": raw_payload_sha256,
            "source_admission_receipt_id": source_receipt_id,
            "source_id": _SOURCE_ID,
            "source_kind": _SOURCE_KIND,
            "source_revision": _SOURCE_REVISION,
            "network": "solana-mainnet",
            "venue_profile": "solana-jupiter-fixture/v1",
            "market_id": _MARKET_ID,
            "base_mint": _BASE_MINT,
            "quote_mint": _QUOTE_MINT,
            "base_decimals": 9,
            "quote_decimals": 6,
            "event_kind": "ROUTE_QUOTE",
            "source_position": {
                "slot": str(slot),
                "transaction_index": 0,
                "instruction_index": 0,
                "event_index": 0,
                "source_native_event_id": f"{label}-event-{sequence}",
                "source_subsequence": "0",
            },
            "revision": {
                "kind": "ORIGINAL",
                "supersedes_event_id": None,
                "retracts_event_id": None,
                "availability_slot": str(slot),
                "availability_admission_sequence": str(sequence),
            },
            "event_time": event_time,
            "observed_at": observed_at,
            "ingested_at": ingested_at,
            "admission_sequence": str(sequence),
            "source_sequence": str(sequence),
            "ingest_sequence": str(sequence),
            "equal_time_group": str(equal_time_group),
            "replay_clock_ns": str(replay_clock_ns),
            "executable": True,
            "market": {
                "base_amount_atoms": "1000000000",
                "quote_amount_atoms": "1000000",
                "route_capacity_base_atoms": "5000000000",
                "liquidity_quote_atoms": "10000000",
                "venue_fee_quote_atoms": "2500",
                "priority_fee_quote_atoms": "0",
                "route_impact_bps": 25,
            },
            "quality_flags": [],
        },
        resolver,
    )


def _populated_snapshot(
    resolver: InMemoryResolver,
    *,
    label: str,
    events: tuple[JsonObject, ...],
    decision_sequence: int,
) -> JsonObject:
    latest = events[-1]
    event_ids = tuple(_self_id(event) for event in events)
    return build_primary_document(
        {
            "schema": "trading.feature-snapshot/v1",
            "feature_set_version": "solana-jupiter-deterministic-features/v1",
            "feature_code_sha256": _retain_payload(resolver, f"{label}-feature-code"),
            "input_merkle_root_sha256": _merkle_root(event_ids),
            "market_id": _MARKET_ID,
            "base_mint": _BASE_MINT,
            "quote_mint": _QUOTE_MINT,
            "base_decimals": 9,
            "quote_decimals": 6,
            "decision_sequence": str(decision_sequence),
            "as_of_event_id": _self_id(latest),
            "as_of_admission_sequence": latest["admission_sequence"],
            "as_of_ingest_sequence": latest["ingest_sequence"],
            "equal_time_group": latest["equal_time_group"],
            "replay_clock_ns": latest["replay_clock_ns"],
            "event_time": latest["event_time"],
            "observed_at": latest["observed_at"],
            "ingested_at": latest["ingested_at"],
            "features": {
                "mid_price_q18": "1000000000000000000",
                "return_1_q18": "0",
                "ema_fast_price_q18": "1010000000000000000",
                "ema_slow_price_q18": "990000000000000000",
                "rsi_14_q18": "50000000000000000000",
                "atr_14_price_q18": "10000000000000000",
                "breakout_high_20_price_q18": "1100000000000000000",
                "breakout_low_20_price_q18": "900000000000000000",
                "volume_20_base_atoms": "2000000000",
                "liquidity_quote_atoms": "10000000",
                "stale_age_ns": "0",
                "route_impact_bps": 25,
                "history_count": len(events),
            },
            "missing_features": [],
        },
        resolver,
    )


def _zero_history_snapshot(resolver: InMemoryResolver, *, label: str) -> JsonObject:
    return build_primary_document(
        {
            "schema": "trading.feature-snapshot/v1",
            "feature_set_version": "solana-jupiter-deterministic-features/v1",
            "feature_code_sha256": _retain_payload(resolver, f"{label}-feature-code"),
            "input_merkle_root_sha256": None,
            "market_id": _MARKET_ID,
            "base_mint": _BASE_MINT,
            "quote_mint": _QUOTE_MINT,
            "base_decimals": 9,
            "quote_decimals": 6,
            "decision_sequence": "0",
            "as_of_event_id": None,
            "as_of_admission_sequence": "0",
            "as_of_ingest_sequence": "0",
            "equal_time_group": "0",
            "replay_clock_ns": "0",
            "event_time": None,
            "observed_at": None,
            "ingested_at": None,
            "features": {
                "mid_price_q18": None,
                "return_1_q18": None,
                "ema_fast_price_q18": None,
                "ema_slow_price_q18": None,
                "rsi_14_q18": None,
                "atr_14_price_q18": None,
                "breakout_high_20_price_q18": None,
                "breakout_low_20_price_q18": None,
                "volume_20_base_atoms": None,
                "liquidity_quote_atoms": None,
                "stale_age_ns": None,
                "route_impact_bps": None,
                "history_count": 0,
            },
            "missing_features": list(_MISSING_FEATURES),
        },
        resolver,
    )


def _genesis_state(
    resolver: InMemoryResolver,
    *,
    fixture_manifest_id: str,
    config_admission_id: str,
    config_id: str,
    run_receipt_id: str,
) -> JsonObject:
    return build_primary_document(
        {
            "schema": "trading.portfolio-state/v1",
            "state_sequence": "0",
            "as_of_ingest_sequence": "0",
            "equal_time_group": "0",
            "replay_clock_ns": "0",
            "previous_portfolio_state_id": None,
            "causation_schema": "RUN_INITIALIZATION",
            "causation_id": run_receipt_id,
            "fixture_manifest_sha256": fixture_manifest_id,
            "config_admission_receipt_id": config_admission_id,
            "validated_config_sha256": config_id,
            "quote_mint": _QUOTE_MINT,
            "quote_decimals": 6,
            "balances": [
                {
                    "mint": _QUOTE_MINT,
                    "decimals": 6,
                    "available_atoms": "1000000",
                    "reserved_atoms": "0",
                    "total_atoms": "1000000",
                }
            ],
            "positions": [],
            "summary": {
                "realized_pnl_quote_atoms": "0",
                "unrealized_pnl_quote_atoms": "0",
                "cumulative_fees_quote_atoms": "0",
                "session_pnl_quote_atoms": "0",
                "equity_quote_atoms": "1000000",
                "peak_equity_quote_atoms": "1000000",
                "drawdown_bps": 0,
            },
            "kill_latched": False,
            "kill_reason_codes": [],
            "open_intent_ids": [],
        },
        resolver,
    )


def _zero_reconciliation(portfolio_state_id: str) -> JsonObject:
    return {
        "portfolio_state_id": portfolio_state_id,
        "asset_residuals": [],
        "equity_residual_quote_atoms": "0",
        "unmatched_reservation_count": "0",
        "account_residuals": [],
        "realized_pnl_residual_quote_atoms": "0",
        "unrealized_pnl_residual_quote_atoms": "0",
        "fee_residual_quote_atoms": "0",
        "peak_equity_residual_quote_atoms": "0",
        "drawdown_residual_bps": "0",
    }


def _ledger_record(
    resolver: InMemoryResolver,
    *,
    sequence: int,
    previous_id: str | None,
    run_receipt_id: str,
    fixture_manifest_id: str,
    config_admission_id: str,
    config_id: str,
    record_type: str,
    object_document: JsonObject,
    portfolio_state_id: str,
    causation_ids: tuple[str, ...],
    entries: list[JsonObject],
    decision_sequence: str | None,
    ingest_sequence: str | None,
    equal_time_group: str | None,
    replay_clock_ns: str | None,
) -> JsonObject:
    object_id = _self_id(object_document)
    return build_primary_document(
        {
            "schema": "trading.ledger-record/v1",
            "ledger_sequence": str(sequence),
            "previous_ledger_record_id": previous_id,
            "run_receipt_id": run_receipt_id,
            "fixture_manifest_sha256": fixture_manifest_id,
            "config_admission_receipt_id": config_admission_id,
            "validated_config_sha256": config_id,
            "record_type": record_type,
            "object_schema": object_document["schema"],
            "object_id": object_id,
            "object_sha256": object_id,
            "decision_sequence": decision_sequence,
            "ingest_sequence": ingest_sequence,
            "equal_time_group": equal_time_group,
            "replay_clock_ns": replay_clock_ns,
            "causation_ids": sorted(causation_ids),
            "entries": entries,
            "reconciliation": _zero_reconciliation(portfolio_state_id),
        },
        resolver,
    )


def _build_supporting_context(
    resolver: InMemoryResolver,
    *,
    label: str,
    source_count: int,
) -> tuple[JsonObject, JsonObject, JsonObject, JsonObject, tuple[JsonObject, ...]]:
    fixture = _supporting(resolver, "trading.fixture-manifest/v1", f"{label}-fixture")
    config = _supporting(resolver, "trading.replay-risk-config/v1", f"{label}-config")
    config_admission = _supporting(
        resolver,
        "trading.config-admission-receipt/v1",
        f"{label}-config-receipt",
        config_sha256=_self_id(config),
    )
    run_receipt = _supporting(
        resolver,
        "trading.run-receipt/v1",
        f"{label}-run-receipt",
        fixture_manifest_sha256=_self_id(fixture),
        config_admission_receipt_id=_self_id(config_admission),
    )
    source_receipts = tuple(
        _supporting(
            resolver,
            "trading.source-admission-receipt/v1",
            f"{label}-source-receipt-{sequence}",
            fixture_manifest_sha256=_self_id(fixture),
            synthetic_sequence=str(sequence),
        )
        for sequence in range(1, source_count + 1)
    )
    return fixture, config, config_admission, run_receipt, source_receipts


def _build_disabled_scenario(
    resolver: InMemoryResolver,
) -> tuple[PrimaryVectorScenario, tuple[str, ...]]:
    fixture, config, config_admission, run_receipt, source_receipts = _build_supporting_context(
        resolver,
        label="disabled",
        source_count=2,
    )
    supporting = (fixture, config, config_admission, run_receipt, *source_receipts)
    fixture_id = _self_id(fixture)
    config_id = _self_id(config)
    config_admission_id = _self_id(config_admission)
    run_receipt_id = _self_id(run_receipt)
    events = tuple(
        _raw_event(
            resolver,
            label="disabled",
            fixture_manifest_id=fixture_id,
            source_receipt_id=_self_id(source_receipts[index]),
            sequence=index + 1,
            slot=index + 1,
            equal_time_group=index + 1,
            replay_clock_ns=index * 1_000_000_000,
        )
        for index in range(2)
    )
    state = _genesis_state(
        resolver,
        fixture_manifest_id=fixture_id,
        config_admission_id=config_admission_id,
        config_id=config_id,
        run_receipt_id=run_receipt_id,
    )
    snapshot = _populated_snapshot(resolver, label="disabled", events=events, decision_sequence=1)
    risk = build_primary_document(
        {
            "schema": "trading.risk-decision/v1",
            "decision_sequence": "1",
            "replay_clock_ns": "1000000000",
            "feature_snapshot_id": _self_id(snapshot),
            "config_admission_receipt_id": config_admission_id,
            "validated_config_sha256": config_id,
            "portfolio_state_before_id": _self_id(state),
            "model_signal_status": "ABSENT",
            "model_signal_id": None,
            "model_validation_receipt_id": None,
            "baseline_id": "ALWAYS_HOLD_V1",
            "baseline_action": "HOLD",
            "fused_action": "HOLD",
            "effective_action": "HOLD",
            "market_id": _MARKET_ID,
            "verdict": "REJECT",
            "reason_codes": ["RISK_NO_ACTION"],
            "reference_price_q18": None,
            "requested_base_atoms": "0",
            "approved_base_atoms": "0",
            "requested_notional_quote_atoms": "0",
            "approved_notional_quote_atoms": "0",
            "measures": {
                "participation_bps": None,
                "impact_bps": None,
                "concentration_bps": None,
                "drawdown_bps": None,
                "projected_market_value_quote_atoms": None,
                "projected_equity_quote_atoms": None,
                "session_pnl_quote_atoms": None,
                "stale_age_ns": None,
            },
            "stop_price_q18": None,
            "take_price_q18": None,
            "reservation_id": None,
            "reserved_quote_atoms": "0",
            "reserved_base_atoms": "0",
        },
        resolver,
    )
    ledger = _ledger_record(
        resolver,
        sequence=0,
        previous_id=None,
        run_receipt_id=run_receipt_id,
        fixture_manifest_id=fixture_id,
        config_admission_id=config_admission_id,
        config_id=config_id,
        record_type="RISK_DECISION",
        object_document=risk,
        portfolio_state_id=_self_id(state),
        causation_ids=(_self_id(snapshot), _self_id(state)),
        entries=[],
        decision_sequence="1",
        ingest_sequence="2",
        equal_time_group="2",
        replay_clock_ns="1000000000",
    )
    scenario = PrimaryVectorScenario(
        name="disabled_primary_vectors",
        evidence_classification=_CLASSIFICATION,
        authority="NONE",
        documents=(*events, state, snapshot, risk, ledger),
    )
    return scenario, tuple(_self_id(document) for document in supporting)


def _build_model_scenario(resolver: InMemoryResolver) -> PrimaryVectorScenario:
    snapshot = _zero_history_snapshot(resolver, label="model-contract")
    signal = build_primary_document(
        {
            "schema": "trading.model-signal/v1",
            "producer_id": "synthetic-contract-producer",
            "producer_sequence": "0",
            "model_id": "unpromoted-synthetic-model",
            "model_version": "synthetic-version-1",
            "runtime_profile_id": "offline-static-vector/v1",
            "calibration_version": "synthetic-calibration-1",
            "model_artifact_sha256": _retain_payload(resolver, "model-contract-artifact"),
            "adapter_sha256": None,
            "calibration_sha256": _retain_payload(resolver, "model-contract-calibration"),
            "feature_snapshot_id": _self_id(snapshot),
            "feature_set_version": "solana-jupiter-deterministic-features/v1",
            "market_id": _MARKET_ID,
            "horizon_ns": "30000000000",
            "decision_sequence": "0",
            "as_of_ingest_sequence": "0",
            "issued_replay_clock_ns": "0",
            "available_replay_clock_ns": "1",
            "decision_close_replay_clock_ns": "2",
            "ttl_ns": "5000000000",
            "expires_replay_clock_ns": "5000000000",
            "action": "ABSTAIN",
            "score_q18": "0",
            "probability_abstain_q18": "1000000000000000000",
            "probability_long_bias_q18": "0",
            "probability_exit_bias_q18": "0",
            "uncertainty_q18": "1000000000000000000",
            "ood_score_q18": "1000000000000000000",
            "inference_duration_ns": "0",
        },
        resolver,
    )
    return PrimaryVectorScenario(
        name="contract_only_model_vector",
        evidence_classification=_CLASSIFICATION,
        authority="CONTRACT_ONLY",
        documents=(snapshot, signal),
    )


def _reservation_state(
    resolver: InMemoryResolver,
    *,
    previous_state: JsonObject,
    intent: JsonObject,
    fixture_manifest_id: str,
    config_admission_id: str,
    config_id: str,
) -> JsonObject:
    return build_primary_document(
        {
            "schema": "trading.portfolio-state/v1",
            "state_sequence": "1",
            "as_of_ingest_sequence": "1",
            "equal_time_group": "1",
            "replay_clock_ns": "0",
            "previous_portfolio_state_id": _self_id(previous_state),
            "causation_schema": "trading.simulated-order-intent/v1",
            "causation_id": _self_id(intent),
            "fixture_manifest_sha256": fixture_manifest_id,
            "config_admission_receipt_id": config_admission_id,
            "validated_config_sha256": config_id,
            "quote_mint": _QUOTE_MINT,
            "quote_decimals": 6,
            "balances": [
                {
                    "mint": _QUOTE_MINT,
                    "decimals": 6,
                    "available_atoms": "899000",
                    "reserved_atoms": "101000",
                    "total_atoms": "1000000",
                }
            ],
            "positions": [],
            "summary": {
                "realized_pnl_quote_atoms": "0",
                "unrealized_pnl_quote_atoms": "0",
                "cumulative_fees_quote_atoms": "0",
                "session_pnl_quote_atoms": "0",
                "equity_quote_atoms": "1000000",
                "peak_equity_quote_atoms": "1000000",
                "drawdown_bps": 0,
            },
            "kill_latched": False,
            "kill_reason_codes": [],
            "open_intent_ids": [_self_id(intent)],
        },
        resolver,
    )


def _post_fill_state(
    resolver: InMemoryResolver,
    *,
    previous_state: JsonObject,
    fill: JsonObject,
    fixture_manifest_id: str,
    config_admission_id: str,
    config_id: str,
) -> JsonObject:
    return build_primary_document(
        {
            "schema": "trading.portfolio-state/v1",
            "state_sequence": "2",
            "as_of_ingest_sequence": "2",
            "equal_time_group": "2",
            "replay_clock_ns": "1000000000",
            "previous_portfolio_state_id": _self_id(previous_state),
            "causation_schema": "trading.simulated-fill-receipt/v1",
            "causation_id": _self_id(fill),
            "fixture_manifest_sha256": fixture_manifest_id,
            "config_admission_receipt_id": config_admission_id,
            "validated_config_sha256": config_id,
            "quote_mint": _QUOTE_MINT,
            "quote_decimals": 6,
            "balances": [
                {
                    "mint": _BASE_MINT,
                    "decimals": 9,
                    "available_atoms": "100000000",
                    "reserved_atoms": "0",
                    "total_atoms": "100000000",
                },
                {
                    "mint": _QUOTE_MINT,
                    "decimals": 6,
                    "available_atoms": "899650",
                    "reserved_atoms": "0",
                    "total_atoms": "899650",
                },
            ],
            "positions": [
                {
                    "market_id": _MARKET_ID,
                    "base_mint": _BASE_MINT,
                    "base_decimals": 9,
                    "quantity_base_atoms": "100000000",
                    "reserved_base_atoms": "0",
                    "cost_basis_quote_atoms": "100350",
                    "mark_price_q18": "1000000000000000000",
                    "market_value_quote_atoms": "100000",
                    "unrealized_pnl_quote_atoms": "-350",
                    "stop_price_q18": "950000000000000000",
                    "take_price_q18": "1100000000000000000",
                }
            ],
            "summary": {
                "realized_pnl_quote_atoms": "0",
                "unrealized_pnl_quote_atoms": "-350",
                "cumulative_fees_quote_atoms": "250",
                "session_pnl_quote_atoms": "-350",
                "equity_quote_atoms": "999650",
                "peak_equity_quote_atoms": "1000000",
                "drawdown_bps": 4,
            },
            "kill_latched": False,
            "kill_reason_codes": [],
            "open_intent_ids": [],
        },
        resolver,
    )


def _build_execution_scenario(
    resolver: InMemoryResolver,
) -> tuple[PrimaryVectorScenario, tuple[str, ...]]:
    fixture, config, config_admission, run_receipt, source_receipts = _build_supporting_context(
        resolver,
        label="execution-contract",
        source_count=2,
    )
    supporting = (fixture, config, config_admission, run_receipt, *source_receipts)
    fixture_id = _self_id(fixture)
    config_id = _self_id(config)
    config_admission_id = _self_id(config_admission)
    run_receipt_id = _self_id(run_receipt)
    decision_event = _raw_event(
        resolver,
        label="execution-contract-decision",
        fixture_manifest_id=fixture_id,
        source_receipt_id=_self_id(source_receipts[0]),
        sequence=1,
        slot=1,
        equal_time_group=1,
        replay_clock_ns=0,
    )
    fill_event = _raw_event(
        resolver,
        label="execution-contract-fill",
        fixture_manifest_id=fixture_id,
        source_receipt_id=_self_id(source_receipts[1]),
        sequence=2,
        slot=2,
        equal_time_group=2,
        replay_clock_ns=1_000_000_000,
    )
    snapshot = _populated_snapshot(
        resolver,
        label="execution-contract",
        events=(decision_event,),
        decision_sequence=1,
    )
    state_before = _genesis_state(
        resolver,
        fixture_manifest_id=fixture_id,
        config_admission_id=config_admission_id,
        config_id=config_id,
        run_receipt_id=run_receipt_id,
    )
    reservation_id = _retain_payload(resolver, "execution-contract-reservation-key")
    risk = build_primary_document(
        {
            "schema": "trading.risk-decision/v1",
            "decision_sequence": "1",
            "replay_clock_ns": "0",
            "feature_snapshot_id": _self_id(snapshot),
            "config_admission_receipt_id": config_admission_id,
            "validated_config_sha256": config_id,
            "portfolio_state_before_id": _self_id(state_before),
            "model_signal_status": "ABSENT",
            "model_signal_id": None,
            "model_validation_receipt_id": None,
            "baseline_id": "MOMENTUM_V1",
            "baseline_action": "ENTER_LONG",
            "fused_action": "ENTER_LONG",
            "effective_action": "ENTER_LONG",
            "market_id": _MARKET_ID,
            "verdict": "APPROVE",
            "reason_codes": [],
            "reference_price_q18": "1000000000000000000",
            "requested_base_atoms": "100000000",
            "approved_base_atoms": "100000000",
            "requested_notional_quote_atoms": "100000",
            "approved_notional_quote_atoms": "100000",
            "measures": {
                "participation_bps": 200,
                "impact_bps": 25,
                "concentration_bps": 1000,
                "drawdown_bps": 0,
                "projected_market_value_quote_atoms": "100000",
                "projected_equity_quote_atoms": "1000000",
                "session_pnl_quote_atoms": "0",
                "stale_age_ns": "0",
            },
            "stop_price_q18": "950000000000000000",
            "take_price_q18": "1100000000000000000",
            "reservation_id": reservation_id,
            "reserved_quote_atoms": "101000",
            "reserved_base_atoms": "0",
        },
        resolver,
    )
    intent = build_primary_document(
        {
            "schema": "trading.simulated-order-intent/v1",
            "intent_sequence": "1",
            "decision_sequence": "1",
            "risk_decision_id": _self_id(risk),
            "config_admission_receipt_id": config_admission_id,
            "validated_config_sha256": config_id,
            "portfolio_state_before_id": _self_id(state_before),
            "reservation_id": reservation_id,
            "market_id": _MARKET_ID,
            "base_mint": _BASE_MINT,
            "quote_mint": _QUOTE_MINT,
            "base_decimals": 9,
            "quote_decimals": 6,
            "action": "OPEN_LONG",
            "quantity_base_atoms": "100000000",
            "reference_price_q18": "1000000000000000000",
            "stop_price_q18": "950000000000000000",
            "take_price_q18": "1100000000000000000",
            "max_participation_bps": 500,
            "max_impact_bps": 50,
            "reserved_quote_atoms": "101000",
            "reserved_base_atoms": "0",
            "created_replay_clock_ns": "0",
            "decision_ingest_sequence": "1",
            "decision_equal_time_group": "1",
            "fill_policy": "STRICT_NEXT_EVENT",
            "time_in_force": "ONE_EVENT_GROUP",
        },
        resolver,
    )
    reservation_state = _reservation_state(
        resolver,
        previous_state=state_before,
        intent=intent,
        fixture_manifest_id=fixture_id,
        config_admission_id=config_admission_id,
        config_id=config_id,
    )
    fill = build_primary_document(
        {
            "schema": "trading.simulated-fill-receipt/v1",
            "receipt_sequence": "1",
            "decision_sequence": "1",
            "intent_id": _self_id(intent),
            "portfolio_state_before_id": _self_id(reservation_state),
            "status": "FILLED",
            "reason_codes": [],
            "fill_event_id": _self_id(fill_event),
            "decision_ingest_sequence": "1",
            "decision_equal_time_group": "1",
            "fill_ingest_sequence": "2",
            "fill_equal_time_group": "2",
            "fill_replay_clock_ns": "1000000000",
            "requested_base_atoms": "100000000",
            "filled_base_atoms": "100000000",
            "unfilled_base_atoms": "0",
            "gross_quote_atoms": "100100",
            "venue_fee_quote_atoms": "250",
            "priority_fee_quote_atoms": "0",
            "simulation_fee_quote_atoms": "0",
            "cash_delta_quote_atoms": "-100350",
            "execution_price_q18": "1001000000000000000",
            "participation_bps": 200,
            "reference_deviation_bps": 10,
            "impact_bps": 25,
            "fee_bps": 25,
            "adverse_fill_bps": 10,
            "adverse_fill_draw_key_sha256": _retain_payload(resolver, "execution-contract-draw-key"),
            "adverse_fill_draw_sha256": _retain_payload(resolver, "execution-contract-draw"),
            "released_quote_atoms": "650",
            "released_base_atoms": "0",
        },
        resolver,
    )
    state_after = _post_fill_state(
        resolver,
        previous_state=reservation_state,
        fill=fill,
        fixture_manifest_id=fixture_id,
        config_admission_id=config_admission_id,
        config_id=config_id,
    )
    intent_entries: list[JsonObject] = [
        {
            "account": "CASH_AVAILABLE",
            "asset_mint": _QUOTE_MINT,
            "decimals": 6,
            "amount_atoms": "-101000",
        },
        {
            "account": "CASH_RESERVED",
            "asset_mint": _QUOTE_MINT,
            "decimals": 6,
            "amount_atoms": "101000",
        },
    ]
    intent_ledger = _ledger_record(
        resolver,
        sequence=0,
        previous_id=None,
        run_receipt_id=run_receipt_id,
        fixture_manifest_id=fixture_id,
        config_admission_id=config_admission_id,
        config_id=config_id,
        record_type="SIMULATED_INTENT",
        object_document=intent,
        portfolio_state_id=_self_id(reservation_state),
        causation_ids=(_self_id(risk), _self_id(state_before)),
        entries=intent_entries,
        decision_sequence="1",
        ingest_sequence="1",
        equal_time_group="1",
        replay_clock_ns="0",
    )
    fill_entries: list[JsonObject] = [
        {
            "account": "POSITION_AVAILABLE",
            "asset_mint": _BASE_MINT,
            "decimals": 9,
            "amount_atoms": "100000000",
        },
        {
            "account": "TRADE_CLEARING",
            "asset_mint": _BASE_MINT,
            "decimals": 9,
            "amount_atoms": "-100000000",
        },
        {
            "account": "CASH_AVAILABLE",
            "asset_mint": _QUOTE_MINT,
            "decimals": 6,
            "amount_atoms": "650",
        },
        {
            "account": "CASH_RESERVED",
            "asset_mint": _QUOTE_MINT,
            "decimals": 6,
            "amount_atoms": "-101000",
        },
        {
            "account": "EQUITY_CONTROL",
            "asset_mint": _QUOTE_MINT,
            "decimals": 6,
            "amount_atoms": "350",
        },
        {
            "account": "FEE_EXPENSE",
            "asset_mint": _QUOTE_MINT,
            "decimals": 6,
            "amount_atoms": "250",
        },
        {
            "account": "TRADE_CLEARING",
            "asset_mint": _QUOTE_MINT,
            "decimals": 6,
            "amount_atoms": "100100",
        },
        {
            "account": "UNREALIZED_PNL",
            "asset_mint": _QUOTE_MINT,
            "decimals": 6,
            "amount_atoms": "-350",
        },
    ]
    fill_ledger = _ledger_record(
        resolver,
        sequence=1,
        previous_id=_self_id(intent_ledger),
        run_receipt_id=run_receipt_id,
        fixture_manifest_id=fixture_id,
        config_admission_id=config_admission_id,
        config_id=config_id,
        record_type="SIMULATED_FILL",
        object_document=fill,
        portfolio_state_id=_self_id(state_after),
        causation_ids=(_self_id(fill_event), _self_id(intent), _self_id(reservation_state)),
        entries=fill_entries,
        decision_sequence="1",
        ingest_sequence="2",
        equal_time_group="2",
        replay_clock_ns="1000000000",
    )
    scenario = PrimaryVectorScenario(
        name="contract_only_execution_vector",
        evidence_classification=_CLASSIFICATION,
        authority="CONTRACT_ONLY",
        documents=(
            decision_event,
            fill_event,
            snapshot,
            state_before,
            risk,
            intent,
            reservation_state,
            fill,
            state_after,
            intent_ledger,
            fill_ledger,
        ),
    )
    return scenario, tuple(_self_id(document) for document in supporting)


def build_primary_vector_set() -> PrimaryVectorSet:
    """Build the complete T01 vector graph without invoking any runtime workflow."""
    resolver = InMemoryResolver()
    disabled, disabled_supporting_ids = _build_disabled_scenario(resolver)
    model = _build_model_scenario(resolver)
    execution, execution_supporting_ids = _build_execution_scenario(resolver)
    return PrimaryVectorSet(
        resolver=resolver,
        disabled_primary_vectors=disabled,
        contract_only_model_vector=model,
        contract_only_execution_vector=execution,
        supporting_content_ids=(*disabled_supporting_ids, *execution_supporting_ids),
    )
