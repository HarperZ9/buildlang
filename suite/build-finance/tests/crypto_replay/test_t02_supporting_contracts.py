"""T02 RED contracts for the thirteen supporting replay records.

The vectors in this module are synthetic contract fixtures.  They deliberately
exercise only local canonical bytes, closed schemas, content IDs, and retained
preimages; they are not admitted market data and provide no replay authority.
"""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from hashlib import sha256
from typing import Any, Literal

from build_finance.crypto_replay.canonical import (
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import (
    compute_content_id,
    seal_content_id,
    verify_content_id,
)
from build_finance.crypto_replay.schema_model import ResolvedContent

MARKET_ID = "synthetic-base/synthetic-quote:jupiter"
BASE_MINT = "synthetic-base"
QUOTE_MINT = "synthetic-quote"
MAX_U64 = 18_446_744_073_709_551_615
MAX_PROOF_ROWS = 1_000_000


def _digest(label: str) -> str:
    return sha256(f"SYNTHETIC_T02_CONTRACT_VECTOR:{label}".encode("ascii")).hexdigest()


def _attachment(document: dict[str, Any]) -> tuple[bytes, str]:
    payload = canonical_json_bytes(document)
    return payload, sha256_hex(payload)


def _seal(schema: str, **fields: Any) -> dict[str, Any]:
    return seal_content_id({"schema": schema, **fields})


def _reseal(document: dict[str, Any]) -> dict[str, Any]:
    from build_finance.crypto_replay.schema_definitions import SELF_ID_FIELDS

    result = deepcopy(document)
    result.pop(SELF_ID_FIELDS[str(result["schema"])], None)
    return seal_content_id(result)


class SyntheticResolver:
    """Minimal immutable resolver for closed, clearly synthetic T02 vectors."""

    def __init__(
        self,
        documents: tuple[dict[str, Any], ...],
        retained_bytes: tuple[bytes, ...],
    ) -> None:
        from build_finance.crypto_replay.schema_definitions import CONTRACT_SPECS_BY_SCHEMA
        from build_finance.crypto_replay.schema_registry import require_valid_contract

        self._objects: dict[str, ResolvedContent] = {}
        for document in documents:
            record = canonical_record_bytes(document)
            parsed = parse_canonical_record(record)
            assert parsed == document
            assert verify_content_id(document)
            family = CONTRACT_SPECS_BY_SCHEMA[str(document["schema"])].family
            assurance: Literal["DIGEST_ONLY", "SCHEMA_VALID"]
            if family == "PRIMARY":
                require_valid_contract(document, expected_schema=str(document["schema"]))
                assurance = "SCHEMA_VALID"
            elif family == "SUPPORTING":
                assurance = "DIGEST_ONLY"
            else:
                raise AssertionError(f"unexpected self-addressed family: {family}")
            content_id = compute_content_id(document)
            existing = self._objects.get(content_id)
            if existing is not None and existing.record != record:
                raise AssertionError("synthetic resolver ContentID collision")
            self._objects[content_id] = ResolvedContent(
                record=record,
                document=deepcopy(document),
                assurance=assurance,
            )
        self._bytes = {sha256_hex(payload): bytes(payload) for payload in retained_bytes}

    def __repr__(self) -> str:
        return f"SyntheticResolver(objects={len(self._objects)}, retained_byte_digests={len(self._bytes)})"

    def resolve_object(self, content_id: str) -> ResolvedContent | None:
        resolved = self._objects.get(content_id)
        if resolved is None:
            return None
        return ResolvedContent(
            record=bytes(resolved.record),
            document=deepcopy(dict(resolved.document)),
            assurance=resolved.assurance,
        )

    def resolve_bytes(self, digest: str) -> bytes | None:
        payload = self._bytes.get(digest)
        return None if payload is None else bytes(payload)


@dataclass(frozen=True, slots=True)
class T02Vector:
    documents: dict[str, dict[str, Any]]
    attachments: dict[str, dict[str, Any]]
    attachment_payloads: dict[str, bytes]
    raw_payloads: tuple[bytes, bytes]
    raw_config_bytes: bytes
    public_seed_bytes: bytes
    code_preimages: dict[str, bytes]
    source_receipts: tuple[dict[str, Any], dict[str, Any]]
    raw_events: tuple[dict[str, Any], dict[str, Any]]

    @property
    def resolver(self) -> SyntheticResolver:
        return SyntheticResolver(
            (*self.documents.values(), *self.source_receipts, *self.raw_events),
            (
                *self.attachment_payloads.values(),
                *self.raw_payloads,
                self.raw_config_bytes,
                self.public_seed_bytes,
                *self.code_preimages.values(),
            ),
        )


def _code_preimages() -> dict[str, bytes]:
    names = (
        "admission_code_sha256",
        "normalization_code_sha256",
        "availability_grouping_code_sha256",
        "run_closure_code_sha256",
        "feature_code_sha256",
        "baseline_code_sha256",
        "risk_code_sha256",
        "fill_code_sha256",
        "accounting_code_sha256",
        "benchmark_code_sha256",
    )
    return {name: f"SYNTHETIC T02 CONTRACT-ONLY CODE PREIMAGE: {name}\n".encode("ascii") for name in names}


def build_t02_vector() -> T02Vector:
    """Build one bottom-up, disabled-model, contract-only T02 graph."""
    raw_payloads = (
        b"SYNTHETIC T02 FIXTURE PAYLOAD - NOT MARKET DATA - GROUP 1\n",
        b"SYNTHETIC T02 FIXTURE PAYLOAD - NOT MARKET DATA - GROUP 2\n",
    )
    raw_payload_digests = tuple(sha256_hex(payload) for payload in raw_payloads)
    schema_bundle_sha256 = _digest("schema-bundle")
    code_preimages = _code_preimages()
    code_hashes = {name: sha256_hex(payload) for name, payload in code_preimages.items()}

    fixture = _seal(
        "trading.fixture-manifest/v1",
        network="solana-mainnet",
        venue_profile="solana-jupiter-fixture/v1",
        quote_mint=QUOTE_MINT,
        quote_decimals=6,
        initial_quote_atoms="1000000",
        replay_tick_ns="1000000000",
        universe_policy="FULL_DECLARED_SOURCE_UNIVERSE",
        point_in_time_mode="PROVEN_AVAILABILITY_SLOT",
        run_end_position_policy="LEAVE_MARKED_OPEN",
        session_start_availability_slot="1",
        session_end_availability_slot="2",
        rights_manifest_sha256=_digest("rights-manifest"),
        selection_failures_retained=True,
        no_route_observations_retained=True,
        inactive_assets_retained=True,
        gaps_retained=True,
        allowed_markets=[
            {
                "market_id": MARKET_ID,
                "base_mint": BASE_MINT,
                "quote_mint": QUOTE_MINT,
                "base_decimals": 9,
                "quote_decimals": 6,
            }
        ],
        files=[
            {
                "relative_path": "quotes/group-1.json",
                "raw_payload_sha256": raw_payload_digests[0],
                "byte_length": str(len(raw_payloads[0])),
                "admission_sequence": "1",
                "availability_slot": "1",
                "media_type": "application/json",
                "source_id": "synthetic-local-fixture",
                "source_kind": "JUPITER_ROUTE_QUOTE_FIXTURE",
                "source_revision": "synthetic-revision-1",
                "market_id": MARKET_ID,
            },
            {
                "relative_path": "quotes/group-2.json",
                "raw_payload_sha256": raw_payload_digests[1],
                "byte_length": str(len(raw_payloads[1])),
                "admission_sequence": "3",
                "availability_slot": "2",
                "media_type": "application/json",
                "source_id": "synthetic-local-fixture",
                "source_kind": "JUPITER_ROUTE_QUOTE_FIXTURE",
                "source_revision": "synthetic-revision-1",
                "market_id": MARKET_ID,
            },
        ],
    )

    config = _seal(
        "trading.replay-risk-config/v1",
        baseline_id="MOMENTUM_V1",
        target_entry_notional_quote_atoms="100000",
        min_notional_quote_atoms="1000",
        max_notional_quote_atoms="500000",
        price_tick_q18="1000000000000",
        max_participation_bps=500,
        max_impact_bps=50,
        max_fee_bps=50,
        max_concentration_bps=2500,
        max_session_loss_quote_atoms="100000",
        max_drawdown_bps=1000,
        stale_after_ns="5000000000",
        stop_loss_bps=500,
        take_profit_bps=1000,
        adverse_fill_bps_max=10,
        max_run_closure_proof_rows=str(MAX_PROOF_ROWS),
        fee_model_version="solana-jupiter-fixture-fee-proration/v1",
        liquidity_model_version="solana-jupiter-route-capacity/v1",
        session_start_replay_clock_ns="0",
        session_end_replay_clock_ns="1000000000",
        kill_exit_mode="CLOSE_ON_NEXT_EVENT",
    )
    raw_config_bytes = canonical_record_bytes(config)
    config_receipt = _seal(
        "trading.config-admission-receipt/v1",
        status="VALID",
        reason_codes=[],
        raw_config_sha256=sha256_hex(raw_config_bytes),
        validated_config_sha256=config["config_sha256"],
        raw_byte_length=str(len(raw_config_bytes)),
        admission_sequence="2",
        validator_code_sha256=_digest("config-validator"),
        schema_bundle_sha256=schema_bundle_sha256,
    )

    source_receipts: list[dict[str, Any]] = []
    for index, (payload, availability_slot, admission_sequence) in enumerate(
        ((raw_payloads[0], "1", "1"), (raw_payloads[1], "2", "3")),
        start=1,
    ):
        source_receipts.append(
            _seal(
                "trading.source-admission-receipt/v1",
                status="ADMITTED",
                reason_codes=[],
                fixture_manifest_sha256=fixture["fixture_manifest_sha256"],
                raw_payload_sha256=sha256_hex(payload),
                terms_sha256=_digest(f"terms-{index}"),
                parser_code_sha256=_digest("source-parser"),
                source_id="synthetic-local-fixture",
                source_kind="JUPITER_ROUTE_QUOTE_FIXTURE",
                source_revision="synthetic-revision-1",
                market_id=MARKET_ID,
                relative_path=f"quotes/group-{index}.json",
                media_type="application/json",
                byte_length=str(len(payload)),
                admission_sequence=admission_sequence,
                availability_slot=availability_slot,
                observed_at=f"2026-01-02T00:00:0{index}.000000000Z",
                ingested_at=f"2026-01-02T00:00:0{index}.000000001Z",
                rights_role="offline_research_replay",
                rights_effective_date="2026-01-01",
                rights_review_date="2026-01-02",
                parser_version="solana-jupiter-fixture-parser/v1",
            )
        )

    raw_events: list[dict[str, Any]] = []
    for index, source_receipt in enumerate(source_receipts, start=1):
        raw_events.append(
            _seal(
                "trading.raw-event/v1",
                fixture_manifest_sha256=fixture["fixture_manifest_sha256"],
                raw_payload_sha256=raw_payload_digests[index - 1],
                source_admission_receipt_id=source_receipt["source_admission_receipt_id"],
                source_id="synthetic-local-fixture",
                source_kind="JUPITER_ROUTE_QUOTE_FIXTURE",
                source_revision="synthetic-revision-1",
                network="solana-mainnet",
                venue_profile="solana-jupiter-fixture/v1",
                market_id=MARKET_ID,
                base_mint=BASE_MINT,
                quote_mint=QUOTE_MINT,
                base_decimals=9,
                quote_decimals=6,
                event_kind="ROUTE_QUOTE",
                source_position={
                    "slot": str(index),
                    "transaction_index": 0,
                    "instruction_index": 0,
                    "event_index": 0,
                    "source_native_event_id": f"synthetic-quote-{index}",
                    "source_subsequence": "0",
                },
                revision={
                    "kind": "ORIGINAL",
                    "supersedes_event_id": None,
                    "retracts_event_id": None,
                    "availability_slot": str(index),
                    "availability_admission_sequence": source_receipt["admission_sequence"],
                },
                event_time=f"2026-01-01T00:00:0{index}.000000000Z",
                observed_at=source_receipt["observed_at"],
                ingested_at=source_receipt["ingested_at"],
                admission_sequence=source_receipt["admission_sequence"],
                source_sequence=str(index),
                ingest_sequence=str(index),
                equal_time_group=str(index),
                replay_clock_ns=str((index - 1) * 1_000_000_000),
                executable=True,
                market={
                    "base_amount_atoms": "1000000000",
                    "quote_amount_atoms": str(1_000_000 + index),
                    "route_capacity_base_atoms": "5000000000",
                    "liquidity_quote_atoms": "10000000",
                    "venue_fee_quote_atoms": "2500",
                    "priority_fee_quote_atoms": "0",
                    "route_impact_bps": 25,
                },
                quality_flags=[],
            )
        )

    source_ids = sorted(receipt["source_admission_receipt_id"] for receipt in source_receipts)
    availability_schedule = {
        "schema": "trading.availability-schedule/v1",
        "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
        "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
        "source_admission_receipt_ids": source_ids,
        "availability_groups": [
            {"availability_slot": "1", "equal_time_group": "1", "admission_cutoff": "2"},
            {"availability_slot": "2", "equal_time_group": "2", "admission_cutoff": "3"},
        ],
    }
    normalized_event_set = {
        "schema": "trading.normalized-event-set/v1",
        "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
        "raw_event_count": "2",
        "event_ids": [event["event_id"] for event in raw_events],
    }
    normalized_payload, normalized_digest = _attachment(normalized_event_set)
    counter_capacity = {
        "schema": "trading.counter-capacity/v1",
        "model_signal_mode": "DISABLED",
        "model_registry_sha256": None,
        "model_signal_manifest_sha256": None,
        "normalized_event_set_sha256": normalized_digest,
        "source_admission_count": "2",
        "raw_event_count": "2",
        "availability_group_count": "2",
        "market_count": "1",
        "model_candidate_count": "0",
        "decision_attempt_upper_bound": "2",
        "decision_sequence_next_upper_bound": "3",
        "producer_sequence_next_upper_bound": "0",
        "intent_sequence_next_upper_bound": "3",
        "fill_receipt_sequence_next_upper_bound": "3",
        "unmatched_reservation_count_upper_bound": "4",
        "state_sequence_next_upper_bound": "13",
        "ledger_sequence_next_upper_bound": "43",
        "status": "WITHIN_LIMIT",
        "first_exceeded_counter": None,
    }
    availability_payload, availability_digest = _attachment(availability_schedule)
    counter_payload, counter_digest = _attachment(counter_capacity)

    closure = _seal(
        "trading.run-closure-receipt/v1",
        status="PASS",
        reason_codes=[],
        fixture_manifest_sha256=fixture["fixture_manifest_sha256"],
        config_admission_receipt_id=config_receipt["config_admission_receipt_id"],
        validated_config_sha256=config["config_sha256"],
        model_signal_mode="DISABLED",
        model_registry_sha256=None,
        model_signal_manifest_sha256=None,
        schema_bundle_sha256=schema_bundle_sha256,
        admission_code_sha256=code_hashes["admission_code_sha256"],
        normalization_code_sha256=code_hashes["normalization_code_sha256"],
        availability_grouping_code_sha256=code_hashes["availability_grouping_code_sha256"],
        run_closure_code_sha256=code_hashes["run_closure_code_sha256"],
        risk_code_sha256=code_hashes["risk_code_sha256"],
        feature_code_sha256=code_hashes["feature_code_sha256"],
        fill_code_sha256=code_hashes["fill_code_sha256"],
        accounting_code_sha256=code_hashes["accounting_code_sha256"],
        source_admission_receipt_ids=source_ids,
        availability_schedule_sha256=availability_digest,
        counter_capacity_sha256=counter_digest,
        counter_capacity_status="WITHIN_LIMIT",
        run_end_position_policy="LEAVE_MARKED_OPEN",
        terminal_equal_time_group="2",
        proof_row_limit=str(MAX_PROOF_ROWS),
        proof_row_count_total="0",
        proof_budget_status="NOT_REQUIRED",
        market_proofs=[],
    )

    source_tree = {
        "schema": "trading.source-tree/v1",
        "root_label": "repository-root",
        "path_policy": "UTF8_NFC_POSIX_RELATIVE_NO_SYMLINK_V1",
        "exclusion_prefixes": [
            ".git/",
            ".hg/",
            ".mypy_cache/",
            ".nox/",
            ".pytest_cache/",
            ".ruff_cache/",
            ".svn/",
            ".tox/",
            ".venv/",
            "__pycache__/",
            "build/",
            "dist/",
            "node_modules/",
            "scratch/",
            "tmp/",
            "venv/",
        ],
        "files": [
            {
                "relative_path": "src/kernel.py",
                "byte_length": "3",
                "file_sha256": sha256_hex(b"abc"),
            },
            {
                "relative_path": "src/risk.py",
                "byte_length": "0",
                "file_sha256": sha256_hex(b""),
            },
        ],
    }
    source_tree_payload, source_tree_digest = _attachment(source_tree)
    public_seed_bytes = bytes.fromhex("00" * 31 + "0f")
    run_receipt = _seal(
        "trading.run-receipt/v1",
        fixture_manifest_sha256=fixture["fixture_manifest_sha256"],
        schema_bundle_sha256=schema_bundle_sha256,
        admission_code_sha256=code_hashes["admission_code_sha256"],
        normalization_code_sha256=code_hashes["normalization_code_sha256"],
        availability_grouping_code_sha256=code_hashes["availability_grouping_code_sha256"],
        run_closure_code_sha256=code_hashes["run_closure_code_sha256"],
        feature_code_sha256=code_hashes["feature_code_sha256"],
        baseline_code_sha256=code_hashes["baseline_code_sha256"],
        risk_code_sha256=code_hashes["risk_code_sha256"],
        fill_code_sha256=code_hashes["fill_code_sha256"],
        accounting_code_sha256=code_hashes["accounting_code_sha256"],
        benchmark_code_sha256=code_hashes["benchmark_code_sha256"],
        public_seed_sha256=sha256_hex(public_seed_bytes),
        source_tree_sha256=source_tree_digest,
        source_admission_receipt_ids=source_ids,
        availability_schedule_sha256=availability_digest,
        config_admission_receipt_id=config_receipt["config_admission_receipt_id"],
        validated_config_sha256=config["config_sha256"],
        run_closure_receipt_id=closure["run_closure_receipt_id"],
        model_registry_sha256=None,
        model_signal_manifest_sha256=None,
        selected_model_scopes=[],
        replay_tick_ns="1000000000",
        model_decision_budget_ns="0",
        model_signal_mode="DISABLED",
        run_end_position_policy="LEAVE_MARKED_OPEN",
        terminal_equal_time_group="2",
        availability_groups=deepcopy(availability_schedule["availability_groups"]),
        os_name="synthetic-os",
        architecture="synthetic-architecture",
        runtime_name="cpython",
        runtime_version="synthetic-runtime",
        decimal_version="1",
        jcs_implementation="build-finance",
        jcs_version="1",
        tool_versions=[{"name": "pytest", "version": "synthetic-version"}],
        public_seed_hex=public_seed_bytes.hex(),
    )

    quarantine = _seal(
        "trading.execution-quarantine-receipt/v1",
        status="QUARANTINED",
        reason_codes=[
            "QUARANTINE_INITIAL_PREFIX_ANCHOR_MISSING",
            "QUARANTINE_LEDGER_SLOT_MISSING",
            "QUARANTINE_FOOTPRINT_MISMATCH",
        ],
        run_receipt_id=run_receipt["run_receipt_id"],
        transition_kind="EXECUTION_TRANSITION",
        transition_key_sha256=_digest("transition-key"),
        expected_footprint_sha256=_digest("expected-footprint"),
        observed_footprint_sha256=_digest("observed-footprint"),
        last_verified_ledger_head_id=None,
        last_verified_portfolio_state_id=None,
        first_unsafe_ledger_sequence="0",
        slot_observations=[
            {
                "ledger_sequence": "0",
                "expected_ledger_record_id": _digest("expected-ledger-record"),
                "observed_ledger_record_id": None,
                "observed_byte_sha256": None,
                "classification": "MISSING",
            }
        ],
        component_observations=[],
    )

    producer_scope = {
        "adapter_sha256": None,
        "calibration_sha256": _digest("calibration"),
        "calibration_version": "synthetic-calibration-v1",
        "feature_set_version": "solana-jupiter-deterministic-features/v1",
        "horizon_ns": "30000000000",
        "market_id": MARKET_ID,
        "model_artifact_sha256": _digest("model-artifact"),
        "model_id": "synthetic-diagnostic-model",
        "model_version": "synthetic-model-v1",
        "producer_id": "synthetic-producer",
        "runtime_profile_id": "offline-cached-signal/v1",
    }
    _, producer_scope_key = _attachment(producer_scope)
    model_registry = _seal(
        "trading.model-registry/v1",
        promotions=[
            {
                "producer_scope_key_sha256": producer_scope_key,
                "producer_id": producer_scope["producer_id"],
                "model_id": producer_scope["model_id"],
                "model_version": producer_scope["model_version"],
                "model_artifact_sha256": producer_scope["model_artifact_sha256"],
                "adapter_sha256": None,
                "runtime_profile_id": producer_scope["runtime_profile_id"],
                "calibration_version": producer_scope["calibration_version"],
                "calibration_sha256": producer_scope["calibration_sha256"],
                "feature_set_version": producer_scope["feature_set_version"],
                "market_id": MARKET_ID,
                "horizon_ns": "30000000000",
                "max_ttl_ns": "60000000000",
                "max_uncertainty_q18": "900000000000000000",
                "max_ood_score_q18": "900000000000000000",
                "status": "DISABLED",
            }
        ],
    )
    raw_signal = b"SYNTHETIC T02 STATIC MODEL CONTRACT BYTES - NO INFERENCE\n"
    model_signal_manifest = _seal(
        "trading.model-signal-manifest/v1",
        model_registry_sha256=model_registry["model_registry_sha256"],
        candidates=[
            {
                "relative_path": "signals/decision-1.json",
                "media_type": "application/json",
                "raw_signal_sha256": sha256_hex(raw_signal),
                "raw_byte_length": str(len(raw_signal)),
                "declared_signal_id": None,
                "feature_snapshot_id": _digest("feature-snapshot"),
                "decision_sequence": "1",
                "producer_scope_key_sha256": None,
                "producer_sequence": None,
                "horizon_ns": "30000000000",
                "issued_replay_clock_ns": "0",
                "available_replay_clock_ns": "1",
                "decision_close_replay_clock_ns": "2",
                "requested_producer_scope_key_sha256": producer_scope_key,
            }
        ],
    )
    attempt_key_document = {
        "schema": "trading.model-validation-attempt-key/v1",
        "model_signal_manifest_sha256": model_signal_manifest["model_signal_manifest_sha256"],
        "relative_path": "signals/decision-1.json",
        "raw_signal_sha256": sha256_hex(raw_signal),
        "feature_snapshot_id": _digest("feature-snapshot"),
        "decision_sequence": "1",
        "requested_producer_scope_key_sha256": producer_scope_key,
        "horizon_ns": "30000000000",
    }
    _, attempt_key = _attachment(attempt_key_document)
    model_validation = _seal(
        "trading.model-validation-receipt/v1",
        validation_attempt_key_sha256=attempt_key,
        status="REJECTED",
        reason_codes=["SIG_BYTES_INVALID"],
        raw_signal_sha256=sha256_hex(raw_signal),
        declared_signal_id=None,
        accepted_signal_id=None,
        feature_snapshot_id=_digest("feature-snapshot"),
        decision_sequence="1",
        producer_sequence=None,
        producer_scope_key_sha256=None,
        issued_replay_clock_ns=None,
        available_replay_clock_ns="1",
        decision_close_replay_clock_ns="2",
        expires_replay_clock_ns=None,
        canonical_action="ABSTAIN",
        canonical_score_q18="0",
        canonical_probability_abstain_q18="1000000000000000000",
        canonical_probability_long_bias_q18="0",
        canonical_probability_exit_bias_q18="0",
        canonical_uncertainty_q18="1000000000000000000",
        canonical_ood_score_q18="1000000000000000000",
    )

    reconciliation = _seal(
        "trading.reconciliation-receipt/v1",
        status="PASS",
        reconciliation_kind="FILL_TRANSITION",
        reason_codes=[],
        decision_sequence="1",
        ingest_sequence="2",
        equal_time_group="2",
        replay_clock_ns="1000000000",
        portfolio_state_before_id=_digest("state-before"),
        portfolio_state_after_id=_digest("state-after"),
        causation_ids=[_digest("fill-receipt")],
        asset_residuals=[
            {"asset_mint": BASE_MINT, "residual_atoms": "0"},
            {"asset_mint": QUOTE_MINT, "residual_atoms": "0"},
        ],
        account_residuals=[],
        realized_pnl_residual_quote_atoms="0",
        unrealized_pnl_residual_quote_atoms="0",
        fee_residual_quote_atoms="0",
        peak_equity_residual_quote_atoms="0",
        drawdown_residual_bps="0",
        equity_residual_quote_atoms="0",
        unmatched_reservation_count="0",
        idempotency_conflict_scope=None,
        idempotency_key_sha256=None,
        original_object_id=None,
        conflicting_body_sha256=None,
        integrity_validation_attempt_key_sha256=None,
        integrity_transition_key_sha256=None,
        integrity_expected_footprint_sha256=None,
        integrity_observed_footprint_sha256=None,
        integrity_ledger_head_before_check_id=None,
        arithmetic_range_key_sha256=None,
        arithmetic_operands_sha256=None,
        arithmetic_operation=None,
        kill_latched=False,
    )

    benchmark_request = {
        "schema": "trading.benchmark-request/v1",
        "request_code_sha256": _digest("benchmark-request-code"),
        "fixture_manifest_sha256": None,
        "config_admission_receipt_id": None,
        "run_closure_receipt_id": None,
        "benchmark_manifest_raw_sha256": None,
        "preregistered_thresholds_raw_sha256": _digest("thresholds-raw"),
        "hardware_profile_raw_sha256": _digest("hardware-raw"),
        "benchmark_manifest_raw_byte_length": "0",
        "preregistered_thresholds_raw_byte_length": "1",
        "hardware_profile_raw_byte_length": "1",
        "requested_metric_set": "OFFLINE_REPLAY_CORE_V1",
    }
    benchmark_request_payload, benchmark_request_digest = _attachment(benchmark_request)
    phase_units = (
        ("admission", "RAW_EVENT", ["100", "110"]),
        ("feature", "FEATURE_SNAPSHOT", ["200", "210"]),
        ("risk", "RISK_DECISION", ["300", "310"]),
        ("fill", "SIMULATED_FILL_RECEIPT", ["400"]),
        ("accounting", "RECONCILIATION_RECEIPT", ["500", "510"]),
        ("end_to_end", "EQUAL_TIME_GROUP", ["1600", "1660"]),
    )
    benchmark_measurement = _seal(
        "trading.benchmark-measurement/v1",
        measurement_status="COMPLETE",
        first_range_failure=None,
        range_failure_fields=[],
        raw_counters=[
            {"counter": "MEASURED_WALL_DURATION", "raw_value": "3260"},
            {"counter": "PROCESS_CPU_TIME", "raw_value": "3000"},
            {"counter": "WALL_DURATION", "raw_value": "4000"},
            {"counter": "PEAK_RSS_BYTES", "raw_value": "1048576"},
            {"counter": "ALLOCATION_COUNT", "raw_value": None},
            {"counter": "INPUT_BYTES", "raw_value": "2048"},
            {"counter": "OUTPUT_BYTES", "raw_value": "4096"},
        ],
        run_receipt_id=run_receipt["run_receipt_id"],
        case_id="latency-core",
        repetition_index="0",
        benchmark_manifest_sha256=_digest("benchmark-manifest"),
        hardware_profile_sha256=_digest("hardware-profile"),
        measurement_tool_sha256=_digest("measurement-tool"),
        warmup_event_count="0",
        measured_event_count="2",
        warmup_group_count="0",
        measured_group_count="2",
        warmup_through_equal_time_group=None,
        phase_samples=[
            {"phase": phase, "unit": unit, "sample_count": str(len(samples)), "samples_ns": samples}
            for phase, unit, samples in phase_units
        ],
        measured_wall_duration_ns="3260",
        process_cpu_time_ns="3000",
        wall_duration_ns="4000",
        peak_rss_bytes="1048576",
        allocation_count=None,
        input_bytes="2048",
        output_bytes="4096",
    )
    benchmark_receipt = _seal(
        "trading.benchmark-receipt/v1",
        status="INELIGIBLE",
        reason_codes=["BENCHMARK_FIXTURE_NOT_ADMITTED", "BENCHMARK_PREREGISTRATION_MISSING"],
        benchmark_request_sha256=benchmark_request_digest,
        benchmark_manifest_sha256=None,
        preregistered_thresholds_sha256=_digest("thresholds"),
        hardware_profile_sha256=_digest("hardware"),
        metrics_artifact_sha256=None,
        percentile_method="NEAREST_RANK_CEIL",
        sample_count="0",
        run_outputs=[],
    )

    documents = {
        document["schema"]: document
        for document in (
            fixture,
            config,
            *source_receipts,
            config_receipt,
            closure,
            quarantine,
            model_registry,
            model_signal_manifest,
            model_validation,
            reconciliation,
            run_receipt,
            benchmark_measurement,
            benchmark_receipt,
        )
    }
    attachments: dict[str, dict[str, Any]] = {
        str(document["schema"]): dict(document)
        for document in (
            availability_schedule,
            normalized_event_set,
            counter_capacity,
            source_tree,
            benchmark_request,
        )
    }
    attachment_payloads = {
        availability_schedule["schema"]: availability_payload,
        normalized_event_set["schema"]: normalized_payload,
        counter_capacity["schema"]: counter_payload,
        source_tree["schema"]: source_tree_payload,
        benchmark_request["schema"]: benchmark_request_payload,
        "synthetic:model-signal-bytes": raw_signal,
    }
    return T02Vector(
        documents=documents,
        attachments=attachments,
        attachment_payloads=attachment_payloads,
        raw_payloads=raw_payloads,
        raw_config_bytes=raw_config_bytes,
        public_seed_bytes=public_seed_bytes,
        code_preimages=code_preimages,
        source_receipts=(source_receipts[0], source_receipts[1]),
        raw_events=(raw_events[0], raw_events[1]),
    )


def _issues(
    document: dict[str, Any],
    *,
    resolver: SyntheticResolver | None = None,
    expected_schema: str | None = None,
) -> tuple[Any, ...]:
    from build_finance.crypto_replay.schema_registry import validate_contract

    declared_schema = document.get("schema")
    target_schema = expected_schema if expected_schema is not None else declared_schema
    return validate_contract(
        document,
        expected_schema=target_schema if isinstance(target_schema, str) else None,
        resolver=resolver,
    )


def _assert_invalid(document: dict[str, Any], *, resolver: SyntheticResolver | None = None) -> None:
    assert _issues(document, resolver=resolver), f"mutation unexpectedly valid: {document['schema']}"


def _schema_is_recursively_closed(schema: dict[str, Any]) -> None:
    def visit(node: Any) -> None:
        if isinstance(node, dict):
            object_keywords = {"properties", "required", "additionalProperties"} & node.keys()
            if object_keywords:
                assert node.get("type") == "object"
                assert node.get("additionalProperties") is False
                properties = node.get("properties")
                assert isinstance(properties, dict)
                assert node.get("required") == list(properties)
            for value in node.values():
                visit(value)
        elif isinstance(node, list):
            for value in node:
                visit(value)

    visit(schema)


def _assert_independent_basic_oracles(
    document: dict[str, Any],
    *,
    resolver: SyntheticResolver,
) -> None:
    """Cross-check one positive, unknown-key, and missing-key vector."""
    from jsonschema import Draft202012Validator

    from build_finance.crypto_replay.schema_definitions import (
        SUPPORTING_SCHEMA_DOCUMENTS,
        SUPPORTING_SELF_ID_FIELDS,
    )

    schema_id = str(document["schema"])
    schema = SUPPORTING_SCHEMA_DOCUMENTS[schema_id]
    assert verify_content_id(document)
    _schema_is_recursively_closed(schema)
    Draft202012Validator.check_schema(schema)
    oracle = Draft202012Validator(schema)
    assert not tuple(oracle.iter_errors(document))
    assert not _issues(document, resolver=resolver)

    unknown = _reseal({**document, "unknown_task7_property": True})
    assert tuple(oracle.iter_errors(unknown))
    assert _issues(unknown, resolver=resolver)

    self_id_field = SUPPORTING_SELF_ID_FIELDS[schema_id]
    properties = schema["properties"]
    assert isinstance(properties, dict)
    missing_field = next(field for field in properties if field not in {"schema", self_id_field})
    missing = deepcopy(document)
    missing.pop(missing_field)
    missing = _reseal(missing)
    assert tuple(oracle.iter_errors(missing))
    assert _issues(missing, resolver=resolver, expected_schema=schema_id)

    forged = deepcopy(document)
    forged[self_id_field] = "0" * 64
    assert not verify_content_id(forged)


_SOURCE_BUILDER_FIELDS = (
    "fixture_manifest_sha256",
    "raw_payload_sha256",
    "terms_sha256",
    "parser_code_sha256",
    "source_id",
    "source_kind",
    "source_revision",
    "market_id",
    "relative_path",
    "media_type",
    "byte_length",
    "admission_sequence",
    "availability_slot",
    "observed_at",
    "ingested_at",
    "rights_role",
    "rights_effective_date",
    "rights_review_date",
    "parser_version",
)


def _source_builder_inputs(document: dict[str, Any]) -> dict[str, Any]:
    return {field: document[field] for field in _SOURCE_BUILDER_FIELDS}


def test_all_supporting_contracts_are_closed() -> None:
    from build_finance.crypto_replay.schema_definitions import (
        SUPPORTING_SCHEMA_DOCUMENTS,
        SUPPORTING_SCHEMA_IDS,
    )

    vector = build_t02_vector()
    expected_schema_ids = tuple(sorted(SUPPORTING_SCHEMA_IDS, key=lambda value: value.encode("utf-8")))
    actual_schema_ids = tuple(sorted(SUPPORTING_SCHEMA_DOCUMENTS, key=lambda value: value.encode("utf-8")))
    vector_schema_ids = tuple(sorted(vector.documents, key=lambda value: value.encode("utf-8")))
    assert actual_schema_ids == expected_schema_ids
    assert vector_schema_ids == expected_schema_ids
    resolver = vector.resolver
    for schema_id in SUPPORTING_SCHEMA_IDS:
        schema = SUPPORTING_SCHEMA_DOCUMENTS[schema_id]
        _schema_is_recursively_closed(schema)
        document = vector.documents[schema_id]
        assert not _issues(document, resolver=resolver)
        mutation = _reseal({**document, "unknown_t02_property": True})
        _assert_invalid(mutation, resolver=resolver)

    reconciliation = vector.documents["trading.reconciliation-receipt/v1"]
    wrong_special_owner = _reseal(
        {
            **reconciliation,
            "status": "KILLED",
            "reason_codes": ["RECONCILIATION_IDEMPOTENCY_CONFLICT", "RECONCILIATION_MISMATCH"],
            "kill_latched": True,
        }
    )
    invalid_genesis = _reseal(
        {
            **reconciliation,
            "reconciliation_kind": "GENESIS",
            "portfolio_state_after_id": reconciliation["portfolio_state_before_id"],
        }
    )
    invalid_terminal_promotion = _reseal(
        {
            **reconciliation,
            "reconciliation_kind": "TERMINAL_KILL_PROMOTION",
        }
    )
    duplicate_assets = deepcopy(reconciliation)
    duplicate_assets.update(
        {
            "status": "KILLED",
            "reason_codes": ["RECONCILIATION_ASSET_RESIDUAL", "RECONCILIATION_MISMATCH"],
            "kill_latched": True,
            "asset_residuals": [
                deepcopy(reconciliation["asset_residuals"][0]),
                {**deepcopy(reconciliation["asset_residuals"][0]), "residual_atoms": "1"},
                deepcopy(reconciliation["asset_residuals"][1]),
            ],
        }
    )
    duplicate_accounts = deepcopy(reconciliation)
    duplicate_accounts.update(
        {
            "status": "KILLED",
            "reason_codes": ["RECONCILIATION_ACCOUNT_RESIDUAL", "RECONCILIATION_MISMATCH"],
            "kill_latched": True,
            "account_residuals": [
                {"asset_mint": BASE_MINT, "account": "POSITION_AVAILABLE", "residual_atoms": "0"},
                {"asset_mint": BASE_MINT, "account": "POSITION_AVAILABLE", "residual_atoms": "1"},
            ],
        }
    )
    equal_integrity = deepcopy(reconciliation)
    footprint = _digest("equal-integrity-footprint")
    equal_integrity.update(
        {
            "status": "KILLED",
            "reconciliation_kind": "MODEL_ATTEMPT_INTEGRITY",
            "reason_codes": ["RECONCILIATION_MODEL_ATTEMPT_INTEGRITY", "RECONCILIATION_MISMATCH"],
            "portfolio_state_after_id": reconciliation["portfolio_state_before_id"],
            "causation_ids": sorted(_digest(f"integrity-cause-{index}") for index in range(4)),
            "integrity_validation_attempt_key_sha256": _digest("integrity-attempt-key"),
            "integrity_expected_footprint_sha256": footprint,
            "integrity_observed_footprint_sha256": footprint,
            "integrity_ledger_head_before_check_id": _digest("integrity-ledger-head"),
            "kill_latched": True,
        }
    )
    phantom_residual_code = _reseal(
        {
            **reconciliation,
            "status": "KILLED",
            "reason_codes": ["RECONCILIATION_ACCOUNT_RESIDUAL", "RECONCILIATION_MISMATCH"],
            "kill_latched": True,
        }
    )
    missing_asset_code = deepcopy(reconciliation)
    missing_asset_code.update(
        {
            "status": "KILLED",
            "reason_codes": ["RECONCILIATION_ACCOUNT_RESIDUAL", "RECONCILIATION_MISMATCH"],
            "asset_residuals": [
                {**deepcopy(reconciliation["asset_residuals"][0]), "residual_atoms": "1"},
                deepcopy(reconciliation["asset_residuals"][1]),
            ],
            "account_residuals": [{"asset_mint": BASE_MINT, "account": "POSITION_AVAILABLE", "residual_atoms": "1"}],
            "kill_latched": True,
        }
    )
    for mutation in (
        wrong_special_owner,
        invalid_genesis,
        invalid_terminal_promotion,
        _reseal(duplicate_assets),
        _reseal(duplicate_accounts),
        _reseal(equal_integrity),
        phantom_residual_code,
        _reseal(missing_asset_code),
    ):
        _assert_invalid(mutation, resolver=resolver)

    run = vector.documents["trading.run-receipt/v1"]
    duplicate_scopes = _reseal(
        {
            **run,
            "model_signal_mode": "CACHED_FIXTURES",
            "model_registry_sha256": _digest("duplicate-scope-registry"),
            "model_signal_manifest_sha256": _digest("duplicate-scope-manifest"),
            "model_decision_budget_ns": "2",
            "selected_model_scopes": [
                {
                    "market_id": MARKET_ID,
                    "decision_sequence": "1",
                    "horizon_ns": "1",
                    "producer_scope_key_sha256": _digest("duplicate-scope-a"),
                },
                {
                    "market_id": MARKET_ID,
                    "decision_sequence": "1",
                    "horizon_ns": "2",
                    "producer_scope_key_sha256": _digest("duplicate-scope-b"),
                },
            ],
        }
    )
    _assert_invalid(duplicate_scopes, resolver=resolver)

    benchmark = vector.documents["trading.benchmark-receipt/v1"]
    _assert_invalid(
        _reseal({**benchmark, "reason_codes": ["BENCHMARK_GATE_FAILED"]}),
        resolver=resolver,
    )
    process_row = {
        "case_id": "latency-core",
        "repetition_index": "0",
        "terminal_status": "PROCESS_FAILED",
        "run_receipt_id": run["run_receipt_id"],
        "ledger_root_id": None,
        "failure_receipt_id": None,
        "execution_quarantine_receipt_id": None,
        "process_failure_code": "LAUNCH_FAILED",
        "process_exit_code": "1",
        "stdout_sha256": _digest("launch-stdout"),
        "stderr_sha256": _digest("launch-stderr"),
        "output_sha256": _digest("launch-output"),
        "measurement_artifact_sha256": vector.documents["trading.benchmark-measurement/v1"]["benchmark_measurement_id"],
    }
    scheduled_benchmark = {
        **benchmark,
        "status": "FAIL",
        "reason_codes": ["BENCHMARK_RUN_FAILED"],
        "benchmark_manifest_sha256": _digest("scheduled-manifest"),
        "preregistered_thresholds_sha256": _digest("scheduled-thresholds"),
        "hardware_profile_sha256": _digest("scheduled-hardware"),
        "metrics_artifact_sha256": _digest("scheduled-metrics"),
        "run_outputs": [process_row],
    }
    launch_with_exit = _reseal(scheduled_benchmark)
    nonzero_without_exit = _reseal(
        {
            **scheduled_benchmark,
            "run_outputs": [{**process_row, "process_failure_code": "NONZERO_EXIT", "process_exit_code": None}],
        }
    )
    nonzero_with_zero_exit = _reseal(
        {
            **scheduled_benchmark,
            "run_outputs": [{**process_row, "process_failure_code": "NONZERO_EXIT", "process_exit_code": "0"}],
        }
    )
    for mutation in (launch_with_exit, nonzero_without_exit, nonzero_with_zero_exit):
        _assert_invalid(mutation, resolver=resolver)

    long_identity = "x" * 257
    long_run = _reseal(
        {
            **run,
            "os_name": long_identity,
            "tool_versions": [{"name": long_identity, "version": long_identity}],
        }
    )
    long_measurement = _reseal(
        {
            **vector.documents["trading.benchmark-measurement/v1"],
            "case_id": long_identity,
        }
    )
    assert not _issues(long_run, resolver=resolver)
    assert not _issues(long_measurement, resolver=resolver)


def test_supporting_self_ids_recompute() -> None:
    """T01 already owns the exact supporting ID inventory and hash omission rule."""
    from build_finance.crypto_replay.schema_definitions import SUPPORTING_SELF_ID_FIELDS

    vector = build_t02_vector()
    assert repr(vector.resolver) == "SyntheticResolver(objects=16, retained_byte_digests=20)"
    assert set(vector.documents) == set(SUPPORTING_SELF_ID_FIELDS)
    for schema_id, document in vector.documents.items():
        self_id_field = SUPPORTING_SELF_ID_FIELDS[schema_id]
        assert verify_content_id(document)
        expected = document[self_id_field]
        assert compute_content_id(document) == expected
        without_self_id = dict(document)
        without_self_id.pop(self_id_field)
        assert compute_content_id(without_self_id) == expected
        changed_self_id = dict(document)
        changed_self_id[self_id_field] = "0" * 64
        assert compute_content_id(changed_self_id) == expected
        changed_body = dict(document)
        changed_body["synthetic_digest_probe"] = True
        assert compute_content_id(changed_body) != expected


def test_fixture_manifest_is_set_closed() -> None:
    vector = build_t02_vector()
    fixture = vector.documents["trading.fixture-manifest/v1"]
    config = vector.documents["trading.replay-risk-config/v1"]
    assert not _issues(fixture, resolver=vector.resolver)
    assert not _issues(config, resolver=vector.resolver)
    _assert_independent_basic_oracles(fixture, resolver=vector.resolver)
    _assert_independent_basic_oracles(config, resolver=vector.resolver)
    mutations: list[dict[str, Any]] = []
    duplicate_market = deepcopy(fixture)
    duplicate_market["allowed_markets"].append(deepcopy(duplicate_market["allowed_markets"][0]))
    mutations.append(_reseal(duplicate_market))
    duplicate_market_key = deepcopy(fixture)
    duplicate_market_key["allowed_markets"].append(
        {
            **deepcopy(duplicate_market_key["allowed_markets"][0]),
            "base_mint": "zz-synthetic-base",
        }
    )
    mutations.append(_reseal(duplicate_market_key))
    duplicate_base = deepcopy(fixture)
    duplicate_base["allowed_markets"].append(
        {**deepcopy(duplicate_base["allowed_markets"][0]), "market_id": "zz-synthetic-market"}
    )
    mutations.append(_reseal(duplicate_base))
    same_mint = deepcopy(fixture)
    same_mint["allowed_markets"][0]["base_mint"] = QUOTE_MINT
    mutations.append(_reseal(same_mint))
    unsafe_path = deepcopy(fixture)
    unsafe_path["files"][0]["relative_path"] = "../escape.json"
    mutations.append(_reseal(unsafe_path))
    reversed_files = deepcopy(fixture)
    reversed_files["files"].reverse()
    mutations.append(_reseal(reversed_files))
    for retention_field in (
        "selection_failures_retained",
        "no_route_observations_retained",
        "inactive_assets_retained",
        "gaps_retained",
    ):
        retention_false = deepcopy(fixture)
        retention_false[retention_field] = False
        mutations.append(_reseal(retention_false))
    zero_session_start = deepcopy(fixture)
    zero_session_start["session_start_availability_slot"] = "0"
    mutations.append(_reseal(zero_session_start))
    reversed_session = deepcopy(fixture)
    reversed_session["session_start_availability_slot"] = "3"
    mutations.append(_reseal(reversed_session))
    zero_admission = deepcopy(fixture)
    zero_admission["files"][0]["admission_sequence"] = "0"
    mutations.append(_reseal(zero_admission))
    duplicate_admission = deepcopy(fixture)
    duplicate_admission["files"][1]["admission_sequence"] = "1"
    mutations.append(_reseal(duplicate_admission))
    quote_mint_mismatch = deepcopy(fixture)
    quote_mint_mismatch["allowed_markets"][0]["quote_mint"] = "other-quote"
    mutations.append(_reseal(quote_mint_mismatch))
    quote_decimals_mismatch = deepcopy(fixture)
    quote_decimals_mismatch["allowed_markets"][0]["quote_decimals"] = 5
    mutations.append(_reseal(quote_decimals_mismatch))
    empty_files = deepcopy(fixture)
    empty_files["files"] = []
    mutations.append(_reseal(empty_files))
    empty_markets = deepcopy(fixture)
    empty_markets["allowed_markets"] = []
    mutations.append(_reseal(empty_markets))
    unsorted_markets = deepcopy(fixture)
    unsorted_markets["allowed_markets"].append(
        {
            **deepcopy(unsorted_markets["allowed_markets"][0]),
            "market_id": "aaa-synthetic-base/synthetic-quote:jupiter",
            "base_mint": "aaa-synthetic-base",
        }
    )
    mutations.append(_reseal(unsorted_markets))
    duplicate_file = deepcopy(fixture)
    duplicate_file["files"].append(deepcopy(duplicate_file["files"][0]))
    mutations.append(_reseal(duplicate_file))
    duplicate_path_key = deepcopy(fixture)
    duplicate_path_key["files"].append(
        {
            **deepcopy(duplicate_path_key["files"][0]),
            "raw_payload_sha256": _digest("different-same-path-payload"),
            "admission_sequence": "4",
        }
    )
    mutations.append(_reseal(duplicate_path_key))
    unknown_file_market = deepcopy(fixture)
    unknown_file_market["files"][0]["market_id"] = "unlisted-market"
    mutations.append(_reseal(unknown_file_market))
    empty_registry = deepcopy(fixture)
    empty_registry["allowed_markets"][0]["market_id"] = ""
    mutations.append(_reseal(empty_registry))
    oversized_utf8_registry = deepcopy(fixture)
    oversized_utf8_registry["quote_mint"] = "é" * 65
    mutations.append(_reseal(oversized_utf8_registry))
    for unsafe in (
        "/absolute.json",
        "C:/drive.json",
        "quotes\\backslash.json",
        "quotes/../escape.json",
        "quotes/./dot.json",
        "quotes//double.json",
        "quotes/trailing/",
        "quotes/colon:name.json",
        "quotes/nul\x00name.json",
        "quotes/e\u0301.json",
    ):
        bad_path = deepcopy(fixture)
        bad_path["files"][0]["relative_path"] = unsafe
        mutations.append(_reseal(bad_path))
    for field in ("initial_quote_atoms", "replay_tick_ns"):
        zero_positive = deepcopy(fixture)
        zero_positive[field] = "0"
        mutations.append(_reseal(zero_positive))
    for field, value in (
        ("quote_decimals", -1),
        ("quote_decimals", 19),
        ("initial_quote_atoms", str(MAX_U64 + 1)),
        ("initial_quote_atoms", "01"),
        ("initial_quote_atoms", 1),
        ("session_end_availability_slot", str(MAX_U64 + 1)),
    ):
        bad_scalar = deepcopy(fixture)
        bad_scalar[field] = value
        mutations.append(_reseal(bad_scalar))
    for field, value in (
        ("network", "synthetic-network"),
        ("venue_profile", "synthetic-venue"),
        ("universe_policy", "SUCCESSFUL_ONLY"),
        ("point_in_time_mode", "LATEST_AVAILABLE"),
        ("run_end_position_policy", "SYNTHETIC_POLICY"),
    ):
        mutation = deepcopy(fixture)
        mutation[field] = value
        mutations.append(_reseal(mutation))
    for mutation in mutations:
        _assert_invalid(mutation, resolver=vector.resolver)

    two_market_fixture = deepcopy(fixture)
    two_market_fixture["allowed_markets"].append(
        {
            "market_id": "zz-synthetic-base/synthetic-quote:jupiter",
            "base_mint": "zz-synthetic-base",
            "quote_mint": QUOTE_MINT,
            "base_decimals": 0,
            "quote_decimals": 6,
        }
    )
    assert not _issues(_reseal(two_market_fixture), resolver=vector.resolver)
    second_quote_mismatch = deepcopy(two_market_fixture)
    second_quote_mismatch["allowed_markets"][1]["quote_decimals"] = 5
    _assert_invalid(_reseal(second_quote_mismatch), resolver=vector.resolver)
    second_quote_identity_mismatch = deepcopy(two_market_fixture)
    second_quote_identity_mismatch["allowed_markets"][1]["quote_mint"] = "other-quote"
    _assert_invalid(_reseal(second_quote_identity_mismatch), resolver=vector.resolver)
    for policy in ("FORCE_CLOSE_NEXT_EVENT", "LEAVE_MARKED_OPEN"):
        assert not _issues(_reseal({**fixture, "run_end_position_policy": policy}), resolver=vector.resolver)
    max_u64_fixture = deepcopy(fixture)
    max_u64_fixture.update({"initial_quote_atoms": str(MAX_U64), "replay_tick_ns": str(MAX_U64)})
    max_u64_fixture["files"][0].update(
        {"byte_length": str(MAX_U64), "admission_sequence": str(MAX_U64), "availability_slot": str(MAX_U64)}
    )
    assert not _issues(_reseal(max_u64_fixture), resolver=vector.resolver)

    boundary_config = deepcopy(config)
    boundary_config.update(
        {
            "target_entry_notional_quote_atoms": "1",
            "min_notional_quote_atoms": "1",
            "max_notional_quote_atoms": "1",
            "price_tick_q18": "1",
            "max_participation_bps": 10000,
            "max_impact_bps": 1_000_000,
            "max_fee_bps": 10000,
            "max_concentration_bps": 10000,
            "max_session_loss_quote_atoms": "0",
            "max_drawdown_bps": 10000,
            "stale_after_ns": "0",
            "stop_loss_bps": 9999,
            "take_profit_bps": 1_000_000,
            "adverse_fill_bps_max": 9999,
            "max_run_closure_proof_rows": "1",
            "session_start_replay_clock_ns": "0",
            "session_end_replay_clock_ns": "1",
            "kill_exit_mode": "FREEZE_NO_NEW_INTENTS",
        }
    )
    baseline_ids = (
        "ALWAYS_HOLD_V1",
        "MEAN_REVERSION_V1",
        "BREAKOUT_V1",
        "MOMENTUM_V1",
        "TREND_V1",
    )
    for baseline_id in baseline_ids:
        valid_boundary = _reseal({**boundary_config, "baseline_id": baseline_id})
        assert not _issues(valid_boundary, resolver=vector.resolver)
    max_range_config = deepcopy(boundary_config)
    max_range_config.update(
        {
            "target_entry_notional_quote_atoms": str(MAX_U64),
            "min_notional_quote_atoms": "1",
            "max_notional_quote_atoms": str(MAX_U64),
            "price_tick_q18": "170141183460469231731687303715884105727",
            "max_session_loss_quote_atoms": str(MAX_U64),
            "stale_after_ns": str(MAX_U64),
            "session_end_replay_clock_ns": str(MAX_U64),
        }
    )
    assert not _issues(_reseal(max_range_config), resolver=vector.resolver)

    config_mutations: list[dict[str, Any]] = []
    for config_field, config_value in (
        ("baseline_id", "SYNTHETIC_BASELINE"),
        ("min_notional_quote_atoms", "0"),
        ("target_entry_notional_quote_atoms", "0"),
        ("max_notional_quote_atoms", "0"),
        ("target_entry_notional_quote_atoms", "999"),
        ("target_entry_notional_quote_atoms", "500001"),
        ("price_tick_q18", "0"),
        ("price_tick_q18", "-1"),
        ("price_tick_q18", "170141183460469231731687303715884105728"),
        ("target_entry_notional_quote_atoms", str(MAX_U64 + 1)),
        ("target_entry_notional_quote_atoms", "01"),
        ("target_entry_notional_quote_atoms", 1),
        ("max_participation_bps", -1),
        ("max_participation_bps", 10001),
        ("max_impact_bps", -1),
        ("max_impact_bps", 1_000_001),
        ("max_fee_bps", -1),
        ("max_fee_bps", 10001),
        ("max_concentration_bps", -1),
        ("max_concentration_bps", 10001),
        ("max_drawdown_bps", -1),
        ("max_drawdown_bps", 10001),
        ("stop_loss_bps", 0),
        ("stop_loss_bps", 10000),
        ("take_profit_bps", 0),
        ("take_profit_bps", 1_000_001),
        ("adverse_fill_bps_max", -1),
        ("adverse_fill_bps_max", 10000),
        ("max_run_closure_proof_rows", "0"),
        ("max_run_closure_proof_rows", str(MAX_PROOF_ROWS + 1)),
        ("session_end_replay_clock_ns", "0"),
        ("session_start_replay_clock_ns", "1000000001"),
        ("fee_model_version", "synthetic-fee-model/v0"),
        ("liquidity_model_version", "synthetic-liquidity-model/v0"),
        ("kill_exit_mode", "SYNTHETIC_EXIT"),
    ):
        mutation = deepcopy(config)
        mutation[config_field] = config_value
        config_mutations.append(_reseal(mutation))
    inverted_notional = deepcopy(config)
    inverted_notional["min_notional_quote_atoms"] = "500001"
    inverted_notional["max_notional_quote_atoms"] = "500000"
    config_mutations.append(_reseal(inverted_notional))
    for mutation in config_mutations:
        _assert_invalid(mutation, resolver=vector.resolver)


def test_source_rejection_receipt_is_total() -> None:
    from inspect import signature

    import pytest

    from build_finance.crypto_replay.contract_semantics import (
        SOURCE_ADMISSION_QUARANTINE_REASONS,
        SOURCE_ADMISSION_REASON_PRECEDENCE,
        derive_source_admission_status,
        normalize_source_admission_reason_codes,
    )
    from tests.crypto_replay.support.builders import build_source_admission_receipt

    vector = build_t02_vector()
    admitted = vector.documents["trading.source-admission-receipt/v1"]
    source_inputs = _source_builder_inputs(admitted)
    _assert_independent_basic_oracles(admitted, resolver=vector.resolver)
    assert tuple(signature(derive_source_admission_status).parameters) == ("reason_codes",)
    assert tuple(signature(build_source_admission_receipt).parameters) == (
        "reason_codes",
        *_SOURCE_BUILDER_FIELDS,
    )
    assert build_source_admission_receipt(reason_codes=(), **source_inputs) == admitted
    for mask in range(1 << len(SOURCE_ADMISSION_REASON_PRECEDENCE)):
        codes = tuple(code for index, code in enumerate(SOURCE_ADMISSION_REASON_PRECEDENCE) if mask & (1 << index))
        expected_status = (
            "ADMITTED"
            if not codes
            else "QUARANTINED"
            if SOURCE_ADMISSION_QUARANTINE_REASONS.intersection(codes)
            else "REJECTED"
        )
        assert derive_source_admission_status(codes) == expected_status
    for code in SOURCE_ADMISSION_REASON_PRECEDENCE:
        singleton = build_source_admission_receipt(reason_codes=(code,), **source_inputs)
        assert singleton["status"] == derive_source_admission_status((code,))
        assert singleton["reason_codes"] == [code]
        assert not _issues(singleton, resolver=vector.resolver)
    with pytest.raises(ValueError, match="duplicate"):
        normalize_source_admission_reason_codes(("ADMISSION_NOT_LOCAL", "ADMISSION_NOT_LOCAL"))
    with pytest.raises(ValueError, match="unknown"):
        normalize_source_admission_reason_codes(("ADMISSION_UNKNOWN",))

    rejected = build_source_admission_receipt(
        reason_codes=("ADMISSION_RIGHTS_MISSING",),
        **{**source_inputs, "rights_review_date": None},
    )
    assert not _issues(rejected, resolver=vector.resolver)
    missing_effective = build_source_admission_receipt(
        reason_codes=("ADMISSION_RIGHTS_MISSING",),
        **{**source_inputs, "rights_effective_date": None},
    )
    assert not _issues(missing_effective, resolver=vector.resolver)
    for receipt, missing_field in (
        (rejected, "rights_review_date"),
        (missing_effective, "rights_effective_date"),
    ):
        for field in _SOURCE_BUILDER_FIELDS:
            if field != missing_field:
                assert receipt[field] == admitted[field]

    quarantined = build_source_admission_receipt(
        reason_codes=("ADMISSION_POSITION_CONFLICT", "ADMISSION_RIGHTS_MISSING"),
        **{**source_inputs, "rights_review_date": None},
    )
    assert quarantined["reason_codes"] == ["ADMISSION_RIGHTS_MISSING", "ADMISSION_POSITION_CONFLICT"]
    assert not _issues(quarantined, resolver=vector.resolver)
    assert quarantined["terms_sha256"] == admitted["terms_sha256"]

    for field in (
        "fixture_manifest_sha256",
        "raw_payload_sha256",
        "terms_sha256",
        "source_id",
        "source_kind",
        "source_revision",
        "market_id",
        "relative_path",
        "media_type",
        "byte_length",
        "availability_slot",
        "observed_at",
        "ingested_at",
        "rights_role",
        "rights_effective_date",
        "rights_review_date",
    ):
        admitted_null = _reseal({**admitted, field: None})
        _assert_invalid(admitted_null, resolver=vector.resolver)

    for media_type in ("application/json", "application/jsonl", "application/octet-stream"):
        media_receipt = build_source_admission_receipt(
            reason_codes=(),
            **{**source_inputs, "media_type": media_type},
        )
        assert not _issues(media_receipt, resolver=vector.resolver)

    partial_observed = build_source_admission_receipt(
        reason_codes=("ADMISSION_POINT_IN_TIME_MISSING",),
        **{**source_inputs, "ingested_at": None},
    )
    partial_ingested = build_source_admission_receipt(
        reason_codes=("ADMISSION_POINT_IN_TIME_MISSING",),
        **{**source_inputs, "observed_at": None},
    )
    assert not _issues(partial_observed, resolver=vector.resolver)
    assert not _issues(partial_ingested, resolver=vector.resolver)

    for rights_field in (
        "terms_sha256",
        "rights_role",
        "rights_effective_date",
        "rights_review_date",
    ):
        correctly_coded = build_source_admission_receipt(
            reason_codes=("ADMISSION_RIGHTS_MISSING",),
            **{**source_inputs, rights_field: None},
        )
        assert not _issues(correctly_coded, resolver=vector.resolver)
        wrong_code = build_source_admission_receipt(
            reason_codes=("ADMISSION_NOT_LOCAL",),
            **{**source_inputs, rights_field: None},
        )
        _assert_invalid(wrong_code, resolver=vector.resolver)
    for timestamp_field in ("observed_at", "ingested_at"):
        wrong_code = build_source_admission_receipt(
            reason_codes=("ADMISSION_RIGHTS_MISSING",),
            **{**source_inputs, timestamp_field: None},
        )
        _assert_invalid(wrong_code, resolver=vector.resolver)

    nullable_evidence_owners = {
        "fixture_manifest_sha256": ("ADMISSION_MANIFEST_MISMATCH",),
        "raw_payload_sha256": ("ADMISSION_NOT_LOCAL",),
        "source_id": ("ADMISSION_PROFILE_MISMATCH",),
        "source_kind": ("ADMISSION_PROFILE_MISMATCH",),
        "source_revision": ("ADMISSION_PROFILE_MISMATCH", "ADMISSION_POINT_IN_TIME_MISSING"),
        "market_id": ("ADMISSION_PROFILE_MISMATCH",),
        "relative_path": ("ADMISSION_NOT_LOCAL",),
        "media_type": ("ADMISSION_PROFILE_MISMATCH",),
        "byte_length": ("ADMISSION_NOT_LOCAL",),
        "availability_slot": ("ADMISSION_POINT_IN_TIME_MISSING",),
    }
    for field, owner_codes in nullable_evidence_owners.items():
        for quarantine in (False, True):
            quarantine_suffix = ("ADMISSION_SET_NOT_CLOSED",) if quarantine else ()
            for owner_code in owner_codes:
                correctly_coded = build_source_admission_receipt(
                    reason_codes=(owner_code, *quarantine_suffix),
                    **{**source_inputs, field: None},
                )
                assert correctly_coded["status"] == ("QUARANTINED" if quarantine else "REJECTED")
                assert not _issues(correctly_coded, resolver=vector.resolver)
            wrong_code = build_source_admission_receipt(
                reason_codes=("ADMISSION_RIGHTS_MISSING", *quarantine_suffix),
                **{**source_inputs, field: None},
            )
            _assert_invalid(wrong_code, resolver=vector.resolver)

    manifest_stage_blocked = build_source_admission_receipt(
        reason_codes=("ADMISSION_MANIFEST_MISMATCH",),
        **{
            **source_inputs,
            "fixture_manifest_sha256": None,
            "raw_payload_sha256": None,
            "relative_path": None,
            "byte_length": None,
            "source_id": None,
            "source_kind": None,
            "source_revision": None,
            "market_id": None,
            "media_type": None,
            "availability_slot": None,
        },
    )
    local_stage_blocked = build_source_admission_receipt(
        reason_codes=("ADMISSION_NOT_LOCAL",),
        **{
            **source_inputs,
            "raw_payload_sha256": None,
            "relative_path": None,
            "byte_length": None,
            "source_id": None,
            "source_kind": None,
            "source_revision": None,
            "market_id": None,
            "media_type": None,
            "availability_slot": None,
        },
    )
    profile_stage_blocked = build_source_admission_receipt(
        reason_codes=("ADMISSION_PROFILE_MISMATCH", "ADMISSION_POINT_IN_TIME_MISSING"),
        **{
            **source_inputs,
            "source_id": None,
            "source_kind": None,
            "source_revision": None,
            "market_id": None,
            "media_type": None,
            "availability_slot": None,
        },
    )
    for earlier_stage_receipt in (manifest_stage_blocked, local_stage_blocked, profile_stage_blocked):
        assert not _issues(earlier_stage_receipt, resolver=vector.resolver)

    profile_with_missing_time = build_source_admission_receipt(
        reason_codes=("ADMISSION_PROFILE_MISMATCH",),
        **{**source_inputs, "source_id": None, "observed_at": None},
    )
    _assert_invalid(profile_with_missing_time, resolver=vector.resolver)
    profile_with_owned_missing_time = build_source_admission_receipt(
        reason_codes=("ADMISSION_PROFILE_MISMATCH", "ADMISSION_POINT_IN_TIME_MISSING"),
        **{**source_inputs, "source_id": None, "observed_at": None},
    )
    assert not _issues(profile_with_owned_missing_time, resolver=vector.resolver)
    profile_with_missing_availability = build_source_admission_receipt(
        reason_codes=("ADMISSION_PROFILE_MISMATCH",),
        **{**source_inputs, "source_id": None, "availability_slot": None},
    )
    _assert_invalid(profile_with_missing_availability, resolver=vector.resolver)
    profile_with_owned_missing_availability = build_source_admission_receipt(
        reason_codes=("ADMISSION_PROFILE_MISMATCH", "ADMISSION_POINT_IN_TIME_MISSING"),
        **{**source_inputs, "source_id": None, "availability_slot": None},
    )
    assert not _issues(profile_with_owned_missing_availability, resolver=vector.resolver)

    wrong_status = _reseal({**rejected, "status": "ADMITTED"})
    rejected_conflict = _reseal({**quarantined, "status": "REJECTED"})
    admitted_with_code = _reseal({**admitted, "reason_codes": ["ADMISSION_RIGHTS_MISSING"]})
    wrong_precedence = _reseal(
        {
            **quarantined,
            "reason_codes": ["ADMISSION_POSITION_CONFLICT", "ADMISSION_RIGHTS_MISSING"],
        }
    )
    source_mutations = [
        wrong_status,
        rejected_conflict,
        admitted_with_code,
        wrong_precedence,
        _reseal({**rejected, "reason_codes": ["ADMISSION_RIGHTS_MISSING", "ADMISSION_RIGHTS_MISSING"]}),
        _reseal({**rejected, "reason_codes": ["ADMISSION_UNKNOWN"]}),
        _reseal({**rejected, "rights_effective_date": "2026-02-30"}),
        _reseal(
            {
                **rejected,
                "rights_effective_date": "2026-02-02",
                "rights_review_date": "2026-02-01",
            }
        ),
        _reseal({**rejected, "observed_at": "2026-02-30T00:00:00.000000000Z"}),
        _reseal(
            {
                **rejected,
                "observed_at": "2026-01-02T00:00:02.000000002Z",
                "ingested_at": "2026-01-02T00:00:02.000000001Z",
            }
        ),
        _reseal({**rejected, "admission_sequence": "0"}),
        _reseal({**rejected, "admission_sequence": str(MAX_U64 + 1)}),
        _reseal({**rejected, "byte_length": str(MAX_U64 + 1)}),
        _reseal({**rejected, "relative_path": "quotes/e\u0301.json"}),
        _reseal({**rejected, "parser_version": "unknown-parser/v1"}),
    ]
    for mutation in source_mutations:
        _assert_invalid(mutation, resolver=vector.resolver)


def test_missing_and_invalid_config_receipts_serialize() -> None:
    from inspect import signature

    import pytest

    from build_finance.crypto_replay.contract_semantics import normalize_config_admission_reason_codes
    from tests.crypto_replay.support.builders import (
        build_invalid_config_admission_receipt,
        build_missing_config_admission_receipt,
        build_valid_config_admission_receipt,
    )

    vector = build_t02_vector()
    valid = vector.documents["trading.config-admission-receipt/v1"]
    common = {
        "admission_sequence": valid["admission_sequence"],
        "validator_code_sha256": valid["validator_code_sha256"],
        "schema_bundle_sha256": valid["schema_bundle_sha256"],
    }
    missing = _seal(
        "trading.config-admission-receipt/v1",
        status="MISSING",
        reason_codes=["CONFIG_MISSING"],
        raw_config_sha256=None,
        validated_config_sha256=None,
        raw_byte_length="0",
        **common,
    )
    invalid_payloads = (b"", b"SYNTHETIC INVALID CONFIG BYTES\n")
    invalid_receipts = tuple(
        _seal(
            "trading.config-admission-receipt/v1",
            status="INVALID",
            reason_codes=["CONFIG_BYTES_INVALID"],
            raw_config_sha256=sha256_hex(payload),
            validated_config_sha256=None,
            raw_byte_length=str(len(payload)),
            **common,
        )
        for payload in invalid_payloads
    )
    assert tuple(signature(build_missing_config_admission_receipt).parameters) == tuple(common)
    assert "validated_config_sha256" not in signature(build_invalid_config_admission_receipt).parameters
    assert "validated_config_sha256" not in signature(build_valid_config_admission_receipt).parameters
    built_missing = build_missing_config_admission_receipt(**common)
    built_invalid_receipts = tuple(
        build_invalid_config_admission_receipt(
            payload,
            reason_codes=("CONFIG_BYTES_INVALID",),
            **common,
        )
        for payload in invalid_payloads
    )
    built_valid = build_valid_config_admission_receipt(
        vector.raw_config_bytes,
        vector.documents["trading.replay-risk-config/v1"],
        **common,
    )
    assert built_missing == missing
    assert built_invalid_receipts == invalid_receipts
    assert built_valid == valid
    assert vector.raw_config_bytes.endswith(b"\n")
    assert parse_canonical_record(vector.raw_config_bytes) == vector.documents["trading.replay-risk-config/v1"]
    assert valid["raw_config_sha256"] == sha256_hex(vector.raw_config_bytes)
    assert valid["raw_byte_length"] == str(len(vector.raw_config_bytes))
    resolver = SyntheticResolver(
        (*vector.documents.values(), *vector.source_receipts, *vector.raw_events),
        (
            *vector.attachment_payloads.values(),
            *invalid_payloads,
            vector.raw_config_bytes,
            vector.raw_config_bytes[:-1],
        ),
    )
    for receipt in (missing, *invalid_receipts, valid):
        record = canonical_record_bytes(receipt)
        assert parse_canonical_record(record) == receipt
        assert verify_content_id(receipt)
        assert not _issues(receipt, resolver=resolver)
    _assert_independent_basic_oracles(valid, resolver=resolver)
    empty_invalid, invalid = invalid_receipts
    assert empty_invalid["raw_byte_length"] == "0"
    assert empty_invalid["raw_config_sha256"] == sha256_hex(b"")
    assert missing["raw_config_sha256"] is None
    assert empty_invalid["raw_config_sha256"] is not None

    invalid_reason_codes = (
        "CONFIG_BYTES_INVALID",
        "CONFIG_SCHEMA_INVALID",
        "CONFIG_ID_MISMATCH",
        "CONFIG_RANGE_INVALID",
    )
    for reason_code in invalid_reason_codes:
        receipt = build_invalid_config_admission_receipt(
            b"SYNTHETIC CONFIG REJECTION INPUT\n",
            reason_codes=(reason_code,),
            **common,
        )
        assert receipt["status"] == "INVALID"
        assert receipt["reason_codes"] == [reason_code]
        assert not _issues(receipt, resolver=resolver)
    ordered_combination = build_invalid_config_admission_receipt(
        b"SYNTHETIC CONFIG REJECTION INPUT\n",
        reason_codes=("CONFIG_RANGE_INVALID", "CONFIG_BYTES_INVALID"),
        **common,
    )
    assert ordered_combination["reason_codes"] == ["CONFIG_BYTES_INVALID", "CONFIG_RANGE_INVALID"]
    assert not _issues(ordered_combination, resolver=resolver)
    with pytest.raises(ValueError, match="duplicate"):
        normalize_config_admission_reason_codes(("CONFIG_BYTES_INVALID", "CONFIG_BYTES_INVALID"))
    with pytest.raises(ValueError, match="unknown"):
        normalize_config_admission_reason_codes(("CONFIG_UNKNOWN",))
    with pytest.raises(ValueError, match="canonical LF"):
        build_valid_config_admission_receipt(
            vector.raw_config_bytes[:-1],
            vector.documents["trading.replay-risk-config/v1"],
            **common,
        )
    mutated_config_bytes = bytearray(vector.raw_config_bytes)
    mutated_config_bytes[1] = ord("X")
    with pytest.raises(ValueError, match="canonical LF"):
        build_valid_config_admission_receipt(
            bytes(mutated_config_bytes),
            vector.documents["trading.replay-risk-config/v1"],
            **common,
        )
    stale_config = deepcopy(vector.documents["trading.replay-risk-config/v1"])
    stale_config["config_sha256"] = "0" * 64
    with pytest.raises(ValueError, match="self-addressed"):
        build_valid_config_admission_receipt(canonical_record_bytes(stale_config), stale_config, **common)

    mutations = (
        _reseal({**missing, "raw_byte_length": "1"}),
        _reseal({**missing, "raw_config_sha256": _digest("missing-raw")}),
        _reseal({**missing, "validated_config_sha256": valid["validated_config_sha256"]}),
        _reseal({**missing, "reason_codes": ["CONFIG_BYTES_INVALID"]}),
        _reseal(
            {
                **invalid,
                "validated_config_sha256": vector.documents["trading.replay-risk-config/v1"]["config_sha256"],
            }
        ),
        _reseal({**invalid, "reason_codes": []}),
        _reseal({**invalid, "reason_codes": ["CONFIG_MISSING"]}),
        _reseal({**valid, "reason_codes": ["CONFIG_RANGE_INVALID"]}),
        _reseal({**valid, "raw_config_sha256": None}),
        _reseal({**valid, "validated_config_sha256": None}),
        _reseal({**valid, "raw_byte_length": "0"}),
        _reseal(
            {
                **invalid,
                "reason_codes": ["CONFIG_RANGE_INVALID", "CONFIG_BYTES_INVALID"],
            }
        ),
        _reseal({**invalid, "reason_codes": ["CONFIG_BYTES_INVALID", "CONFIG_BYTES_INVALID"]}),
        _reseal({**invalid, "reason_codes": ["CONFIG_UNKNOWN"]}),
        _reseal({**invalid, "admission_sequence": "0"}),
        _reseal({**invalid, "admission_sequence": str(MAX_U64 + 1)}),
        _reseal({**invalid, "raw_byte_length": str(MAX_U64 + 1)}),
    )
    for mutation in mutations:
        _assert_invalid(mutation, resolver=resolver)


def test_run_closure_receipt_is_total_by_config_status() -> None:
    vector = build_t02_vector()
    closure = vector.documents["trading.run-closure-receipt/v1"]
    _assert_independent_basic_oracles(closure, resolver=vector.resolver)
    assert not _issues(closure, resolver=vector.resolver)
    zero_authority_rows: list[tuple[dict[str, Any], SyntheticResolver, dict[str, Any]]] = []
    status_inputs = (
        ("MISSING", "CONFIG_MISSING", None),
        ("INVALID", "CONFIG_BYTES_INVALID", b""),
    )
    for status, code, raw_bytes in status_inputs:
        config_receipt = _reseal(
            {
                **vector.documents["trading.config-admission-receipt/v1"],
                "status": status,
                "reason_codes": [code],
                "raw_config_sha256": None if raw_bytes is None else sha256_hex(raw_bytes),
                "validated_config_sha256": None,
                "raw_byte_length": "0" if raw_bytes is None else str(len(raw_bytes)),
            }
        )
        row = _reseal(
            {
                **closure,
                "status": "NOT_REQUIRED_ZERO_AUTHORITY",
                "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
                "validated_config_sha256": None,
                "terminal_equal_time_group": "2",
                "proof_row_limit": None,
                "proof_row_count_total": "0",
                "proof_budget_status": "NOT_REQUIRED",
                "market_proofs": [],
            }
        )
        resolver = SyntheticResolver(
            (
                *vector.documents.values(),
                config_receipt,
                row,
                *vector.source_receipts,
                *vector.raw_events,
            ),
            (
                *vector.attachment_payloads.values(),
                *((raw_bytes,) if raw_bytes is not None else ()),
            ),
        )
        assert not _issues(config_receipt, resolver=resolver)
        assert not _issues(row, resolver=resolver)
        zero_authority_rows.append((row, resolver, config_receipt))
    assert len({row["run_closure_receipt_id"] for row, _, _ in zero_authority_rows}) == 2

    exceeded_capacity = {
        **vector.attachments["trading.counter-capacity/v1"],
        "decision_sequence_next_upper_bound": None,
        "status": "EXCEEDED",
        "first_exceeded_counter": "DECISION_SEQUENCE",
    }
    exceeded_payload, exceeded_digest = _attachment(exceeded_capacity)
    capacity_failures: list[dict[str, Any]] = []
    for (zero_row, _, _), (status, _, _) in zip(zero_authority_rows, status_inputs, strict=True):
        failed = _reseal(
            {
                **zero_row,
                "status": "FAIL",
                "reason_codes": ["ADMISSION_COUNTER_CAPACITY", "ADMISSION_RUN_END_UNCLOSED"],
                "counter_capacity_sha256": exceeded_digest,
                "counter_capacity_status": "EXCEEDED",
                "terminal_equal_time_group": None,
            }
        )
        assert failed["validated_config_sha256"] is None
        assert failed["proof_row_limit"] is None
        assert failed["proof_row_count_total"] == "0"
        assert failed["proof_budget_status"] == "NOT_REQUIRED"
        assert failed["market_proofs"] == []
        capacity_failures.append(failed)
        assert status in ("MISSING", "INVALID")

    valid_capacity_failure = _reseal(
        {
            **closure,
            "status": "FAIL",
            "reason_codes": ["ADMISSION_COUNTER_CAPACITY", "ADMISSION_RUN_END_UNCLOSED"],
            "counter_capacity_sha256": exceeded_digest,
            "counter_capacity_status": "EXCEEDED",
            "terminal_equal_time_group": None,
        }
    )
    assert valid_capacity_failure["proof_row_limit"] == str(MAX_PROOF_ROWS)
    assert valid_capacity_failure["proof_row_count_total"] == "0"
    assert valid_capacity_failure["proof_budget_status"] == "NOT_REQUIRED"
    capacity_resolver = SyntheticResolver(
        (
            *vector.documents.values(),
            *(config_receipt for _, _, config_receipt in zero_authority_rows),
            *capacity_failures,
            valid_capacity_failure,
            *vector.source_receipts,
            *vector.raw_events,
        ),
        (*vector.attachment_payloads.values(), exceeded_payload, b""),
    )
    for failed in (*capacity_failures, valid_capacity_failure):
        assert not _issues(failed, resolver=capacity_resolver)

    missing_row, missing_resolver, _ = zero_authority_rows[0]
    invalid_fail = _reseal({**closure, "status": "FAIL", "reason_codes": [], "terminal_equal_time_group": None})
    invalid_proof = _reseal({**closure, "proof_budget_status": "WITHIN_LIMIT"})
    valid_zero_authority = _reseal({**closure, "status": "NOT_REQUIRED_ZERO_AUTHORITY"})
    zero_authority_pass = _reseal({**missing_row, "status": "PASS"})
    zero_authority_limit = _reseal({**missing_row, "proof_row_limit": "1"})
    counter_reason_mismatch = _reseal(
        {
            **valid_capacity_failure,
            "reason_codes": ["ADMISSION_RUN_END_UNCLOSED"],
        }
    )
    for mutation, resolver in (
        (invalid_fail, vector.resolver),
        (invalid_proof, vector.resolver),
        (valid_zero_authority, vector.resolver),
        (zero_authority_pass, missing_resolver),
        (zero_authority_limit, missing_resolver),
        (counter_reason_mismatch, capacity_resolver),
    ):
        _assert_invalid(mutation, resolver=resolver)

    market_row = {
        "market_id": MARKET_ID,
        "failure_codes": [],
        "earliest_trigger_equal_time_group": "3",
        "fill_event_id": _digest("force-fill-event"),
        "q_cap_base_atoms": "1",
        "capacity_base_atoms": "1",
        "fill_candidate_semantic_sha256": _digest("force-candidate-semantic"),
        "reference_price_count": "1",
        "adverse_fill_extreme_count": "2",
        "proof_row_count": "2",
        "state_envelope_sha256": _digest("force-state-envelope"),
        "proof_domain": "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        "reference_set_root_sha256": _digest("force-reference-root"),
        "proof_root_sha256": _digest("force-proof-root"),
    }

    def force_receipt(
        *,
        status: str,
        reasons: list[str],
        proof_limit: str,
        proof_total: str | None,
        budget_status: str,
        row: dict[str, Any],
    ) -> dict[str, Any]:
        return _reseal(
            {
                **closure,
                "status": status,
                "reason_codes": reasons,
                "run_end_position_policy": "FORCE_CLOSE_NEXT_EVENT",
                "terminal_equal_time_group": "3" if status == "PASS" else None,
                "proof_row_limit": proof_limit,
                "proof_row_count_total": proof_total,
                "proof_budget_status": budget_status,
                "market_proofs": [row],
            }
        )

    passing_force = force_receipt(
        status="PASS",
        reasons=[],
        proof_limit="2",
        proof_total="2",
        budget_status="WITHIN_LIMIT",
        row=market_row,
    )
    budget_row = {**market_row, "proof_domain": None, "proof_root_sha256": None}
    budget_failure = force_receipt(
        status="FAIL",
        reasons=["ADMISSION_RUN_END_PROOF_BUDGET", "ADMISSION_RUN_END_UNCLOSED"],
        proof_limit="1",
        proof_total="2",
        budget_status="EXCEEDED",
        row=budget_row,
    )
    preproof_row = {
        **market_row,
        "failure_codes": ["CLOSURE_CAPACITY_INSUFFICIENT"],
        "q_cap_base_atoms": "2",
        "capacity_base_atoms": "1",
        "proof_row_count": "4",
        "proof_domain": None,
        "proof_root_sha256": None,
    }
    preproof_failure = force_receipt(
        status="FAIL",
        reasons=["ADMISSION_RUN_END_UNCLOSED"],
        proof_limit="4",
        proof_total="4",
        budget_status="WITHIN_LIMIT",
        row=preproof_row,
    )
    enumeration_row = {
        **market_row,
        "failure_codes": ["CLOSURE_PROOF_PREDICATE_FAILED"],
        "proof_root_sha256": None,
    }
    enumeration_failure = force_receipt(
        status="FAIL",
        reasons=["ADMISSION_RUN_END_UNCLOSED"],
        proof_limit="2",
        proof_total="2",
        budget_status="WITHIN_LIMIT",
        row=enumeration_row,
    )
    force_receipts = (passing_force, budget_failure, preproof_failure, enumeration_failure)
    force_resolver = SyntheticResolver(
        (*vector.documents.values(), *force_receipts, *vector.source_receipts, *vector.raw_events),
        tuple(vector.attachment_payloads.values()),
    )
    for receipt in force_receipts:
        assert not _issues(receipt, resolver=force_resolver)

    force_counter_failure = _reseal(
        {
            **passing_force,
            "status": "FAIL",
            "reason_codes": ["ADMISSION_COUNTER_CAPACITY", "ADMISSION_RUN_END_UNCLOSED"],
            "counter_capacity_sha256": exceeded_digest,
            "counter_capacity_status": "EXCEEDED",
            "terminal_equal_time_group": None,
        }
    )
    assert not _issues(force_counter_failure, resolver=force_resolver)

    second_market_row = {
        **market_row,
        "market_id": "z-synthetic-market",
        "fill_event_id": _digest("force-fill-event-z"),
        "fill_candidate_semantic_sha256": _digest("force-candidate-semantic-z"),
        "state_envelope_sha256": _digest("force-state-envelope-z"),
        "reference_set_root_sha256": _digest("force-reference-root-z"),
        "proof_root_sha256": _digest("force-proof-root-z"),
    }
    two_market_force = _reseal(
        {
            **passing_force,
            "proof_row_count_total": "4",
            "proof_row_limit": "4",
            "market_proofs": [market_row, second_market_row],
        }
    )
    assert not _issues(two_market_force, resolver=force_resolver)
    max_terminal = _reseal({**closure, "terminal_equal_time_group": str(MAX_U64)})
    assert not _issues(max_terminal, resolver=force_resolver)

    closure_mutations = (
        _reseal({**closure, "reason_codes": ["ADMISSION_RUN_END_UNCLOSED", "ADMISSION_COUNTER_CAPACITY"]}),
        _reseal({**closure, "reason_codes": ["ADMISSION_RUN_END_UNCLOSED", "ADMISSION_RUN_END_UNCLOSED"]}),
        _reseal({**closure, "reason_codes": ["ADMISSION_UNKNOWN"]}),
        _reseal({**closure, "source_admission_receipt_ids": list(reversed(closure["source_admission_receipt_ids"]))}),
        _reseal(
            {
                **closure,
                "source_admission_receipt_ids": [
                    closure["source_admission_receipt_ids"][0],
                    closure["source_admission_receipt_ids"][0],
                ],
            }
        ),
        force_receipt(
            status="PASS",
            reasons=[],
            proof_limit=str(MAX_PROOF_ROWS + 1),
            proof_total="2",
            budget_status="WITHIN_LIMIT",
            row=market_row,
        ),
        force_receipt(
            status="FAIL",
            reasons=["ADMISSION_RUN_END_UNCLOSED"],
            proof_limit="2",
            proof_total="2",
            budget_status="WITHIN_LIMIT",
            row={**preproof_row, "proof_domain": "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1"},
        ),
        force_receipt(
            status="FAIL",
            reasons=["ADMISSION_RUN_END_UNCLOSED"],
            proof_limit="2",
            proof_total="2",
            budget_status="WITHIN_LIMIT",
            row={**enumeration_row, "proof_root_sha256": _digest("forbidden-enumeration-root")},
        ),
        _reseal({**closure, "model_registry_sha256": _digest("forbidden-disabled-registry")}),
        _reseal(
            {
                **closure,
                "model_signal_mode": "CACHED_FIXTURES",
                "model_registry_sha256": _digest("cached-registry"),
                "model_signal_manifest_sha256": None,
            }
        ),
        _reseal({**closure, "terminal_equal_time_group": "02"}),
        _reseal({**closure, "terminal_equal_time_group": str(MAX_U64 + 1)}),
        _reseal({**two_market_force, "market_proofs": list(reversed(two_market_force["market_proofs"]))}),
        _reseal(
            {
                **two_market_force,
                "market_proofs": [market_row, {**second_market_row, "market_id": MARKET_ID}],
            }
        ),
        _reseal(
            {
                **passing_force,
                "market_proofs": [{**market_row, "market_id": "é" * 65}],
            }
        ),
        _reseal({**capacity_failures[0], "proof_row_limit": "1"}),
        _reseal(
            {
                **valid_capacity_failure,
                "proof_row_count_total": "2",
                "proof_budget_status": "WITHIN_LIMIT",
                "market_proofs": [market_row],
            }
        ),
        _reseal(
            {
                **force_counter_failure,
                "proof_row_count_total": "0",
                "proof_budget_status": "NOT_REQUIRED",
                "market_proofs": [],
            }
        ),
        force_receipt(
            status="FAIL",
            reasons=["ADMISSION_RUN_END_UNCLOSED"],
            proof_limit="4",
            proof_total="4",
            budget_status="WITHIN_LIMIT",
            row={
                **preproof_row,
                "failure_codes": [
                    "CLOSURE_CAPACITY_INSUFFICIENT",
                    "CLOSURE_PROOF_PREDICATE_FAILED",
                ],
            },
        ),
        force_receipt(
            status="FAIL",
            reasons=["ADMISSION_RUN_END_UNCLOSED"],
            proof_limit="2",
            proof_total="2",
            budget_status="WITHIN_LIMIT",
            row={
                **market_row,
                "failure_codes": [
                    "CLOSURE_CANDIDATE_CARDINALITY",
                    "CLOSURE_CANDIDATE_SCHEMA_INVALID",
                ],
                "fill_event_id": None,
                "capacity_base_atoms": None,
                "fill_candidate_semantic_sha256": None,
                "proof_domain": None,
                "proof_root_sha256": None,
            },
        ),
        _reseal({**passing_force, "proof_row_count_total": "1"}),
    )
    for mutation in closure_mutations:
        _assert_invalid(mutation, resolver=force_resolver)

    def phase_row(*codes: str, market_id: str = MARKET_ID) -> dict[str, Any]:
        row = {**market_row, "market_id": market_id, "failure_codes": list(codes)}
        code_set = set(codes)
        if "CLOSURE_H_MISSING" in code_set:
            row["earliest_trigger_equal_time_group"] = None
            row["state_envelope_sha256"] = None
        if code_set.intersection(
            {
                "CLOSURE_H_MISSING",
                "CLOSURE_CANDIDATE_CARDINALITY",
                "CLOSURE_CANDIDATE_SCHEMA_INVALID",
            }
        ):
            row["fill_event_id"] = None
            row["capacity_base_atoms"] = None
            row["fill_candidate_semantic_sha256"] = None
        if "CLOSURE_Q_CAP_RANGE" in code_set:
            row["q_cap_base_atoms"] = None
            row["proof_row_count"] = None
        if "CLOSURE_REFERENCE_SET_INVALID" in code_set:
            row["reference_price_count"] = None
            row["reference_set_root_sha256"] = None
            row["proof_row_count"] = None
        if "CLOSURE_PROOF_ROW_COUNT_RANGE" in code_set:
            row["proof_row_count"] = None
        if "CLOSURE_STATE_ENVELOPE_RANGE" in code_set:
            row["state_envelope_sha256"] = None
        row["proof_domain"] = None
        row["proof_root_sha256"] = None
        return row

    candidate_later = (
        "CLOSURE_CANDIDATE_NON_EXECUTABLE",
        "CLOSURE_IDENTITY_MISMATCH",
        "CLOSURE_DECIMALS_MISMATCH",
        "CLOSURE_CAPACITY_INSUFFICIENT",
    )
    run_closure_phase_precedence = (
        "CLOSURE_H_MISSING",
        "CLOSURE_CANDIDATE_CARDINALITY",
        "CLOSURE_CANDIDATE_SCHEMA_INVALID",
        "CLOSURE_CANDIDATE_NON_EXECUTABLE",
        "CLOSURE_IDENTITY_MISMATCH",
        "CLOSURE_DECIMALS_MISMATCH",
        "CLOSURE_Q_CAP_RANGE",
        "CLOSURE_CAPACITY_INSUFFICIENT",
        "CLOSURE_REFERENCE_SET_INVALID",
        "CLOSURE_PROOF_ROW_COUNT_RANGE",
        "CLOSURE_STATE_ENVELOPE_RANGE",
        "CLOSURE_PROOF_PREDICATE_FAILED",
    )
    phase_order = {code: index for index, code in enumerate(run_closure_phase_precedence)}
    invalid_phase_pairs = {
        *(
            tuple(sorted(("CLOSURE_H_MISSING", code), key=lambda value: phase_order[value]))
            for code in (
                "CLOSURE_CANDIDATE_CARDINALITY",
                "CLOSURE_CANDIDATE_SCHEMA_INVALID",
                *candidate_later,
                "CLOSURE_STATE_ENVELOPE_RANGE",
                "CLOSURE_PROOF_PREDICATE_FAILED",
            )
        ),
        *(
            tuple(sorted(("CLOSURE_CANDIDATE_CARDINALITY", code), key=lambda value: phase_order[value]))
            for code in (
                "CLOSURE_CANDIDATE_SCHEMA_INVALID",
                *candidate_later,
                "CLOSURE_PROOF_PREDICATE_FAILED",
            )
        ),
        *(
            tuple(sorted(("CLOSURE_CANDIDATE_SCHEMA_INVALID", code), key=lambda value: phase_order[value]))
            for code in (
                *candidate_later,
                "CLOSURE_PROOF_PREDICATE_FAILED",
            )
        ),
        tuple(
            sorted(
                ("CLOSURE_Q_CAP_RANGE", "CLOSURE_CAPACITY_INSUFFICIENT"),
                key=lambda value: phase_order[value],
            )
        ),
        tuple(
            sorted(
                ("CLOSURE_Q_CAP_RANGE", "CLOSURE_PROOF_ROW_COUNT_RANGE"),
                key=lambda value: phase_order[value],
            )
        ),
        tuple(
            sorted(
                ("CLOSURE_REFERENCE_SET_INVALID", "CLOSURE_PROOF_ROW_COUNT_RANGE"),
                key=lambda value: phase_order[value],
            )
        ),
        *(
            tuple((code, "CLOSURE_PROOF_PREDICATE_FAILED"))
            for code in run_closure_phase_precedence
            if code != "CLOSURE_PROOF_PREDICATE_FAILED"
        ),
    }
    for codes in invalid_phase_pairs:
        row = phase_row(*codes)
        count_range = "CLOSURE_PROOF_ROW_COUNT_RANGE" in codes
        total = None if count_range else ("0" if row["proof_row_count"] is None else row["proof_row_count"])
        budget = "EXCEEDED" if count_range else "WITHIN_LIMIT"
        reasons = (
            ["ADMISSION_RUN_END_PROOF_BUDGET", "ADMISSION_RUN_END_UNCLOSED"]
            if count_range
            else ["ADMISSION_RUN_END_UNCLOSED"]
        )
        _assert_invalid(
            force_receipt(
                status="FAIL",
                reasons=reasons,
                proof_limit="2",
                proof_total=total,
                budget_status=budget,
                row=row,
            ),
            resolver=force_resolver,
        )

    cross_row_preproof = phase_row("CLOSURE_CAPACITY_INSUFFICIENT", market_id="a-market")
    cross_row_predicate = phase_row("CLOSURE_PROOF_PREDICATE_FAILED", market_id="b-market")
    cross_row_closure = _reseal(
        {
            **passing_force,
            "status": "FAIL",
            "reason_codes": ["ADMISSION_RUN_END_UNCLOSED"],
            "terminal_equal_time_group": None,
            "proof_row_limit": "4",
            "proof_row_count_total": "4",
            "market_proofs": [cross_row_preproof, cross_row_predicate],
        }
    )
    _assert_invalid(cross_row_closure, resolver=force_resolver)


def test_execution_quarantine_receipt_is_exhaustive() -> None:
    vector = build_t02_vector()
    base = vector.documents["trading.execution-quarantine-receipt/v1"]
    _assert_independent_basic_oracles(base, resolver=vector.resolver)
    assert not _issues(base, resolver=vector.resolver)
    wrong = _reseal(
        {
            **base,
            "reason_codes": [
                "QUARANTINE_INITIAL_PREFIX_ANCHOR_MISSING",
                "QUARANTINE_LEDGER_SLOT_WRONG",
                "QUARANTINE_APPEND_SLOT_OCCUPIED",
                "QUARANTINE_FOOTPRINT_MISMATCH",
            ],
            "slot_observations": [
                {
                    "ledger_sequence": "0",
                    "expected_ledger_record_id": _digest("expected-ledger-record"),
                    "observed_ledger_record_id": _digest("wrong-ledger-record"),
                    "observed_byte_sha256": _digest("wrong-ledger-bytes"),
                    "classification": "WRONG",
                }
            ],
        }
    )
    extra = _reseal(
        {
            **base,
            "reason_codes": [
                "QUARANTINE_INITIAL_PREFIX_ANCHOR_MISSING",
                "QUARANTINE_LEDGER_SLOT_EXTRA",
                "QUARANTINE_APPEND_SLOT_OCCUPIED",
                "QUARANTINE_FOOTPRINT_MISMATCH",
            ],
            "slot_observations": [
                {
                    "ledger_sequence": "0",
                    "expected_ledger_record_id": None,
                    "observed_ledger_record_id": _digest("extra-ledger-record"),
                    "observed_byte_sha256": _digest("extra-ledger-bytes"),
                    "classification": "EXTRA",
                }
            ],
        }
    )
    for receipt in (wrong, extra):
        assert not _issues(receipt, resolver=vector.resolver)

    component_kinds = (
        "JOURNAL_KEY",
        "CANONICAL_OBJECT",
        "LEDGER_SEQUENCE_START",
        "LEDGER_SEQUENCE_END",
        "CONSUMED_PRODUCER_SEQUENCE",
        "CLAIMED_REQUEST_KEY",
        "PRIOR_LEDGER_HEAD",
        "PRIOR_PORTFOLIO_STATE",
        "RESULTING_LEDGER_HEAD",
        "RESULTING_PORTFOLIO_STATE",
        "NEXT_LEDGER_SEQUENCE",
        "NEXT_STATE_SEQUENCE",
        "NEXT_DECISION_SEQUENCE",
        "NEXT_INTENT_SEQUENCE",
        "NEXT_FILL_RECEIPT_SEQUENCE",
    )
    component_receipts: list[dict[str, Any]] = []
    for index, component_kind in enumerate(component_kinds):
        for classification in ("MISSING", "WRONG", "EXTRA"):
            expected = None if classification == "EXTRA" else _digest(f"component-expected-{index}-{classification}")
            observed = None if classification == "MISSING" else _digest(f"component-observed-{index}-{classification}")
            classification_code = {
                "MISSING": "QUARANTINE_COMPONENT_MISSING",
                "WRONG": "QUARANTINE_COMPONENT_WRONG",
                "EXTRA": "QUARANTINE_COMPONENT_EXTRA",
            }[classification]
            component_receipts.append(
                _reseal(
                    {
                        **base,
                        "reason_codes": [classification_code, "QUARANTINE_FOOTPRINT_MISMATCH"],
                        "last_verified_ledger_head_id": _digest("verified-head"),
                        "last_verified_portfolio_state_id": _digest("verified-state"),
                        "first_unsafe_ledger_sequence": None,
                        "slot_observations": [],
                        "component_observations": [
                            {
                                "component_kind": component_kind,
                                "ordinal": "0" if component_kind == "CANONICAL_OBJECT" else None,
                                "expected_component_sha256": expected,
                                "observed_component_sha256": observed,
                                "classification": classification,
                            }
                        ],
                    }
                )
            )
    component_resolver = SyntheticResolver(
        (*vector.documents.values(), *component_receipts, *vector.source_receipts, *vector.raw_events),
        tuple(vector.attachment_payloads.values()),
    )
    for receipt in component_receipts:
        assert not _issues(receipt, resolver=component_resolver)

    mixed = _reseal(
        {
            **wrong,
            "last_verified_ledger_head_id": _digest("verified-head"),
            "last_verified_portfolio_state_id": _digest("verified-state"),
            "reason_codes": [
                "QUARANTINE_LEDGER_SLOT_WRONG",
                "QUARANTINE_APPEND_SLOT_OCCUPIED",
                "QUARANTINE_COMPONENT_EXTRA",
                "QUARANTINE_FOOTPRINT_MISMATCH",
            ],
            "component_observations": [
                {
                    "component_kind": "CANONICAL_OBJECT",
                    "ordinal": "0",
                    "expected_component_sha256": None,
                    "observed_component_sha256": _digest("extra-object-component"),
                    "classification": "EXTRA",
                }
            ],
        }
    )
    assert not _issues(mixed, resolver=component_resolver)
    malformed_wrong = _reseal(
        {
            **wrong,
            "slot_observations": [
                {
                    **wrong["slot_observations"][0],
                    "observed_ledger_record_id": None,
                }
            ],
        }
    )
    assert not _issues(malformed_wrong, resolver=component_resolver)
    numeric_slots = _reseal(
        {
            **base,
            "reason_codes": [
                "QUARANTINE_LEDGER_SLOT_MISSING",
                "QUARANTINE_LEDGER_SLOT_WRONG",
                "QUARANTINE_LEDGER_SLOT_EXTRA",
                "QUARANTINE_APPEND_SLOT_OCCUPIED",
                "QUARANTINE_FOOTPRINT_MISMATCH",
            ],
            "last_verified_ledger_head_id": _digest("verified-head"),
            "last_verified_portfolio_state_id": _digest("verified-state"),
            "first_unsafe_ledger_sequence": "2",
            "slot_observations": [
                (
                    {
                        **base["slot_observations"][0],
                        "ledger_sequence": str(sequence),
                    }
                    if sequence == 2
                    else {
                        "ledger_sequence": str(sequence),
                        "expected_ledger_record_id": (
                            None if sequence == 10 else _digest(f"numeric-expected-{sequence}")
                        ),
                        "observed_ledger_record_id": _digest(f"numeric-observed-{sequence}"),
                        "observed_byte_sha256": _digest(f"numeric-bytes-{sequence}"),
                        "classification": "EXTRA" if sequence == 10 else "WRONG",
                    }
                )
                for sequence in range(2, 11)
            ],
        }
    )
    assert not _issues(numeric_slots, resolver=component_resolver)
    canonical_objects = _reseal(
        {
            **base,
            "reason_codes": [
                "QUARANTINE_COMPONENT_MISSING",
                "QUARANTINE_COMPONENT_EXTRA",
                "QUARANTINE_FOOTPRINT_MISMATCH",
            ],
            "last_verified_ledger_head_id": _digest("verified-head"),
            "last_verified_portfolio_state_id": _digest("verified-state"),
            "first_unsafe_ledger_sequence": None,
            "slot_observations": [],
            "component_observations": [
                {
                    "component_kind": "CANONICAL_OBJECT",
                    "ordinal": "0",
                    "expected_component_sha256": _digest("canonical-object-0"),
                    "observed_component_sha256": None,
                    "classification": "MISSING",
                },
                {
                    "component_kind": "CANONICAL_OBJECT",
                    "ordinal": "2",
                    "expected_component_sha256": None,
                    "observed_component_sha256": _digest("canonical-object-2"),
                    "classification": "EXTRA",
                },
            ],
        }
    )
    assert not _issues(canonical_objects, resolver=component_resolver)
    malformed = _reseal({**wrong, "reason_codes": ["QUARANTINE_FOOTPRINT_MISMATCH"]})
    mixed_anchor = _reseal({**base, "last_verified_ledger_head_id": _digest("head")})
    empty_observations = _reseal({**base, "slot_observations": [], "component_observations": []})
    wrong_first_slot = _reseal({**wrong, "first_unsafe_ledger_sequence": "1"})
    wrong_component_ordinal = deepcopy(component_receipts[0])
    wrong_component_ordinal["component_observations"][0]["ordinal"] = "0"
    wrong_component_ordinal = _reseal(wrong_component_ordinal)
    missing_canonical_ordinal = next(
        deepcopy(receipt)
        for receipt in component_receipts
        if receipt["component_observations"][0]["component_kind"] == "CANONICAL_OBJECT"
    )
    missing_canonical_ordinal["component_observations"][0]["ordinal"] = None
    missing_canonical_ordinal = _reseal(missing_canonical_ordinal)
    duplicate_slot = deepcopy(wrong)
    duplicate_slot["slot_observations"].append(deepcopy(duplicate_slot["slot_observations"][0]))
    duplicate_slot = _reseal(duplicate_slot)
    reversed_numeric_slots = deepcopy(numeric_slots)
    reversed_numeric_slots["slot_observations"].reverse()
    reversed_numeric_slots = _reseal(reversed_numeric_slots)
    reversed_components = deepcopy(canonical_objects)
    reversed_components["component_observations"].reverse()
    reversed_components = _reseal(reversed_components)
    duplicate_components = deepcopy(canonical_objects)
    duplicate_components["component_observations"].append(deepcopy(duplicate_components["component_observations"][0]))
    duplicate_components = _reseal(duplicate_components)
    gapped_numeric_slots = deepcopy(numeric_slots)
    del gapped_numeric_slots["slot_observations"][4]
    gapped_numeric_slots = _reseal(gapped_numeric_slots)
    extreme_gap_slots = deepcopy(numeric_slots)
    extreme_gap_slots.update(
        {
            "reason_codes": [
                "QUARANTINE_LEDGER_SLOT_MISSING",
                "QUARANTINE_LEDGER_SLOT_EXTRA",
                "QUARANTINE_APPEND_SLOT_OCCUPIED",
                "QUARANTINE_FOOTPRINT_MISMATCH",
            ],
            "first_unsafe_ledger_sequence": "0",
            "slot_observations": [
                {**numeric_slots["slot_observations"][0], "ledger_sequence": "0"},
                {
                    **numeric_slots["slot_observations"][-1],
                    "ledger_sequence": str(MAX_U64),
                },
            ],
        }
    )
    extreme_gap_slots = _reseal(extreme_gap_slots)
    assert "semantic_slot_window" in {issue.code for issue in _issues(extreme_gap_slots, resolver=component_resolver)}
    slot_overflow = deepcopy(base)
    slot_overflow["slot_observations"][0]["ledger_sequence"] = str(MAX_U64 + 1)
    slot_overflow["first_unsafe_ledger_sequence"] = str(MAX_U64 + 1)
    slot_overflow = _reseal(slot_overflow)
    slot_leading_zero = deepcopy(base)
    slot_leading_zero["slot_observations"][0]["ledger_sequence"] = "00"
    slot_leading_zero["first_unsafe_ledger_sequence"] = "00"
    slot_leading_zero = _reseal(slot_leading_zero)
    for mutation in (
        malformed,
        mixed_anchor,
        empty_observations,
        wrong_first_slot,
        wrong_component_ordinal,
        missing_canonical_ordinal,
        duplicate_slot,
        reversed_numeric_slots,
        reversed_components,
        duplicate_components,
        gapped_numeric_slots,
        slot_overflow,
        slot_leading_zero,
        _reseal({**base, "observed_footprint_sha256": base["expected_footprint_sha256"]}),
    ):
        _assert_invalid(mutation, resolver=vector.resolver)

    assert _issues(base, resolver=vector.resolver, expected_schema="trading.ledger-record/v1")
    from build_finance.crypto_replay.schema_definitions import PRIMARY_SCHEMA_DOCUMENTS

    ledger_record_types = PRIMARY_SCHEMA_DOCUMENTS["trading.ledger-record/v1"]["properties"]["record_type"]["enum"]
    assert "EXECUTION_QUARANTINE" not in ledger_record_types


def test_source_admission_is_config_independent() -> None:
    from build_finance.crypto_replay.schema_definitions import SUPPORTING_SCHEMA_DOCUMENTS
    from tests.crypto_replay.support.builders import build_source_admission_receipt

    vector = build_t02_vector()
    receipt = vector.documents["trading.source-admission-receipt/v1"]
    schema = SUPPORTING_SCHEMA_DOCUMENTS["trading.source-admission-receipt/v1"]
    property_names = set(schema["properties"])
    assert (
        not {
            "config_sha256",
            "config_admission_receipt_id",
            "run_closure_receipt_id",
            "feature_snapshot_id",
            "fill_receipt_id",
        }
        & property_names
    )
    assert not _issues(receipt, resolver=vector.resolver)
    builder_inputs = _source_builder_inputs(receipt)
    before_config_change = build_source_admission_receipt(reason_codes=(), **builder_inputs)
    changed_config = _reseal(
        {
            **vector.documents["trading.replay-risk-config/v1"],
            "target_entry_notional_quote_atoms": "200000",
        }
    )
    after_config_change = build_source_admission_receipt(reason_codes=(), **builder_inputs)
    assert before_config_change == after_config_change == receipt
    resolver = SyntheticResolver(
        (*vector.documents.values(), changed_config, *vector.raw_events),
        tuple(vector.attachment_payloads.values()),
    )
    assert not _issues(receipt, resolver=resolver)
    assert canonical_record_bytes(receipt) == canonical_record_bytes(
        vector.documents["trading.source-admission-receipt/v1"]
    )


def test_model_registry_and_signal_manifest_are_set_closed() -> None:
    vector = build_t02_vector()
    registry = vector.documents["trading.model-registry/v1"]
    manifest = vector.documents["trading.model-signal-manifest/v1"]
    _assert_independent_basic_oracles(registry, resolver=vector.resolver)
    _assert_independent_basic_oracles(manifest, resolver=vector.resolver)
    assert not _issues(registry, resolver=vector.resolver)
    assert not _issues(manifest, resolver=vector.resolver)
    promotion = registry["promotions"][0]
    candidate = manifest["candidates"][0]
    assert candidate["requested_producer_scope_key_sha256"] == promotion["producer_scope_key_sha256"]

    def scope_key(row: dict[str, Any]) -> str:
        scope_fields = (
            "adapter_sha256",
            "calibration_sha256",
            "calibration_version",
            "feature_set_version",
            "horizon_ns",
            "market_id",
            "model_artifact_sha256",
            "model_id",
            "model_version",
            "producer_id",
            "runtime_profile_id",
        )
        return sha256_hex(canonical_json_bytes({field: row[field] for field in scope_fields}))

    assert promotion["producer_scope_key_sha256"] == scope_key(promotion)
    second_promotion = deepcopy(promotion)
    second_promotion["producer_id"] = "synthetic-producer-2"
    second_promotion["producer_scope_key_sha256"] = scope_key(second_promotion)
    two_promotions = deepcopy(registry)
    two_promotions["promotions"] = sorted(
        [deepcopy(promotion), second_promotion],
        key=lambda row: row["producer_scope_key_sha256"].encode("utf-8"),
    )
    two_promotions = _reseal(two_promotions)
    assert not _issues(two_promotions, resolver=vector.resolver)

    enabled_with_adapter = deepcopy(promotion)
    enabled_with_adapter.update(
        {
            "adapter_sha256": _digest("static-adapter"),
            "max_uncertainty_q18": "1000000000000000000",
            "max_ood_score_q18": "1000000000000000000",
            "status": "ENABLED",
        }
    )
    enabled_with_adapter["producer_scope_key_sha256"] = scope_key(enabled_with_adapter)
    enabled_registry = _reseal({**registry, "promotions": [enabled_with_adapter]})
    assert not _issues(enabled_registry, resolver=vector.resolver)

    second_candidate = deepcopy(candidate)
    second_candidate.update(
        {
            "relative_path": "signals/decision-2.json",
            "raw_signal_sha256": _digest("second-static-signal"),
            "raw_byte_length": "0",
            "declared_signal_id": None,
            "feature_snapshot_id": _digest("feature-snapshot-2"),
            "decision_sequence": "2",
            "producer_sequence": None,
            "producer_scope_key_sha256": None,
            "issued_replay_clock_ns": "1",
            "available_replay_clock_ns": "2",
            "decision_close_replay_clock_ns": "3",
        }
    )
    two_candidates = deepcopy(manifest)
    two_candidates["candidates"].append(second_candidate)
    two_candidates = _reseal(two_candidates)
    assert not _issues(two_candidates, resolver=vector.resolver)

    composite_candidates: list[dict[str, Any]] = []
    for producer_sequence, decision_sequence, suffix in (("2", "2", "a"), ("10", "3", "b"), (None, "4", "c")):
        row = deepcopy(candidate)
        row.update(
            {
                "relative_path": f"signals/composite-{suffix}.json",
                "raw_signal_sha256": _digest(f"composite-{suffix}"),
                "raw_byte_length": "1",
                "declared_signal_id": _digest(f"declared-{suffix}"),
                "feature_snapshot_id": _digest(f"snapshot-{suffix}"),
                "decision_sequence": decision_sequence,
                "producer_sequence": producer_sequence,
                "producer_scope_key_sha256": (
                    promotion["producer_scope_key_sha256"] if producer_sequence is not None else None
                ),
            }
        )
        composite_candidates.append(row)

    def candidate_sort_key(row: dict[str, Any]) -> tuple[Any, ...]:
        producer_sequence = row["producer_sequence"]
        return (
            row["requested_producer_scope_key_sha256"].encode("utf-8"),
            1 if producer_sequence is None else 0,
            0 if producer_sequence is None else int(producer_sequence),
            int(row["decision_sequence"]),
            row["feature_snapshot_id"].encode("utf-8"),
            int(row["horizon_ns"]),
            row["raw_signal_sha256"].encode("utf-8"),
            row["relative_path"].encode("utf-8"),
        )

    composite_manifest = _reseal({**manifest, "candidates": sorted(composite_candidates, key=candidate_sort_key)})
    assert not _issues(composite_manifest, resolver=vector.resolver)

    oversized = deepcopy(candidate)
    oversized.update(
        {
            "relative_path": "signals/oversized.json",
            "raw_signal_sha256": _digest("oversized-static-bytes"),
            "raw_byte_length": "16385",
            "declared_signal_id": None,
            "producer_sequence": None,
            "producer_scope_key_sha256": None,
        }
    )
    assert not _issues(_reseal({**manifest, "candidates": [oversized]}), resolver=vector.resolver)
    partial_hint = deepcopy(candidate)
    partial_hint.update(
        {
            "relative_path": "signals/partial-hint.json",
            "raw_signal_sha256": _digest("partial-hint-static-bytes"),
            "raw_byte_length": "1",
            "declared_signal_id": _digest("partial-declared-signal"),
            "producer_sequence": None,
            "producer_scope_key_sha256": None,
        }
    )
    assert not _issues(_reseal({**manifest, "candidates": [partial_hint]}), resolver=vector.resolver)

    duplicate_promotion = deepcopy(registry)
    duplicate_promotion["promotions"].append(deepcopy(promotion))
    duplicate_candidate = deepcopy(manifest)
    duplicate_candidate["candidates"].append(deepcopy(candidate))
    unsafe_path = deepcopy(manifest)
    unsafe_path["candidates"][0]["relative_path"] = "../signal.json"
    stale_scope_key = deepcopy(registry)
    stale_scope_key["promotions"][0]["producer_scope_key_sha256"] = _digest("stale-producer-scope")
    reversed_promotions = deepcopy(two_promotions)
    reversed_promotions["promotions"].reverse()
    reversed_candidates = deepcopy(two_candidates)
    reversed_candidates["candidates"].reverse()
    duplicate_path = deepcopy(two_candidates)
    duplicate_path["candidates"][1]["relative_path"] = candidate["relative_path"]
    duplicate_request_tuple = deepcopy(two_candidates)
    duplicate_request_tuple["candidates"][1].update(
        {
            "requested_producer_scope_key_sha256": candidate["requested_producer_scope_key_sha256"],
            "producer_sequence": candidate["producer_sequence"],
            "raw_signal_sha256": candidate["raw_signal_sha256"],
        }
    )
    empty_registry = _reseal({**registry, "promotions": []})
    empty_manifest = _reseal({**manifest, "candidates": []})
    assert not _issues(empty_registry, resolver=vector.resolver)
    assert not _issues(empty_manifest, resolver=vector.resolver)
    zero_horizon = deepcopy(registry)
    zero_horizon["promotions"][0]["horizon_ns"] = "0"
    zero_horizon["promotions"][0]["producer_scope_key_sha256"] = scope_key(zero_horizon["promotions"][0])
    uncertainty_over = deepcopy(registry)
    uncertainty_over["promotions"][0]["max_uncertainty_q18"] = "1000000000000000001"
    zero_ttl = deepcopy(registry)
    zero_ttl["promotions"][0]["max_ttl_ns"] = "0"
    keyed_duplicate = deepcopy(registry)
    distinct_same_scope = deepcopy(promotion)
    distinct_same_scope["status"] = "ENABLED"
    keyed_duplicate["promotions"].append(distinct_same_scope)
    non_nfc_path = deepcopy(manifest)
    non_nfc_path["candidates"][0]["relative_path"] = "signals/e\u0301.json"
    reversed_composite = deepcopy(composite_manifest)
    reversed_composite["candidates"].reverse()
    zero_with_hint = deepcopy(candidate)
    zero_with_hint.update(
        {
            "raw_byte_length": "0",
            "declared_signal_id": _digest("forbidden-zero-byte-hint"),
        }
    )
    oversized_with_hint = deepcopy(oversized)
    oversized_with_hint["producer_sequence"] = "0"
    for mutation in (
        _reseal(duplicate_promotion),
        _reseal(duplicate_candidate),
        _reseal(unsafe_path),
        _reseal(stale_scope_key),
        _reseal(reversed_promotions),
        _reseal(reversed_candidates),
        _reseal(duplicate_path),
        _reseal(duplicate_request_tuple),
        _reseal(zero_horizon),
        _reseal(uncertainty_over),
        _reseal(zero_ttl),
        _reseal(keyed_duplicate),
        _reseal(non_nfc_path),
        _reseal(reversed_composite),
        _reseal({**manifest, "candidates": [zero_with_hint]}),
        _reseal({**manifest, "candidates": [oversized_with_hint]}),
    ):
        _assert_invalid(mutation, resolver=vector.resolver)


def test_model_validation_fallback_tuple_is_closed() -> None:
    vector = build_t02_vector()
    rejected = vector.documents["trading.model-validation-receipt/v1"]
    _assert_independent_basic_oracles(rejected, resolver=vector.resolver)
    fallback = {
        "accepted_signal_id": None,
        "canonical_action": "ABSTAIN",
        "canonical_score_q18": "0",
        "canonical_probability_abstain_q18": "1000000000000000000",
        "canonical_probability_long_bias_q18": "0",
        "canonical_probability_exit_bias_q18": "0",
        "canonical_uncertainty_q18": "1000000000000000000",
        "canonical_ood_score_q18": "1000000000000000000",
    }
    parsed_evidence = {
        "declared_signal_id": _digest("declared-signal"),
        "producer_sequence": "0",
        "producer_scope_key_sha256": _digest("producer-scope"),
        "issued_replay_clock_ns": "0",
        "expires_replay_clock_ns": "1",
    }
    expired = _reseal(
        {
            **rejected,
            **parsed_evidence,
            "status": "EXPIRED",
            "reason_codes": ["SIG_EXPIRED"],
        }
    )
    expired_with_deadline = _reseal(
        {
            **expired,
            "reason_codes": ["SIG_EXPIRED", "SIG_DEADLINE_MISS"],
        }
    )
    drift_disabled = _reseal(
        {
            **rejected,
            **parsed_evidence,
            "status": "DRIFT_DISABLED",
            "reason_codes": ["SIG_DRIFT_DISABLED"],
            "expires_replay_clock_ns": "2",
        }
    )
    rejected_deadline_only = _reseal(
        {
            **rejected,
            **parsed_evidence,
            "status": "REJECTED",
            "reason_codes": ["SIG_DEADLINE_MISS"],
        }
    )
    rejected_disabled_ood = _reseal(
        {
            **rejected,
            **parsed_evidence,
            "status": "REJECTED",
            "reason_codes": ["SIG_UNCERTAIN_OR_OOD", "SIG_DRIFT_DISABLED"],
        }
    )
    reason_precedence = (
        "SIG_BYTES_INVALID",
        "SIG_SCHEMA_UNKNOWN",
        "SIG_ID_MISMATCH",
        "SIG_ORDER_SHAPED",
        "SIG_MODEL_UNPINNED",
        "SIG_RUNTIME_SUBSTITUTION",
        "SIG_SCOPE_MISMATCH",
        "SIG_FEATURE_MISMATCH",
        "SIG_PRODUCER_SEQUENCE_INVALID",
        "SIG_REPLAYED",
        "SIG_TIME_INVALID",
        "SIG_TTL_RANGE",
        "SIG_EXPIRED",
        "SIG_DEADLINE_MISS",
        "SIG_NUMERIC_INVALID",
        "SIG_PROBABILITY_INVALID",
        "SIG_ACTION_INCONSISTENT",
        "SIG_CALIBRATION_UNKNOWN",
        "SIG_UNCERTAIN_OR_OOD",
        "SIG_DRIFT_DISABLED",
    )
    reason_rank = {reason: index for index, reason in enumerate(reason_precedence)}

    def validation_receipt(*reasons: str) -> dict[str, Any]:
        ordered_reasons = sorted(set(reasons), key=reason_rank.__getitem__)
        reason_set = frozenset(ordered_reasons)
        if reason_set == {"SIG_DRIFT_DISABLED"}:
            status = "DRIFT_DISABLED"
        elif reason_set and reason_set <= {"SIG_EXPIRED", "SIG_DEADLINE_MISS"} and "SIG_EXPIRED" in reason_set:
            status = "EXPIRED"
        else:
            status = "REJECTED"
        evidence = {} if "SIG_BYTES_INVALID" in reason_set else parsed_evidence
        return _reseal(
            {
                **rejected,
                **evidence,
                "status": status,
                "reason_codes": ordered_reasons,
            }
        )

    valid_singleton_reasons = tuple(reason for reason in reason_precedence if reason != "SIG_ORDER_SHAPED")
    singleton_receipts = [validation_receipt(reason) for reason in valid_singleton_reasons]
    identity_codes = (
        "SIG_MODEL_UNPINNED",
        "SIG_RUNTIME_SUBSTITUTION",
        "SIG_SCOPE_MISMATCH",
        "SIG_FEATURE_MISMATCH",
        "SIG_CALIBRATION_UNKNOWN",
    )
    sequence_codes = ("SIG_PRODUCER_SEQUENCE_INVALID", "SIG_REPLAYED")
    later_codes = (
        "SIG_TIME_INVALID",
        "SIG_TTL_RANGE",
        "SIG_EXPIRED",
        "SIG_DEADLINE_MISS",
        "SIG_NUMERIC_INVALID",
        "SIG_PROBABILITY_INVALID",
        "SIG_ACTION_INCONSISTENT",
        "SIG_UNCERTAIN_OR_OOD",
    )
    structural_order_shaped = validation_receipt("SIG_SCHEMA_UNKNOWN", "SIG_ORDER_SHAPED")
    structural_without_hints = _reseal(
        {
            **rejected,
            "status": "REJECTED",
            "reason_codes": ["SIG_SCHEMA_UNKNOWN"],
        }
    )
    identity_with_drift = validation_receipt(*identity_codes, "SIG_DRIFT_DISABLED")
    sequence_with_drift = tuple(validation_receipt(reason, "SIG_DRIFT_DISABLED") for reason in sequence_codes)
    later_with_time_failure = validation_receipt(
        "SIG_TIME_INVALID",
        "SIG_TTL_RANGE",
        "SIG_NUMERIC_INVALID",
        "SIG_PROBABILITY_INVALID",
        "SIG_ACTION_INCONSISTENT",
        "SIG_UNCERTAIN_OR_OOD",
        "SIG_DRIFT_DISABLED",
    )
    later_with_expiry = validation_receipt(
        "SIG_TTL_RANGE",
        "SIG_EXPIRED",
        "SIG_DEADLINE_MISS",
        "SIG_NUMERIC_INVALID",
        "SIG_PROBABILITY_INVALID",
        "SIG_ACTION_INCONSISTENT",
        "SIG_UNCERTAIN_OR_OOD",
        "SIG_DRIFT_DISABLED",
    )
    valid_stage_receipts = (
        structural_order_shaped,
        structural_without_hints,
        identity_with_drift,
        *sequence_with_drift,
        later_with_time_failure,
        later_with_expiry,
    )
    accepted_shape = _reseal(
        {
            **rejected,
            **parsed_evidence,
            "status": "ACCEPTED",
            "reason_codes": [],
            "accepted_signal_id": _digest("static-accepted-shape-only"),
            "canonical_action": "LONG_BIAS",
            "canonical_score_q18": "1",
            "canonical_probability_abstain_q18": "0",
            "canonical_probability_long_bias_q18": "1000000000000000000",
            "canonical_probability_exit_bias_q18": "0",
            "canonical_uncertainty_q18": "0",
            "canonical_ood_score_q18": "0",
        }
    )
    receipts = (
        rejected,
        expired,
        expired_with_deadline,
        drift_disabled,
        rejected_deadline_only,
        rejected_disabled_ood,
    )
    resolver = SyntheticResolver(
        (
            *vector.documents.values(),
            *receipts,
            *singleton_receipts,
            *valid_stage_receipts,
            accepted_shape,
            *vector.source_receipts,
            *vector.raw_events,
        ),
        (*vector.attachment_payloads.values(),),
    )
    for receipt in receipts:
        assert not _issues(receipt, resolver=resolver)
        assert {field: receipt[field] for field in fallback} == fallback
        assert receipt["accepted_signal_id"] is None
    assert [receipt["status"] for receipt in receipts] == [
        "REJECTED",
        "EXPIRED",
        "EXPIRED",
        "DRIFT_DISABLED",
        "REJECTED",
        "REJECTED",
    ]
    for reason, receipt in zip(valid_singleton_reasons, singleton_receipts, strict=True):
        assert receipt["reason_codes"] == [reason]
        assert not _issues(receipt, resolver=resolver)
    for receipt in valid_stage_receipts:
        assert not _issues(receipt, resolver=resolver)
        assert {field: receipt[field] for field in fallback} == fallback
    covered_reasons = {
        reason for receipt in (*singleton_receipts, *valid_stage_receipts) for reason in receipt["reason_codes"]
    }
    assert covered_reasons == set(reason_precedence)
    assert not _issues(accepted_shape, resolver=resolver)

    invalid_stage_sets: set[tuple[str, ...]] = set()

    def add_invalid_stage(*reasons: str) -> None:
        invalid_stage_sets.add(tuple(sorted(set(reasons), key=reason_rank.__getitem__)))

    for reason in reason_precedence:
        if reason != "SIG_BYTES_INVALID":
            add_invalid_stage("SIG_BYTES_INVALID", reason)
    add_invalid_stage("SIG_ORDER_SHAPED")
    post_structural_codes = tuple(
        reason
        for reason in reason_precedence
        if reason not in {"SIG_BYTES_INVALID", "SIG_SCHEMA_UNKNOWN", "SIG_ORDER_SHAPED"}
    )
    for reason in post_structural_codes:
        add_invalid_stage("SIG_ORDER_SHAPED", reason)
        add_invalid_stage("SIG_SCHEMA_UNKNOWN", reason)
        add_invalid_stage("SIG_SCHEMA_UNKNOWN", "SIG_ORDER_SHAPED", reason)
    for reason in reason_precedence:
        if reason != "SIG_ID_MISMATCH":
            add_invalid_stage("SIG_ID_MISMATCH", reason)
    for identity_code in identity_codes:
        for reason in (*sequence_codes, *later_codes):
            add_invalid_stage(identity_code, reason)
    add_invalid_stage(*sequence_codes)
    for sequence_code in sequence_codes:
        for reason in later_codes:
            add_invalid_stage(sequence_code, reason)
    add_invalid_stage("SIG_TIME_INVALID", "SIG_EXPIRED")
    add_invalid_stage("SIG_TIME_INVALID", "SIG_DEADLINE_MISS")
    for reasons in invalid_stage_sets:
        issues = _issues(validation_receipt(*reasons), resolver=resolver)
        assert "semantic_validation_stage" in {issue.code for issue in issues}, reasons

    mutations: list[dict[str, Any]] = []
    for receipt in receipts:
        for field, wrong in (
            ("accepted_signal_id", _digest("forbidden-accepted-signal")),
            ("canonical_action", "LONG_BIAS"),
            ("canonical_score_q18", "1"),
            ("canonical_probability_abstain_q18", "999999999999999999"),
            ("canonical_probability_long_bias_q18", "1"),
            ("canonical_probability_exit_bias_q18", "1"),
            ("canonical_uncertainty_q18", "0"),
            ("canonical_ood_score_q18", "0"),
        ):
            mutations.append(_reseal({**receipt, field: wrong}))
    mutations.extend(
        (
            _reseal({**expired, "status": "REJECTED"}),
            _reseal({**drift_disabled, "status": "EXPIRED"}),
            _reseal({**drift_disabled, "reason_codes": ["SIG_EXPIRED"]}),
            _reseal({**expired_with_deadline, "reason_codes": ["SIG_DEADLINE_MISS", "SIG_EXPIRED"]}),
            _reseal({**drift_disabled, "reason_codes": ["SIG_DRIFT_DISABLED", "SIG_DRIFT_DISABLED"]}),
            _reseal({**rejected, "reason_codes": ["SIG_UNKNOWN"]}),
            _reseal({**rejected_disabled_ood, "status": "DRIFT_DISABLED"}),
            _reseal(
                {
                    **later_with_expiry,
                    "reason_codes": list(reversed(later_with_expiry["reason_codes"])),
                }
            ),
            _reseal({**accepted_shape, "accepted_signal_id": None}),
            _reseal({**accepted_shape, "canonical_probability_long_bias_q18": "999999999999999999"}),
            _reseal({**accepted_shape, "canonical_probability_long_bias_q18": "1000000000000000001"}),
            _reseal({**rejected, "declared_signal_id": _digest("fabricated-byte-invalid-declaration")}),
            _reseal({**rejected, "available_replay_clock_ns": None}),
        )
    )
    id_mismatch_receipt = singleton_receipts[valid_singleton_reasons.index("SIG_ID_MISMATCH")]
    for field in parsed_evidence:
        mutations.append(_reseal({**id_mismatch_receipt, field: None}))
    for mutation in mutations:
        _assert_invalid(mutation, resolver=resolver)


def test_benchmark_measurement_sample_arrays_are_closed() -> None:
    vector = build_t02_vector()
    measurement = vector.documents["trading.benchmark-measurement/v1"]
    assert not _issues(measurement, resolver=vector.resolver)
    assert [row["phase"] for row in measurement["phase_samples"]] == [
        "admission",
        "feature",
        "risk",
        "fill",
        "accounting",
        "end_to_end",
    ]
    assert all(int(row["sample_count"]) == len(row["samples_ns"]) for row in measurement["phase_samples"])
    assert int(measurement["measured_wall_duration_ns"]) == sum(
        int(value) for value in measurement["phase_samples"][-1]["samples_ns"]
    )
    count_mismatch = deepcopy(measurement)
    count_mismatch["phase_samples"][0]["sample_count"] = "1"
    wrong_unit = deepcopy(measurement)
    wrong_unit["phase_samples"][0]["unit"] = "EQUAL_TIME_GROUP"
    wrong_order = deepcopy(measurement)
    wrong_order["phase_samples"].reverse()
    missing_counter = deepcopy(measurement)
    missing_counter["raw_counters"].pop()
    for mutation in (count_mismatch, wrong_unit, wrong_order, missing_counter):
        _assert_invalid(_reseal(mutation), resolver=vector.resolver)
