# Model boundary receipts v1 (design)

Status: DRAFT for review. Design only: no implementation, no plan tasks.
Register: internal (.superpowers/sdd). Date: 2026-07-29.
Scope: v1 of the boundary receipt the five-modes brief promised for the Model
capability ("receipted like any other boundary crossing: model digest, prompt
hash, parameters, seed"), designed so the propose/dispose rule is never
weakened: the scientific-receipt path keeps refusing Model-observing programs
(CAPABILITY_INADMISSIBLE), and a model receipt is a DIFFERENT artifact kind
that cannot masquerade as scientific evidence.

Ground truth read for this design (all verified in-tree, high confidence):

- Slice 4 plan: docs/superpowers/plans/2026-07-28-model-capability.md (honest
  scope, line 9: v0 ships NO model receipts; they belong harness-side).
- Shipped emit refusal: compiler/src/main.rs:7799; shipped verify refusal:
  compiler/src/scientific_runtime.rs:2291-2293 (CAPABILITY_INADMISSIBLE on the
  RE-DERIVED capability union).
- The runtime client: build_model_complete in compiler/src/codegen/runtime.rs
  (one prompt line out, read to CLOSE, trim one trailing \n and a preceding \r).
- Seal idiom: seal_receipt / recompute_seal_hex, scientific_runtime.rs:1319-1335
  (sha256 over canonical bytes with seal.hex blanked, algorithm fixed).
- Chain machinery: ReceiptChainLink / receipt_chain_seal_hex / build_receipt_chain,
  scientific_runtime.rs:1579-1648; cmd_receipt_chain_build main.rs:1874-1933;
  cmd_receipt_chain_verify main.rs:1939-2018.
- Verifier schema dispatch: cmd_receipt_verify main.rs:2656-2701 (three artifact
  kinds already dispatch: gpu at 2679, scientific-runtime at 2695, check at 2698).
- The shim: local-model branch feat/model-shim, commit fefecd0c,
  harness/model_shim.py + tests/test_model_shim.py (echo + ollama modes; the
  ollama path is UNTESTED-LIVE, hardware gated; fail-closed writes nothing).

## 0. Problem and non-goals

Slice 4 deliberately shipped a model call with no record of what was asked,
what came back, or what served it. This design adds that record WITHOUT
touching the admission rule. Non-goals for v1: no quality or correctness
claims about completions; no in-language changes (no new builtins, no receipt
fields on the scientific side); no live-model verification (hardware gated);
no plaintext capture by default.

The thesis sentence, up front: a model receipt is a PROVENANCE artifact about
a boundary crossing. It carries no invariant, no oracle, no verdict, and no
field that could be mistaken for one, by construction. Models propose; this
artifact witnesses THAT they proposed and WHAT bytes crossed, nothing more.

## 1. Decision: the shim emits the receipt

**Chosen: (a) the shim emits.** One receipt per connection, written as a JSON
artifact when (and only when) the shim is started with a new `--receipt-dir`
flag. No flag, no receipt, byte-identical behavior to today.

Why the shim: it is the only party that observes all three fact families at
once: the prompt bytes as received, the reply bytes as written, and its own
adapter identity (mode, endpoint, model name, what the daemon declared). The
compiled program's runtime cannot emit this artifact: it is candidate-side and
untrusted by thesis (a receipt about the proposer written by the proposer is
not evidence). buildc at `run` time sees neither prompt nor reply: the TCP
session is program-to-shim.

**Killed: (b) a buildc `model-proxy` mode.** It would put live model traffic
inside the compiler, which the slice 4 plan explicitly placed on the harness
side of the seam ("the model adapter... lives on the harness side of this
seam, never in the compiler"). Worse, it buys no trust: a proxy witnesses
transport bytes one hop earlier than the shim does, but still cannot witness
adapter identity (the model name and daemon digest live behind the shim's
HTTP hop), and both processes are operator-run on the same host, so there is
no trust gradient in which buildc's word outranks the shim's. Scope growth
with no added claim.

**Killed for v1: (c) both, layered.** Two emitters of the same facts is double
schema surface for zero additional claim while shim and buildc share a trust
domain. Revisit only if a real trust gradient appears (for example the shim
running on remote hardware while buildc runs locally); the schema below does
not preclude a second, independently sealed witness later.

Trust framing, stated in the artifact's own vocabulary: every field is tagged
either SHIM-WITNESSED (the shim observed the bytes or performed the act
itself) or DECLARED (someone's say-so passed through: the operator's model
name argument, the ollama daemon's self-reported digest, the shim host's wall
clock). The seal makes tampering evident; it does not upgrade a declaration
into a witness.

## 2. The artifact

Schema tag: `buildlang-model-boundary-receipt/v0` (flat top-level `schema`
string plus top-level `seal`, mirroring the scientific receipt's shape so the
existing chain pointers `/schema` and `/seal/hex` read it unchanged).

Filename: `model-receipt-<utc-compact-stamp>-<nonce8>.json` in `--receipt-dir`
(nonce: 8 hex chars of urandom, collision guard only, not a claim).

Fields, in canonical (sealed) order:

| field | content | epistemic tag |
|---|---|---|
| `schema` | the tag above | structural |
| `source` | `model:<mode>:<name>` (e.g. `model:echo:echo/v1`, `model:ollama:llama3.2`) | DECLARED label; exists so ReceiptChainLink.source (scientific_runtime.rs:1592) carries a human-readable member label with zero chain-code change |
| `shim` | `{ name: "model_shim.py", version: <const>, mode: "echo"\|"ollama" }` | SHIM-WITNESSED (self-identity) |
| `session` | `{ listen: "host:port", nonce, request_received_utc, reply_written_utc\|null }` | timestamps SHIM-CLOCK-DECLARED (ordering witnessed, wall accuracy is the host's) |
| `prompt` | `{ sha256, bytes }` over the RAW prompt-line bytes as received, after stripping the single trailing `\n` and one preceding `\r`, BEFORE utf-8 decode | SHIM-WITNESSED. Raw bytes, not the decoded string: `_read_prompt_line` decodes with errors="replace", which is lossy; the boundary fact is bytes |
| `reply` | `{ sha256, bytes }` over the sanitized completion bytes exactly as written, EXCLUDING the protocol-terminator `\n` | SHIM-WITNESSED. This equals sha256 of the string the program observed (the client trims exactly that terminator), which is what makes downstream binding possible (section 6) |
| `model` | echo: `{ name: "echo/v1" }`. ollama: `{ name: <DECLARED string>, endpoint: <base URL>, request_body_sha256, daemon_digest: { status: "FETCHED"\|"UNAVAILABLE", hex? } }` | name is DECLARED (a string is not a digest); request_body_sha256 is SHIM-WITNESSED over the exact JSON POSTed to /api/generate, and is the parameters witness by construction (model, prompt, stream flag, and any future options all live inside that body); daemon_digest is DAEMON-DECLARED even when FETCHED (section 3) |
| `seed` | `{ status: "NOT_SENT" }` in v1 (the shim sends no options.seed); when a --seed flag lands: `{ status: "SENT", value: <int> }`, and the value also rides inside request_body | SHIM-WITNESSED as to what was SENT; never a claim the daemon honored it |
| `outcome` | `"COMPLETED"` \| `"FAILED_CLOSED"` (adapter failure, nothing written; reply is null) \| `"PROTOCOL_VIOLATION"` (overlong or unterminated prompt line; prompt is null) | SHIM-WITNESSED. Refusals get receipts too: the fail-closed path is a boundary fact worth witnessing |
| `seal` | `{ algorithm: "sha256", hex }` | integrity, not truth |

Deliberate exclusions: NO plaintext prompt or reply (hashes only; the receipt
is shareable, and whoever holds the plaintext can re-hash to check it), NO
floating-point fields anywhere in the sealed body (integers and strings only;
durations if ever added are integer milliseconds), NO invariant/oracle/verdict
vocabulary. The no-floats rule is what makes the cross-language seal below
trivially stable.

**Seal and the cross-language canonicalization contract.** Same idiom as
seal_receipt (scientific_runtime.rs:1319): sha256 over the canonical bytes of
the receipt with `seal.hex` set to `""` and `seal.algorithm` fixed to
`"sha256"`. Because the emitter is Python and the verifier is Rust, the
canonical form must be pinned, not assumed: UTF-8, compact separators (no
whitespace), object keys in the FIXED schema order above (matching the Rust
struct's serde field order; serde_json::to_vec preserves it), non-ASCII
unescaped (Python: `ensure_ascii=False`, matching serde_json), no floats. The
contract is enforced by a GOLDEN FIXTURE: one byte-exact receipt with its
known seal committed in BOTH repos, with a test in each repo that recomputes
the seal from the fixture bytes. If the fixture tests disagree, the contract
is broken and both sides know before any artifact does.

## 3. What the receipt claims and refuses

Claims (all offline-checkable): these exact bytes crossed the boundary, in
this session, in this order, under this shim mode, and this is what the
adapter's daemon declared about itself at the time. Nothing else.

Refuses, explicitly, in the doc and in the schema's absence of fields:

- Quality or correctness of the completion. No field exists to carry it.
- That `model.name` corresponds to any particular weights. An ollama model
  name is a string the operator typed, tagged DECLARED.
- That `daemon_digest.hex` corresponds to the weights actually consulted. The
  adapter CAN fetch a real digest: spec is `GET <endpoint>/api/tags`, match
  the entry whose name equals `model.name`, take its `digest` field (exact
  JSON shape: moderate confidence, from memory; pin it during the gated live
  session, which the ollama path needs anyway as UNTESTED-LIVE). But even
  when FETCHED, the digest is the DAEMON'S declaration about itself; the shim
  witnesses the fetch, not the weights. Status UNAVAILABLE keeps the receipt
  valid and honest: the model block is then fully DECLARED and says so. A
  `hex` present alongside status UNAVAILABLE is a field-contract violation.
- Determinism. Not claimed even for echo mode in the sealed fields; echo's
  reply is re-derivable ("echo: " + prompt) and the verify arm MAY offer that
  re-check when handed the prompt (section 5), but re-derivability is a
  property of the spec, not a sealed claim.
- That the reply derived from the prompt at all. The shim observed a request
  and a response on one connection; causality inside the daemon is not
  witnessed and never stated.

## 4. Wire contract v1.1: the wire does not change

The shipped client (build_model_complete) writes the prompt line and then
reads to CONNECTION CLOSE, trimming one trailing `\n` (and a `\r` before it).
Every byte the shim writes lands in the program's returned string. Therefore
ANY in-band addition (a header line before the completion, a trailer after
it, a length prefix) is not a compatible extension: it does not break the
transport, it silently corrupts the completion every v1 client returns. The
line protocol also has no channel in which a client could negotiate ("I
understand headers"), so in-band versioning is unreachable from here.

Consequence, stated precisely: model identity travels OUT-OF-BAND, into the
receipt only. Wire v1.1 is byte-identical to wire v1; the ".1" names the SHIM
CONTRACT, not the wire grammar: a v1.1 shim additionally emits one boundary
receipt per connection when started with `--receipt-dir`. v1 clients (the
shipped runtime, the cli.rs TCP-listener test) are untouched and cannot
observe the difference. If a future protocol truly needs in-band metadata, it
is a v2 wire with a new builtin or an explicit env-var opt-in on the client
side; out of scope here and probably never needed, because the receipt is the
metadata channel.

## 5. The `receipt verify` model arm (buildlang side)

cmd_receipt_verify already dispatches over artifact kinds: gpu cross-check
receipts route at main.rs:2679 (verified as pure JSON + SHA-256, no Vulkan,
which is the exact precedent: buildc verifying an artifact it did not emit,
offline), scientific-runtime at 2695, check receipts at 2698. The model
receipt becomes the fourth arm on the same flat `/schema` lookup: a typed
struct in a new module (or a sibling section of scientific_runtime.rs),
recompute-seal in the reseal idiom, then field contracts. Both the plain and
`--json` paths get the arm (they share the dispatch shape; chain verify uses
the plain path).

What offline verification of a model receipt CHECKS: seal integrity
(SEAL_MISMATCH), schema and structure (MALFORMED / SCHEMA_UNSUPPORTED at the
load stage via receipt_load_failure, main.rs:2797, which already fires before
schema dispatch), digest well-formedness (DIGEST_MALFORMED: 64 hex chars, the
existing rule that an absent hash cannot masquerade as witnessed provenance),
and status coherence (FIELD_CONTRACT_VIOLATION: daemon_digest hex present
with UNAVAILABLE; a COMPLETED outcome with a null reply; a PROTOCOL_VIOLATION
with a present prompt). Deliberately NO new failure classes in v1: the shared
taxonomy is a feature; a reader of any buildc refusal already knows these
words.

What it cannot check, and says so in its output line: anything about the
model (there is no re-run; the artifact witnesses a past crossing). One
SHOULD-level extra: `receipt verify <r> --prompt <file>` re-hashes the given
bytes against `prompt.sha256`, and for echo mode additionally re-derives the
expected reply hash from the spec. Cheap, offline, and it makes the golden
fixture self-demonstrating. Not required for v1 acceptance.

The scientific verifier is UNTOUCHED. CAPABILITY_INADMISSIBLE
(scientific_runtime.rs:2293) keeps firing on any scientific receipt whose
re-derived capabilities include Model. The two artifact kinds share a seal
idiom and a verifier binary, never a claim vocabulary.

## 6. Chain integration and the propose/dispose demo

The precise code answer first: **the shipped chain machinery does NOT bind
both artifact kinds as-is.** `cmd_receipt_chain_build` refuses any member
whose flat `schema` is not SCIENTIFIC_RUNTIME_SCHEMA (main.rs:1886-1893), so
a model receipt cannot become a link today. The verify side, however, is
already agnostic: the chain seal binds only (index, member seal) pairs
(receipt_chain_seal_hex, scientific_runtime.rs:1614-1624), seal pinning reads
the schema-agnostic pointer `/seal/hex` (main.rs:1978-1981), and member
re-verification shells out to `buildc receipt verify <member>`
(main.rs:1989-1991), which is exactly the schema dispatch section 5 extends.

So the chain extension is two small, separable edits:

1. Chain build: widen the main.rs:1886 gate from a single-schema equality to
   an allowlist { scientific-runtime/v0, model-boundary-receipt/v0 }. The
   `source` extraction at main.rs:1902 needs no change: the model receipt
   carries a top-level `source` label precisely so this line keeps working.
2. Receipt verify: the model arm (section 5). Chain verify then works with
   ZERO changes: pinned seals and subprocess re-verification compose.

Member-kind substitution is already caught: the member's `schema` sits inside
its own sealed body, so swapping artifact kinds under a pinned seal is
CHAIN_LINK_TAMPERED, and re-sealing changes the seal, which is
CHAIN_SEAL_MISMATCH.

**The demo's final shape.** Three artifacts: the shim's model receipt (the
proposal crossing), a scientific receipt over a Model-FREE disposer kernel
that checks the proposed value (so CAPABILITY_INADMISSIBLE never fires; the
rule is demonstrated by what each member IS, not bent), and a chain manifest
binding the two in order (proposer link 0, disposer link 1; the two-member
minimum at main.rs:1875/1946 is exactly met). `receipt chain verify` then
re-checks order, membership, both seals, and both members through one
verifier. That is the propose/dispose thesis as a single command.

Honest limit, stated in the demo doc and here: the chain proves co-presence
and order, NOT data flow. The fact that the disposer consumed the proposer's
output is carried by hash equality a reader can check across the two sealed
artifacts (the model receipt's `reply.sha256` against the disposer run's
witnessed input provenance), because `reply.sha256` was defined in section 2
to hash exactly the string the program observed. Automating that equality as
an optional cross-member binding check is a chain v1.1 candidate, not v1: the
chain machinery has no cross-member field relations today, and adding one is
a schema change to the manifest, not a widening.

## 7. Cross-repo split

| lands in | what | why there |
|---|---|---|
| local-model (follow-on to feat/model-shim) | `--receipt-dir` emission in harness/model_shim.py; receipt construction + Python-side seal; tests in tests/test_model_shim.py (echo receipts end-to-end over a real socket, fail-closed receipt cases, seal recompute); the golden fixture + its test | the emitter is the shim and the shim lives there; its tests already mock urllib at the network boundary |
| buildlang | docs contract of record (a Model-receipt section: SCIENTIFIC-RECEIPT.md grows a pointer paragraph, the schema itself gets docs/MODEL-RECEIPT.md, continuing the pattern where the shim commit cited SCIENTIFIC-RECEIPT.md as its contract source); the `receipt verify` model arm + typed schema; the chain-build allowlist widening; the golden fixture + tamper-table tests (SEAL_MISMATCH, DIGEST_MALFORMED, FIELD_CONTRACT_VIOLATION, and the chain cases: model member pre-widening refused, post-widening chained, tampered model member fails CHAIN_LINK_TAMPERED) | one verifier for every artifact kind is the established shape (the gpu arm proves it); chain verify REQUIRES the arm because it subprocesses `receipt verify`; and the demo's audience runs one binary |

Sequencing note: buildlang's docs/MODEL-RECEIPT.md is the contract; the shim
implements it; the fixture pins both. Either implementation slice can land
first behind its flag, but the fixture must be identical bytes in both repos
before either claims done.

## 8. Honest scope: what v1 does not claim

- Everything in v1 is offline-verifiable: echo-mode receipts over a real
  local socket, golden fixtures, tamper tables, chain build/verify with the
  widened gate. No live network calls in any test, matching the shim commit's
  own discipline.
- The ollama path stays UNTESTED-LIVE until the hardware-gated session runs;
  the daemon_digest fetch spec (the /api/tags shape) is pinned in that same
  session. Until then the ollama receipt path is unit-tested with urllib
  mocked, exactly like the completion path already is.
- The live propose/dispose demo (real model proposing, buildc-verified kernel
  disposing) is gated on the same hardware and says so wherever it is
  mentioned. The chain demo is buildable TODAY with an echo-mode receipt, and
  that is the v1 acceptance demo: the epistemics are identical, only the
  proposer is boring.
- No public-surface copy changes in this design; register stays internal
  until the feature ships and earns its user-facing paragraph.

## 9. Open questions for review

1. Module placement of the verify arm: a new compiler/src/model_receipt.rs
   (mirroring gpu_receipt's separation) vs a section in scientific_runtime.rs.
   Leaning new module: the point of the artifact is that it is NOT scientific.
2. Should FAILED_CLOSED receipts be on by default once --receipt-dir is set,
   or behind a second flag? Leaning on by default: a refusal is a boundary
   fact, and silent refusal receipts cost nothing.
3. Does the demo's disposer kernel read the proposed value from stdin or from
   a file? Whichever the existing corpus idiom prefers; it only affects how
   the reply-hash equality is presented, not the schema.
