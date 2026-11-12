"""Offline-only local fixture admission for crypto replay source evidence."""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from dataclasses import dataclass

from build_finance.crypto_replay.canonical import JsonValue, canonical_record_bytes
from build_finance.crypto_replay.content_ids import seal_content_id
from build_finance.crypto_replay.contract_semantics import (
    derive_source_admission_status,
    normalize_source_admission_reason_codes,
)
from build_finance.crypto_replay.jupiter_fixture import (
    PARSER_VERSION,
    ParsedJupiterFixture,
    current_parser_identity,
    parse_jupiter_fixture_payload,
)
from build_finance.crypto_replay.local_fixture import CapturedFile, CapturedFixture, CapturedTerm
from build_finance.crypto_replay.schema_model import ValidationIssue

_PROFILE_VENUE = "solana-jupiter-fixture/v1"
_SOURCE_KIND = "solana-jupiter-quote"
_RIGHTS_ROLE = "offline_research_replay"
_MAX_U64 = 18_446_744_073_709_551_615
_MAX_U64_TEXT = str(_MAX_U64)
_REQUIRED_MANIFEST_VALUES = {
    "schema": "trading.fixture-manifest/v1",
    "network": "solana-mainnet",
    "venue_profile": _PROFILE_VENUE,
    "universe_policy": "FULL_DECLARED_SOURCE_UNIVERSE",
    "point_in_time_mode": "PROVEN_AVAILABILITY_SLOT",
}
_REQUIRED_TRUE_MANIFEST_FIELDS = (
    "selection_failures_retained",
    "no_route_observations_retained",
    "inactive_assets_retained",
    "gaps_retained",
)
_SOURCE_REASON_CODES = frozenset(
    {
        "ADMISSION_NOT_LOCAL",
        "ADMISSION_RIGHTS_MISSING",
        "ADMISSION_MANIFEST_MISMATCH",
        "ADMISSION_HASH_MISMATCH",
        "ADMISSION_POINT_IN_TIME_MISSING",
        "ADMISSION_LEAKAGE_FIELD",
        "ADMISSION_UNIVERSE_BIASED",
        "ADMISSION_PROFILE_MISMATCH",
        "ADMISSION_POSITION_CONFLICT",
        "ADMISSION_SEQUENCE_INVALID",
        "ADMISSION_REVISION_CAUSALITY",
        "ADMISSION_REVISION_FORK",
        "ADMISSION_SET_NOT_CLOSED",
    }
)


@dataclass(frozen=True, slots=True)
class AdmissionBatch:
    """Deterministic admission result for one captured local fixture set."""

    status: str
    reason_codes: tuple[str, ...]
    source_receipt_records: tuple[bytes, ...]
    candidates: tuple[ParsedSourceCandidate, ...]


@dataclass(frozen=True, slots=True)
class ParsedSourceCandidate:
    """Non-authoritative parsed source candidate retained only in memory."""

    admission_sequence: str
    relative_path: str
    raw_payload_sha256: str
    source_id: str | None
    source_kind: str | None
    source_revision: str | None
    market_id: str | None
    source_position: Mapping[str, JsonValue] | None
    revision: Mapping[str, JsonValue] | None
    event_time: str | None
    observed_at: str
    ingested_at: str


@dataclass(frozen=True, slots=True)
class _RightsEvidence:
    terms_sha256: str | None
    rights_role: str | None
    rights_effective_date: str | None
    rights_review_date: str | None
    reason_codes: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class _UniverseEvidence:
    allowed_markets: Mapping[str, Mapping[str, JsonValue]]
    reason_codes: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class _CandidateRow:
    index: int
    captured_file: CapturedFile
    parsed: ParsedJupiterFixture


