# Model Boundary Receipt (`buildlang-model-boundary-receipt/v0`)

> Status: **v1, verify arm shipped 2026-07-29** (buildc side only). Emission is
> harness-side (`harness/model_shim.py`, the local-model repo's `--receipt-dir`
> flag), never buildc. This is the contract that side implements against.
> Design of record:
> `docs/superpowers/specs/2026-07-29-model-boundary-receipts-design.md`.

A model boundary receipt is a PROVENANCE artifact about one connection to
BuildLang's `Model` capability wire contract (`model_complete`, a line
protocol over TCP to `BUILD_MODEL_ENDPOINT`; see the `model_complete`
paragraph in [SCIENTIFIC-RECEIPT.md](SCIENTIFIC-RECEIPT.md)). It carries no
invariant, no oracle, and no verdict, by construction: models propose; this
artifact witnesses THAT they proposed and WHAT bytes crossed, nothing more.

## Honest scope

Claims (all offline-checkable): these exact bytes crossed the boundary, in
this session, in this order, under this shim mode, and this is what the
adapter's daemon declared about itself at the time.

Refuses, explicitly, by the schema's absence of fields: quality or
correctness of the completion; that `model.name` corresponds to any
particular weights; that `daemon_digest.hex` corresponds to the weights
actually consulted (it is the daemon's declaration about itself, witnessed
only as a fetch, never as ground truth); determinism (not claimed even for
echo mode, whose reply happens to be re-derivable from the spec); that the
reply derived from the prompt at all (the shim observed a request and a
response on one connection; causality inside the daemon is not witnessed).

The scientific-runtime receipt's `CAPABILITY_INADMISSIBLE` refusal of any
`Model`-observing program (`scientific_runtime.rs`, `receipt verify`) is
**untouched** by this schema. A model receipt cannot become scientific
evidence and a scientific receipt cannot carry `Model` -- the two artifact
kinds share a seal idiom and a verifier binary, never a claim vocabulary.

## The schema

Flat top-level `schema` string plus top-level `seal`, mirroring the
scientific receipt's shape so the existing chain pointers `/schema` and
`/seal/hex` read it unchanged. Fields, in canonical (sealed) order -- this
order is load-bearing: it is exactly the Rust struct's serde field order in
`compiler/src/model_receipt.rs::ModelBoundaryReceipt`, and `serde_json::to_vec`
preserves struct field order, which is half of the cross-language
canonicalization contract below.

| field | content | epistemic tag |
|---|---|---|
| `schema` | `buildlang-model-boundary-receipt/v0` | structural |
| `source` | `model:<mode>:<name>` (e.g. `model:echo:echo/v1`) | DECLARED label; carries a human-readable chain-member label |
| `shim` | `{ name: "model_shim.py", version, mode: "echo"\|"ollama" }` | SHIM-WITNESSED (self-identity) |
| `session` | `{ listen, nonce, request_received_utc, reply_written_utc\|null }` | timestamps SHIM-CLOCK-DECLARED; `reply_written_utc` is `null` unless the outcome reached a reply |
| `prompt` | `{ sha256, bytes }` over the raw prompt-line bytes as received (line terminator stripped, before UTF-8 decode), or `null` iff `outcome == "PROTOCOL_VIOLATION"` | SHIM-WITNESSED |
| `reply` | `{ sha256, bytes }` over the sanitized completion bytes exactly as written (protocol terminator excluded), or `null` unless `outcome == "COMPLETED"` | SHIM-WITNESSED |
| `model` | echo: `{ name: "echo/v1" }`. ollama: `{ name, endpoint, request_body_sha256, daemon_digest: { status: "FETCHED"\|"UNAVAILABLE", hex? } }` | `name` is DECLARED; `request_body_sha256` is SHIM-WITNESSED over the exact JSON POSTed; `daemon_digest` is DAEMON-DECLARED even when `FETCHED` (the shim witnesses the fetch, not the weights) |
| `seed` | `{ status: "NOT_SENT" }` in v1; `{ status: "SENT", value }` is schema headroom for a future `--seed` flag, not exercised by v1 | SHIM-WITNESSED as to what was sent, never a claim the daemon honored it |
| `outcome` | `"COMPLETED"` \| `"FAILED_CLOSED"` (adapter failure, nothing written; `reply` null) \| `"PROTOCOL_VIOLATION"` (overlong or unterminated prompt line; `prompt` null) | SHIM-WITNESSED |
| `seal` | `{ algorithm: "sha256", hex }` | integrity, not truth |

Deliberate exclusions: no plaintext prompt or reply (hashes only), no
floating-point fields anywhere in the sealed body (integers and strings only
-- this is what makes the cross-language seal trivially stable), no
invariant/oracle/verdict vocabulary.

`model.endpoint`, `model.request_body_sha256`, and `model.daemon_digest` are
OMITTED (not `null`) on an echo receipt: only `model.name` is present. This
differs from `prompt`/`reply`, which are always present as either an object or
an explicit JSON `null`.

## The seal and the cross-language canonicalization contract

Same idiom as the scientific receipt's `seal_receipt` /
`recompute_seal_hex` (`scientific_runtime.rs`): sha256 over the canonical
bytes of the receipt with `seal.hex` set to `""` and `seal.algorithm` fixed to
`"sha256"`. Because the emitter is Python (the shim) and the verifier is Rust
(buildc), the canonical form is pinned exactly, not assumed:

- UTF-8, compact separators (no whitespace): `serde_json::to_vec` on the
  Rust side; `json.dumps(obj, separators=(",", ":"))` on the Python side.
- Object keys in the FIXED schema order above. On the Rust side this falls
  out of the struct's field declaration order for free. On the Python side it
  requires building the dict with keys inserted in that exact order (Python
  dicts preserve insertion order; `json.dumps` does not re-sort unless asked
  to).
