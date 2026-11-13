from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from types import SimpleNamespace

import pytest

from build_engine.capital import CapitalSupervisor, MissingCapitalDependencyError
from build_engine.capital.supervisor import _load_paper_core


@dataclass(frozen=True)
class FakeVerifiedInputs:
    bundle: object


class FakeProfiles:
    pass


class FakePaperCore:
    def __init__(self) -> None:
        self.calls: list[tuple[object, ...]] = []

    def capture_local_fixture(self, root: Path) -> object:
        self.calls.append(("capture", root))
        return "captured"

    def admit_local_fixture(self, captured: object) -> object:
        self.calls.append(("admit", captured))
        return SimpleNamespace(
            status="ADMITTED",
            reason_codes=(),
            candidates=("candidate",),
            source_receipt_records=(b"source-receipt",),
        )

    def load_replay_envelope_context(
        self,
        fixture_root: Path,
        captured: object,
        admission: object,
    ) -> object:
        self.calls.append(("disk-context", fixture_root, captured, admission))
        return SimpleNamespace(
            run_receipt_record=b"run-receipt",
            resolver="resolver",
            profiles=FakeProfiles(),
        )

    def build_verified_run_inputs(
        self,
        run_receipt_record: bytes,
        admitted_candidates: object,
        source_receipt_records: object,
        resolver: object,
        profiles: object,
    ) -> FakeVerifiedInputs:
        self.calls.append(
            (
                "verify",
                run_receipt_record,
                admitted_candidates,
                source_receipt_records,
                resolver,
                profiles,
            )
        )
        return FakeVerifiedInputs(
            bundle=SimpleNamespace(model_registry=None, model_signal_manifest=None)
        )

    def run_offline_paper_kernel(
        self,
        verified: FakeVerifiedInputs,
        resolver: object,
        profiles: object,
    ) -> object:
        self.calls.append(("kernel", verified, resolver, profiles))
        return SimpleNamespace(
            closure=SimpleNamespace(
                status="CLOSED",
                reason_codes=(),
                closure_id="closure-1",
                ledger_head_id="ledger-1",
                final_portfolio_state_id="portfolio-1",
            ),
            projection=SimpleNamespace(
                receipt_ids=SimpleNamespace(
                    run_receipt_id="run-1",
                    reconciliation_receipt_ids=("reconciliation-1",),
                    ledger_head_id="ledger-1",
                    final_portfolio_state_id="portfolio-1",
                    closure_id="closure-1",
                ),
                model_validation=(
                    SimpleNamespace(
                        decision_sequence="1",
                        disposition="ABSTAIN",
                        reason_code="MODEL_DISABLED",
                    ),
                ),
                decisions=(),
                fills=(),
            ),
        )

    def kernel_result_canonical_bytes(self, result: object) -> bytes:
        self.calls.append(("projection", result))
        return b"model-disabled-paper-projection"


def _run_context_loader(fake: FakePaperCore) -> Callable[[Path, object, object], object]:
    def load(fixture: Path, captured: object, admission: object) -> object:
        fake.calls.append(("context", fixture, captured, admission))
        return SimpleNamespace(
            run_receipt_record=b"run-receipt",
            resolver="resolver",
            profiles=FakeProfiles(),
        )

    return load


def _paper_core_modules(
    fake: FakePaperCore,
    *,
    version: str = "1.1.0",
    override_name: str | None = None,
    override: object | None = None,
) -> dict[str, object]:
    exports: dict[str, object] = {
        "capture_local_fixture": fake.capture_local_fixture,
        "admit_local_fixture": fake.admit_local_fixture,
        "load_replay_envelope_context": fake.load_replay_envelope_context,
        "build_verified_run_inputs": fake.build_verified_run_inputs,
        "run_offline_paper_kernel": fake.run_offline_paper_kernel,
        "kernel_result_canonical_bytes": fake.kernel_result_canonical_bytes,
    }
    if override_name is not None:
        exports[override_name] = override
    return {
        "build_finance": SimpleNamespace(__version__=version),
        "build_finance.crypto_replay.local_fixture": SimpleNamespace(
            capture_local_fixture=exports["capture_local_fixture"],
        ),
        "build_finance.crypto_replay.admission": SimpleNamespace(
            admit_local_fixture=exports["admit_local_fixture"],
        ),
        "build_finance.paper_core_loader": SimpleNamespace(
            load_replay_envelope_context=exports["load_replay_envelope_context"],
        ),
        "build_finance.live_paper": SimpleNamespace(
            build_verified_run_inputs=exports["build_verified_run_inputs"],
            run_offline_paper_kernel=exports["run_offline_paper_kernel"],
            kernel_result_canonical_bytes=exports["kernel_result_canonical_bytes"],
            PaperKernelProfiles=FakeProfiles,
        ),
    }


