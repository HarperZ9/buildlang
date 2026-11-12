"""Focused regressions for the complete T01 primary semantic authority."""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from copy import deepcopy
from pathlib import Path
from typing import Any

import pytest

EXAMPLE_ROOT = Path(__file__).parent / "resources" / "primary-v1" / "examples"
EXAMPLES = {
    "trading.feature-snapshot/v1": ("feature-snapshot.json", "snapshot_id"),
    "trading.ledger-record/v1": ("ledger-record.json", "ledger_record_id"),
    "trading.model-signal/v1": ("model-signal.json", "signal_id"),
    "trading.portfolio-state/v1": ("portfolio-state.json", "portfolio_state_id"),
    "trading.raw-event/v1": ("raw-event.json", "event_id"),
    "trading.risk-decision/v1": ("risk-decision.json", "risk_decision_id"),
    "trading.simulated-fill-receipt/v1": ("simulated-fill-receipt.json", "fill_receipt_id"),
    "trading.simulated-order-intent/v1": ("simulated-order-intent.json", "intent_id"),
}


def _example(schema_id: str) -> dict[str, Any]:
    filename, _ = EXAMPLES[schema_id]
    document = json.loads((EXAMPLE_ROOT / filename).read_text(encoding="utf-8"))
    assert document["schema"] == schema_id
    return document


def _replace(document: dict[str, Any], path: Sequence[str | int], value: Any) -> None:
    target: Any = document
    for part in path[:-1]:
        target = target[part]
    target[path[-1]] = value


def _sealed(
    schema_id: str,
    changes: Sequence[tuple[Sequence[str | int], Any]] = (),
) -> dict[str, Any]:
    from build_finance.crypto_replay.content_ids import seal_content_id

    document = _example(schema_id)
    for path, value in changes:
        _replace(document, path, value)
    document.pop(EXAMPLES[schema_id][1])
    return seal_content_id(document)


def _reseal(document: Mapping[str, Any]) -> dict[str, Any]:
    from build_finance.crypto_replay.content_ids import seal_content_id

    schema_id = str(document["schema"])
    mutable = deepcopy(document)
    mutable.pop(EXAMPLES[schema_id][1], None)
    return seal_content_id(mutable)


def _assert_structurally_valid(document: Mapping[str, Any]) -> None:
    from jsonschema import Draft202012Validator

    from build_finance.crypto_replay.schema_definitions import PRIMARY_SCHEMA_DOCUMENTS

    errors = tuple(Draft202012Validator(PRIMARY_SCHEMA_DOCUMENTS[str(document["schema"])]).iter_errors(document))
    assert errors == ()


def _issues(document: Mapping[str, Any]) -> tuple[Any, ...]:
    from build_finance.crypto_replay.schema_registry import validate_contract

    _assert_structurally_valid(document)
    return validate_contract(document, expected_schema=str(document["schema"]))


def _assert_rejected(
    document: Mapping[str, Any],
    *,
    code: str,
    path: tuple[str | int, ...],
) -> None:
    issues = _issues(document)
    assert any(issue.code == code and issue.path == path for issue in issues), issues


def _assert_valid(document: Mapping[str, Any]) -> None:
    assert _issues(document) == ()


def _risk_config_zero_tuple(reason: str = "RISK_CONFIG_MISSING") -> dict[str, Any]:
    document = _example("trading.risk-decision/v1")
    document.update(
        {
            "validated_config_sha256": None,
            "model_signal_status": "ABSENT",
            "model_signal_id": None,
            "model_validation_receipt_id": None,
            "baseline_id": None,
            "baseline_action": "HOLD",
            "fused_action": "HOLD",
            "effective_action": "HOLD",
            "verdict": "KILL",
            "reason_codes": [reason],
            "reference_price_q18": None,
            "requested_base_atoms": "0",
            "approved_base_atoms": "0",
            "requested_notional_quote_atoms": "0",
            "approved_notional_quote_atoms": "0",
            "stop_price_q18": None,
            "take_price_q18": None,
            "reservation_id": None,
            "reserved_quote_atoms": "0",
            "reserved_base_atoms": "0",
        }
    )
    document["measures"] = {field: None for field in document["measures"]}
    return _reseal(document)


