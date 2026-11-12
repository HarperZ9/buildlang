# Crypto Replay Kernel T01-T03 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Add the approved offline-only crypto replay foundation to Build Finance: strict canonical bytes and content IDs, eight primary and thirteen supporting closed contracts, and deterministic local Solana/Jupiter fixture admission, while leaving every live/paper/provider/model/execution capability absent.

**Architecture:** A new build_finance.crypto_replay subpackage forms a capability island. Byte parsing, canonicalization, schema validation, graph validation, and admission are separate stages. Runtime code uses only the Python standard library. Generated JSON Schemas and their locked digests are checked by an independent jsonschema test oracle. T03 reads only explicitly supplied local files and emits deterministic source-admission evidence; it does not normalize replay sequences, infer signals, simulate fills, construct orders, or call any external surface.

**Tech Stack:** Python 3.10+, standard-library runtime, pytest, jsonschema as a test-only oracle, setuptools package data, ruff, mypy, GitHub Actions.

## Global Constraints

- Scope is exactly T01, T02, and T03 from the approved specification. Stop after T03.
- Normative source: C:\dev\project-docs\specs\SPEC-CRYPTO-REPLAY-KERNEL-V0-20260711.md at project-docs commit 1b5f90a.
- Normative source SHA-256: 243e2ab607b010cc2ae3bcc6c669562c40e7cdff28e7fa36f5d72212804f1b39.
- Implement in the isolated worktree C:\dev\worktrees\build-finance-crypto-replay-t01-t03 on branch feat/crypto-replay-t01-t03. Do not implement on Build Finance main.
- Do not inspect .env files, credentials, wallet material, account data, or the previously exposed Jupiter credential. No secret belongs in source, tests, fixtures, logs, evidence, or commits.
- Do not call a provider or network. Do not download a dataset. Do not create or import a broker, wallet, signer, transaction, paper-trading adapter, live adapter, model runtime, or inference endpoint.
- Do not install from a package index during execution. The current environment already has jsonschema 4.26.0 for the independent test oracle; if that verified local dependency disappears, stop rather than fetching it.
- Do not modify build_finance/autotrader.py, build_finance/broker.py, build_finance/market_data.py, or any GUI page.
- Do not add model inference or training. Model mode is DISABLED for the executable v0 path. Static model-contract examples are schema vectors only.
- Do not add an order actuator. T03 ends at local source-admission receipts and non-authoritative parsed candidates; RawEvent sequencing is T04.
- Every authoritative numeric amount remains integer atoms or a declared integer string alias. Floats and Decimal values are rejected before authority validation.
- Persisted objects are strict UTF-8 RFC 8785/JCS bytes followed by exactly one LF. The LF is excluded from all object digests.
- Duplicate keys, BOMs, comments, NaN, Infinity, non-canonical whitespace/escapes/order, invalid UTF-8, extra trailing bytes, unknown properties, and missing required fields fail closed.
- Every primary and supporting self-ID excludes only its designated top-level self-ID field. No helper may silently omit nulls, lineage, schema, or any other field.
- Every generated object schema is recursively closed. additionalProperties is false for the root and every object child.
- Test fixtures are synthetic contract/adversarial vectors. They are never evidence for P2 and must never be described as observed market data.
- P0 remains BLOCKED because fixture/config/run choices are unresolved. P2 remains FAIL: zero admitted real fixture. The T03 engine may be GREEN without changing that external gate.
- A replay-subpackage confinement check may pass, but it does not make the existing whole-repository P5 gate pass while legacy broker/live modules remain elsewhere in Build Finance.
- No package version bump, tag, release, publication, marketing claim, profitability claim, or performance claim is authorized in this plan.
- Run each node RED first, capture the expected failure receipt, commit it, implement the smallest GREEN change, rerun all predecessor nodes, and capture the GREEN receipt.

---

## Design Decisions Frozen by This Plan

### 1. The replay subpackage adds no runtime dependency

Do not add jsonschema to project.dependencies. Add jsonschema>=4.23,<5 only to the test and dev extras. Code under build_finance.crypto_replay imports only the Python standard library. The Build Finance distribution still retains its existing NumPy, pandas, and SciPy requirements; this plan does not claim the whole wheel has zero dependencies. The runtime validator supports the deliberately generated schema subset and has no URI resolver. The independent test oracle loads only local schema dictionaries and rejects every non-local reference.

This avoids giving the new runtime package transitive network functionality while retaining an independent implementation against which generated schemas are checked.

### 2. Canonical JSON is a restricted full-authority profile

canonical_json_bytes accepts only null, bool, int, str, list, and dict[str, JsonValue]. It rejects floats, Decimal, bytes, tuples, non-string keys, and lone UTF-16 surrogates. JSON-number integers are limited to the interoperable range -9007199254740991..9007199254740991; larger protocol authorities are strings. Owning schemas further restrict actual JSON integers to their signed/unsigned 32-bit or smaller contract bounds. Object keys sort by UTF-16 code units as required by RFC 8785. Strings use JSON escaping with UTF-8 output. Integers emit base-10 with no plus sign, leading zero, or negative zero.

parse_canonical_json parses one canonical object payload with no record LF. parse_canonical_record requires exactly one terminal LF and hashes only the payload.

### 3. Schema and semantic validation are separate

Generated JSON Schema proves lexical type, required/closed shape, enums, patterns, bounds, array shape, and child closure. Explicit semantic validators prove cross-field and cross-object rules such as revision tuples, sorted uniqueness, fixture set closure, receipt status/code matrices, content-ID resolution, causation formulas, and run/closure binding.

No schema validator performs file I/O. No graph validator resolves an unknown digest from a provider.

Every generated schema declares draft 2020-12, uses an absolute local URN ID of the form urn:build-finance:contract:<hyphenated-contract-name>:v1, and carries x-contract-schema with the exact trading.*/v1 protocol tag. References are limited to local #/$defs paths or preregistered urn:build-finance: definitions. HTTP(S), file, and unknown URIs are forbidden even when a test oracle could resolve them.

### 4. T01 seals a primary bundle; T02 seals the final schema bundle

T01 must not emit a partial final schema bundle. It creates:

- resources/primary-schema-bundle.json
- resources/primary-schema-bundle.sha256

The primary bundle has schema tag build-finance.primary-schema-bundle/v1 and exactly jcs_profile, protocol_constants, aliases, and schemas in addition to schema. Its schemas array contains exactly the eight primary rows.

T02 creates the final non-self-addressed attachment and locks:

- resources/schema-bundle.json
- resources/schema-bundle.sha256
- resources/schema-lock.json

The final body is local metadata build-finance.schema-bundle/v1, not a 50th trading protocol contract. It contains exactly schema, jcs_profile, protocol_constants, primary_bundle_sha256, aliases, schemas, and binary_formulas. schema and jcs_profile are the exact strings build-finance.schema-bundle/v1 and RFC8785_INTEGER_AUTHORITY_V1. protocol_constants contains only uppercase MAX_RUN_CLOSURE_PROOF_ROWS_V0="1000000" and MAX_U64_V0="18446744073709551615". primary_bundle_sha256 is the independently sealed T01 digest. No example or sentinel digest is permitted.

The final bundle's jcs_profile, protocol_constants, complete aliases array, and eight PRIMARY schema rows must be byte-identical to the corresponding values/rows in the sealed primary bundle. It may append supporting/attachment rows but may not restate or regenerate a different primary authority.

Each aliases row contains exactly name, json_type, pattern, minimum, maximum, and scale. The UTF-8-byte-sorted rows are:

| name | json_type | pattern | minimum | maximum | scale |
|---|---|---|---|---|---|
| CausalDigest | string | ^[0-9a-f]{64}$ | null | null | null |
| ContentID | string | ^[0-9a-f]{64}$ | null | null | null |
| i128s | string | ^(0|-?[1-9][0-9]*)$ | -170141183460469231731687303715884105728 | 170141183460469231731687303715884105727 | null |
| sq18s | string | ^(0|-?[1-9][0-9]*)$ | -170141183460469231731687303715884105728 | 170141183460469231731687303715884105727 | 1000000000000000000 |
| u64s | string | ^(0|[1-9][0-9]*)$ | 0 | 18446744073709551615 | null |
| uints | string | ^(0|[1-9][0-9]*)$ | 0 | null | null |
| uq18s | string | ^(0|[1-9][0-9]*)$ | 0 | 1000000000000000000 | 1000000000000000000 |

minimum, maximum, and scale are JSON strings when non-null. JSON Schema enforces their lexical patterns; two independent semantic integer parsers enforce the listed string bounds. JSON-number bounds such as basis points, position indexes, and decimals remain in the owning schemas.

Each schemas row contains exactly contract_schema, json_schema_id, family, self_id_field, and schema_sha256. family is PRIMARY, SUPPORTING, or ATTACHMENT. self_id_field is non-null for the 8 primary and 13 supporting rows and null for attachment schemas. Rows sort by unsigned UTF-8 bytes of contract_schema.

There are 48 generated JSON Schemas: 8 primary, 13 supporting, and 27 JSON attachment schemas. trading.adverse-fill-draw/v1 is the 49th authority contract but is a binary domain-separated hash formula, not JSON. binary_formulas therefore contains one row with exactly contract_schema, family, formula_resource, and formula_sha256. Its contract is trading.adverse-fill-draw/v1, family is ATTACHMENT_BINARY_FORMULA, and formula_resource is formulas/adverse-fill-draw-v1.txt.

That formula resource freezes:

~~~text
SHA256(UTF8("trading.adverse-fill-draw/v1") || 0x00 || seed_bytes[32] || decode_hex(draw_key_sha256)[32] || uint64_be(0))
~~~

Test it with domain/key/counter mutation vectors; never pass it to a JSON Schema validator.

schema_bundle_sha256 is SHA256(JCS(the complete final body)). It has no self-ID and does not change the thirteen-supporting-contract count.

schema-lock.json has schema tag build-finance.schema-lock/v1 and exactly primary_contract_count=8, supporting_contract_count=13, json_attachment_schema_count=27, binary_formula_count=1, generated_json_schema_count=48, total_authority_contract_count=49, primary_bundle_sha256, supporting_bundle_sha256, attachment_bundle_sha256, binary_formula_bundle_sha256, and schema_bundle_sha256. primary_bundle_sha256 hashes the complete T01 primary bundle body. The supporting, JSON-attachment, and binary-formula family digests each hash their exact canonical sorted row array. schema_bundle_sha256 hashes the complete final schema-bundle body.

Every generated JSON resource is canonical JSON followed by one LF, and its digest excludes that LF. Each .sha256 file is one lowercase 64-hex digest followed by LF. schema_codegen --check rejects a missing, extra, or byte-different JSON, formula, or digest resource.

At the T01 sealing step, independently compute primary-schema-bundle.json twice—once through the runtime canonicalizer and once through a separately implemented test serializer that does not import it—copy the agreed digest as a literal EXPECTED_PRIMARY_BUNDLE_SHA256 in test_t01_primary_bundle.py, delete generated outputs, regenerate, and require literal equality. The expected value must not be derived from schema-lock at test time.

T02 repeats the same two-implementation sealing ceremony for schema-bundle.json: independently compute it twice, copy the agreed digest as literal EXPECTED_SCHEMA_BUNDLE_SHA256 in test_t02_round_closures.py, delete all T02-generated final resources, regenerate from the still-sealed T01 files, and require exact literal equality before T02 GREEN.

Schema resource naming is exact: remove the leading trading., replace / with -, and append .schema.json. For example, trading.raw-event/v1 is resources/schemas/raw-event-v1.schema.json and trading.run-closure-receipt/v1 is resources/schemas/run-closure-receipt-v1.schema.json. The two local input contracts are validated in local_fixture.py and are not placed in this trading resource set.

### 5. T03 has a sensor and a pure admission core

The local sensor receives one explicit root and reads this layout only:

~~~text
fixture-manifest.json
rights-manifest.json
terms/<terms-sha256>.bin
witnesses/<admission-sequence>.json
payloads/<manifest relative_path>
~~~

fixture-manifest.json, rights-manifest.json, and witnesses are canonical LF records. Payload and terms bytes are exact and unmodified.

rights-manifest.json is the non-authoritative local input build-finance.local-admission-evidence/v1. It contains exactly schema and a nonempty source_rights array sorted by source_id. Each row contains exactly source_id, terms_sha256, rights_role, rights_effective_date, rights_review_date, retention_posture, redistribution_posture, and provenance_requirements.

The only T03-admissible values are rights_role=offline_research_replay, retention_posture=LOCAL_RESEARCH_RETENTION, redistribution_posture=NO_REDISTRIBUTION, and the exact sorted unique provenance_requirements array ["CITE_SOURCE_ID","PRESERVE_RAW_SHA256","PRESERVE_TERMS_SHA256"]. Dates are still parsed independently so partial evidence is preserved in a rejected receipt. These semantics are sufficient for synthetic admission tests only; genuine P2 remains failed until the user supplies and approves actual terms/retention evidence.