def test_supervisor_uses_actual_low_level_paper_core_signatures(tmp_path: Path) -> None:
    fake = FakePaperCore()
    fixture = tmp_path / "fixture"
    result = CapitalSupervisor(
        core_loader=lambda: fake,
        run_context_loader=_run_context_loader(fake),
    ).replay(fixture)

    assert [call[0] for call in fake.calls] == [
        "capture",
        "admit",
        "context",
        "verify",
        "kernel",
        "projection",
    ]
    verify_call = fake.calls[3]
    profiles = verify_call[-1]
    assert verify_call[1:] == (
        b"run-receipt",
        ("candidate",),
        (b"source-receipt",),
        "resolver",
        profiles,
    )
    verified = fake.calls[4][1]
    assert verified.bundle.model_registry is None
    assert verified.bundle.model_signal_manifest is None
    assert fake.calls[4][2:] == ("resolver", profiles)
    assert result.fixture == fixture
    assert result.closure_status == "CLOSED"
    assert result.receipt_ids == (
        "run-1",
        "reconciliation-1",
        "ledger-1",
        "portfolio-1",
        "closure-1",
    )
    assert result.model_observations[0].status == "ABSTAIN"
    assert result.model_observations[0].reason_codes == ("MODEL_DISABLED",)


def test_low_level_loader_binds_current_paper_core_exports(tmp_path: Path) -> None:
    fake = FakePaperCore()
    imported: list[str] = []
    modules = _paper_core_modules(fake)

    def module_loader(name: str) -> object:
        imported.append(name)
        return modules[name]

    core = _load_paper_core(module_loader=module_loader)
    CapitalSupervisor(core_loader=lambda: core).replay(tmp_path / "fixture")

    assert imported == [
        "build_finance",
        "build_finance.crypto_replay.local_fixture",
        "build_finance.crypto_replay.admission",
        "build_finance.paper_core_loader",
        "build_finance.live_paper",
    ]
    assert [call[0] for call in fake.calls] == [
        "capture",
        "admit",
        "disk-context",
        "verify",
        "kernel",
        "projection",
    ]


def test_installed_paper_core_exports_are_accepted_when_available() -> None:
    pytest.importorskip("build_finance.live_paper")

    core = _load_paper_core()

    assert core.profiles_type.__name__ == "PaperKernelProfiles"


def test_supervisor_uses_paper_core_disk_loader_when_no_context_is_injected(tmp_path: Path) -> None:
    fake = FakePaperCore()

    result = CapitalSupervisor(core_loader=lambda: fake).replay(tmp_path / "fixture")

    assert [call[0] for call in fake.calls] == [
        "capture",
        "admit",
        "disk-context",
        "verify",
        "kernel",
        "projection",
    ]
    assert result.closure_status == "CLOSED"


def test_incompatible_paper_core_fails_before_fixture_access() -> None:
    fake = FakePaperCore()
    fixture_touched = False
    modules = _paper_core_modules(
        fake,
        override_name="load_replay_envelope_context",
        override=None,
    )

    class FixtureProbe:
        def __fspath__(self) -> str:
            nonlocal fixture_touched
            fixture_touched = True
            return "must-not-be-touched"

    def module_loader(name: str) -> object:
        return modules[name]

    with pytest.raises(MissingCapitalDependencyError, match=r"build-engine\[capital\]"):
        CapitalSupervisor(core_loader=lambda: _load_paper_core(module_loader=module_loader)).replay(
            FixtureProbe()
        )

    assert fixture_touched is False


@pytest.mark.parametrize(
    "name",
    [
        "capture_local_fixture",
        "admit_local_fixture",
        "load_replay_envelope_context",
        "build_verified_run_inputs",
        "run_offline_paper_kernel",
        "kernel_result_canonical_bytes",
    ],
)
def test_signature_incompatible_paper_core_fails_before_fixture_access(name: str) -> None:
    fake = FakePaperCore()
    fixture_touched = False

    class FixtureProbe:
        def __fspath__(self) -> str:
            nonlocal fixture_touched
            fixture_touched = True
            return "must-not-be-touched"

    def zero_argument_export() -> object:
        return object()

    modules = _paper_core_modules(
        fake,
        override_name=name,
        override=zero_argument_export,
    )

    with pytest.raises(MissingCapitalDependencyError, match=r"build-engine\[capital\]"):
        CapitalSupervisor(core_loader=lambda: _load_paper_core(module_loader=modules.__getitem__)).replay(
            FixtureProbe()
        )

    assert fixture_touched is False
    assert fake.calls == []