def _risk_fatal_zero_tuple(reason: str) -> dict[str, Any]:
    document = _example("trading.risk-decision/v1")
    document.update(
        {
            "model_signal_status": "ABSENT",
            "model_signal_id": None,
            "model_validation_receipt_id": None,
            "baseline_action": "HOLD",
            "fused_action": "HOLD",
            "effective_action": "HOLD",
            "verdict": "KILL",
            "reason_codes": [reason],
            "reference_price_q18": None,
            "requested_base_atoms": "0",
            "approved_base_atoms": "0",
            "requested_notional_quote_atoms": "0",
            "approved_notional_quote_atoms": "0",
            "stop_price_q18": None,
            "take_price_q18": None,
            "reservation_id": None,
            "reserved_quote_atoms": "0",
            "reserved_base_atoms": "0",
        }
    )
    document["measures"] = {field: None for field in document["measures"]}
    return _reseal(document)


def _risk_zero_history_rejection(reason: str = "RISK_NO_ACTION") -> dict[str, Any]:
    document = _risk_fatal_zero_tuple(reason)
    document["verdict"] = "REJECT"
    return _reseal(document)


def _risk_exit(
    reasons: Sequence[str] = (),
    *,
    notional: str = "100000",
    reserved_base_atoms: str = "100000000",
) -> dict[str, Any]:
    document = _example("trading.risk-decision/v1")
    document.update(
        {
            "baseline_action": "HOLD" if reasons else "EXIT_LONG",
            "fused_action": "HOLD" if reasons else "EXIT_LONG",
            "effective_action": "EXIT_LONG",
            "reason_codes": list(reasons),
            "requested_notional_quote_atoms": notional,
            "approved_notional_quote_atoms": notional,
            "reserved_quote_atoms": "0",
            "reserved_base_atoms": reserved_base_atoms,
        }
    )
    return _reseal(document)


def _risk_latching_loss(
    reasons: Sequence[str] = ("RISK_SESSION_LOSS",),
    *,
    observation: bool = False,
    positioned: bool = False,
) -> dict[str, Any]:
    document = _risk_fatal_zero_tuple(str(reasons[0]))
    document["reason_codes"] = list(reasons)
    document["measures"] = {
        "participation_bps": 0,
        "impact_bps": 25 if observation else None,
        "concentration_bps": 1000,
        "drawdown_bps": 500,
        "projected_market_value_quote_atoms": "100000",
        "projected_equity_quote_atoms": "1000000",
        "session_pnl_quote_atoms": "-100000",
        "stale_age_ns": "0" if observation else None,
    }
    if positioned:
        document["stop_price_q18"] = "950000000000000000"
        document["take_price_q18"] = "1100000000000000000"
    return _reseal(document)


def _risk_nonpositive_equity() -> dict[str, Any]:
    document = _risk_fatal_zero_tuple("RISK_NONPOSITIVE_EQUITY")
    document["measures"] = {
        "participation_bps": 0,
        "impact_bps": None,
        "concentration_bps": None,
        "drawdown_bps": 0,
        "projected_market_value_quote_atoms": "0",
        "projected_equity_quote_atoms": "0",
        "session_pnl_quote_atoms": "0",
        "stale_age_ns": None,
    }
    return _reseal(document)


@pytest.mark.parametrize(
    ("path", "value"),
    (
        (("reference_price_q18",), "1000000000000000000"),
        (("requested_base_atoms",), "1"),
        (("requested_notional_quote_atoms",), "1"),
        (("measures", "participation_bps"), 0),
        (("measures", "projected_equity_quote_atoms"), "0"),
        (("stop_price_q18",), "1"),
        (("take_price_q18",), "1"),
    ),
)
def test_null_config_requires_every_zero_authority_field(
    path: tuple[str | int, ...],
    value: Any,
) -> None:
    valid = _risk_config_zero_tuple()
    _assert_valid(valid)
    invalid = deepcopy(valid)
    _replace(invalid, path, value)
    invalid = _reseal(invalid)
    _assert_rejected(invalid, code="semantic_config_tuple", path=("validated_config_sha256",))


def test_populated_snapshot_requires_both_retrieval_times() -> None:
    invalid = _sealed(
        "trading.feature-snapshot/v1",
        ((("observed_at",), None),),
    )
    invalid["ingested_at"] = None
    invalid = _reseal(invalid)
    _assert_rejected(invalid, code="semantic_time", path=("observed_at",))


def test_ledger_unmatched_reservation_count_is_exact_zero() -> None:
    invalid = _sealed(
        "trading.ledger-record/v1",
        ((("reconciliation", "unmatched_reservation_count"), "1"),),
    )
    _assert_rejected(
        invalid,
        code="semantic_residual",
        path=("reconciliation", "unmatched_reservation_count"),
    )