FixtureManifest.rights_manifest_sha256 hashes the exact canonical rights-manifest payload without its LF. Each SourceAdmissionReceipt.terms_sha256 hashes the exact terms/<terms-sha256>.bin bytes named by that source row. The two digests are intentionally different authorities. This input is not a fourteenth supporting contract and is not added to the trading schema bundle.

Witness records have schema tag build-finance.local-witness/v1 and exactly schema, admission_sequence, relative_path, raw_payload_sha256, observed_at, and ingested_at. The filename is the canonical decimal admission_sequence, so two manifest paths with identical bytes still have distinct witnesses. They are local input evidence, not new authoritative trading contracts. File mtimes and wall time are never substituted. A missing or invalid timestamp remains null in captured evidence while the valid hash/path/sibling timestamp is preserved.

The sensor rejects absolute paths, drive prefixes, backslashes, empty/dot/parent segments, symlinks, junctions/reparse points, non-regular files, and pre/open/post identity changes. The pure admission core receives captured bytes and derives parser lineage internally; it cannot fetch missing data.

The only T03-admissible payload media type is application/json. application/jsonl and application/octet-stream remain schema-valid SourceAdmissionReceipt enum values but this parser rejects them with ADMISSION_PROFILE_MISMATCH. Raw payload hashes always cover exact source bytes; the payload JSON need not be JCS. Parsing still requires strict UTF-8, one complete JSON object, no duplicate keys, NaN, Infinity, comments, or trailing value.

The local payload envelope tag is build-finance.solana-jupiter-fixture-event/v1. Its root key set is exactly schema, source_id, source_kind, source_revision, network, venue_profile, market_id, base_mint, quote_mint, base_decimals, quote_decimals, event_kind, source_position, revision, event_time, observed_at, ingested_at, executable, market, and quality_flags. source_kind is exactly JUPITER_ROUTE_QUOTE_FIXTURE; network/profile/event_kind are exactly solana-mainnet, solana-jupiter-fixture/v1, and ROUTE_QUOTE.

source_position contains exactly slot, transaction_index, instruction_index, event_index, source_native_event_id, and source_subsequence. revision contains exactly kind, supersedes_event_id, retracts_event_id, availability_slot, and availability_admission_sequence. market contains exactly base_amount_atoms, quote_amount_atoms, route_capacity_base_atoms, liquidity_quote_atoms, venue_fee_quote_atoms, priority_fee_quote_atoms, and route_impact_bps. quality_flags uses the RawEvent closed enum and sorted-unique order. All admitted scalar aliases, revision tuples, executable/quality rules, identity/decimal checks, and timestamp formats match RawEvent. The local input permits event_time, observed_at, and ingested_at to be null so rejection receipts can preserve partial evidence; ADMITTED still requires both witness times and the envelope/witness equality below.

The envelope observed_at and ingested_at values must byte-equal the witness values and ingested_at must not precede observed_at. Any missing or disagreement emits ADMISSION_POINT_IN_TIME_MISSING while preserving safely parsed siblings.

Unknown keys are never ignored. Recursively encountered unknown keys in the exact ASCII set actual_fill, fill_result, future_price, label, later_holder_state, later_liquidity, outcome, realized_pnl, revised_risk_score, route_success, survival, or target add ADMISSION_LEAKAGE_FIELD. Every other unknown key adds ADMISSION_PROFILE_MISMATCH. Values are not token-scanned, so opaque native IDs cannot accidentally trigger a leakage code.

### 6. Synthetic tests cannot satisfy P2

All synthetic local-admission fixtures are built inside pytest temporary directories from tests/crypto_replay/support/synthetic_local_fixture.py. No committed directory is presented as a real fixture root. docs/crypto-replay/promotion-status.json remains:

~~~json
{"p0":"BLOCKED","p1":"NOT_IMPLEMENTED","p2":"FAIL_ZERO_ADMITTED_FIXTURE","p5":"FAIL_WHOLE_REPOSITORY","real_fixture_manifest_sha256":null}
~~~

Node completion updates only p1 to PASS when its evidence exists and records replay_subpackage_confinement separately. It does not change p0, p2, or whole-repository p5.

T03 can validate the literal FULL_DECLARED_SOURCE_UNIVERSE policy, the four required retention booleans, and local manifest/file set closure. It cannot prove that the declared universe is actually complete without an approved source census and selection-rule preimage. Synthetic T03 GREEN therefore cannot promote the universe or P2 gate.

### 7. T02 verifies code-digest binding but does not invent code provenance

The approved spec names admission, normalization, availability-grouping, run-closure, feature, baseline, risk, fill, accounting, and benchmark code SHA-256 fields but does not define a multi-file byte preimage for each. This plan does not silently invent one.

T02 validates lexical SHA-256 values, exact equality across RunClosureReceipt and RunReceipt, and resolution to exact retained bytes when the contract graph supplies a preimage. The exact required keys are admission_code_sha256, normalization_code_sha256, availability_grouping_code_sha256, run_closure_code_sha256, feature_code_sha256, baseline_code_sha256, risk_code_sha256, fill_code_sha256, accounting_code_sha256, and benchmark_code_sha256. Every synthetic preimage is nonempty and explicitly CONTRACT_ONLY; missing, extra, duplicate, shared-payload/digest alias, or mismatched rows fail. No production RunReceipt constructor is exposed until a later P0 decision freezes the real code-surface and source-tree preimage convention. This limitation leaves P0 BLOCKED and prevents a schema test from becoming a provenance claim.

---

## Planned File Map

### New runtime files

