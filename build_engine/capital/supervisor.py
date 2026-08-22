"""Fail-closed orchestration for model-disabled offline capital replay."""

from __future__ import annotations

from collections.abc import Callable, Sequence
from hashlib import sha256
from importlib import import_module
from os import PathLike
from pathlib import Path
from typing import Protocol, cast

from build_engine.capital.contracts import CapitalReplayResult, EvidenceSummary, ModelObservation

_INSTALL_INSTRUCTION = "install with: pip install 'build-engine[capital]'"
_REQUIRED_CORE_METHODS = (
    "capture_local_fixture",
    "admit_local_fixture",
    "build_model_disabled_inputs",
    "run_offline_paper_kernel",
    "projection_bytes",
)


class MissingCapitalDependencyError(RuntimeError):
    """Raised before fixture access when the optional paper core is unavailable."""


class _CapitalCore(Protocol):
    def capture_local_fixture(self, fixture: Path) -> object: ...

    def admit_local_fixture(self, captured: object) -> object: ...

    def build_model_disabled_inputs(self, captured: object, admission: object) -> object: ...

    def run_offline_paper_kernel(self, verified: object) -> object: ...

    def projection_bytes(self, result: object) -> bytes: ...


def _load_paper_core() -> _CapitalCore:
    try:
        core = import_module("build_finance.live_paper")
    except ImportError as error:
        raise MissingCapitalDependencyError(_INSTALL_INSTRUCTION) from error

    if any(not callable(getattr(core, method, None)) for method in _REQUIRED_CORE_METHODS):
        raise MissingCapitalDependencyError(f"incompatible paper core; {_INSTALL_INSTRUCTION}")
    return cast(_CapitalCore, core)


def _reason_codes(value: object) -> tuple[str, ...]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        return ()
    return tuple(item for item in value if isinstance(item, str))


def _receipt_ids(projection: object) -> tuple[str, ...]:
    identifiers = projection.receipt_ids
    values = (
        identifiers.run_receipt_id,
        *identifiers.reconciliation_receipt_ids,
        identifiers.ledger_head_id,
        identifiers.final_portfolio_state_id,
        identifiers.closure_id,
    )
    return tuple(value for value in values if isinstance(value, str))


def _evidence(projection: object, closure: object) -> tuple[EvidenceSummary, ...]:
    summaries = [
        EvidenceSummary(
            kind="DECISION",
            evidence_id=str(decision.decision_sequence),
            status=str(decision.risk_verdict),
            reason_codes=_reason_codes(decision.reason_codes),
        )
        for decision in projection.decisions
    ]
    summaries.extend(
        EvidenceSummary(
            kind="FILL",
            evidence_id=str(fill.fill_receipt_id),
            status=str(fill.status),
            reason_codes=_reason_codes(fill.reason_codes),
        )
        for fill in projection.fills
    )
    portfolio_id = projection.receipt_ids.final_portfolio_state_id
    if isinstance(portfolio_id, str):
        summaries.append(
            EvidenceSummary(
                kind="PORTFOLIO",
                evidence_id=portfolio_id,
                status=str(closure.status),
                reason_codes=_reason_codes(closure.reason_codes),
            )
        )
    summaries.append(
        EvidenceSummary(
            kind="CLOSURE",
            evidence_id=str(closure.closure_id),
            status=str(closure.status),
            reason_codes=_reason_codes(closure.reason_codes),
        )
    )
    return tuple(summaries)


def _model_observations(projection: object) -> tuple[ModelObservation, ...]:
    return tuple(
        ModelObservation(
            decision_sequence=str(observation.decision_sequence),
            status=str(observation.disposition),
            reason_codes=(str(observation.reason_code),) if observation.reason_code else (),
        )
        for observation in projection.model_validation
    )


class CapitalSupervisor:
    """Run explicit local fixtures through an injected offline paper core."""

    def __init__(self, *, core_loader: Callable[[], _CapitalCore] = _load_paper_core) -> None:
        self._core_loader = core_loader

    def replay(self, fixture: str | PathLike[str]) -> CapitalReplayResult:
        """Replay a local fixture after loading the optional core fail-closed."""
        core = self._core_loader()
        fixture_path = Path(fixture)
        captured = core.capture_local_fixture(fixture_path)
        admission = core.admit_local_fixture(captured)
        if admission.status != "ADMITTED":
            reasons = _reason_codes(admission.reason_codes)
            detail = ", ".join(reasons) or str(admission.status)
            raise ValueError(f"capital fixture was not admitted: {detail}")

        verified = core.build_model_disabled_inputs(captured, admission)
        if verified.model_registry is not None or verified.model_signal_manifest is not None:
            raise ValueError("capital replay requires model-disabled verified inputs")

        paper_result = core.run_offline_paper_kernel(verified)
        projection_payload = bytes(core.projection_bytes(paper_result))
        closure = paper_result.closure
        projection = paper_result.projection
        return CapitalReplayResult(
            fixture=fixture_path,
            projection_id=sha256(projection_payload).hexdigest(),
            closure_status=str(closure.status),
            closure_reason_codes=_reason_codes(closure.reason_codes),
            receipt_ids=_receipt_ids(projection),
            evidence=_evidence(projection, closure),
            model_observations=_model_observations(projection),
            closure_id=str(closure.closure_id),
        )


__all__ = ["CapitalSupervisor", "MissingCapitalDependencyError"]