@pytest.mark.parametrize(
    ("collection", "path"),
    (
        ("asset_residuals", ("reconciliation", "asset_residuals", 0, "asset_mint")),
        ("account_residuals", ("reconciliation", "account_residuals", 0, "asset_mint")),
    ),
)
def test_ledger_residual_mints_use_utf8_bytes_not_code_points(
    collection: str,
    path: tuple[str | int, ...],
) -> None:
    at_limit = ("\u00e9" * 63) + "aa"
    over_limit = ("\u00e9" * 64) + "a"
    assert len(at_limit.encode("utf-8")) == 128
    assert len(over_limit.encode("utf-8")) == 129

    valid = _example("trading.ledger-record/v1")
    row = deepcopy(valid["reconciliation"][collection][0])
    row["asset_mint"] = at_limit
    valid["reconciliation"][collection] = [row]
    _assert_valid(_reseal(valid))

    row["asset_mint"] = over_limit
    invalid = _example("trading.ledger-record/v1")
    invalid["reconciliation"][collection] = [row]
    _assert_rejected(_reseal(invalid), code="semantic_registry", path=path)


def test_raw_native_event_id_uses_256_utf8_byte_limit() -> None:
    at_limit = ("\u00e9" * 127) + "aa"
    over_limit = ("\u00e9" * 128) + "a"
    assert len(at_limit.encode("utf-8")) == 256
    assert len(over_limit.encode("utf-8")) == 257
    _assert_valid(_sealed("trading.raw-event/v1", ((("source_position", "source_native_event_id"), at_limit),)))
    _assert_rejected(
        _sealed("trading.raw-event/v1", ((("source_position", "source_native_event_id"), over_limit),)),
        code="semantic_registry",
        path=("source_position", "source_native_event_id"),
    )


@pytest.mark.parametrize(
    ("schema_id", "path"),
    (
        *(
            ("trading.raw-event/v1", (field,))
            for field in ("source_id", "source_kind", "source_revision", "market_id", "base_mint", "quote_mint")
        ),
        *(("trading.feature-snapshot/v1", (field,)) for field in ("market_id", "base_mint", "quote_mint")),
        *(
            ("trading.model-signal/v1", (field,))
            for field in (
                "producer_id",
                "model_id",
                "model_version",
                "runtime_profile_id",
                "calibration_version",
                "feature_set_version",
                "market_id",
            )
        ),
        ("trading.risk-decision/v1", ("market_id",)),
        *(("trading.simulated-order-intent/v1", (field,)) for field in ("market_id", "base_mint", "quote_mint")),
        ("trading.portfolio-state/v1", ("quote_mint",)),
        ("trading.portfolio-state/v1", ("balances", 0, "mint")),
        ("trading.portfolio-state/v1", ("positions", 0, "market_id")),
        ("trading.portfolio-state/v1", ("positions", 0, "base_mint")),
        ("trading.ledger-record/v1", ("entries", 0, "asset_mint")),
        ("trading.ledger-record/v1", ("reconciliation", "asset_residuals", 0, "asset_mint")),
        ("trading.ledger-record/v1", ("reconciliation", "account_residuals", 0, "asset_mint")),
    ),
)
def test_every_primary_registry_alias_uses_utf8_byte_authority(
    schema_id: str,
    path: tuple[str | int, ...],
) -> None:
    over_limit = ("\u00e9" * 64) + "a"
    assert len(over_limit) <= 128
    assert len(over_limit.encode("utf-8")) == 129
    document = _example(schema_id)
    _replace(document, path, over_limit)
    _assert_rejected(_reseal(document), code="semantic_registry", path=path)


@pytest.mark.parametrize("status", ("FILLED", "PARTIAL"))
def test_fill_never_releases_both_assets(status: str) -> None:
    document = _example("trading.simulated-fill-receipt/v1")
    document["released_base_atoms"] = "1"
    if status == "PARTIAL":
        document.update(
            {
                "status": "PARTIAL",
                "reason_codes": ["FILL_PARTIAL"],
                "filled_base_atoms": "50000000",
                "unfilled_base_atoms": "50000000",
            }
        )
    _assert_rejected(_reseal(document), code="semantic_fill_release", path=("released_quote_atoms",))