def admit_local_fixture(captured: CapturedFixture) -> AdmissionBatch:
    """Admit or reject an explicitly captured fixture without external effects."""
    parser_identity = current_parser_identity()
    fixture_reasons = _fixture_reason_codes(captured)
    rights = _rights_evidence(captured)
    universe = _universe_evidence(captured.manifest)
    batch_reasons: list[str] = []
    records: list[bytes] = []
    candidate_rows: list[_CandidateRow] = []
    captured_files = tuple(sorted(captured.files, key=_captured_file_sort_key))
    parsed_files = tuple(_parse_or_none(captured_file) for captured_file in captured_files)
    local_reasons_by_index: list[tuple[str, ...]] = []

    for index, captured_file in enumerate(captured_files):
        parsed = parsed_files[index]
        file_reasons = _file_reason_codes(captured_file)
        profile_reasons = () if file_reasons else _profile_reason_codes(captured_file, parsed, universe)
        revision_reasons = (
            () if file_reasons or profile_reasons or parsed is None else _revision_reason_codes(captured_file, parsed)
        )
        local_reasons_by_index.append(_normalize((*file_reasons, *profile_reasons, *revision_reasons)))

    conflict_reasons_by_index = _lineage_conflict_reasons(captured_files, parsed_files, tuple(local_reasons_by_index))

    for index, captured_file in enumerate(captured_files):
        parsed = parsed_files[index]
        local_reasons = local_reasons_by_index[index]
        raw_reasons = (
            *fixture_reasons,
            *rights.reason_codes,
            *universe.reason_codes,
            *local_reasons,
            *conflict_reasons_by_index.get(index, ()),
        )
        reason_codes = _normalize(_drop_path_set_closure_if_manifest_path_failed(captured, raw_reasons))
        record = _source_receipt_record(
            captured,
            captured_file,
            parsed,
            rights,
            reason_codes=reason_codes,
            parser_code_sha256=parser_identity.code_sha256,
        )
        records.append(record)
        batch_reasons.extend(reason_codes)
        if not reason_codes and parsed is not None:
            candidate_rows.append(_CandidateRow(index=index, captured_file=captured_file, parsed=parsed))

    if not records:
        batch_reasons.extend((*fixture_reasons, *rights.reason_codes, *universe.reason_codes))
    reason_codes = _normalize(batch_reasons)
    return AdmissionBatch(
        status=derive_source_admission_status(reason_codes),
        reason_codes=reason_codes,
        source_receipt_records=tuple(records),
        candidates=_collapse_candidates(candidate_rows) if not reason_codes else (),
    )


def _source_receipt_record(
    captured: CapturedFixture,
    captured_file: CapturedFile,
    parsed: ParsedJupiterFixture | None,
    rights: _RightsEvidence,
    *,
    reason_codes: tuple[str, ...],
    parser_code_sha256: str,
) -> bytes:
    document = {
        "schema": "trading.source-admission-receipt/v1",
        "status": derive_source_admission_status(reason_codes),
        "reason_codes": list(reason_codes),
        "fixture_manifest_sha256": captured.fixture_manifest_sha256,
        "raw_payload_sha256": captured_file.sha256,
        "terms_sha256": rights.terms_sha256,
        "parser_code_sha256": parser_code_sha256,
        "source_id": _coalesce(parsed.source_id if parsed is not None else None, captured_file.source_id),
        "source_kind": _coalesce(parsed.source_kind if parsed is not None else None, captured_file.source_kind),
        "source_revision": _coalesce(
            parsed.source_revision if parsed is not None else None, captured_file.source_revision
        ),
        "market_id": _coalesce(parsed.market_id if parsed is not None else None, captured_file.market_id),
        "relative_path": captured_file.relative_path or None,
        "media_type": captured_file.media_type,
        "byte_length": None if captured_file.byte_length is None else str(captured_file.byte_length),
        "admission_sequence": captured_file.admission_sequence or "0",
        "availability_slot": captured_file.availability_slot,
        "observed_at": captured_file.observed_at,
        "ingested_at": captured_file.ingested_at,
        "rights_role": rights.rights_role,
        "rights_effective_date": rights.rights_effective_date,
        "rights_review_date": rights.rights_review_date,
        "parser_version": PARSER_VERSION,
    }
    return canonical_record_bytes(seal_content_id(document))