@pytest.mark.parametrize("version", ["1.0.9", "1.2.0"])
def test_version_incompatible_paper_core_fails_before_fixture_access(version: str) -> None:
    fake = FakePaperCore()
    fixture_touched = False

    class FixtureProbe:
        def __fspath__(self) -> str:
            nonlocal fixture_touched
            fixture_touched = True
            return "must-not-be-touched"

    modules = _paper_core_modules(fake, version=version)

    with pytest.raises(MissingCapitalDependencyError, match=r"build-engine\[capital\]"):
        CapitalSupervisor(core_loader=lambda: _load_paper_core(module_loader=modules.__getitem__)).replay(
            FixtureProbe()
        )

    assert fixture_touched is False
    assert fake.calls == []


def test_falsey_injected_context_loader_takes_precedence_over_disk_loader(tmp_path: Path) -> None:
    fake = FakePaperCore()

    class FalseyContextLoader:
        def __bool__(self) -> bool:
            return False

        def __call__(self, fixture: Path, captured: object, admission: object) -> object:
            fake.calls.append(("falsey-context", fixture, captured, admission))
            return SimpleNamespace(
                run_receipt_record=b"run-receipt",
                resolver="resolver",
                profiles=FakeProfiles(),
            )

    result = CapitalSupervisor(
        core_loader=lambda: fake,
        run_context_loader=FalseyContextLoader(),
    ).replay(tmp_path / "fixture")

    assert [call[0] for call in fake.calls] == [
        "capture",
        "admit",
        "falsey-context",
        "verify",
        "kernel",
        "projection",
    ]
    assert result.closure_status == "CLOSED"


def test_supervisor_fails_closed_on_rejected_admission(tmp_path: Path) -> None:
    fake = FakePaperCore()
    fake.admit_local_fixture = lambda captured: SimpleNamespace(  # type: ignore[method-assign]
        status="QUARANTINED",
        reason_codes=("ADMISSION_NOT_LOCAL",),
    )

    with pytest.raises(ValueError, match="ADMISSION_NOT_LOCAL"):
        CapitalSupervisor(core_loader=lambda: fake).replay(tmp_path / "fixture")

    assert all(call[0] not in {"verify", "kernel"} for call in fake.calls)


def test_supervisor_rejects_verified_inputs_with_model_evidence(tmp_path: Path) -> None:
    fake = FakePaperCore()
    fake.build_verified_run_inputs = lambda *args: FakeVerifiedInputs(  # type: ignore[method-assign]
        bundle=SimpleNamespace(model_registry=object(), model_signal_manifest=None)
    )

    with pytest.raises(ValueError, match="model-disabled"):
        CapitalSupervisor(
            core_loader=lambda: fake,
            run_context_loader=_run_context_loader(fake),
        ).replay(tmp_path / "fixture")

    assert all(call[0] != "kernel" for call in fake.calls)


def test_supervisor_rejects_non_abstain_model_projection(tmp_path: Path) -> None:
    fake = FakePaperCore()
    run_kernel = fake.run_offline_paper_kernel

    def run_with_model_action(verified: object, resolver: object, profiles: object) -> object:
        result = run_kernel(verified, resolver, profiles)  # type: ignore[arg-type]
        result.projection.model_validation = (
            SimpleNamespace(
                decision_sequence="1",
                disposition="ACCEPT",
                reason_code="MODEL_ENABLED",
            ),
        )
        return result

    fake.run_offline_paper_kernel = run_with_model_action  # type: ignore[method-assign]

    with pytest.raises(ValueError, match="ABSTAIN / MODEL_DISABLED"):
        CapitalSupervisor(
            core_loader=lambda: fake,
            run_context_loader=_run_context_loader(fake),
        ).replay(tmp_path / "fixture")

    assert all(call[0] != "projection" for call in fake.calls)


def test_supervisor_rejects_missing_model_abstain_projection(tmp_path: Path) -> None:
    fake = FakePaperCore()
    run_kernel = fake.run_offline_paper_kernel

    def run_without_model_evidence(verified: object, resolver: object, profiles: object) -> object:
        result = run_kernel(verified, resolver, profiles)  # type: ignore[arg-type]
        result.projection.model_validation = ()
        return result

    fake.run_offline_paper_kernel = run_without_model_evidence  # type: ignore[method-assign]

    with pytest.raises(ValueError, match="ABSTAIN / MODEL_DISABLED"):
        CapitalSupervisor(
            core_loader=lambda: fake,
            run_context_loader=_run_context_loader(fake),
        ).replay(tmp_path / "fixture")

    assert all(call[0] != "projection" for call in fake.calls)