def test_zero_non_quote_portfolio_balance_is_omitted() -> None:
    document = _example("trading.portfolio-state/v1")
    document["balances"].append(
        {
            "mint": "zz-zero-base-mint",
            "decimals": 9,
            "available_atoms": "0",
            "reserved_atoms": "0",
            "total_atoms": "0",
        }
    )
    document["balances"].sort(key=lambda row: row["mint"].encode("utf-8"))
    _assert_rejected(_reseal(document), code="semantic_balance", path=("balances",))


_LEDGER_OBJECT_SCHEMAS = {
    "SOURCE_ADMISSION": "trading.source-admission-receipt/v1",
    "CONFIG_ADMISSION": "trading.config-admission-receipt/v1",
    "RAW_ADMISSION": "trading.raw-event/v1",
    "FEATURE_SNAPSHOT": "trading.feature-snapshot/v1",
    "MODEL_VALIDATION": "trading.model-validation-receipt/v1",
    "MODEL_SIGNAL_ACCEPTED": "trading.model-signal/v1",
    "RISK_DECISION": "trading.risk-decision/v1",
    "RECONCILIATION": "trading.reconciliation-receipt/v1",
    "KILL_STATE": "trading.portfolio-state/v1",
    "RUN_RECEIPT": "trading.run-receipt/v1",
    "RUN_END": "trading.portfolio-state/v1",
}


@pytest.mark.parametrize("record_type", tuple(_LEDGER_OBJECT_SCHEMAS))
def test_evidence_only_ledger_types_forbid_postings(record_type: str) -> None:
    document = _example("trading.ledger-record/v1")
    document["record_type"] = record_type
    document["object_schema"] = _LEDGER_OBJECT_SCHEMAS[record_type]
    assert document["entries"]
    _assert_rejected(_reseal(document), code="semantic_entries", path=("entries",))


def test_portfolio_state_ledger_type_may_carry_group_mark_postings() -> None:
    document = _example("trading.ledger-record/v1")
    document["record_type"] = "PORTFOLIO_STATE"
    document["object_schema"] = "trading.portfolio-state/v1"
    _assert_valid(_reseal(document))


def test_previous_ledger_head_is_not_an_explicit_cause() -> None:
    document = _example("trading.ledger-record/v1")
    document["causation_ids"].append(document["previous_ledger_record_id"])
    document["causation_ids"].sort(key=lambda value: value.encode("utf-8"))
    _assert_rejected(_reseal(document), code="semantic_self_cause", path=("causation_ids",))


def test_previous_ledger_head_cannot_equal_current_record_id() -> None:
    # Equality is checked by the semantic layer independently of content-ID
    # verification, so this deliberately stale self-ID is still a closed,
    # structurally valid counterexample to the local tuple.
    document = _example("trading.ledger-record/v1")
    document["previous_ledger_record_id"] = document["ledger_record_id"]
    _assert_rejected(document, code="semantic_ledger_head", path=("previous_ledger_record_id",))


@pytest.mark.parametrize(
    "case",
    (
        "approve_entry_no_action",
        "approve_exit_no_action",
        "approve_exit_both_boundary_prefixes",
        "approve_exit_latched_code",
        "reject_latching_loss",
        "kill_no_action",
        "kill_session_closed",
        "reject_session_closed_no_action",
        "reject_pending_min_notional",
        "state_unreconciled_with_state_derived_fatal",
    ),
)
def test_risk_verdict_action_reason_families_are_closed(case: str) -> None:
    if case == "approve_entry_no_action":
        document = _sealed("trading.risk-decision/v1", ((("reason_codes",), ["RISK_NO_ACTION"]),))
    elif case == "approve_exit_no_action":
        document = _risk_exit(["RISK_NO_ACTION"])
    elif case == "approve_exit_both_boundary_prefixes":
        document = _risk_exit(["RISK_RUN_END_EXIT", "RISK_KILL_EXIT"])
    elif case == "approve_exit_latched_code":
        document = _risk_exit(["RISK_KILL_LATCHED"])
    elif case == "reject_latching_loss":
        document = _risk_zero_history_rejection("RISK_SESSION_LOSS")
    elif case == "kill_no_action":
        document = _risk_fatal_zero_tuple("RISK_NO_ACTION")
    elif case == "kill_session_closed":
        document = _risk_fatal_zero_tuple("RISK_SESSION_CLOSED")
    elif case == "reject_session_closed_no_action":
        document = _risk_zero_history_rejection()
        document["reason_codes"] = ["RISK_SESSION_CLOSED", "RISK_NO_ACTION"]
        document = _reseal(document)
    elif case == "reject_pending_min_notional":
        document = _risk_zero_history_rejection()
        document["reason_codes"] = ["RISK_INTENT_PENDING", "RISK_MIN_NOTIONAL"]
        document = _reseal(document)
    else:
        document = _risk_fatal_zero_tuple("RISK_STATE_UNRECONCILED")
        document["reason_codes"] = ["RISK_STATE_UNRECONCILED", "RISK_DECIMALS_MISMATCH"]
        document = _reseal(document)
    _assert_rejected(document, code="semantic_reason_family", path=("reason_codes",))