def _fixture_reason_codes(captured: CapturedFixture) -> tuple[str, ...]:
    return _normalize(_issue_codes(captured.capture_issues))


def _file_reason_codes(captured_file: CapturedFile) -> tuple[str, ...]:
    reasons = list(_issue_codes(captured_file.issues))
    if "ADMISSION_HASH_MISMATCH" in reasons:
        reasons.append("ADMISSION_SET_NOT_CLOSED")
    if captured_file.sha256 is None or captured_file.byte_length is None:
        reasons.append("ADMISSION_NOT_LOCAL")
    return _normalize(reasons)


def _rights_evidence(captured: CapturedFixture) -> _RightsEvidence:
    rights = captured.rights_manifest if isinstance(captured.rights_manifest, Mapping) else {}
    terms = _selected_terms(captured.terms)
    terms_sha256 = terms.sha256 if terms is not None and terms.sha256 is not None else None
    rights_role = _string_or_none(rights.get("rights_role"))
    rights_effective_date = _string_or_none(rights.get("rights_effective_date"))
    rights_review_date = _string_or_none(rights.get("rights_review_date"))
    reasons = list(_issue_codes(captured.capture_issues, only=("ADMISSION_RIGHTS_MISSING",)))
    if terms is None or terms_sha256 is None:
        reasons.append("ADMISSION_RIGHTS_MISSING")
    if rights_role != _RIGHTS_ROLE or rights_effective_date is None or rights_review_date is None:
        reasons.append("ADMISSION_RIGHTS_MISSING")
    return _RightsEvidence(
        terms_sha256=terms_sha256,
        rights_role=rights_role,
        rights_effective_date=rights_effective_date,
        rights_review_date=rights_review_date,
        reason_codes=_normalize(reasons),
    )


def _selected_terms(terms: tuple[CapturedTerm, ...]) -> CapturedTerm | None:
    if not terms:
        return None
    with_payload = tuple(term for term in terms if term.sha256 is not None)
    if with_payload:
        return sorted(with_payload, key=lambda term: (term.relative_path or "", term.sha256 or ""))[0]
    return terms[0]


def _universe_evidence(manifest: Mapping[str, JsonValue] | None) -> _UniverseEvidence:
    if not isinstance(manifest, Mapping):
        return _UniverseEvidence(allowed_markets={}, reason_codes=())
    allowed_markets_value = manifest.get("allowed_markets")
    allowed_markets: dict[str, Mapping[str, JsonValue]] = {}
    reasons: list[str] = []
    base_mints: dict[str, set[str]] = {}
    if any(manifest.get(field) != expected for field, expected in _REQUIRED_MANIFEST_VALUES.items()):
        reasons.extend(("ADMISSION_UNIVERSE_BIASED", "ADMISSION_SET_NOT_CLOSED"))
    if any(manifest.get(field) is not True for field in _REQUIRED_TRUE_MANIFEST_FIELDS):
        reasons.extend(("ADMISSION_UNIVERSE_BIASED", "ADMISSION_SET_NOT_CLOSED"))

    if not isinstance(allowed_markets_value, list) or not allowed_markets_value:
        reasons.extend(("ADMISSION_UNIVERSE_BIASED", "ADMISSION_SET_NOT_CLOSED"))
    else:
        for row in allowed_markets_value:
            if not isinstance(row, Mapping):
                reasons.extend(("ADMISSION_UNIVERSE_BIASED", "ADMISSION_SET_NOT_CLOSED"))
                continue
            market_id = _string_or_none(row.get("market_id"))
            base_mint = _string_or_none(row.get("base_mint"))
            quote_mint = _string_or_none(row.get("quote_mint"))
            base_decimals = _int_or_none(row.get("base_decimals"))
            quote_decimals = _int_or_none(row.get("quote_decimals"))
            if (
                market_id is None
                or base_mint is None
                or quote_mint is None
                or base_decimals is None
                or quote_decimals is None
            ):
                reasons.extend(("ADMISSION_UNIVERSE_BIASED", "ADMISSION_SET_NOT_CLOSED"))
                continue
            if market_id in allowed_markets:
                reasons.extend(("ADMISSION_UNIVERSE_BIASED", "ADMISSION_SET_NOT_CLOSED"))
            allowed_markets[market_id] = row
            base_mints.setdefault(base_mint, set()).add(quote_mint)
    if any(len(quotes) > 1 for quotes in base_mints.values()):
        reasons.extend(("ADMISSION_UNIVERSE_BIASED", "ADMISSION_SET_NOT_CLOSED"))
    return _UniverseEvidence(allowed_markets=allowed_markets, reason_codes=_normalize(reasons))