- build_finance/crypto_replay/__init__.py — narrow public API; no eager import of legacy Build Finance modules.
- build_finance/crypto_replay/errors.py — typed canonical, schema, graph, local-I/O, and admission failures.
- build_finance/crypto_replay/canonical.py — strict parse, RFC 8785 integer-authority serialization, LF records, SHA-256.
- build_finance/crypto_replay/content_ids.py — exact self-ID compute/seal/verify maps.
- build_finance/crypto_replay/formats.py — pure scalar, timestamp, path, sort, and range predicates.
- build_finance/crypto_replay/schema_model.py — ContractSpec, ValidationIssue, resolver interfaces, and registry metadata.
- build_finance/crypto_replay/schema_definitions.py — explicit Python definitions for the 8 primary, 13 supporting, and closed hashed-attachment schemas.
- build_finance/crypto_replay/schema_codegen.py — deterministic schema and lock generation/check mode.
- build_finance/crypto_replay/schema_registry.py — importlib.resources loader and local-only recursive validator.
- build_finance/crypto_replay/contract_semantics.py — standalone and graph semantic validators.
- build_finance/crypto_replay/ledger_contract.py — T02 record/object/cause/order formulas only; no append engine.
- build_finance/crypto_replay/run_inputs.py — T02 retained-body reconstruction and cross-binding only; no replay execution.
- build_finance/crypto_replay/source_tree.py — T02 canonical local source-tree preimage scanner.
- build_finance/crypto_replay/local_fixture.py — T03 secure local-byte sensor and immutable captured-input dataclasses.
- build_finance/crypto_replay/jupiter_fixture.py — T03 strict offline fixture-envelope parser.
- build_finance/crypto_replay/admission.py — T03 pure admission, rights, checksum, set, point-in-time, and conflict decisions.
- build_finance/crypto_replay/resources/primary-schema-bundle.json and primary-schema-bundle.sha256 — T01 immutable primary seal.
- build_finance/crypto_replay/resources/schema-bundle.json and schema-bundle.sha256 — T02 final canonical attachment and digest.
- build_finance/crypto_replay/resources/schema-lock.json — generated build-finance lock receipt.
- build_finance/crypto_replay/resources/schemas/*.schema.json — generated closed schemas.
- build_finance/crypto_replay/resources/formulas/adverse-fill-draw-v1.txt — exact binary hash-domain formula.

### New test/evidence files

- tests/crypto_replay/conftest.py — local registry and builder fixtures.
- tests/crypto_replay/support/builders.py — bottom-up content-addressed object builders.
- tests/crypto_replay/support/primary_vectors.py — T01 two-event primary vectors with retained cryptographic preimages.
- tests/crypto_replay/support/known_good_graph.py — T02 all-links-schema-valid synthetic graph builder.
- tests/crypto_replay/support/synthetic_local_fixture.py — temporary T03 fixture writer, explicitly synthetic.
- tests/crypto_replay/resources/primary-v1/examples/*.json — eight frozen primary examples copied once from the approved spec and then locally locked.
- tests/crypto_replay/resources/jcs-known-vectors.json — positive JCS values and expected base64 bytes/digests.
- tests/crypto_replay/resources/adversarial-bytes.json — named invalid byte cases stored as base64, never parsed as source code.
- tests/crypto_replay/test_t01_canonical.py.
- tests/crypto_replay/test_t01_primary_contracts.py.
- tests/crypto_replay/test_t01_primary_bundle.py.
- tests/crypto_replay/test_t02_supporting_contracts.py.
- tests/crypto_replay/test_t02_cross_artifact.py.
- tests/crypto_replay/test_t02_ledger_contract.py.
- tests/crypto_replay/test_t02_round_closures.py.
- tests/crypto_replay/test_t03_fixture_admission.py.
- tests/crypto_replay/test_t03_confinement.py.
- scripts/capture_crypto_replay_gate.py — deterministic RED/GREEN test receipt recorder; development tooling, not runtime.
- scripts/verify_crypto_replay_artifacts.py — archive census, schema-lock, forbidden-content, and METADATA assertions.
- docs/crypto-replay/evidence/baseline.json.
- docs/crypto-replay/evidence/T01-red.json and T01-green.json.
- docs/crypto-replay/evidence/T02-red.json and T02-green.json.
- docs/crypto-replay/evidence/T03-red.json and T03-green.json.
- docs/crypto-replay/README.md — install, scope, API, fixture layout, troubleshooting, and non-goals.
- docs/crypto-replay/promotion-status.json — honest gate state.

### Repository control and existing files modified or created

- pyproject.toml — test-only jsonschema, generated JSON package data, no runtime network dependency.
- .github/workflows/ci.yml — install .[all,test], check generated schemas, and run tests/crypto_replay/test_t03_confinement.py.
- .gitignore — ignore .artifacts/crypto-replay raw command output.
- .gitattributes — new path-scoped LF rules for replay source/resources/tests/docs/scripts only.

### Files explicitly untouched

- build_finance/autotrader.py
- build_finance/broker.py
- build_finance/market_data.py
- build_finance/backtest.py
- build_finance/gui/**
- release workflow, version, changelog, and package publishing metadata

---

## Public API and Core Types

Implement these signatures exactly unless a type-checker requires only a spelling change:

~~~text
# canonical.py
from typing import TypeAlias

JsonScalar: TypeAlias = type(None) | bool | int | str
JsonValue: TypeAlias = JsonScalar | list["JsonValue"] | dict[str, "JsonValue"]
JsonObject: TypeAlias = dict[str, JsonValue]

canonical_json_bytes(value: JsonValue) -> bytes
canonical_record_bytes(value: JsonObject) -> bytes
parse_canonical_json(payload: bytes) -> JsonObject
parse_canonical_record(record: bytes) -> JsonObject
sha256_hex(payload: bytes) -> str
~~~

~~~text
# content_ids.py
compute_content_id(document: Mapping[str, JsonValue]) -> str
seal_content_id(document: Mapping[str, JsonValue]) -> dict[str, JsonValue]
verify_content_id(document: Mapping[str, JsonValue]) -> bool
~~~

~~~text
# schema_model.py and schema_registry.py
ValidationIssue(code: str, path: tuple[str | int, ...], message: str)
ResolvedContent(
    record: bytes,
    document: Mapping[str, JsonValue],
    assurance: Literal["DIGEST_ONLY", "SCHEMA_VALID"],
)
ContractSpec(
    schema_id: str,
    self_id_field: str | None,
    family: Literal["PRIMARY", "SUPPORTING", "ATTACHMENT"],
    schema_filename: str,
)
EvidenceResolver.resolve_object(content_id: str) -> ResolvedContent | None
EvidenceResolver.resolve_bytes(sha256: str) -> bytes | None

validate_contract(
    document: Mapping[str, JsonValue],
    *,
    expected_schema: str | None = None,
    resolver: EvidenceResolver | None = None,
) -> tuple[ValidationIssue, ...]

require_valid_contract(
    document: Mapping[str, JsonValue],
    *,
    expected_schema: str | None = None,
    resolver: EvidenceResolver | None = None,
) -> None
~~~

~~~text
# run_inputs.py
RunInputBundle(
    fixture_manifest: Mapping[str, JsonValue],
    config_admission_receipt: Mapping[str, JsonValue],
    replay_risk_config: Mapping[str, JsonValue] | None,
    raw_config_bytes: bytes | None,
    source_admission_receipts: tuple[Mapping[str, JsonValue], ...],
    normalized_events: tuple[Mapping[str, JsonValue], ...],
    run_closure_receipt: Mapping[str, JsonValue],
    availability_schedule: Mapping[str, JsonValue],
    counter_capacity: Mapping[str, JsonValue],
    source_tree: Mapping[str, JsonValue],
    model_registry: Mapping[str, JsonValue] | None,
    model_signal_manifest: Mapping[str, JsonValue] | None,
    public_seed_bytes: bytes,
    code_preimages: tuple[ContractCodePreimage, ...],
)
ContractCodePreimage(
    field_name: Literal[
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
    ],
    payload: bytes,
    assurance: Literal["CONTRACT_ONLY"],
)
ContractVerifiedRunInputs(
    authority: Literal["CONTRACT_ONLY"],
    run_receipt: Mapping[str, JsonValue],
    bundle: RunInputBundle,
    public_seed_bytes: bytes,
)

derive_availability_schedule(bundle: RunInputBundle) -> Mapping[str, JsonValue]
derive_normalized_event_set(bundle: RunInputBundle) -> Mapping[str, JsonValue]
derive_counter_capacity(bundle: RunInputBundle) -> Mapping[str, JsonValue]
verify_contract_run_inputs(
    run_receipt: Mapping[str, JsonValue],
    bundle: RunInputBundle,
) -> ContractVerifiedRunInputs
~~~

The private counter arithmetic helper may accept integer counts for boundary tests. verify_contract_run_inputs must derive counts from actual retained bodies and may never accept caller-supplied counts. It requires exactly one nonempty CONTRACT_ONLY code preimage for each of the ten field names, rejects extras, duplicate fields, shared payload/digest aliases, and digest mismatches, and never upgrades them to production lineage.

Raw config binding is exact: MISSING requires raw_config_bytes=None, raw_config_sha256=null, and raw_byte_length="0"; INVALID requires exact nonempty raw bytes, their SHA-256 and full byte length, and no parsed ReplayRiskConfig; VALID requires raw_config_bytes to equal canonical_record_bytes(replay_risk_config), hashes/counts those exact bytes including the terminal LF, and separately verifies config_sha256 over the canonical object payload excluding LF.

T02 returns only ContractVerifiedRunInputs with authority CONTRACT_ONLY. It does not expose a production RunReceipt constructor or runnable capability, create genesis state, append a ledger, or run a group. A later P0 decision must define production code/source-tree provenance and a separately reviewed promotion API.

~~~text
# local_fixture.py and admission.py
@dataclass(frozen=True)
class CapturedFile:
    admission_sequence: str
    relative_path: str
    payload: bytes
    sha256: str
    byte_length: int
    observed_at: str | None
    ingested_at: str | None

@dataclass(frozen=True)
class CapturedFixture:
    manifest: Mapping[str, JsonValue] | None
    manifest_payload: bytes | None
    rights_manifest: Mapping[str, JsonValue] | None
    rights_manifest_payload: bytes | None
    terms_by_sha256: Mapping[str, bytes]
    files: tuple[CapturedFile, ...]
    capture_issues: tuple[ValidationIssue, ...]

@dataclass(frozen=True)
class ParsedSourceCandidate:
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

@dataclass(frozen=True)
class AdmissionBatch:
    status: Literal["ADMITTED", "REJECTED", "QUARANTINED"]
    reason_codes: tuple[str, ...]
    source_receipt_records: tuple[bytes, ...]
    candidates: tuple[ParsedSourceCandidate, ...]

ParserIdentity(
    version: Literal["solana-jupiter-fixture-parser/v1"],
    code_sha256: str,
)
capture_local_fixture(root: Path) -> CapturedFixture

current_parser_identity() -> ParserIdentity
admit_local_fixture(captured: CapturedFixture) -> AdmissionBatch
~~~

AdmissionBatch and ParsedSourceCandidate are in-memory orchestration values, not additional authoritative trading contracts. T03 must not emit RawEvent.

Every source_receipt_records member is an immutable canonical LF record sorted by numeric admission_sequence. AdmissionBatch.candidates is nonempty only when status is ADMITTED. Any rejected or quarantined batch exposes an empty tuple even if individual fields were safely parsed; the total SourceAdmissionReceipt records retain that evidence instead.

Per-file SourceAdmissionReceipt status follows the specification exactly: no codes is ADMITTED; any list containing ADMISSION_POSITION_CONFLICT, ADMISSION_SEQUENCE_INVALID, ADMISSION_REVISION_CAUSALITY, ADMISSION_REVISION_FORK, or ADMISSION_SET_NOT_CLOSED is QUARANTINED; every other nonempty list is REJECTED.

AdmissionBatch status is ADMITTED only when every receipt is ADMITTED and all set-level checks pass. It is QUARANTINED when any receipt is QUARANTINED or the indivisible set has ADMISSION_MANIFEST_MISMATCH, ADMISSION_HASH_MISMATCH, or ADMISSION_SET_NOT_CLOSED. Otherwise any rejected receipt makes the batch REJECTED. Batch reason_codes is the specification-precedence-sorted unique union and never invents a new trading reason code.

Evidence error mapping is exact:

- canonical rights-manifest payload digest differs from FixtureManifest.rights_manifest_sha256: ADMISSION_MANIFEST_MISMATCH;
- terms bytes missing/unreadable or actual terms SHA-256 differs from its source-rights row: ADMISSION_RIGHTS_MISSING; preserve the actual safely read digest and use null only when bytes were unavailable;
- witness admission sequence, path, or expected hash differs from its manifest row: ADMISSION_MANIFEST_MISMATCH;
- witness missing/noncanonical or either timestamp null/invalid: ADMISSION_POINT_IN_TIME_MISSING while preserving every valid sibling;
- actual payload SHA-256 differs from FixtureManifest.files[].raw_payload_sha256: ADMISSION_HASH_MISMATCH.

jupiter_fixture.py owns PARSER_VERSION="solana-jupiter-fixture-parser/v1". parser_code_sha256 is derived internally from a canonical build-finance.parser-code-bundle/v1 preimage containing exact sorted relative_path/file_sha256 rows for the complete AST-reachable build_finance.crypto_replay runtime closure beginning at admission.py and jupiter_fixture.py, plus every schema/resource file dynamically opened by that closure. This necessarily covers local_fixture.py, canonical.py, content_ids.py, formats.py, schema_registry.py, contract_semantics.py, errors.py, and any later local dependency rather than relying on a hand-maintained three-file list.

An independent test derives the local import/resource closure and requires exact row equality with the bundle before checking its digest. Changing admission status, receipt body/self-ID, parsing, validation, or a newly imported helper therefore changes parser_code_sha256. Admission rejects if any retained file cannot be read or rehashed. No caller can supply or override parser lineage.

---

## Contract Inventory

### T01 primary contracts and self-ID fields

| Schema | Self-ID field |
|---|---|
| trading.raw-event/v1 | event_id |
| trading.feature-snapshot/v1 | snapshot_id |
| trading.model-signal/v1 | signal_id |
| trading.risk-decision/v1 | risk_decision_id |
| trading.simulated-order-intent/v1 | intent_id |
| trading.simulated-fill-receipt/v1 | fill_receipt_id |
| trading.portfolio-state/v1 | portfolio_state_id |
| trading.ledger-record/v1 | ledger_record_id |

### T02 supporting contracts and self-ID fields

| Schema | Self-ID field |
|---|---|
| trading.fixture-manifest/v1 | fixture_manifest_sha256 |
| trading.replay-risk-config/v1 | config_sha256 |
| trading.source-admission-receipt/v1 | source_admission_receipt_id |
| trading.config-admission-receipt/v1 | config_admission_receipt_id |
| trading.run-closure-receipt/v1 | run_closure_receipt_id |
| trading.execution-quarantine-receipt/v1 | execution_quarantine_receipt_id |
| trading.model-registry/v1 | model_registry_sha256 |
| trading.model-signal-manifest/v1 | model_signal_manifest_sha256 |
| trading.model-validation-receipt/v1 | model_validation_receipt_id |
| trading.reconciliation-receipt/v1 | reconciliation_receipt_id |
| trading.run-receipt/v1 | run_receipt_id |
| trading.benchmark-measurement/v1 | benchmark_measurement_id |
| trading.benchmark-receipt/v1 | benchmark_receipt_id |

### Closed hashed-attachment JSON Schemas in the same lock

Generate these exact 27 attachment JSON Schemas:

- trading.adverse-fill-draw-key/v1
- trading.availability-schedule/v1
- trading.benchmark-manifest/v1
- trading.benchmark-metrics/v1
- trading.benchmark-request/v1
- trading.counter-capacity/v1
- trading.execution-footprint-component/v1
- trading.execution-transition-footprint/v1
- trading.execution-transition-key/v1
- trading.fill-idempotency-key/v1
- trading.force-close-state-envelope/v1
- trading.group-mark-key/v1
- trading.hardware-profile/v1
- trading.model-attempt-footprint/v1
- trading.model-request-key/v1
- trading.model-validation-attempt-key/v1
- trading.normalized-event-set/v1
- trading.preregistered-thresholds/v1
- trading.reconciliation-arithmetic-operands/v1
- trading.reconciliation-arithmetic-range-key/v1
- trading.reservation-key/v1
- trading.risk-idempotency-key/v1
- trading.run-closure-fill-candidate-semantic/v1
- trading.run-closure-full-fill-proof-row/v1
- trading.run-closure-full-fill-proof-set/v1
- trading.run-closure-reference-set/v1
- trading.source-tree/v1

Lock trading.adverse-fill-draw/v1 separately as the one binary formula resource defined above. Do not generate a JSON Schema for it or for schema-bundle.json itself. Do not create a new top-level self-addressed contract for any attachment.

---

## Task 0: Create the Isolated Worktree and Reconfirm Baseline

**Files:**

- Create: docs/crypto-replay/evidence/baseline.json

- [ ] Confirm main is clean except for the committed plan.

  Run:

  ~~~powershell
  git status --short --branch
  git rev-parse HEAD
  ~~~

  Expected: main tracks origin/main and contains no uncommitted implementation changes.

- [ ] Create the authorized worktree.

  Run from C:\dev\public\build-finance:

  ~~~powershell
  git ls-files --error-unmatch docs/superpowers/plans/2026-07-11-crypto-replay-kernel-t01-t03.md
  $planCommit = git rev-parse HEAD
  git worktree add C:\dev\worktrees\build-finance-crypto-replay-t01-t03 -b feat/crypto-replay-t01-t03 $planCommit
  ~~~

  Expected: a new branch rooted at the plan commit.

- [ ] Re-run the existing baseline in the worktree.

  ~~~powershell
  $env:PYTHONDONTWRITEBYTECODE = '1'
  python -m pytest -p no:cacheprovider tests -q
  python -m pip check
  git status --short
  ~~~

  Expected: 142 tests pass at the recorded baseline; pip check reports no broken requirements; worktree is clean. If the test count has legitimately increased, record the actual count and require zero failures.

- [ ] Record the baseline commit and commands in docs/crypto-replay/evidence/baseline.json without host paths, timestamps, usernames, or secrets.

- [ ] Commit the passing baseline receipt before writing RED tests.

  ~~~powershell
  git add docs/crypto-replay/evidence/baseline.json
  git commit -m "chore(crypto-replay): record implementation baseline"
  ~~~

Do not commit a baseline failure. Diagnose any pre-existing failure before continuing.

---

## Task 1: Add the Gate Recorder and Capture T01 RED

**Files:**

- Create: scripts/capture_crypto_replay_gate.py
- Create: tests/crypto_replay/test_t01_canonical.py
- Create: tests/crypto_replay/test_t01_primary_contracts.py
- Create: tests/crypto_replay/test_t01_primary_bundle.py
- Create: tests/crypto_replay/resources/primary-v1/examples/raw-event.json
- Create: tests/crypto_replay/resources/primary-v1/examples/feature-snapshot.json
- Create: tests/crypto_replay/resources/primary-v1/examples/model-signal.json
- Create: tests/crypto_replay/resources/primary-v1/examples/risk-decision.json
- Create: tests/crypto_replay/resources/primary-v1/examples/simulated-order-intent.json
- Create: tests/crypto_replay/resources/primary-v1/examples/simulated-fill-receipt.json
- Create: tests/crypto_replay/resources/primary-v1/examples/portfolio-state.json
- Create: tests/crypto_replay/resources/primary-v1/examples/ledger-record.json
- Create: tests/crypto_replay/resources/jcs-known-vectors.json
- Create: tests/crypto_replay/resources/adversarial-bytes.json
- Create: docs/crypto-replay/evidence/T01-red.json
- Create: .gitattributes
- Modify: .gitignore

- [ ] Add .artifacts/crypto-replay/ to .gitignore. Add path-scoped text eol=lf rules for /build_finance/crypto_replay/**, /tests/crypto_replay/**, /docs/crypto-replay/**, /scripts/capture_crypto_replay_gate.py, and /scripts/verify_crypto_replay_artifacts.py. Do not change line-ending behavior for existing unrelated files.

- [ ] Implement the stdlib-only gate recorder. It must:

  1. accept NODE, PHASE, OUTPUT_JSON, an optional -- separator, then the pytest arguments, removing the separator before invocation;
  2. run python -m pytest with capture;
  3. replace the repository root with <repo> in normalized output;
  4. write exact command args, exit code, sorted failing/passing test IDs, stdout SHA-256, and current git commit;
  5. omit time, host, username, environment variables, and full stdout;
  6. set PYTHONDONTWRITEBYTECODE=1 and add -p no:cacheprovider unless already present;
  7. return the pytest exit code.

- [ ] Write T01 named tests before creating build_finance.crypto_replay. Import inside each test so the named test fails rather than collection aborting.

  Required tests:

  - test_unknown_property_rejected_for_all_primary_v1_schemas
  - test_duplicate_key_rejected_before_parse
  - test_primary_id_digest_excludes_only_self_id
  - test_primary_examples_validate_and_self_ids_recompute
  - test_noncanonical_byte_matrix_is_rejected
  - test_record_lf_is_excluded_from_digest
  - test_primary_bundle_hash_is_locked
  - test_primary_vectors_contain_two_events_and_resolve_retained_byte_digests
  - test_primary_examples_cover_exactly_eight_schema_ids
  - test_raw_event_example_validates
  - test_feature_snapshot_example_validates
  - test_model_signal_example_validates
  - test_risk_decision_example_validates
  - test_simulated_order_intent_example_validates
  - test_simulated_fill_receipt_example_validates
  - test_portfolio_state_example_validates
  - test_ledger_record_example_validates
  - test_raw_event_semantic_matrix
  - test_feature_snapshot_semantic_matrix
  - test_model_signal_semantic_matrix
  - test_risk_decision_semantic_matrix
  - test_simulated_order_intent_semantic_matrix
  - test_simulated_fill_receipt_semantic_matrix
  - test_portfolio_state_semantic_matrix
  - test_ledger_record_semantic_matrix
  - test_registry_rejects_unsupported_keywords_and_remote_refs
  - test_local_and_jsonschema_oracles_agree_on_structural_vectors

- [ ] Copy the eight approved primary examples once into the exact local resource paths above, preserving canonical object content and recomputing each self-ID independently. Tests/runtime must never parse the external Markdown specification. Store invalid raw bytes only as named base64 strings in adversarial-bytes.json.

- [ ] Parameterize the adversarial byte matrix with BOM, duplicate key, leading/trailing whitespace, CRLF, missing LF, double LF, comment, NaN, Infinity, float, negative zero, unsafe JSON integer, trailing object, invalid UTF-8, noncanonical key order, unnecessary escape, and lone surrogate cases.

- [ ] Capture RED:

  ~~~powershell
  python scripts/capture_crypto_replay_gate.py T01 RED docs/crypto-replay/evidence/T01-red.json -- tests/crypto_replay/test_t01_canonical.py tests/crypto_replay/test_t01_primary_contracts.py tests/crypto_replay/test_t01_primary_bundle.py -q
  ~~~

  Expected: non-zero exit; all required named tests are present; failure is missing T01 functionality, not a syntax or fixture error.

- [ ] Commit only the recorder, RED tests, ignore rule, and RED receipt.

  ~~~powershell
  git add .gitattributes .gitignore scripts/capture_crypto_replay_gate.py tests/crypto_replay docs/crypto-replay/evidence/T01-red.json
  git commit -m "test(crypto-replay): capture T01 red gate"
  ~~~

---

## Task 2: Implement Strict Canonical Bytes

**Files:**

- Create: build_finance/crypto_replay/__init__.py
- Create: build_finance/crypto_replay/errors.py
- Create: build_finance/crypto_replay/canonical.py
- Create: build_finance/crypto_replay/formats.py
- Test: tests/crypto_replay/test_t01_canonical.py

- [ ] Add focused known vectors for UTF-16 key order, control/string escaping, integer rendering, and canonical nested arrays/objects.

- [ ] Define the Python 3.10 aliases exactly with typing.TypeAlias: JsonScalar uses type(None), JsonValue uses quoted recursive list/dict references, and JsonObject is dict[str, JsonValue].

- [ ] Implement duplicate rejection at parse time:

  ~~~python
  def _unique_object(pairs: list[tuple[str, JsonValue]]) -> dict[str, JsonValue]:
      result: dict[str, JsonValue] = {}
      for key, value in pairs:
          if key in result:
              raise DuplicateKeyError(key)
          result[key] = value
      return result
  ~~~

- [ ] Parse with object_pairs_hook=_unique_object, parse_float=_reject_float, and parse_constant=_reject_constant. Decode UTF-8 with errors=strict. Reject any non-object root for a persisted contract.

- [ ] Implement canonical_json_bytes recursively. Check bool before int. Reject integers outside -9007199254740991..9007199254740991. Sort keys with key.encode("utf-16-be", errors="strict"). Reject lone surrogates before encoding. Never Unicode-normalize and never call locale functions.

- [ ] Implement exact JCS string escaping: short escapes for backspace, tab, newline, form feed, and carriage return; quote/backslash escapes; lowercase \u00xx for other controls; never escape slash or non-ASCII unnecessarily.

- [ ] Make parse_canonical_json reject non-object roots, reserialize the parsed object with canonical_json_bytes, and require byte equality before returning. Canonical checking must not exist only in parse_canonical_record.

- [ ] Implement record rules:

  ~~~python
  def canonical_record_bytes(value: JsonObject) -> bytes:
      return canonical_json_bytes(value) + b"\n"


  def parse_canonical_record(record: bytes) -> JsonObject:
      if not record.endswith(b"\n") or record.endswith(b"\n\n"):
          raise CanonicalJSONError("record requires exactly one LF")
      payload = record[:-1]
      value = parse_canonical_json(payload)
      if canonical_json_bytes(value) != payload:
          raise CanonicalJSONError("payload is not canonical")
      return value
  ~~~

  canonical_json_bytes stays generic because typed JCS key/draw preimages may be arrays; persisted record APIs are object-only.

- [ ] Run these tests one at a time before the aggregate file:

  ~~~powershell
  python -m pytest tests/crypto_replay/test_t01_canonical.py::test_duplicate_key_rejected_before_parse -q
  python -m pytest tests/crypto_replay/test_t01_canonical.py::test_jcs_integer_authority_boundaries -q
  python -m pytest tests/crypto_replay/test_t01_canonical.py::test_jcs_known_vectors -q
  python -m pytest tests/crypto_replay/test_t01_canonical.py::test_noncanonical_byte_matrix_is_rejected -q
  python -m pytest tests/crypto_replay/test_t01_canonical.py::test_record_lf_is_excluded_from_digest -q
  ~~~

  Boundary vectors include 2**53-1 accepted, 2**53 rejected, non-ASCII preserved, UTF-16 key ordering, short/control escapes, unnecessary escapes rejected, and non-object record roots rejected.

- [ ] Run:

  ~~~powershell
  python -m pytest tests/crypto_replay/test_t01_canonical.py -q
  python -m ruff check build_finance/crypto_replay/canonical.py build_finance/crypto_replay/formats.py
  python -m mypy build_finance/crypto_replay/canonical.py build_finance/crypto_replay/formats.py
  ~~~

  Expected: canonical tests pass; T01 contract/bundle tests remain RED.

- [ ] Commit.

  ~~~powershell
  git add build_finance/crypto_replay tests/crypto_replay/test_t01_canonical.py
  git commit -m "feat(crypto-replay): add strict canonical byte contract"
  ~~~

---

## Task 3: Implement Exact Content IDs and Schema Infrastructure

**Files:**

- Create: build_finance/crypto_replay/content_ids.py
- Create: build_finance/crypto_replay/schema_model.py
- Create: build_finance/crypto_replay/schema_registry.py
- Create: build_finance/crypto_replay/schema_codegen.py
- Create: build_finance/crypto_replay/schema_definitions.py
- Modify: pyproject.toml
- Modify: .github/workflows/ci.yml
- Test: tests/crypto_replay/test_t01_primary_contracts.py

- [ ] Add the exact 8/13 schema-to-self-ID maps from this plan. The document schema tag owns the lookup; no public function accepts an arbitrary omission field. Raise for an unknown schema.

- [ ] Implement compute_content_id by looking up document["schema"], making one shallow top-level copy, deleting exactly the registered self-ID field if present, and hashing canonical_json_bytes. compute may accept the registered field absent; verify requires it present; seal accepts it absent or already matching and rejects a supplied mismatch instead of overwriting it.

- [ ] Write a regression that mutates every non-self-ID field one at a time and proves the digest changes. Mutate only the self-ID and prove the computed digest does not change.

- [ ] Implement ContractSpec and the local registry. The runtime schema walker may support only:

  - $schema, $id, $defs, and the exact x-contract-schema metadata key;
  - $ref to a registered local schema ID or local #/$defs path;
  - type, const, enum;
  - required, properties, additionalProperties;
  - items, minItems, maxItems, uniqueItems;
  - pattern, minLength, maxLength;
  - minimum, maximum;
  - anyOf, oneOf, allOf, not.

  Any unsupported keyword or unknown/non-local reference is a hard registry-load error.

- [ ] Implement schema_codegen scopes exactly: primary writes/checks the eight primary schemas plus primary seal; supporting writes/checks only the thirteen supporting schema resources without a final bundle; full writes/checks all 48 JSON schemas, the binary formula, final bundle/digest, and lock while preserving the sealed primary files.

- [ ] Treat bool separately from int so JSON Schema integer never accepts true/false. Unknown metadata/extensions fail instead of being ignored.

- [ ] Add jsonschema>=4.23,<5 to project.optional-dependencies.test and project.optional-dependencies.dev only. Change CI install to:

  ~~~yaml
  pip install -e ".[all,test]"
  ~~~

- [ ] Add package data:

  ~~~toml
  [tool.setuptools.package-data]
  "build_finance.crypto_replay" = [
      "resources/*.json",
      "resources/*.sha256",
      "resources/schemas/*.json",
      "resources/formulas/*.txt",
  ]
  ~~~

- [ ] Add an independent-oracle test that loads each generated dictionary into jsonschema.Draft202012Validator.check_schema using an explicit preloaded referencing.Registry. Its retrieval callback always raises AssertionError for any URI not preloaded. Validate the same structural positive/negative instances as the runtime walker and assert the callback is never reached for valid local schemas.

- [ ] Keep numeric-string range and UTF-8 byte-length checks out of stock JSON Schema claims. Generated schemas enforce lexical patterns and code-point-safe structural limits only; formats.py parses bounded decimal strings independently, and a separately written test-oracle predicate parses and compares them without calling the runtime helper. UTF-8 byte maxima are semantic checks because JSON Schema maxLength counts code points. Add min/max/max+1 and multi-byte boundary vectors to both predicates.

- [ ] Run the two infrastructure node IDs before the rest:

  ~~~powershell
  python -m pytest tests/crypto_replay/test_t01_primary_contracts.py::test_primary_id_digest_excludes_only_self_id -q
  python -m pytest tests/crypto_replay/test_t01_primary_contracts.py::test_registry_rejects_unsupported_keywords_and_remote_refs -q
  ~~~

- [ ] Run:

  ~~~powershell
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t01_canonical.py tests/crypto_replay/test_t01_primary_contracts.py::test_primary_id_digest_excludes_only_self_id tests/crypto_replay/test_t01_primary_contracts.py::test_registry_rejects_unsupported_keywords_and_remote_refs tests/crypto_replay/test_t01_primary_contracts.py::test_local_and_jsonschema_oracles_agree_on_structural_vectors -q
  ~~~

  Expected: canonical, content-ID, registry, and independent structural-oracle tests pass; the eight schema-specific example tests remain RED and are not included in this command.

- [ ] Commit.

  ~~~powershell
  git add build_finance/crypto_replay pyproject.toml .github/workflows/ci.yml tests/crypto_replay
  git commit -m "feat(crypto-replay): add local schema and content-id core"
  ~~~

---

## Task 4: Define and Generate the Eight Primary Schemas

**Files:**

- Modify: build_finance/crypto_replay/schema_definitions.py
- Create: build_finance/crypto_replay/contract_semantics.py
- Generate: build_finance/crypto_replay/resources/schemas/*.schema.json
- Generate: build_finance/crypto_replay/resources/primary-schema-bundle.json
- Generate: build_finance/crypto_replay/resources/primary-schema-bundle.sha256
- Test: tests/crypto_replay/test_t01_primary_contracts.py

- [ ] Run test_raw_event_example_validates RED, add raw_event_schema from the exhaustive table, then rerun it GREEN.

- [ ] Run test_feature_snapshot_example_validates RED, add feature_snapshot_schema, then rerun it GREEN.

- [ ] Run test_model_signal_example_validates RED, add model_signal_schema, then rerun it GREEN.

- [ ] Run test_risk_decision_example_validates RED, add risk_decision_schema, then rerun it GREEN.

- [ ] Run test_simulated_order_intent_example_validates RED, add simulated_order_intent_schema, then rerun it GREEN.

- [ ] Run test_simulated_fill_receipt_example_validates RED, add simulated_fill_receipt_schema, then rerun it GREEN.

- [ ] Run test_portfolio_state_example_validates RED, add portfolio_state_schema, then rerun it GREEN.

- [ ] Run test_ledger_record_example_validates RED, add ledger_record_schema, then rerun it GREEN.

  Use the exact node ID form tests/crypto_replay/test_t01_primary_contracts.py::<name> for each command. Do not parse Markdown at build or runtime. The explicit Python definitions are the machine-readable source in this repo. After each checkbox, generate to a temporary dictionary, run Draft202012Validator.check_schema, and run only that contract's positive/unknown/missing/type tests before moving to the next.

- [ ] Every object helper must emit required equal to the complete property set and additionalProperties false. Every nullable field remains required with a null union.

- [ ] Add shared scalar definitions for exact lowercase SHA-256, ContentID, CausalDigest, u64s, uints, i128s, sq18s, uq18s, RFC 3339 nanosecond UTC, RFC 3339 full-date, safe relative path, and bounded UTF-8 registry string.

- [ ] Run test_raw_event_semantic_matrix RED; add revision tuple, original availability equality, quality blocking, mint inequality, decimals, event/observed/ingested ordering, and sorted unique quality flags; rerun GREEN.

- [ ] Run test_feature_snapshot_semantic_matrix RED; add zero-history/nullability and feature missing-reason alignment; rerun GREEN.

- [ ] Run test_model_signal_semantic_matrix RED; add flat directional-only action/probability fields and no order-shaped extension; rerun GREEN.

- [ ] Run test_risk_decision_semantic_matrix RED; add exact nullable evidence/status tuple and reason ordering; rerun GREEN.

- [ ] Run test_simulated_order_intent_semantic_matrix RED; add action, quantity, reservation, stop/take, and lineage tuple semantics; rerun GREEN.

- [ ] Run test_simulated_fill_receipt_semantic_matrix RED; add trigger, status, quantity, fee, release, and terminal tuple semantics; rerun GREEN.

- [ ] Run test_portfolio_state_semantic_matrix RED; add sorted unique balances/positions/reservations/open intents and exact summary-field semantics; rerun GREEN.

- [ ] Run test_ledger_record_semantic_matrix RED; add record-type/object-schema mapping, sorted causes/entries, prior-head rules, and self-cause prohibition; rerun GREEN.

- [ ] Generate with check mode support:

  ~~~powershell
  python -m build_finance.crypto_replay.schema_codegen --scope primary --write
  python -m build_finance.crypto_replay.schema_codegen --scope primary --check
  ~~~

  --check must fail on a missing, extra, or byte-different resource.

- [ ] Perform the T01 sealing ceremony: independently compute the primary bundle digest twice, insert the agreed literal as EXPECTED_PRIMARY_BUNDLE_SHA256 in test_t01_primary_bundle.py, delete generated outputs, regenerate, and require literal equality. Confirm the primary bundle contains exactly eight rich PRIMARY rows and its .sha256 file is exact.

- [ ] Run:

  ~~~powershell
  python -m pytest tests/crypto_replay/test_t01_primary_contracts.py -q
  python -m pytest tests/crypto_replay/test_t01_primary_bundle.py -q
  ~~~

  Expected: all shape/ID/closure/schema-oracle tests pass except the unresolved graph fixture test.

- [ ] Commit.

  ~~~powershell
  git add build_finance/crypto_replay tests/crypto_replay
  git commit -m "feat(crypto-replay): seal eight primary contracts"
  ~~~

---

## Task 5: Build Two-Event Primary Vectors and Close T01

**Files:**

- Create: tests/crypto_replay/support/builders.py
- Create: tests/crypto_replay/support/primary_vectors.py
- Modify: tests/crypto_replay/test_t01_primary_bundle.py
- Create: docs/crypto-replay/evidence/T01-green.json

- [ ] Implement an in-memory resolver keyed separately by ContentID and retained-byte SHA-256. A content preimage always means the complete canonical LF record, never the self-ID-omitted body. Duplicate key with different bytes must fail.

- [ ] Build two synthetic primary-vector scenarios:

  1. disabled_primary_vectors: two RawEvents in two availability groups, model status ABSENT, and one deterministic no-entry primary-object chain;
  2. contract_only_model_vector: one static ModelSignal shape vector, never connected to an executable run and never passed to admission.

- [ ] Add contract_only_execution_vector containing one SimulatedOrderIntent and one SimulatedFillReceipt plus their primary state/ledger references. It is a schema/link vector only and is never passed to an admission, replay, or actuator API.

- [ ] Construct every self-ID bottom-up. Never paste or invent a digest. Serialize each object, reparse it, validate it, then insert it into the resolver.

- [ ] Retain exact canonical records for every supporting ContentID referenced by a T01 primary object and mark each ResolvedContent assurance DIGEST_ONLY. Validate only digest identity in T01. Their thirteen supporting schemas and cross-contract meaning do not exist until T02 and must not be claimed here.

- [ ] Add primary-vector assertions:

  - every primary self-ID resolves to canonical bytes and verifies;
  - every raw payload digest resolves to retained synthetic bytes;
  - every external supporting ContentID resolves cryptographically to retained canonical bytes, without claiming supporting-schema validity;
  - there are at least two RawEvents and two distinct availability slots;
  - set(primary example schema IDs) equals the exact eight-entry PRIMARY_SCHEMA_IDS set;
  - no object or helper claims provider origin, observed market truth, profitability, or P2 admission.

- [ ] Capture GREEN:

  ~~~powershell
  python scripts/capture_crypto_replay_gate.py T01 GREEN docs/crypto-replay/evidence/T01-green.json -- tests/crypto_replay/test_t01_canonical.py tests/crypto_replay/test_t01_primary_contracts.py tests/crypto_replay/test_t01_primary_bundle.py -q
  ~~~

  Expected: exit 0; all T01 tests pass.

- [ ] Rerun the exact predecessor gate:

  ~~~powershell
  python -m build_finance.crypto_replay.schema_codegen --scope primary --check
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t01_canonical.py tests/crypto_replay/test_t01_primary_contracts.py tests/crypto_replay/test_t01_primary_bundle.py -q
  ~~~

  Expected: schema check and all T01 tests pass.

- [ ] Commit.

  ~~~powershell
  git add tests/crypto_replay docs/crypto-replay/evidence/T01-green.json
  git commit -m "test(crypto-replay): close T01 with resolving vectors"
  ~~~

---

## Task 6: Write T02 RED for Thirteen Supporting Contracts

**Files:**

- Create: tests/crypto_replay/test_t02_supporting_contracts.py
- Create: tests/crypto_replay/test_t02_cross_artifact.py
- Create: tests/crypto_replay/test_t02_ledger_contract.py
- Create: tests/crypto_replay/test_t02_round_closures.py
- Create: docs/crypto-replay/evidence/T02-red.json

- [ ] Add all nineteen T02 node-table tests with their exact names:

  - test_all_supporting_contracts_are_closed
  - test_supporting_self_ids_recompute
  - test_fixture_manifest_is_set_closed
  - test_source_rejection_receipt_is_total
  - test_missing_and_invalid_config_receipts_serialize
  - test_run_closure_receipt_is_total_by_config_status
  - test_execution_quarantine_receipt_is_exhaustive
  - test_source_admission_is_config_independent
  - test_run_receipt_requires_matching_closure_receipt
  - test_public_seed_bytes_resolve_from_closed_run_inputs
  - test_public_seed_hash_mismatch_prevents_genesis
  - test_causal_digest_kind_and_resolution_are_closed
  - test_model_registry_and_signal_manifest_are_set_closed
  - test_run_receipt_has_no_output_cycle
  - test_model_validation_fallback_tuple_is_closed
  - test_benchmark_measurement_sample_arrays_are_closed
  - test_every_record_type_causation_set_is_exact
  - test_initial_ledger_prefix_order_is_exact
  - test_fill_ledger_record_cannot_self_cause

- [ ] Add the T02-only Round 9-18 closure tests listed in the appendix of this plan.

- [ ] For every joint T02/T11 requirement, add the separately named T02 structural test listed in the deferred-joint section of the appendix. Do not add or mark GREEN the original joint normative test name in this branch; that name remains owned by the later T11 executable proof. This prevents a schema-only assertion from falsely claiming the later fill/arithmetic behavior.

- [ ] Capture T02 RED while T01 remains GREEN:

  ~~~powershell
  python scripts/capture_crypto_replay_gate.py T02 RED docs/crypto-replay/evidence/T02-red.json -- tests/crypto_replay/test_t02_supporting_contracts.py tests/crypto_replay/test_t02_cross_artifact.py tests/crypto_replay/test_t02_ledger_contract.py tests/crypto_replay/test_t02_round_closures.py -q
  python -m pytest tests/crypto_replay/test_t01_canonical.py tests/crypto_replay/test_t01_primary_contracts.py tests/crypto_replay/test_t01_primary_bundle.py -q
  ~~~

  Expected: T02 non-zero for missing supporting behavior; T01 stays fully GREEN.

- [ ] Commit RED tests and receipt only.

  ~~~powershell
  git add tests/crypto_replay/test_t02_supporting_contracts.py tests/crypto_replay/test_t02_cross_artifact.py tests/crypto_replay/test_t02_ledger_contract.py tests/crypto_replay/test_t02_round_closures.py docs/crypto-replay/evidence/T02-red.json
  git commit -m "test(crypto-replay): capture T02 red gate"
  ~~~

---

## Task 7: Add Fixture, Config, and Source-Admission Supporting Contracts

**Files:**

- Modify: build_finance/crypto_replay/schema_definitions.py
- Modify: build_finance/crypto_replay/contract_semantics.py
- Modify: tests/crypto_replay/support/builders.py
- Test: tests/crypto_replay/test_t02_supporting_contracts.py

- [ ] Add FixtureManifest schema, self-ID field, positive/unknown/missing oracle cases, and regenerate in check mode.

- [ ] Add ReplayRiskConfig schema, self-ID field, positive/unknown/missing oracle cases, and regenerate in check mode.

- [ ] Add SourceAdmissionReceipt schema, self-ID field, positive/unknown/missing oracle cases, and regenerate in check mode.

- [ ] Add ConfigAdmissionReceipt schema, self-ID field, positive/unknown/missing oracle cases, and regenerate in check mode.

- [ ] Enforce FixtureManifest:

  - exact network/profile/universe/point-in-time enums;
  - nonempty sorted unique markets and files;
  - unique market_id and base_mint;
  - base_mint differs from quote_mint;
  - quote mint/decimals agree everywhere;
  - all four retention booleans are true;
  - safe normalized relative paths;
  - positive session bounds with start before end;
  - one-based unique admission sequences.

- [ ] Enforce ReplayRiskConfig scalar and cross-field bounds from the table, including min <= target <= max, start < end, positive price tick, exact model versions, and protocol proof cap.

- [ ] Implement total ConfigAdmissionReceipt builders for MISSING, INVALID, and VALID. They must preserve raw digest/length when bytes exist and never invent a validated digest.

- [ ] Implement SourceAdmissionReceipt status derivation as a pure function of the complete sorted reason set. Preserve independently valid rights fields and witnessed values. Confirm it never accepts a risk config argument.

- [ ] Generate/check the supporting resources and run only these four families:

  ~~~powershell
  python -m build_finance.crypto_replay.schema_codegen --scope supporting --write
  python -m build_finance.crypto_replay.schema_codegen --scope supporting --check
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t02_supporting_contracts.py::test_fixture_manifest_is_set_closed tests/crypto_replay/test_t02_supporting_contracts.py::test_source_rejection_receipt_is_total tests/crypto_replay/test_t02_supporting_contracts.py::test_missing_and_invalid_config_receipts_serialize tests/crypto_replay/test_t02_supporting_contracts.py::test_source_admission_is_config_independent -q
  ~~~

  Expected: these tests pass; closure/quarantine/model/run/benchmark/ledger T02 tests remain RED and are not included.

- [ ] Commit.

  ~~~powershell
  git add build_finance/crypto_replay tests/crypto_replay
  git commit -m "feat(crypto-replay): add admission and config contracts"
  ~~~

---

## Task 8: Add Closure, Quarantine, Registry, and Model-Evidence Contracts

**Files:**

- Modify: build_finance/crypto_replay/schema_definitions.py
- Modify: build_finance/crypto_replay/contract_semantics.py
- Create: build_finance/crypto_replay/run_inputs.py
- Modify: tests/crypto_replay/test_t02_supporting_contracts.py
- Modify: tests/crypto_replay/test_t02_cross_artifact.py
- Modify: tests/crypto_replay/test_t02_round_closures.py

- [ ] Add RunClosureReceipt schema and self-ID cases.

- [ ] Add ExecutionQuarantineReceipt schema and self-ID cases.

- [ ] Add ModelRegistry schema and self-ID cases.

- [ ] Add ModelSignalManifest schema and self-ID cases.

- [ ] Add ModelValidationReceipt schema and self-ID cases.

- [ ] Implement counter-capacity construction with unbounded Python integers followed by a single alias conversion. Derive all counts from retained arrays/bodies; do not accept caller-supplied counts.

- [ ] Implement run-closure schema/status/nullability validation for supplied closed examples of:

  - missing/invalid config zero authority;
  - valid LEAVE_MARKED_OPEN;
  - FORCE_CLOSE_NEXT_EVENT preproof/budget/enumeration failures;
  - protocol cap equality and one-over;
  - exact reason order and complete market-row serialization.

  This validates receipt and attachment shape/field presence only. It does not construct H_m, Q_cap, state envelopes, reference/extreme enumeration, fill arithmetic, proof roots, prices, reservations, or portfolio mutations; those predicates and their original test names remain deferred to T11.

- [ ] Implement quarantine slot/component classifiers and exact exhaustive observations. A quarantine receipt remains out-of-band and cannot become a LedgerRecord.

- [ ] Implement closed model registry/manifest set checks and the exact rejected/expired/drift-disabled fallback tuple. Do not create a model worker, inference call, scheduler, or accepted runtime path.

- [ ] Regenerate supporting resources and run the exact closure/quarantine/model subset:

  ~~~powershell
  python -m build_finance.crypto_replay.schema_codegen --scope supporting --write
  python -m build_finance.crypto_replay.schema_codegen --scope supporting --check
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t02_supporting_contracts.py::test_run_closure_receipt_is_total_by_config_status tests/crypto_replay/test_t02_supporting_contracts.py::test_execution_quarantine_receipt_is_exhaustive tests/crypto_replay/test_t02_supporting_contracts.py::test_model_registry_and_signal_manifest_are_set_closed tests/crypto_replay/test_t02_supporting_contracts.py::test_model_validation_fallback_tuple_is_closed -q
  ~~~

  Expected: these four tests pass; run-input, reconciliation, benchmark, and ledger tests remain RED.

- [ ] Commit.

  ~~~powershell
  git add build_finance/crypto_replay tests/crypto_replay
  git commit -m "feat(crypto-replay): add closure and evidence contracts"
  ~~~

---

## Task 9: Add Reconciliation, Run, Measurement, and Benchmark Contracts

**Files:**

- Modify: build_finance/crypto_replay/schema_definitions.py
- Modify: build_finance/crypto_replay/contract_semantics.py
- Modify: build_finance/crypto_replay/run_inputs.py
- Create: build_finance/crypto_replay/ledger_contract.py
- Test: tests/crypto_replay/test_t02_cross_artifact.py
- Test: tests/crypto_replay/test_t02_ledger_contract.py

- [ ] Add ReconciliationReceipt schema and self-ID cases.

- [ ] Add RunReceipt schema and self-ID cases.

- [ ] Add BenchmarkMeasurement schema and self-ID cases.

- [ ] Add BenchmarkReceipt schema and self-ID cases.

- [ ] Implement contract-only RunReceipt input verification, returning ContractVerifiedRunInputs(authority="CONTRACT_ONLY") only after all checks:

  - validate all input self-IDs before reading bodies;
  - require the exact matching RunClosureReceipt;
  - reconstruct availability-schedule and normalized-event-set bytes;
  - derive source/event/group/market/model counts;
  - bind fixture tick, model mode/null IDs, code/schema lineage, terminal group, and policy;
  - resolve exactly 32 public seed bytes and require digest equality;
  - forbid output root/hash fields that would create a cycle.

  A seed mismatch returns validation issues and no ContractVerifiedRunInputs. T02 has no genesis constructor; test_public_seed_hash_mismatch_prevents_genesis proves the prerequisite capability is absent, not that T02 attempted a state mutation.

- [ ] Bind exact raw config bytes under the MISSING/INVALID/VALID matrix and bind the exact ten CONTRACT_ONLY code preimages. Add coordinated raw-byte/LF/length/hash and missing/extra/aliased code-preimage mutations.

- [ ] Implement benchmark shape/measurement checks only. Do not schedule or run benchmarks. Sample arrays are closed and bounded; profitability_claims_permitted is false in the hashed manifest.

- [ ] Implement causal digest classification:

  - resolving ContentID;
  - exact typed-key JCS digest;
  - exact retained local-byte digest.

  Reject unknown free-form digests.

- [ ] Implement record-type to object-schema and exact cause formulas in ledger_contract.py. Implement initial-prefix ordering as a validator over a supplied sequence, not as a mutable ledger.

- [ ] Regenerate supporting resources and run the exact run/benchmark/causation subset:

  ~~~powershell
  python -m build_finance.crypto_replay.schema_codegen --scope supporting --write
  python -m build_finance.crypto_replay.schema_codegen --scope supporting --check
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t02_cross_artifact.py tests/crypto_replay/test_t02_ledger_contract.py tests/crypto_replay/test_t02_supporting_contracts.py::test_benchmark_measurement_sample_arrays_are_closed -q
  ~~~

  Expected: node-table run-input, public-seed, causation, output-cycle, benchmark-measurement, prefix, and fill-self-cause tests pass; attachment/round-oracle tests remain RED.

- [ ] Commit.

  ~~~powershell
  git add build_finance/crypto_replay tests/crypto_replay
  git commit -m "feat(crypto-replay): close run and ledger input contracts"
  ~~~

---

## Task 10: Close T02 Hashed Attachments and Independent-Oracles

**Files:**

- Modify: build_finance/crypto_replay/schema_definitions.py
- Modify: build_finance/crypto_replay/run_inputs.py
- Create: build_finance/crypto_replay/source_tree.py
- Modify: tests/crypto_replay/test_t02_round_closures.py
- Create: build_finance/crypto_replay/resources/formulas/adverse-fill-draw-v1.txt
- Create: build_finance/crypto_replay/resources/schema-bundle.json
- Create: build_finance/crypto_replay/resources/schema-bundle.sha256
- Create: build_finance/crypto_replay/resources/schema-lock.json

- [ ] Add the availability-schedule, normalized-event-set, counter-capacity, and source-tree attachment schemas; regenerate and run their oracle cases.

- [ ] Add reservation, risk-idempotency, group-mark, fill-idempotency, adverse-fill-draw-key, and model request/attempt/key attachment schemas; regenerate and run their oracle cases.

- [ ] Add the exact adverse-fill-draw-v1.txt binary formula resource and domain/key/counter mutation vectors. Do not generate or invoke a JSON Schema for trading.adverse-fill-draw/v1.

- [ ] Add execution transition/footprint/component attachment schemas; regenerate and run their oracle cases.

- [ ] Add force-close semantic/envelope/reference/proof-row/proof-set attachment schemas; regenerate and run their oracle cases.

- [ ] Add benchmark manifest/request/metrics/hardware/threshold attachment schemas; regenerate and run their oracle cases.

- [ ] Generate the final build-finance.schema-bundle/v1 metadata attachment, schema-bundle.sha256, and schema-lock.json from the sealed primary bundle, 13 supporting schemas, 27 JSON attachment schemas, and one binary formula row. Do not generate a JSON Schema for schema-bundle.json itself. Assert exact 8/13/27/1/48/49 counts and one occurrence of every inventory member.

  ~~~powershell
  python -m build_finance.crypto_replay.schema_codegen --scope full --write
  python -m build_finance.crypto_replay.schema_codegen --scope full --check
  ~~~

  Expected: check mode reports zero missing, extra, or byte-different resources and does not alter the sealed primary bundle.

- [ ] Require the final jcs_profile, protocol_constants, aliases, and eight PRIMARY rows to be byte-identical to primary-schema-bundle.json. Then perform the independent T02 sealing ceremony and pin EXPECTED_SCHEMA_BUNDLE_SHA256 as a literal in test_t02_round_closures.py; delete/regenerate T02 final resources and require literal equality.

- [ ] Implement source-tree canonical scanning:

  - fixed exclusion list from the spec;
  - UTF-8 NFC POSIX relative paths;
  - no symlink/junction/reparse/root resolution;
  - no-follow classification where the platform supports it;
  - stable pre/open/post identity;
  - regular files only;
  - exact byte length and SHA-256;
  - sorted unique rows;
  - explicit failure when stability cannot be proven.

- [ ] Reconstruct availability-schedule, normalized-event-set, counter-capacity, and source-tree bodies independently from their retained inputs. Compare full canonical bytes before comparing digests.

- [ ] Implement independent test constructors for 0, 1, MAX_U64_V0, and MAX_U64_V0+1 market/candidate counts. Do not allocate enormous arrays; the constructor takes a count-only test source and applies the same unbounded arithmetic.

- [ ] Add Round 18 run/source decisions. The mechanical and semantic test constructors must not read the frozen expected booleans as their oracle.

- [ ] Run the exact T02 attachment gate:

  ~~~powershell
  python -m build_finance.crypto_replay.schema_codegen --scope full --check
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t02_round_closures.py -q
  python -m ruff check build_finance/crypto_replay/schema_definitions.py build_finance/crypto_replay/run_inputs.py build_finance/crypto_replay/source_tree.py
  python -m mypy build_finance/crypto_replay/schema_definitions.py build_finance/crypto_replay/run_inputs.py build_finance/crypto_replay/source_tree.py
  ~~~

  Expected: all T02-only and narrower structural tests pass; no deferred T11-dependent original name is collected.

- [ ] Commit.

  ~~~powershell
  git add build_finance/crypto_replay tests/crypto_replay
  git commit -m "feat(crypto-replay): seal T02 attachment oracles"
  ~~~

---

## Task 11: Build the Joint Fully Resolving Graph and Capture T02 GREEN

**Files:**

- Create: tests/crypto_replay/support/known_good_graph.py
- Modify: tests/crypto_replay/support/builders.py
- Create: docs/crypto-replay/evidence/T02-green.json

- [ ] Consume the T01 primary vectors and replace each cryptographic-only supporting preimage with a schema-valid object built bottom-up. Add all thirteen supporting contracts and every retained attachment required by their links.

- [ ] Promote resolver entries to SCHEMA_VALID only after canonical-byte, JSON Schema, semantic, and self-ID checks all pass. The T02 graph validator rejects any remaining DIGEST_ONLY supporting reference.

- [ ] Keep the runnable scenario model_signal_mode=DISABLED. Keep model registry/manifest/signal vectors in a separate contract-only graph with no actuation or run authority.

- [ ] Add a separate contract-only benchmark subgraph resolving benchmark manifest, thresholds, hardware, measurement, metrics, and an ineligible BenchmarkReceipt; do not schedule or execute a benchmark.

- [ ] Add one standalone ExecutionQuarantineReceipt with every referenced footprint/component preimage resolved and no ledger membership.

- [ ] Freeze the disabled-run graph coordinates:

  - source 1 admission_sequence 1, availability slot/group 1;
  - config admission_sequence 2;
  - source 2 admission_sequence 3, availability slot/group 2;
  - one allowed market, two normalized events, two groups, zero model candidates;
  - public seed hex 000000000000000000000000000000000000000000000000000000000000000f;
  - LEAVE_MARKED_OPEN, terminal group 2, empty market proofs, proof count 0, proof status NOT_REQUIRED.

- [ ] Retain exact canonical LF ReplayRiskConfig bytes. Require ConfigAdmissionReceipt.raw_config_sha256 and raw_byte_length to hash/count those bytes including LF, while config_sha256 verifies the object payload excluding LF. Add one LF removal and one byte mutation rejection.

- [ ] Supply exactly ten unique nonempty CONTRACT_ONLY code preimages and prove that missing, extra, duplicated-field, shared-payload/digest, and rehashed-mismatch variants cannot produce ContractVerifiedRunInputs.

- [ ] Assert the exact availability schedule rows:

  ~~~json
  [
    {"availability_slot":"1","equal_time_group":"1","admission_cutoff":"2"},
    {"availability_slot":"2","equal_time_group":"2","admission_cutoff":"3"}
  ]
  ~~~

- [ ] Assert counter-capacity values derived from S=2, R=2, G=2, M=1, C=0: decision attempt 2; next decision 3; next producer 0; next intent 3; next fill receipt 3; unmatched reservation upper bound 4; next state 13; next ledger 43.

- [ ] Assert the exact initial prefix plan without appending it: sequence 0 source admission 1, 1 config admission, 2 source admission 2, 3 run receipt, 4 genesis portfolio-state shape vector, 5 genesis reconciliation shape vector.

- [ ] Require:

  - all 21 primary/supporting self-IDs verify;
  - all referenced IDs and retained digests resolve;
  - exact fixture/config/source/closure/run arrays and hashes cross-bind;
  - exact ledger cause/order formulas pass;
  - every generated schema hash matches schema-lock.json;
  - supporting_contract_count is exactly 13.

- [ ] Capture GREEN:

  ~~~powershell
  python scripts/capture_crypto_replay_gate.py T02 GREEN docs/crypto-replay/evidence/T02-green.json -- tests/crypto_replay/test_t02_supporting_contracts.py tests/crypto_replay/test_t02_cross_artifact.py tests/crypto_replay/test_t02_ledger_contract.py tests/crypto_replay/test_t02_round_closures.py -q
  python -m pytest tests/crypto_replay/test_t01_canonical.py tests/crypto_replay/test_t01_primary_contracts.py tests/crypto_replay/test_t01_primary_bundle.py -q
  ~~~

  Expected: T01 and T02 exit 0.

- [ ] Commit.

  ~~~powershell
  git add tests/crypto_replay docs/crypto-replay/evidence/T02-green.json
  git commit -m "test(crypto-replay): close T02 resolving contract graph"
  ~~~

---

## Task 12: Write T03 RED and the Secure Local Sensor Contract

**Files:**

- Create: tests/crypto_replay/support/synthetic_local_fixture.py
- Create: tests/crypto_replay/test_t03_fixture_admission.py
- Create: tests/crypto_replay/test_t03_confinement.py
- Create: docs/crypto-replay/evidence/T03-red.json

- [ ] Add the seven exact T03 tests:

  - test_fixture_hash_mismatch_quarantines_set
  - test_point_in_time_fields_required
  - test_duplicate_base_mint_quarantines_set
  - test_base_mint_cannot_equal_quote_mint
  - test_partial_rights_evidence_is_preserved
  - test_conflicting_source_position_rejected
  - test_same_slot_different_market_is_not_conflict

- [ ] Add local sensor adversarial tests for path traversal, absolute/drive path, backslash, symlink, junction/reparse point, missing payload, extra payload, non-regular file, witness mismatch, and pre/open/post replacement.

- [ ] Add companion tests:

  - test_rights_manifest_and_terms_digest_failures_are_distinct
  - test_witness_identity_mismatch_reason_mapping_is_exact
  - test_identical_bytes_in_different_roots_emit_identical_receipts
  - test_identical_version_payload_emits_one_candidate
  - test_parser_identity_covers_complete_import_closure
  - test_universe_declaration_does_not_promote_p2

- [ ] Build every test fixture in tmp_path. The helper must include SYNTHETIC in Python fixture names/docstrings, use source_id SYNTHETIC_TEST_ONLY_INVALID, base mint SYNTHETIC_BASE_MINT_0OIl_INVALID, fixed UTC nanosecond timestamps, and terms bytes beginning SYNTHETIC TEST FIXTURE — NOT MARKET DATA. It must never write payload, terms, witness, manifest, ADMITTED receipt, or candidate bytes into docs/crypto-replay or package resources.

- [ ] Add confinement RED tests:

  - AST import denylist for direct/transitive urllib.request, http.client, socket, ssl, ftplib, smtplib, requests, httpx, aiohttp, websocket, websockets, subprocess, ctypes, multiprocessing, solana, solders, anchorpy, web3, ccxt, alpaca, and any build_finance module outside build_finance.crypto_replay;
  - AST call denylist for __import__, importlib.import_module, exec, and eval;
  - fresh-process import of every crypto_replay submodule discovered by pkgutil, followed by a newly loaded sys.modules denylist check;
  - prove importing the subpackage does not import build_finance.autotrader, broker, or market_data;
  - prove core metadata has no network/broker/wallet/provider dependency.

- [ ] Capture RED:

  ~~~powershell
  python scripts/capture_crypto_replay_gate.py T03 RED docs/crypto-replay/evidence/T03-red.json -- tests/crypto_replay/test_t03_fixture_admission.py tests/crypto_replay/test_t03_confinement.py -q
  ~~~

  Expected: non-zero for missing T03 code; T01 and T02 remain GREEN.

- [ ] Commit RED tests and receipt.

  ~~~powershell
  git add tests/crypto_replay docs/crypto-replay/evidence/T03-red.json
  git commit -m "test(crypto-replay): capture T03 red gate"
  ~~~

---

## Task 13: Implement Local Capture, Rights, Checksum, and Set Closure

**Files:**

- Create: build_finance/crypto_replay/local_fixture.py
- Create: build_finance/crypto_replay/jupiter_fixture.py
- Create: build_finance/crypto_replay/admission.py
- Test: tests/crypto_replay/test_t03_fixture_admission.py

- [ ] Run the checksum test RED, implement exact byte/hash receipt and batch status, then rerun GREEN:

  ~~~powershell
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t03_fixture_admission.py::test_fixture_hash_mismatch_quarantines_set -q
  ~~~

- [ ] Run the point-in-time and partial-rights tests RED, implement only their evidence parsing/error mapping, then rerun GREEN:

  ~~~powershell
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t03_fixture_admission.py::test_point_in_time_fields_required tests/crypto_replay/test_t03_fixture_admission.py::test_partial_rights_evidence_is_preserved -q
  ~~~

- [ ] Run the two manifest-universe identity tests RED, implement only market/base set closure, then rerun GREEN:

  ~~~powershell
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t03_fixture_admission.py::test_duplicate_base_mint_quarantines_set tests/crypto_replay/test_t03_fixture_admission.py::test_base_mint_cannot_equal_quote_mint -q
  ~~~

- [ ] Implement capture_local_fixture with explicit root, no default path, no environment-variable lookup, and no network fallback.

- [ ] Read fixture manifest, rights manifest, and witnesses as strict canonical LF records. Read raw payloads and terms files as exact bytes. Read each witness by canonical admission-sequence filename, then cross-check its admission_sequence, relative_path, and raw_payload_sha256 against the manifest row. Read each terms file by the digest named in its source-rights row.

- [ ] Validate the non-authoritative local-admission-evidence input separately from the trading schema bundle. Require one unique source-rights row for every manifest source_id and no unrelated row. Recompute FixtureManifest.rights_manifest_sha256 from the canonical manifest payload and every row terms_sha256 from exact terms bytes.

- [ ] Validate file identity before/open/after read. When Windows cannot establish a safe no-follow identity for a reparse point, reject it rather than resolving it.

- [ ] Implement the strict local Jupiter fixture envelope parser. It may extract only fields needed for T03:

  - source and market identity;
  - immutable mint decimals;
  - source-position lineage;
  - revision kind/target/availability slot/admission sequence;
  - event_time;
  - presence of route/liquidity/fee profile fields;
  - leakage-field denylist.

  It must not assign source_sequence, ingest_sequence, equal_time_group, replay_clock_ns, decision_sequence, or RawEvent ID.

- [ ] Implement current_parser_identity from the complete AST/resource closure and make test_parser_identity_covers_complete_import_closure GREEN before any receipt uses parser_code_sha256.

- [ ] Implement rights normalization field-by-field. A valid role/effective date survives an invalid review date; readable exact terms bytes still produce terms_sha256; retention, redistribution, and provenance failures reject admission without discarding independently valid date/role evidence.

- [ ] Implement manifest/file set closure and reason precedence. A hash mismatch gives the affected SourceAdmissionReceipt ADMISSION_HASH_MISMATCH/REJECTED and makes AdmissionBatch QUARANTINED because the fixture set is indivisible.

- [ ] Run the T03 non-position subset:

  ~~~powershell
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t03_fixture_admission.py::test_fixture_hash_mismatch_quarantines_set tests/crypto_replay/test_t03_fixture_admission.py::test_point_in_time_fields_required tests/crypto_replay/test_t03_fixture_admission.py::test_duplicate_base_mint_quarantines_set tests/crypto_replay/test_t03_fixture_admission.py::test_base_mint_cannot_equal_quote_mint tests/crypto_replay/test_t03_fixture_admission.py::test_partial_rights_evidence_is_preserved tests/crypto_replay/test_t03_fixture_admission.py::test_rights_manifest_and_terms_digest_failures_are_distinct tests/crypto_replay/test_t03_fixture_admission.py::test_witness_identity_mismatch_reason_mapping_is_exact tests/crypto_replay/test_t03_fixture_admission.py::test_parser_identity_covers_complete_import_closure -q
  ~~~

  Expected: this subset passes; only position/idempotence tests remain RED.

- [ ] Commit.

  ~~~powershell
  git add build_finance/crypto_replay tests/crypto_replay
  git commit -m "feat(crypto-replay): add offline fixture admission sensor"
  ~~~

---

## Task 14: Implement Source-Position Conflict Rules and Capture T03 GREEN

**Files:**

- Modify: build_finance/crypto_replay/admission.py
- Modify: build_finance/crypto_replay/jupiter_fixture.py
- Modify: tests/crypto_replay/test_t03_fixture_admission.py
- Create: docs/crypto-replay/evidence/T03-green.json

- [ ] Run the two position tests RED before adding lineage/version logic:

  ~~~powershell
  python -m pytest -p no:cacheprovider tests/crypto_replay/test_t03_fixture_admission.py::test_conflicting_source_position_rejected tests/crypto_replay/test_t03_fixture_admission.py::test_same_slot_different_market_is_not_conflict -q
  ~~~

- [ ] Implement the exact lineage key:

  ~~~python
  (
      source_id,
      source_kind,
      market_id,
      slot,
      transaction_index,
      instruction_index,
      event_index,
      source_native_event_id,
      source_subsequence,
  )
  ~~~

- [ ] Implement the version key by appending revision kind, revision availability slot, revision availability admission sequence, supersedes target or empty, and retracts target or empty.

- [ ] Enforce:

  - same version key/same payload is idempotent;
  - same version key/different bytes conflicts;
  - two distinct ORIGINAL bodies for one lineage conflict;
  - same slot with a different market/native ID is not a conflict;
  - correction/retraction target and availability checks are exact;
  - any position/set conflict quarantines the batch.

- [ ] For identical version-key/payload rows, retain every per-file source receipt but emit one ParsedSourceCandidate, selecting the lowest numeric admission_sequence and then unsigned UTF-8 relative_path as an impossible-tie guard. Add test_identical_version_payload_emits_one_candidate.

- [ ] For ADMISSION_POSITION_CONFLICT, add the code to every receipt whose candidate participates in the conflicting lineage/version group and make those receipts QUARANTINED. Unrelated receipts retain their independently derived status, while the batch is QUARANTINED and exposes no candidates.

- [ ] Sort all batch reasons by the specification precedence and source receipts by numeric admission_sequence. Re-running the same CapturedFixture must produce byte-identical source receipt records.

- [ ] Capture GREEN:

  ~~~powershell
  python scripts/capture_crypto_replay_gate.py T03 GREEN docs/crypto-replay/evidence/T03-green.json -- tests/crypto_replay/test_t03_fixture_admission.py tests/crypto_replay/test_t03_confinement.py -q
  python -m pytest tests/crypto_replay/test_t01_canonical.py tests/crypto_replay/test_t01_primary_contracts.py tests/crypto_replay/test_t01_primary_bundle.py tests/crypto_replay/test_t02_supporting_contracts.py tests/crypto_replay/test_t02_cross_artifact.py tests/crypto_replay/test_t02_ledger_contract.py tests/crypto_replay/test_t02_round_closures.py -q
  ~~~

  Expected: T01, T02, and T03 all exit 0; no network/provider/broker/wallet/signer module is imported by the new subpackage.

- [ ] Run the read-only internal-edge confinement gate in the worktree:

  ~~~powershell
  $g = index internals --root . --json | ConvertFrom-Json
  $bad = @($g.edges | Where-Object {
      $_.from -like 'build_finance/crypto_replay*' -and
      $_.to -like 'build_finance/*' -and
      $_.to -notlike 'build_finance/crypto_replay*'
  })
  if ($bad) {
      $bad | ConvertTo-Json -Depth 5
      throw 'crypto_replay import escaped capability island'
  }
  ~~~

  Expected: zero internal import edges escape the crypto_replay subtree. Keep the AST/dynamic-import tests because Index does not prove external or dynamic edges.

- [ ] Commit.

  ~~~powershell
  git add build_finance/crypto_replay tests/crypto_replay docs/crypto-replay/evidence/T03-green.json
  git commit -m "feat(crypto-replay): close deterministic T03 admission"
  ~~~

---

## Task 15: Package, Document, and Preserve Honest Gate State

**Files:**

- Create: docs/crypto-replay/README.md
- Create: docs/crypto-replay/promotion-status.json
- Create: scripts/verify_crypto_replay_artifacts.py
- Modify: pyproject.toml
- Modify: .github/workflows/ci.yml
- Test: tests/crypto_replay/test_t03_confinement.py

- [ ] Document:

  - package purpose and offline-only boundary;
  - installation with core and test extras;
  - canonical byte and validation API;
  - local fixture directory contract;
  - how to run T01/T02/T03 gates;
  - why synthetic fixtures do not satisfy P2;
  - troubleshooting for canonical JSON, duplicate keys, path/reparse rejection, rights fields, hash mismatch, and conflict receipts;
  - known limitations: no T04 sequencing, features, signal inference, risk execution, fill simulator, broker, wallet, live/paper trading, profitability evidence, or real fixture;
  - ownership and next node T04, which remains unauthorized in this session.

- [ ] Write promotion-status.json with p0 BLOCKED, p1 PASS only if T01/T02 lock evidence passes, p2 FAIL_ZERO_ADMITTED_FIXTURE, whole-repository p5 FAIL, replay_subpackage_confinement PASS, and real_fixture_manifest_sha256 null.

- [ ] Implement verify_crypto_replay_artifacts.py. It must assert, rather than print for manual review:

  - wheel contains primary/final bundle JSON and .sha256 files, schema-lock.json, formula resource, and every schema named by the lock;
  - wheel excludes tests/, docs/crypto-replay/evidence/, payloads/, terms/, witnesses/, .env names, fixture/receipt bytes, and the exact synthetic terms/payload byte sentinels;
  - sdist may contain source tests/docs but excludes generated local fixture roots, payload/terms/witness directories, .env names, secrets, and exact generated sentinel payload/terms bytes;
  - schema-lock counts/digests match archive members;
  - unconditional Requires-Dist is exactly numpy>=1.24, pandas>=2.0, and scipy>=1.10;
  - jsonschema appears only with extra == "test" or extra == "dev";
  - no forbidden provider/network/broker/wallet requirement appears.

- [ ] Build exactly one fresh wheel and sdist without index access, then run the verifier:

  ~~~powershell
  $artifact = Join-Path $env:TEMP ("bf-crypto-replay-" + [guid]::NewGuid())
  New-Item -ItemType Directory -Path "$artifact\dist" | Out-Null
  $env:PIP_NO_INDEX = '1'
  $env:PIP_DISABLE_PIP_VERSION_CHECK = '1'
  python -m build --no-isolation --outdir "$artifact\dist"
  $wheels = @(Get-ChildItem "$artifact\dist\*.whl")
  $sdists = @(Get-ChildItem "$artifact\dist\*.tar.gz")
  if ($wheels.Count -ne 1 -or $sdists.Count -ne 1) {
      throw 'unexpected artifact census'
  }
  python scripts/verify_crypto_replay_artifacts.py --wheel $wheels[0].FullName --sdist $sdists[0].FullName
  ~~~

  Expected: verifier exits 0 with the exact package-data, exclusion, digest, and requirement assertions above.

- [ ] Install the same wheel into a clean no-dependency venv and import every replay submodule:

  ~~~powershell
  python -m venv "$artifact\venv"
  & "$artifact\venv\Scripts\python" -m pip install --no-index --no-deps $wheels[0].FullName
  & "$artifact\venv\Scripts\python" -I -c "import importlib,pkgutil,build_finance.crypto_replay as p; [importlib.import_module(m.name) for m in pkgutil.walk_packages(p.__path__,p.__name__+'.')]"
  ~~~

  Expected: every replay submodule imports with NumPy, pandas, SciPy, jsonschema, and every forbidden network/provider/broker/wallet package absent. Do not run pip check in this no-deps venv: the unchanged Build Finance numerical requirements are intentionally absent.

- [ ] Run credential-shaped scans only over changed/added tracked files. Do not read ignored .env files.

- [ ] Commit.

  ~~~powershell
  git add docs/crypto-replay scripts/verify_crypto_replay_artifacts.py pyproject.toml .github/workflows/ci.yml tests/crypto_replay
  git commit -m "docs(crypto-replay): package offline T01-T03 evidence"
  ~~~

---

## Task 16: Final Verification and Scope Stop

**Files:** verification only; fix only failures caused by this branch.

- [ ] Verify generated resources and focused gates:

  ~~~powershell
  python -m build_finance.crypto_replay.schema_codegen --scope full --check
  python -m pytest -p no:cacheprovider tests/crypto_replay -q
  ~~~

- [ ] Verify the complete repository:

  ~~~powershell
  python -m pytest -p no:cacheprovider tests -q
  python -m ruff check .
  python -m ruff format --check .
  python -m mypy
  python -m pip check
  git diff --check main...HEAD
  ~~~

- [ ] Verify confinement in a fresh process and wheel install. Confirm no change to old broker/live files.

  ~~~powershell
  git diff --name-only main...HEAD
  git status --short
  ~~~

- [ ] Verify evidence/gate honesty:

  - T01-green, T02-green, and T03-green exit codes are zero;
  - RED receipts are non-zero and precede their GREEN implementation commits;
  - p0 is still BLOCKED;
  - p2 is still FAIL_ZERO_ADMITTED_FIXTURE;
  - real fixture ID remains null;
  - no model/runtime/provider/execution claims appear;
  - no profitability or split-second claim appears.

- [ ] Request a code review using superpowers:requesting-code-review. Address only in-scope correctness, security, packaging, determinism, and test issues.

- [ ] Stop. Do not begin T04, source a dataset, call Jupiter, configure a wallet, run a model, simulate a portfolio, publish a package, or merge without the next explicit workflow decision.

---

## T02 Round-Owned Test Appendix

Place these in tests/crypto_replay/test_t02_round_closures.py unless the named concern belongs in the ledger or cross-artifact file. They are part of T02 GREEN where the approved spec assigns T02 ownership.

### Quarantine and prefix totality

- test_initial_prefix_missing_anchor_emits_out_of_band_quarantine
- test_completed_footprint_wrong_slot_emits_out_of_band_quarantine
- test_completed_footprint_extra_slot_emits_out_of_band_quarantine
- test_initial_prefix_sequence_zero_missing_wrong_extra_vectors_serialize
- test_component_only_missing_claim_emits_quarantine
- test_component_only_wrong_counter_emits_quarantine
- test_component_only_missing_resulting_state_emits_quarantine

### Run-closure proof and capacity totality

- test_counter_capacity_preflight_is_total
- test_run_closure_failed_field_presence_matrix_is_exact
- test_counter_capacity_binds_selected_model_manifest_count
- test_preclosure_cardinality_caps_are_exact

### Run-input and retained-body identity

- test_run_gate_recomputes_model_capacity_and_fixture_tick
- test_run_input_gate_recomputes_normalized_event_set_from_bodies
- test_run_input_oracle_binds_canonical_availability_schedule
- test_public_seed_bytes_resolve_from_closed_run_inputs
- test_public_seed_hash_mismatch_prevents_genesis
- test_availability_cutoff_plateau_matches_max_formula

### Source-tree closure

- test_source_tree_preimage_is_canonical
- test_source_tree_root_junction_is_rejected_without_resolution
- test_source_tree_replacement_race_is_rejected
- test_source_tree_git_control_file_is_canonically_excluded
- test_source_tree_rejects_missing_extra_nonregular_unreadable_and_reparse_entries

### Benchmark attachment schema totality owned at T02

- test_benchmark_preflight_failure_receipt_is_total
- test_benchmark_hardware_profile_is_closed

### Counts and frozen independent decisions

- test_supporting_contract_count_is_thirteen
- test_count_totalizer_market_0_rejects
- test_count_totalizer_market_1_accepts
- test_count_totalizer_market_max_accepts
- test_count_totalizer_market_max_plus_one_rejects
- test_count_totalizer_candidate_0_obeys_disabled_mode
- test_count_totalizer_candidate_1_accepts_cached_mode
- test_count_totalizer_candidate_max_accepts
- test_count_totalizer_candidate_max_plus_one_rejects
- test_round18_run_and_source_decision_vector_is_derived

If a test name appears both in the node-table list and this appendix, implement it once. Do not silently rename any normative test.

### Deferred T11-dependent executable tests

The following original names are not collected or claimed GREEN in the T01-T03 worktree because their complete predicates require T11 fill/arithmetic execution. The first six are explicitly joint T02/T11 in the approved spec; the remaining force-proof names have no narrower explicit owner but cannot honestly pass at schema-only T02:

- test_run_closure_budget_failure_has_total_market_row_layout
- test_run_closure_preproof_failure_has_total_market_row_layout
- test_run_closure_enumeration_failure_completes_all_other_market_roots
- test_force_close_proves_state_dependent_sell_transition_aliases
- test_force_close_proves_favorable_and_max_adverse_fill_aliases
- test_run_closure_and_run_receipt_cross_bind_all_proof_lineage
- test_force_proof_row_schema_order_and_known_root
- test_force_proof_budget_exact_limit_passes
- test_force_proof_budget_one_over_fails_before_enumeration
- test_run_closure_proof_rows_at_protocol_cap
- test_run_closure_proof_rows_protocol_cap_plus_one_fails_before_hashing

T02 still closes the schema side with these explicitly narrower tests:

- test_t02_run_closure_budget_failure_market_row_schema_is_total
- test_t02_run_closure_preproof_failure_market_row_schema_is_total
- test_t02_run_closure_enumeration_failure_preserves_other_market_row_shape
- test_t02_force_close_sell_alias_attachment_schema_is_closed
- test_t02_force_close_fill_extreme_attachment_schema_is_closed
- test_t02_run_closure_and_run_receipt_bind_declared_lineage_fields
- test_t02_force_proof_row_attachment_schema_and_sort_order_are_closed
- test_t02_force_proof_budget_fields_accept_boundary_shaped_vectors
- test_t02_force_proof_protocol_cap_fields_reject_out_of-range_serialization

The later T11 plan must add the original normative names and prove the executable predicates without weakening these T02 structural closures.

---

## Test-to-Responsibility Map

| Requirement | Owning implementation |
|---|---|
| Duplicate key and canonical bytes | canonical.py |
| Self-ID excludes only named field | content_ids.py |
| Closed required object shapes | schema_definitions.py plus schema_registry.py |
| Schema/resource hash locks | schema_codegen.py plus resources/schema-lock.json |
| Primary/supporting field invariants | contract_semantics.py |
| T01 cryptographic graph links | EvidenceResolver plus primary_vectors.py with DIGEST_ONLY supporting records |
| T02 schema-valid graph links | EvidenceResolver plus known_good_graph.py with SCHEMA_VALID supporting records |
| Ledger object/cause/order formulas | ledger_contract.py |
| Run/closure/schedule/capacity cross-binding | run_inputs.py |
| Source-tree byte preimage | source_tree.py |
| Local-only file sensing | local_fixture.py |
| Jupiter fixture field extraction | jupiter_fixture.py |
| Rights/checksum/set/conflict decisions | admission.py |
| No provider/network/broker/wallet capability | test_t03_confinement.py plus package metadata |
| P0/P2 honesty | docs/crypto-replay/promotion-status.json |

---

## Required Self-Review Before Execution

- [ ] Search this plan for unresolved planning markers or deferred choices. There must be none.
- [ ] Confirm every new/modified path is named and no task touches excluded live/broker files.
- [ ] Confirm all 3 T01, 19 T02, and 7 T03 normative node-table tests are mapped.
- [ ] Confirm every T02-only Round 9-18 closure is mapped and every T11-dependent original name is explicitly deferred with a narrower T02 structural test.
- [ ] Confirm every public function in the snippets has one owning file and compatible argument/return types.
- [ ] Confirm the runtime dependency decision and CI install command agree.
- [ ] Confirm schema-bundle is a hashed attachment and supporting-contract count remains thirteen.
- [ ] Confirm both primary and final bundle digests are independently pinned literals and the final repeated primary fields/rows are byte-identical.
- [ ] Confirm run-input verification returns CONTRACT_ONLY authority, binds exact raw config LF bytes, and requires the exact ten unique CONTRACT_ONLY code preimages.
- [ ] Confirm T03 emits no RawEvent and therefore does not implement T04 sequencing.
- [ ] Confirm synthetic graphs and tmp_path fixtures cannot become real P2 evidence.
- [ ] Confirm p0/p2/whole-repository-p5 remain honestly failed or blocked.
- [ ] Confirm no secret, endpoint credential, wallet address, account balance, or private dataset path appears.
- [ ] Confirm no profit, speed, accuracy, safety, or readiness claim is made without a named passing gate.

---

## Completion Definition

This plan is implemented only when:

1. T01, T02, and T03 RED receipts exist at commits before their implementations.
2. All T01/T02/T03 tests and the pre-existing Build Finance suite pass.
3. Generated schema resources are byte-locked and ship in wheel/sdist.
4. A core wheel imports the replay package without network/provider/broker/wallet/signer dependencies.
5. All T03 outputs are deterministic for identical captured local bytes.
6. No actual provider, dataset, model, wallet, broker, or order path was used or added.
7. promotion-status.json still reports P0 blocked, P2 failed with null real fixture ID, and whole-repository P5 failed.
8. Work stops before T04.
