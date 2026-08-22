from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from types import SimpleNamespace

from build_engine.capital import CapitalSupervisor


@dataclass(frozen=True)
class FakeVerifiedInputs:
    model_registry: None
    model_signal_manifest: None


class FakePaperCore:
    def __init__(self) -> None:
        self.calls: list[tuple[object, ...]] = []

    def capture_local_fixture(self, fixture: Path) -> object:
        self.calls.append(("capture", fixture))
        return "captured"

    def admit_local_fixture(self, captured: object) -> object:
        self.calls.append(("admit", captured))
        return SimpleNamespace(status="ADMITTED", reason_codes=())

    def build_model_disabled_inputs(self, captured: object, admission: object) -> FakeVerifiedInputs:
        self.calls.append(("verify", captured, admission))
        return FakeVerifiedInputs(model_registry=None, model_signal_manifest=None)

    def run_offline_paper_kernel(self, verified: FakeVerifiedInputs) -> object:
        self.calls.append(("kernel", verified))
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

    def projection_bytes(self, result: object) -> bytes:
        self.calls.append(("projection", result))
        return b"model-disabled-paper-projection"


def test_supervisor_captures_admits_verifies_and_evaluates_model_disabled_fixture(tmp_path: Path) -> None:
    fake = FakePaperCore()
    fixture = tmp_path / "fixture"
    result = CapitalSupervisor(core_loader=lambda: fake).replay(fixture)

    assert [call[0] for call in fake.calls] == ["capture", "admit", "verify", "kernel", "projection"]
    verified = fake.calls[3][1]
    assert verified.model_registry is None
    assert verified.model_signal_manifest is None
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


def test_supervisor_fails_closed_on_rejected_admission(tmp_path: Path) -> None:
    fake = FakePaperCore()
    fake.admit_local_fixture = lambda captured: SimpleNamespace(  # type: ignore[method-assign]
        status="QUARANTINED",
        reason_codes=("ADMISSION_NOT_LOCAL",),
    )

    try:
        CapitalSupervisor(core_loader=lambda: fake).replay(tmp_path / "fixture")
    except ValueError as error:
        assert "ADMISSION_NOT_LOCAL" in str(error)
    else:
        raise AssertionError("quarantined fixture must not reach verification")

    assert all(call[0] not in {"verify", "kernel"} for call in fake.calls)


def test_supervisor_rejects_verified_inputs_with_model_evidence(tmp_path: Path) -> None:
    fake = FakePaperCore()
    fake.build_model_disabled_inputs = lambda captured, admission: SimpleNamespace(  # type: ignore[method-assign]
        model_registry=object(),
        model_signal_manifest=None,
    )

    try:
        CapitalSupervisor(core_loader=lambda: fake).replay(tmp_path / "fixture")
    except ValueError as error:
        assert "model-disabled" in str(error)
    else:
        raise AssertionError("model evidence must not reach the paper kernel")

    assert all(call[0] != "kernel" for call in fake.calls)