def _parse_or_none(captured_file: CapturedFile) -> ParsedJupiterFixture | None:
    if captured_file.payload is None:
        return None
    try:
        return parse_jupiter_fixture_payload(captured_file.payload)
    except ValueError:
        return None


def _revision_reason_codes(captured_file: CapturedFile, parsed: ParsedJupiterFixture) -> tuple[str, ...]:
    reasons: list[str] = []
    kind = parsed.revision_kind
    supersedes = parsed.supersedes_event_id
    retracts = parsed.retracts_event_id

    if (
        captured_file.availability_slot is not None
        and parsed.revision_availability_slot != captured_file.availability_slot
    ):
        reasons.append("ADMISSION_REVISION_CAUSALITY")
    if kind == "ORIGINAL":
        if supersedes is not None or retracts is not None:
            reasons.append("ADMISSION_REVISION_CAUSALITY")
        if parsed.revision_availability_slot != parsed.source_position_slot:
            reasons.append("ADMISSION_REVISION_CAUSALITY")
    elif kind == "CORRECTION":
        if not _is_content_id(supersedes) or retracts is not None:
            reasons.append("ADMISSION_REVISION_CAUSALITY")
    elif kind == "RETRACTION":
        if not _is_content_id(retracts) or supersedes is not None:
            reasons.append("ADMISSION_REVISION_CAUSALITY")
    else:
        reasons.append("ADMISSION_REVISION_CAUSALITY")
    return _normalize(reasons)


def _lineage_conflict_reasons(
    captured_files: tuple[CapturedFile, ...],
    parsed_files: tuple[ParsedJupiterFixture | None, ...],
    local_reasons_by_index: tuple[tuple[str, ...], ...],
) -> dict[int, tuple[str, ...]]:
    rows = tuple(
        _CandidateRow(index=index, captured_file=captured_file, parsed=parsed)
        for index, (captured_file, parsed, local_reasons) in enumerate(
            zip(captured_files, parsed_files, local_reasons_by_index, strict=True)
        )
        if parsed is not None and not local_reasons and captured_file.sha256 is not None
    )
    reasons_by_index: dict[int, list[str]] = {}
    _mark_same_version_byte_conflicts(rows, reasons_by_index)
    _mark_original_lineage_conflicts(rows, reasons_by_index)
    _mark_revision_forks(rows, reasons_by_index)
    return {index: _normalize(reasons) for index, reasons in reasons_by_index.items()}


def _mark_same_version_byte_conflicts(
    rows: tuple[_CandidateRow, ...],
    reasons_by_index: dict[int, list[str]],
) -> None:
    by_version: dict[tuple[object, ...], list[_CandidateRow]] = {}
    for row in rows:
        by_version.setdefault(_version_key(row.parsed), []).append(row)
    for version_rows in by_version.values():
        raw_hashes = {row.captured_file.sha256 for row in version_rows}
        if len(raw_hashes) > 1:
            _append_reason(version_rows, reasons_by_index, "ADMISSION_POSITION_CONFLICT")


def _mark_original_lineage_conflicts(
    rows: tuple[_CandidateRow, ...],
    reasons_by_index: dict[int, list[str]],
) -> None:
    by_lineage: dict[tuple[object, ...], list[_CandidateRow]] = {}
    for row in rows:
        if row.parsed.revision_kind == "ORIGINAL":
            by_lineage.setdefault(_lineage_key(row.parsed), []).append(row)
    for lineage_rows in by_lineage.values():
        original_versions = {(_version_key(row.parsed), row.captured_file.sha256) for row in lineage_rows}
        if len(original_versions) > 1:
            _append_reason(lineage_rows, reasons_by_index, "ADMISSION_POSITION_CONFLICT")


