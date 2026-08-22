"""Fail-closed orchestration for model-disabled offline capital replay."""

from __future__ import annotations

from collections.abc import Callable, Sequence
from dataclasses import dataclass
from hashlib import sha256
from importlib import import_module
from os import PathLike
from pathlib import Path
from typing import NoReturn, Protocol, cast

from build_engine.capital.contracts import CapitalReplayResult, EvidenceSummary, ModelObservation

_INSTALL_INSTRUCTION = "install with: pip install 'build-engine[capital]'"


class MissingCapitalDependencyError(RuntimeError):
    """Raised before fixture access when the optional paper core is unavailable."""


class _CapitalCore(Protocol):
    profiles_type: type[object]

    def capture_local_fixture(self, fixture: Path) -> object: ...

    def admit_local_fixture(self, captured: object) -> object: ...

    def build_verified_run_inputs(
        self,
        run_receipt_record: bytes,
        admitted_candidates: object,
        source_receipt_records: object,
        resolver: object,
        profiles: object,
    ) -> object: ...

    def run_offline_paper_kernel(
        self,
        verified: object,
        resolver: object,
        profiles: object,
    ) -> object: ...

    def kernel_result_canonical_bytes(self, result: object) -> bytes: ...


class _RunContext(Protocol):
    run_receipt_record: bytes
    resolver: object
    profiles: object


@dataclass(frozen=True, slots=True)
class _PaperCoreAdapter:
    capture_local_fixture: Callable[..., object]
    admit_local_fixture: Callable[..., object]
    build_verified_run_inputs: Callable[..., object]
    run_offline_paper_kernel: Callable[..., object]
    kernel_result_canonical_bytes: Callable[..., bytes]
    profiles_type: type[object]


def _required_callable(module: object, name: str) -> Callable[..., object]:
    value = getattr(module, name, None)
    if not callable(value):
        raise AttributeError(name)
    return cast(Callable[..., object], value)


def _load_paper_core(
    *,
    module_loader: Callable[[str], object] = import_module,
) -> _CapitalCore:
    """Bind the optional package's reviewed low-level offline exports lazily."""
    try:
        fixture_module = module_loader("build_finance.crypto_replay.local_fixture")
        admission_module = module_loader("build_finance.crypto_replay.admission")
        paper_module = module_loader("build_finance.live_paper")
        profiles_type = paper_module.PaperKernelProfiles
        if not isinstance(profiles_type, type):
            raise TypeError("PaperKernelProfiles must be a type")
        return _PaperCoreAdapter(
            capture_local_fixture=_required_callable(fixture_module, "capture_local_fixture"),
            admit_local_fixture=_required_callable(admission_module, "admit_local_fixture"),
            build_verified_run_inputs=_required_callable(paper_module, "build_verified_run_inputs"),
            run_offline_paper_kernel=_required_callable(paper_module, "run_offline_paper_kernel"),
            kernel_result_canonical_bytes=cast(
                Callable[..., bytes],
                _required_callable(paper_module, "kernel_result_canonical_bytes"),
            ),
            profiles_type=profiles_type,
        )
    except (AttributeError, ImportError, TypeError) as error:
        raise MissingCapitalDependencyError(_INSTALL_INSTRUCTION) from error


def _missing_run_context(fixture: Path, captured: object, admission: object) -> NoReturn:
    raise ValueError(
        "admitted fixture is missing a run-level replay envelope; "
        "configure a run_context_loader with the rooted run receipt, resolver, and paper profiles"
    )


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

    def __init__(
        self,
        *,
        core_loader: Callable[[], _CapitalCore] = _load_paper_core,
        run_context_loader: Callable[[Path, object, object], _RunContext] = _missing_run_context,
    ) -> None:
        self._core_loader = core_loader
        self._run_context_loader = run_context_loader

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

        context = self._run_context_loader(fixture_path, captured, admission)
        verified = core.build_verified_run_inputs(
            bytes(context.run_receipt_record),
            admission.candidates,
            admission.source_receipt_records,
            context.resolver,
            context.profiles,
        )
        bundle = verified.bundle
        if bundle.model_registry is not None or bundle.model_signal_manifest is not None:
            raise ValueError("capital replay requires model-disabled verified inputs")

        paper_result = core.run_offline_paper_kernel(
            verified,
            context.resolver,
            context.profiles,
        )
        projection_payload = bytes(core.kernel_result_canonical_bytes(paper_result))
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
