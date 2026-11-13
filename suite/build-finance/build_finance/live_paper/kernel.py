"""Sole deterministic orchestrator for the in-memory G2 paper run."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, cast

from build_finance.crypto_replay.canonical import (
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import verify_content_id
from build_finance.crypto_replay.run_inputs import ContractVerifiedRunInputs
from build_finance.crypto_replay.schema_registry import require_valid_contract
from build_finance.live_paper.accounting import (
    apply_fill_receipt,
    initialize_accounting,
    reserve_intent,
)
from build_finance.live_paper.algorithms import derive_algorithm_candidates
from build_finance.live_paper.event_store import InMemoryLedgerStore
from build_finance.live_paper.features import derive_feature_snapshot
from build_finance.live_paper.fills import simulate_terminal_fill
from build_finance.live_paper.fusion import fuse_signal_evidence
from build_finance.live_paper.grouping import EventGroup, group_committed_events
from build_finance.live_paper.model_validation import validate_model_signal
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.projections import PaperKernelProjection, build_kernel_projection
from build_finance.live_paper.resolver import EvidenceResolver
from build_finance.live_paper.risk import evaluate_risk


@dataclass(frozen=True, slots=True)
class PaperKernelClosure:
    """Deterministic run evidence; it grants no execution authority."""

    status: str
    reason_codes: tuple[str, ...]
    failure_boundary: str | None
    failure_type: str | None
    error_message: str | None
    ledger_head_id: str | None
    final_portfolio_state_id: str | None
    closure_id: str

    def __post_init__(self) -> None:
        object.__setattr__(self, "reason_codes", tuple(self.reason_codes))


@dataclass(frozen=True, slots=True)
class PaperKernelResult:
    """Complete immutable evidence retained by one offline kernel run."""

    closure: PaperKernelClosure
    closure_record: bytes
    profile_records: PaperKernelProfiles
    event_groups: tuple[EventGroup, ...]
    feature_snapshot_records: tuple[bytes, ...]
    algorithm_candidate_records: tuple[tuple[bytes, ...], ...]
    fusion_decision_records: tuple[bytes, ...]
    decision_group_manifest_records: tuple[bytes, ...]
    risk_decision_records: tuple[bytes, ...]
    simulated_order_intent_records: tuple[bytes, ...]
    simulated_fill_receipt_records: tuple[bytes, ...]
    reconciliation_receipt_records: tuple[bytes, ...]
    portfolio_state_records: tuple[bytes, ...]
    ledger_records: tuple[bytes, ...]
    final_portfolio_state_record: bytes | None
    store: InMemoryLedgerStore
    projection: PaperKernelProjection

    def __post_init__(self) -> None:
        object.__setattr__(self, "closure_record", bytes(self.closure_record))
        object.__setattr__(self, "event_groups", tuple(self.event_groups))
        for name in (
            "feature_snapshot_records",
            "fusion_decision_records",
            "decision_group_manifest_records",
            "risk_decision_records",
            "simulated_order_intent_records",
            "simulated_fill_receipt_records",
            "reconciliation_receipt_records",
            "portfolio_state_records",
            "ledger_records",
        ):
            object.__setattr__(self, name, tuple(bytes(record) for record in getattr(self, name)))
        object.__setattr__(
            self,
            "algorithm_candidate_records",
            tuple(tuple(bytes(record) for record in records) for records in self.algorithm_candidate_records),
        )
        final = None if self.final_portfolio_state_record is None else bytes(self.final_portfolio_state_record)
        object.__setattr__(self, "final_portfolio_state_record", final)


class _Evidence:
    __slots__ = (
        "algorithm_candidates",
        "current_state",
        "decision_manifests",
        "feature_snapshots",
        "fills",
        "fusion_decisions",
        "groups",
        "intents",
        "model_dispositions",
        "portfolio_states",
        "reconciliations",
        "risk_decisions",
        "store",
    )

    def __init__(self) -> None:
        self.groups: tuple[EventGroup, ...] = ()
        self.feature_snapshots: list[bytes] = []
        self.algorithm_candidates: list[tuple[bytes, ...]] = []
        self.fusion_decisions: list[bytes] = []
        self.decision_manifests: list[bytes] = []
        self.risk_decisions: list[bytes] = []
        self.intents: list[bytes] = []
        self.fills: list[bytes] = []
        self.reconciliations: list[bytes] = []
        self.portfolio_states: list[bytes] = []
        self.model_dispositions: list[tuple[str, str, str]] = []
        self.store = InMemoryLedgerStore(())
        self.current_state: bytes | None = None


class _BoundaryFailure(Exception):
    def __init__(self, boundary: str, reason_code: str, error: Exception) -> None:
        super().__init__(str(error))
        self.boundary = boundary
        self.reason_code = reason_code
        self.error = error


def _at_boundary(
    boundary: str,
    reason_code: str,
    operation: Callable[..., Any],
    /,
    *args: Any,
    **kwargs: Any,
) -> Any:
    try:
        return operation(*args, **kwargs)
    except Exception as error:  # noqa: BLE001 - the kernel converts boundary failures to inert evidence.
        raise _BoundaryFailure(boundary, reason_code, error) from error


def _snapshot_profiles(profiles: object) -> PaperKernelProfiles:
    if not isinstance(profiles, PaperKernelProfiles):
        return PaperKernelProfiles(b"", b"", (), b"", b"", b"", b"")
    return PaperKernelProfiles(
        normalization_profile_record=profiles.normalization_profile_record,
        feature_profile_record=profiles.feature_profile_record,
        algorithm_profile_records=profiles.algorithm_profile_records,
        model_validation_profile_record=profiles.model_validation_profile_record,
        fusion_profile_record=profiles.fusion_profile_record,
        risk_config_record=profiles.risk_config_record,
        fill_profile_record=profiles.fill_profile_record,
    )


def _profile_inputs(
    verified: object,
    supplied_profiles: object,
    profiles: PaperKernelProfiles,
) -> tuple[str, int, int]:
    if not isinstance(supplied_profiles, PaperKernelProfiles):
        raise ValueError("kernel requires closed PaperKernelProfiles")
    if not isinstance(verified, ContractVerifiedRunInputs) or verified.authority != "CONTRACT_ONLY":
        raise ValueError("kernel profiles require frozen contract-verified run inputs")
    try:
        risk = parse_canonical_record(profiles.risk_config_record)
        require_valid_contract(risk, expected_schema="trading.replay-risk-config/v1")
        if not verify_content_id(risk) or canonical_record_bytes(risk) != profiles.risk_config_record:
            raise ValueError("risk profile content identity does not match its canonical body")
    except (TypeError, ValueError) as error:
        raise ValueError("risk profile is not an exact self-addressed canonical record") from error
    try:
        fixture = parse_canonical_record(profiles.normalization_profile_record)
        require_valid_contract(fixture, expected_schema="trading.fixture-manifest/v1")
        rooted_fixture_id = verified.run_receipt["fixture_manifest_sha256"]
        if not verify_content_id(fixture) or fixture.get("fixture_manifest_sha256") != rooted_fixture_id:
            raise ValueError("fixture profile content identity is not rooted by the run")
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("normalization profile is not the verified rooted fixture manifest") from error
    quote_mint = fixture.get("quote_mint")
    quote_decimals = fixture.get("quote_decimals")
    starting_quote_atoms = fixture.get("initial_quote_atoms")
    if (
        not isinstance(quote_mint, str)
        or type(quote_decimals) is not int
        or not isinstance(starting_quote_atoms, str)
        or not starting_quote_atoms.isascii()
        or not starting_quote_atoms.isdigit()
        or str(int(starting_quote_atoms)) != starting_quote_atoms
    ):
        raise ValueError("rooted fixture has invalid initial quote identity or amount")
    return quote_mint, quote_decimals, int(starting_quote_atoms)


def _require_reconciliation_pass(record: bytes) -> None:
    document = parse_canonical_record(record)
    require_valid_contract(document, expected_schema="trading.reconciliation-receipt/v1")
    if document["status"] != "PASS":
        raise ValueError("accounting transition failed closed reconciliation")


def _record_id(record: bytes | None, field: str) -> str | None:
    if record is None:
        return None
    try:
        value = parse_canonical_record(record).get(field)
    except (TypeError, ValueError):
        return None
    return value if isinstance(value, str) else None


def _ledger_head_id(evidence: _Evidence, run_receipt_id: str | None) -> str | None:
    if evidence.store.ledger_records:
        return _record_id(evidence.store.ledger_records[-1], "ledger_record_id")
    state_id = _record_id(evidence.current_state, "portfolio_state_id")
    if state_id is None:
        return None
    return sha256_hex(
        canonical_json_bytes(
            {
                "schema": "build-finance.live-paper.empty-ledger-head/v1",
                "run_receipt_id": run_receipt_id,
                "portfolio_state_id": state_id,
                "ledger_count": "0",
            }
        )
    )


def _closure(
    evidence: _Evidence,
    *,
    run_receipt_id: str | None,
    failure: _BoundaryFailure | None,
) -> tuple[PaperKernelClosure, bytes]:
    ledger_head_id = _ledger_head_id(evidence, run_receipt_id)
    final_state_id = _record_id(evidence.current_state, "portfolio_state_id")
    body: dict[str, Any] = {
        "schema": "build-finance.live-paper.kernel-closure/v1",
        "status": "CLOSED" if failure is None else "FAILED_CLOSED",
        "reason_codes": [] if failure is None else [failure.reason_code],
        "failure_boundary": None if failure is None else failure.boundary,
        "failure_type": None if failure is None else type(failure.error).__name__,
        "error_message": None if failure is None else str(failure.error),
        "run_receipt_id": run_receipt_id,
        "ledger_head_id": ledger_head_id,
        "final_portfolio_state_id": final_state_id,
        "retained_counts": {
            "event_groups": str(len(evidence.groups)),
            "feature_snapshots": str(len(evidence.feature_snapshots)),
            "algorithm_candidate_groups": str(len(evidence.algorithm_candidates)),
            "fusion_decisions": str(len(evidence.fusion_decisions)),
            "risk_decisions": str(len(evidence.risk_decisions)),
            "intents": str(len(evidence.intents)),
            "fills": str(len(evidence.fills)),
            "portfolio_states": str(len(evidence.portfolio_states)),
            "ledger_records": str(len(evidence.store.ledger_records)),
        },
        "authority": "DETERMINISTIC_EVIDENCE_ONLY",
    }
    closure_id = sha256_hex(canonical_json_bytes(body))
    record = canonical_record_bytes({**body, "closure_id": closure_id})
    closure = PaperKernelClosure(
        status=cast(str, body["status"]),
        reason_codes=() if failure is None else (failure.reason_code,),
        failure_boundary=None if failure is None else failure.boundary,
        failure_type=None if failure is None else type(failure.error).__name__,
        error_message=None if failure is None else str(failure.error),
        ledger_head_id=ledger_head_id,
        final_portfolio_state_id=final_state_id,
        closure_id=closure_id,
    )
    return closure, record


def _run_receipt_id(verified: object) -> str | None:
    if not isinstance(verified, ContractVerifiedRunInputs):
        return None
    value = verified.run_receipt.get("run_receipt_id")
    return value if isinstance(value, str) else None


def _result(
    evidence: _Evidence,
    *,
    profiles: PaperKernelProfiles,
    run_receipt_id: str | None,
    failure: _BoundaryFailure | None,
) -> PaperKernelResult:
    closure, closure_record = _closure(evidence, run_receipt_id=run_receipt_id, failure=failure)
    projection = build_kernel_projection(
        closure=closure,
        run_receipt_id=run_receipt_id,
        final_portfolio_state_record=evidence.current_state,
        model_dispositions=evidence.model_dispositions,
        fusion_decision_records=evidence.fusion_decisions,
        risk_decision_records=evidence.risk_decisions,
        simulated_order_intent_records=evidence.intents,
        simulated_fill_receipt_records=evidence.fills,
        reconciliation_receipt_records=evidence.reconciliations,
    )
    return PaperKernelResult(
        closure=closure,
        closure_record=closure_record,
        profile_records=profiles,
        event_groups=evidence.groups,
        feature_snapshot_records=tuple(evidence.feature_snapshots),
        algorithm_candidate_records=tuple(evidence.algorithm_candidates),
        fusion_decision_records=tuple(evidence.fusion_decisions),
        decision_group_manifest_records=tuple(evidence.decision_manifests),
        risk_decision_records=tuple(evidence.risk_decisions),
        simulated_order_intent_records=tuple(evidence.intents),
        simulated_fill_receipt_records=tuple(evidence.fills),
        reconciliation_receipt_records=tuple(evidence.reconciliations),
        portfolio_state_records=tuple(evidence.portfolio_states),
        ledger_records=evidence.store.ledger_records,
        final_portfolio_state_record=evidence.current_state,
        store=evidence.store,
        projection=projection,
    )


def run_offline_paper_kernel(
    verified: ContractVerifiedRunInputs,
    resolver: EvidenceResolver,
    profiles: PaperKernelProfiles,
) -> PaperKernelResult:
    """Replay verified inputs through every reviewed G2 layer, in memory only."""
    evidence = _Evidence()
    profile_snapshot = _snapshot_profiles(profiles)
    run_receipt_id = _run_receipt_id(verified)
    try:
        evidence.groups = _at_boundary(
            "GROUPING",
            "KERNEL_GROUPING_FAILED",
            group_committed_events,
            verified,
        )
        quote_mint, quote_decimals, starting_quote_atoms = _at_boundary(
            "PROFILE_VALIDATION",
            "KERNEL_PROFILE_INVALID",
            _profile_inputs,
            verified,
            profiles,
            profile_snapshot,
        )
        genesis = _at_boundary(
            "ACCOUNTING_INITIALIZATION",
            "KERNEL_ACCOUNTING_FAILED",
            initialize_accounting,
            verified,
            quote_mint=quote_mint,
            quote_decimals=quote_decimals,
            starting_quote_atoms=starting_quote_atoms,
        )
        evidence.reconciliations.append(genesis.reconciliation_receipt_record)
        _at_boundary(
            "RECONCILIATION",
            "KERNEL_RECONCILIATION_FAILED",
            _require_reconciliation_pass,
            genesis.reconciliation_receipt_record,
        )
        evidence.store = genesis.store
        evidence.current_state = genesis.portfolio_state_record
        evidence.portfolio_states.append(genesis.portfolio_state_record)

        for index, group in enumerate(evidence.groups):
            snapshot = _at_boundary(
                "FEATURE_DERIVATION",
                "KERNEL_FEATURE_FAILED",
                derive_feature_snapshot,
                evidence.groups[: index + 1],
            )
            evidence.feature_snapshots.append(snapshot)
            candidates = _at_boundary(
                "ALGORITHM_DERIVATION",
                "KERNEL_ALGORITHM_FAILED",
                derive_algorithm_candidates,
                group,
                snapshot,
            )
            evidence.algorithm_candidates.append(candidates)
            model = _at_boundary(
                "MODEL_VALIDATION",
                "KERNEL_MODEL_VALIDATION_FAILED",
                validate_model_signal,
                None,
                None,
                None,
                snapshot,
            )
            evidence.model_dispositions.append((group.group_sequence, model.disposition, model.reason_code))
            fusion = _at_boundary(
                "FUSION",
                "KERNEL_FUSION_FAILED",
                fuse_signal_evidence,
                group,
                snapshot,
                candidates,
                model,
            )
            evidence.fusion_decisions.append(fusion.fusion_decision_record)
            evidence.decision_manifests.append(fusion.decision_group_manifest_record)
            assert evidence.current_state is not None
            risk = _at_boundary(
                "RISK_EVALUATION",
                "KERNEL_RISK_FAILED",
                evaluate_risk,
                group,
                snapshot,
                candidates,
                model,
                fusion,
                evidence.current_state,
                profile_snapshot.risk_config_record,
            )
            evidence.risk_decisions.append(risk.risk_decision_record)
            intent = risk.simulated_order_intent_record
            if intent is None:
                continue
            evidence.intents.append(intent)
            reserved = _at_boundary(
                "INTENT_RESERVATION",
                "KERNEL_RESERVATION_FAILED",
                reserve_intent,
                verified,
                evidence.store,
                cast(bytes, evidence.current_state),
                intent,
            )
            evidence.reconciliations.append(reserved.reconciliation_receipt_record)
            _at_boundary(
                "RECONCILIATION",
                "KERNEL_RECONCILIATION_FAILED",
                _require_reconciliation_pass,
                reserved.reconciliation_receipt_record,
            )
            evidence.store = reserved.store
            evidence.current_state = reserved.portfolio_state_record
            evidence.portfolio_states.append(reserved.portfolio_state_record)
            selected_group = evidence.groups[index + 1] if index + 1 < len(evidence.groups) else None
            fill = _at_boundary(
                "FILL_SIMULATION",
                "KERNEL_FILL_FAILED",
                simulate_terminal_fill,
                verified,
                intent,
                cast(bytes, evidence.current_state),
                selected_group,
                resolver,
            )
            evidence.fills.append(fill.fill_receipt_record)
            applied = _at_boundary(
                "FILL_ACCOUNTING",
                "KERNEL_ACCOUNTING_FAILED",
                apply_fill_receipt,
                verified,
                evidence.store,
                cast(bytes, evidence.current_state),
                intent,
                fill.fill_receipt_record,
            )
            evidence.reconciliations.append(applied.reconciliation_receipt_record)
            _at_boundary(
                "RECONCILIATION",
                "KERNEL_RECONCILIATION_FAILED",
                _require_reconciliation_pass,
                applied.reconciliation_receipt_record,
            )
            evidence.store = applied.store
            evidence.current_state = applied.portfolio_state_record
            evidence.portfolio_states.append(applied.portfolio_state_record)
    except _BoundaryFailure as failure:
        return _result(
            evidence,
            profiles=profile_snapshot,
            run_receipt_id=run_receipt_id,
            failure=failure,
        )
    return _result(
        evidence,
        profiles=profile_snapshot,
        run_receipt_id=run_receipt_id,
        failure=None,
    )


__all__ = ["PaperKernelClosure", "PaperKernelResult", "run_offline_paper_kernel"]
