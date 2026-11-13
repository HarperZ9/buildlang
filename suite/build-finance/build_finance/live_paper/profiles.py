"""Closed immutable profile carrier for the offline paper kernel."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class PaperKernelProfiles:
    """Snapshot the exact profile bytes supplied to deterministic consumers."""

    normalization_profile_record: bytes
    feature_profile_record: bytes
    algorithm_profile_records: tuple[bytes, ...]
    model_validation_profile_record: bytes
    fusion_profile_record: bytes
    risk_config_record: bytes
    fill_profile_record: bytes

    def __post_init__(self) -> None:
        object.__setattr__(self, "normalization_profile_record", bytes(self.normalization_profile_record))
        object.__setattr__(self, "feature_profile_record", bytes(self.feature_profile_record))
        object.__setattr__(
            self,
            "algorithm_profile_records",
            tuple(bytes(record) for record in self.algorithm_profile_records),
        )
        object.__setattr__(self, "model_validation_profile_record", bytes(self.model_validation_profile_record))
        object.__setattr__(self, "fusion_profile_record", bytes(self.fusion_profile_record))
        object.__setattr__(self, "risk_config_record", bytes(self.risk_config_record))
        object.__setattr__(self, "fill_profile_record", bytes(self.fill_profile_record))