@pytest.mark.parametrize(
    "reasons",
    (
        (),
        ("RISK_STOP_TRIGGERED",),
        ("RISK_STOP_TRIGGERED", "RISK_TAKE_TRIGGERED"),
        ("RISK_RUN_END_EXIT",),
        ("RISK_RUN_END_EXIT", "RISK_STOP_TRIGGERED", "RISK_TAKE_TRIGGERED"),
        ("RISK_KILL_EXIT", "RISK_TAKE_TRIGGERED"),
        ("RISK_SESSION_LOSS", "RISK_KILL_EXIT", "RISK_STOP_TRIGGERED"),
        (
            "RISK_SESSION_LOSS",
            "RISK_DRAWDOWN",
            "RISK_RUN_END_EXIT",
            "RISK_STOP_TRIGGERED",
            "RISK_TAKE_TRIGGERED",
        ),
    ),
)
def test_risk_approved_exit_accepts_only_exact_informational_forms(reasons: tuple[str, ...]) -> None:
    _assert_valid(_risk_exit(reasons))


_SUPPRESSED_EXIT_REASON_PROBES = (
    ("RISK_RUN_END_EXIT",),
    ("RISK_KILL_EXIT",),
    ("RISK_STOP_TRIGGERED",),
    ("RISK_TAKE_TRIGGERED",),
    (
        "RISK_SESSION_LOSS",
        "RISK_DRAWDOWN",
        "RISK_RUN_END_EXIT",
        "RISK_STOP_TRIGGERED",
        "RISK_TAKE_TRIGGERED",
    ),
)


@pytest.mark.parametrize("reasons", _SUPPRESSED_EXIT_REASON_PROBES)
@pytest.mark.parametrize("case", ("accepted_model", "baseline_action", "fused_action"))
def test_nonempty_approved_exit_requires_exact_suppressed_evidence(
    reasons: tuple[str, ...],
    case: str,
) -> None:
    invalid = _risk_exit(reasons)
    if case == "accepted_model":
        invalid.update(
            {
                "model_signal_status": "ACCEPTED",
                "model_signal_id": "a" * 64,
                "model_validation_receipt_id": "b" * 64,
            }
        )
    else:
        invalid[case] = "EXIT_LONG"
    _assert_rejected(
        _reseal(invalid),
        code="semantic_suppressed_evidence",
        path=("reason_codes",),
    )


def test_empty_reason_approved_exit_may_consume_ordinary_model_and_fusion_evidence() -> None:
    document = _risk_exit()
    document.update(
        {
            "model_signal_status": "ACCEPTED",
            "model_signal_id": "a" * 64,
            "model_validation_receipt_id": "b" * 64,
            "baseline_action": "EXIT_LONG",
            "fused_action": "EXIT_LONG",
        }
    )
    _assert_valid(_reseal(document))


@pytest.mark.parametrize(
    ("reasons", "observation", "positioned"),
    (
        (("RISK_SESSION_LOSS",), False, False),
        (("RISK_DRAWDOWN",), True, False),
        (("RISK_SESSION_LOSS", "RISK_DRAWDOWN"), False, True),
        (("RISK_SESSION_LOSS", "RISK_INTENT_PENDING"), True, True),
        (("RISK_DRAWDOWN", "RISK_INTENT_PENDING"), False, False),
        (("RISK_SESSION_LOSS", "RISK_DRAWDOWN", "RISK_INTENT_PENDING"), True, True),
    ),
)
def test_null_reference_latching_loss_retains_state_derived_measures(
    reasons: tuple[str, ...],
    observation: bool,
    positioned: bool,
) -> None:
    _assert_valid(_risk_latching_loss(reasons, observation=observation, positioned=positioned))