- Non-ASCII unescaped: `ensure_ascii=False` on the Python side, matching
  `serde_json`'s default.
- No floats anywhere in the schema, which sidesteps float-formatting
  divergence between the two serializers entirely.

The contract is enforced by a GOLDEN FIXTURE: one byte-exact receipt with its
known seal, committed in BOTH repos
(`compiler/tests/fixtures/model-receipt-golden.json` here; the same file,
same bytes, same seal, in local-model's `tests/fixtures/`), with a test in
each repo that recomputes the seal from the fixture and asserts it against the
pinned hex. The fixture is an echo-mode `COMPLETED` receipt: prompt `"ping"`
(sha256 `758d61f2...9411fe931`), reply `"echo: ping"` (sha256
`de2406a7...abe5afae`), pinned seal
`6bb2a09c47f5eaa2e3208a5eadcd6d57d1faffa74a567e024e920571c3794035`. If the
fixture tests ever disagree between the two repos, the cross-language contract
is broken and both sides know before any live artifact does.

## Verifying a receipt (`buildc receipt verify`)

`receipt verify` dispatches on the receipt's flat `/schema`, the same
lookup that already routes GPU cross-check, scientific-runtime, and
check-receipts. A model receipt is the fourth arm, implemented in
`compiler/src/model_receipt.rs` and wired into both the plain and `--json`
verify paths. It is **offline only**: there is no re-run, because the
artifact witnesses a PAST boundary crossing, not a re-derivable one.

Checks, in order:

1. **Structural.** The document deserializes into the typed schema
   (`MALFORMED` otherwise; a missing/unrecognized `/schema` is caught by the
   load-stage dispatch before this arm is reached, `SCHEMA_UNSUPPORTED`).
2. **Seal integrity** (`SEAL_MISMATCH`), recomputed BEFORE any sealed field is
   interpreted -- the same ordering discipline the scientific verifier uses,
   so every field-level rejection below is known to concern a genuinely
   author-sealed value, not an unsealed hand-edit.
3. **Digest well-formedness** (`DIGEST_MALFORMED`): `prompt.sha256`,
   `reply.sha256` (when present), `model.request_body_sha256` (when present),
   and `model.daemon_digest.hex` (when present) must each be 64 hex chars. An
   absent or malformed hash cannot masquerade as witnessed provenance.
4. **Status coherence** (`FIELD_CONTRACT_VIOLATION`), exactly three cases:
   `model.daemon_digest.hex` present alongside status `"UNAVAILABLE"`; outcome
   `"COMPLETED"` with a `null` `reply`; outcome `"PROTOCOL_VIOLATION"` with a
   present (non-null) `prompt`.

Deliberately **no new failure classes** for v1: the shared taxonomy with the
scientific verifier is a feature, not a gap. A reader of any buildc refusal
already knows these words.

What it cannot check, and says so in its human output line: anything about
the model itself. There is no re-run, so no claim about model quality,
weights, or determinism rides on a `VERIFIED` result.

`--prompt <file>` re-hashing (a SHOULD-level extra the design names, for
re-checking a held prompt against `prompt.sha256` and, for echo mode,
re-deriving the expected reply hash from the spec) is **not implemented in
v1**; the design marks it optional and not required for v1 acceptance.

## Chain integration

`receipt chain build`'s member-schema gate
(`compiler/src/main.rs::cmd_receipt_chain_build`) is an allowlist of two
schemas: `buildlang-scientific-runtime-receipt/v0` and
`buildlang-model-boundary-receipt/v0`. Nothing else about chain build or
chain verify changed: `source` extraction already reads a top-level `source`
field present on both schemas, chain seal computation only ever touches
`(index, receipt_seal)` pairs, and `receipt chain verify`'s member
re-verification shells out to `buildc receipt verify <member>`, which is
exactly the arm this document describes.

This makes the propose/dispose demo a single chain: a model receipt (the
proposal crossing) as one member, a Model-FREE disposer kernel's
scientific-runtime receipt (checking the proposed value) as the other,
bound in order. `CAPABILITY_INADMISSIBLE` never fires in this demo, because
the disposer kernel does not observe `Model` -- the propose/dispose rule is
demonstrated by what each chain member IS, not bent to fit. Tampering the
model member (without re-sealing) breaks the chain at re-verification with
`CHAIN_LINK_UNVERIFIED`, exactly like a tampered scientific member would.

Honest limit: the chain proves co-presence and order, not data flow. That the
disposer consumed the proposer's output is a hash equality a reader can check
by hand across the two sealed artifacts (the model receipt's `reply.sha256`
against the disposer's witnessed input); the chain machinery has no
cross-member field-relation check in v1.

## Not a corpus member

`examples/scientific-corpus.json` and the `29/29` corpus count
(`buildc receipt corpus`) are about scientific-runtime receipts emitted from
`.bld` kernels. A model receipt has no invariant to classify PASS or
FAIL_EXPECTED against and is emitted by a different program entirely (the
shim, not buildc), so it is not corpus-shaped -- this is by construction, not
an oversight, and the corpus count is unchanged by this schema landing. The
`10/10` `--self-test` count is likewise scientific-runtime-only (its tamper
table is built from `ScientificRuntimeReceipt`); the model arm's tamper
coverage lives in `compiler/src/model_receipt.rs`'s unit tests (seal mismatch,
each named `FIELD_CONTRACT_VIOLATION` case, `DIGEST_MALFORMED`, `MALFORMED`,
and the golden-fixture reseal pin) and `compiler/tests/cli.rs`'s CLI-level
tests (the same tamper shapes through the real `buildc` binary, plus the
propose/dispose chain and its tampered-member break).