def _mark_revision_forks(
    rows: tuple[_CandidateRow, ...],
    reasons_by_index: dict[int, list[str]],
) -> None:
    by_target: dict[tuple[str, str], list[_CandidateRow]] = {}
    for row in rows:
        fork_key = _revision_fork_key(row.parsed)
        if fork_key is None:
            continue
        by_target.setdefault(fork_key, []).append(row)
    for target_rows in by_target.values():
        version_keys = {_version_key(row.parsed) for row in target_rows}
        if len(version_keys) > 1:
            _append_reason(target_rows, reasons_by_index, "ADMISSION_REVISION_FORK")


def _revision_fork_key(parsed: ParsedJupiterFixture) -> tuple[str, str] | None:
    target = _revision_target(parsed)
    if target is None:
        return None
    return parsed.revision_kind, target


def _append_reason(
    rows: Iterable[_CandidateRow],
    reasons_by_index: dict[int, list[str]],
    reason_code: str,
) -> None:
    for row in rows:
        reasons_by_index.setdefault(row.index, []).append(reason_code)


def _collapse_candidates(rows: Iterable[_CandidateRow]) -> tuple[ParsedSourceCandidate, ...]:
    selected: dict[tuple[object, ...], _CandidateRow] = {}
    for row in rows:
        key = (*_version_key(row.parsed), row.captured_file.sha256 or "")
        previous = selected.get(key)
        if previous is None or _captured_file_sort_key(row.captured_file) < _captured_file_sort_key(
            previous.captured_file
        ):
            selected[key] = row
    return tuple(
        _source_candidate(row)
        for row in sorted(selected.values(), key=lambda row: _captured_file_sort_key(row.captured_file))
    )


def _source_candidate(row: _CandidateRow) -> ParsedSourceCandidate:
    captured_file = row.captured_file
    parsed = row.parsed
    if captured_file.sha256 is None or captured_file.observed_at is None or captured_file.ingested_at is None:
        raise ValueError("admitted source candidate is missing total captured evidence")
    return ParsedSourceCandidate(
        admission_sequence=captured_file.admission_sequence,
        relative_path=captured_file.relative_path,
        raw_payload_sha256=captured_file.sha256,
        source_id=parsed.source_id,
        source_kind=parsed.source_kind,
        source_revision=parsed.source_revision,
        market_id=parsed.market_id,
        source_position=parsed.source_position,
        revision=parsed.revision,
        event_time=parsed.event_time,
        observed_at=captured_file.observed_at,
        ingested_at=captured_file.ingested_at,
    )


def _lineage_key(parsed: ParsedJupiterFixture) -> tuple[object, ...]:
    return (
        parsed.source_id,
        parsed.source_kind,
        parsed.market_id,
        parsed.source_position_slot,
        parsed.source_position_transaction_index,
        parsed.source_position_instruction_index,
        parsed.source_position_event_index,
        parsed.source_native_event_id,
        parsed.source_subsequence,
    )


def _version_key(parsed: ParsedJupiterFixture) -> tuple[object, ...]:
    return (
        *_lineage_key(parsed),
        parsed.revision_kind,
        parsed.revision_availability_slot,
        parsed.revision_availability_admission_sequence,
        parsed.supersedes_event_id or "",
        parsed.retracts_event_id or "",
    )


def _revision_target(parsed: ParsedJupiterFixture) -> str | None:
    if parsed.revision_kind == "CORRECTION":
        return parsed.supersedes_event_id
    if parsed.revision_kind == "RETRACTION":
        return parsed.retracts_event_id
    return None