@pytest.mark.parametrize(
    ("path", "value"),
    (
        (("measures", "participation_bps"), None),
        (("measures", "participation_bps"), 1),
        (("measures", "impact_bps"), 25),
        (("measures", "concentration_bps"), None),
        (("measures", "drawdown_bps"), None),
        (("measures", "projected_market_value_quote_atoms"), None),
        (("measures", "projected_equity_quote_atoms"), None),
        (("measures", "projected_equity_quote_atoms"), "0"),
        (("measures", "session_pnl_quote_atoms"), None),
        (("stop_price_q18",), "950000000000000000"),
        (("take_price_q18",), "1100000000000000000"),
    ),
)
def test_null_reference_latching_loss_rejects_missing_or_wrong_retained_fields(
    path: tuple[str | int, ...],
    value: Any,
) -> None:
    invalid = _risk_latching_loss()
    _replace(invalid, path, value)
    _assert_rejected(
        _reseal(invalid),
        code="semantic_loss_authority",
        path=("reason_codes",),
    )


@pytest.mark.parametrize(
    "case", ("accepted_model", "baseline_action", "fused_action", "requested_base", "requested_notional")
)
def test_latching_loss_requires_suppressed_zero_authority(case: str) -> None:
    invalid = _risk_latching_loss()
    if case == "accepted_model":
        invalid.update(
            {
                "model_signal_status": "ACCEPTED",
                "model_signal_id": "a" * 64,
                "model_validation_receipt_id": "b" * 64,
            }
        )
    elif case == "requested_base":
        invalid["requested_base_atoms"] = "1"
    elif case == "requested_notional":
        invalid["requested_notional_quote_atoms"] = "1"
    else:
        invalid[case] = "ENTER_LONG"
    _assert_rejected(
        _reseal(invalid),
        code="semantic_loss_authority",
        path=("reason_codes",),
    )


def test_ordinary_zero_history_cannot_retain_state_measures() -> None:
    invalid = _risk_zero_history_rejection()
    invalid["measures"]["participation_bps"] = 0
    _assert_rejected(
        _reseal(invalid),
        code="semantic_zero_authority",
        path=("reference_price_q18",),
    )


def test_state_unreconciled_may_retain_independently_provable_sequence_failure() -> None:
    document = _risk_fatal_zero_tuple("RISK_STATE_UNRECONCILED")
    document["reason_codes"] = ["RISK_SEQUENCE_INVALID", "RISK_STATE_UNRECONCILED"]
    _assert_valid(_reseal(document))


def test_risk_approved_exit_allows_zero_quote_notional() -> None:
    _assert_valid(_risk_exit(notional="0"))


def test_risk_approved_exit_base_reservation_equals_approved_base() -> None:
    invalid = _risk_exit(reserved_base_atoms="1")
    _assert_rejected(invalid, code="semantic_reservation", path=("reserved_base_atoms",))


def test_risk_approved_entry_still_requires_positive_quote_notional() -> None:
    invalid = _sealed(
        "trading.risk-decision/v1",
        (
            (("requested_notional_quote_atoms",), "0"),
            (("approved_notional_quote_atoms",), "0"),
        ),
    )
    _assert_rejected(invalid, code="semantic_approval", path=("verdict",))


def test_nonpositive_equity_allows_null_mid_with_owned_measures() -> None:
    _assert_valid(_risk_nonpositive_equity())


@pytest.mark.parametrize("case", ("consumed_model", "baseline_action", "fused_action"))
def test_zero_history_null_mid_retains_suppressed_evidence_tuple(case: str) -> None:
    invalid = _risk_zero_history_rejection()
    if case == "consumed_model":
        invalid.update(
            {
                "model_signal_status": "ACCEPTED",
                "model_signal_id": "a" * 64,
                "model_validation_receipt_id": "b" * 64,
            }
        )
    else:
        invalid[case] = "ENTER_LONG"
    _assert_rejected(
        _reseal(invalid),
        code="semantic_zero_authority",
        path=("reference_price_q18",),
    )


@pytest.mark.parametrize(
    ("path", "value"),
    (
        (("measures", "participation_bps"), None),
        (("measures", "concentration_bps"), 0),
        (("measures", "drawdown_bps"), None),
        (("measures", "projected_market_value_quote_atoms"), None),
        (("measures", "projected_equity_quote_atoms"), "1"),
        (("measures", "session_pnl_quote_atoms"), None),
    ),
)
def test_nonpositive_equity_null_mid_tuple_is_exact(
    path: tuple[str | int, ...],
    value: Any,
) -> None:
    invalid = _risk_nonpositive_equity()
    _replace(invalid, path, value)
    _assert_rejected(_reseal(invalid), code="semantic_zero_authority", path=("reference_price_q18",))