def _profile_reason_codes(
    captured_file: CapturedFile,
    parsed: ParsedJupiterFixture | None,
    universe: _UniverseEvidence,
) -> tuple[str, ...]:
    reasons: list[str] = []
    if captured_file.media_type != "application/json" or captured_file.source_kind != _SOURCE_KIND:
        reasons.append("ADMISSION_PROFILE_MISMATCH")
    if parsed is None:
        if captured_file.payload is not None:
            reasons.append("ADMISSION_PROFILE_MISMATCH")
        return _normalize(reasons)

    expected_pairs = (
        (parsed.source_id, captured_file.source_id),
        (parsed.source_kind, captured_file.source_kind),
        (parsed.source_revision, captured_file.source_revision),
        (parsed.market_id, captured_file.market_id),
    )
    if any(actual != expected for actual, expected in expected_pairs):
        reasons.append("ADMISSION_PROFILE_MISMATCH")
    if parsed.source_kind != _SOURCE_KIND:
        reasons.append("ADMISSION_PROFILE_MISMATCH")
    if parsed.base_mint == parsed.quote_mint:
        reasons.append("ADMISSION_PROFILE_MISMATCH")
    if not (0 <= parsed.base_decimals <= 18 and 0 <= parsed.quote_decimals <= 18):
        reasons.append("ADMISSION_PROFILE_MISMATCH")

    if not universe.reason_codes:
        allowed = universe.allowed_markets.get(parsed.market_id)
        if allowed is None:
            reasons.append("ADMISSION_PROFILE_MISMATCH")
        else:
            expected_market = (
                _string_or_none(allowed.get("base_mint")),
                _string_or_none(allowed.get("quote_mint")),
                _int_or_none(allowed.get("base_decimals")),
                _int_or_none(allowed.get("quote_decimals")),
            )
            actual_market = (parsed.base_mint, parsed.quote_mint, parsed.base_decimals, parsed.quote_decimals)
            if actual_market != expected_market:
                reasons.append("ADMISSION_PROFILE_MISMATCH")
    return _normalize(reasons)


def _drop_path_set_closure_if_manifest_path_failed(
    captured: CapturedFixture,
    reason_codes: Iterable[str],
) -> tuple[str, ...]:
    codes = tuple(reason_codes)
    if "ADMISSION_MANIFEST_MISMATCH" not in codes or "ADMISSION_SET_NOT_CLOSED" not in codes:
        return codes
    if not any(
        "unsafe fixture relative path" in issue.message or "fixture path escapes" in issue.message
        for issue in captured.capture_issues
    ):
        return codes
    return tuple(code for code in codes if code != "ADMISSION_SET_NOT_CLOSED")


def _issue_codes(
    issues: Iterable[ValidationIssue],
    *,
    only: tuple[str, ...] | None = None,
) -> tuple[str, ...]:
    allowed = _SOURCE_REASON_CODES if only is None else frozenset(only)
    return tuple(issue.code for issue in issues if issue.code in allowed)


def _normalize(reason_codes: Iterable[str]) -> tuple[str, ...]:
    return normalize_source_admission_reason_codes(tuple(dict.fromkeys(reason_codes)))


def _coalesce(primary: str | None, fallback: str | None) -> str | None:
    return primary if primary is not None else fallback


def _captured_file_sort_key(captured_file: CapturedFile) -> tuple[int, bytes]:
    return (_admission_sequence_value(captured_file.admission_sequence), captured_file.relative_path.encode("utf-8"))


def _admission_sequence_value(value: str) -> int:
    if _is_canonical_u64_string(value):
        return int(value)
    return 0


def _is_canonical_u64_string(value: object) -> bool:
    if not isinstance(value, str):
        return False
    if value == "0":
        return True
    if not value or not value.isascii() or not value.isdecimal() or value.startswith("0"):
        return False
    return len(value) < len(_MAX_U64_TEXT) or (len(value) == len(_MAX_U64_TEXT) and value <= _MAX_U64_TEXT)


def _is_content_id(value: str | None) -> bool:
    return value is not None and len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def _string_or_none(value: JsonValue | object) -> str | None:
    return value if isinstance(value, str) else None


def _int_or_none(value: JsonValue | object) -> int | None:
    if isinstance(value, bool):
        return None
    return value if isinstance(value, int) else None
