# Build Data Format (BDF) v0 - Design

Date: 2026-06-29
Status: design (approved direction; implementation to follow brick by brick)
Scope: the native data and information interface through which tools function together.

## Why

Tools in the ecosystem interoperate today via ad-hoc JSON text over stdin and stdout.
The flagships share a `project-telos.flagship-action/v1` envelope, which is good, but it
is JSON only, schema by convention, with no typed boundary and no record of which
capabilities a data exchange exercised. The native data suite (qdb, qsql, qkv, qjq,
qcsv), itself written in buildlang, is siloed: the tools do not share a data ABI or a
serialization contract. So "all tools function natively together" is, honestly, JSON by
habit rather than a verified, effect-typed, buildlang-native interchange.

BDF is the format that makes the interchange first class: a value carries a typed shape,
the capability effects the producing action exercised, and a re-checkable receipt. It is
built on three pieces buildlang already has and has verified: the algebraic-effects type
system, the receipt model, and the serializable MIR interlingua (`buildlang.mir/v0`),
whose lossless-encoding discipline BDF reuses.

## BDF v0 - the format

A compact, self-describing encoding with a canonical JSON projection for debugging and
host interop.

- Tagged fields with length prefixes, so a reader can skip unknown fields (forward
  compatibility).
- A schema id and version at the envelope head (`buildlang.bdf/v0`), like the MIR
  envelope.
- Canonical encoding (deterministic field order) so a content hash is stable. The digest
  is the receipt anchor.
- Lossless scalars: reuse the MIR float discipline (the IEEE-754 bit pattern for f64) so
  NaN, the infinities, negative zero, and subnormals round-trip bit-exact.
- Dual mode: `to_json` and `from_json` round-trips for the transition period, so a tool
  can speak JSON to legacy hosts and BDF to native peers, byte-for-byte reconcilable.

## The effect-typed envelope

```
BdfMessage {
  schema:        "buildlang.bdf-message/v0",
  produced_by:   { tool, tool_version },
  effects:       [Capability],     // FileSystem | Network | Process | Gpu | Environment | Console | Clock | Foreign
  payload_schema: <schema id>,
  payload:       <BDF value>,
  receipt:       { sha256, derived_from: [sha256], method },
  next:          [ { tool, action, reason } ]   // continuation, like flagship-action next_actions
}
```

Invariant carried from the reconcile spine and the rl-scaling receipt work: admission is
separate from verification. The envelope records what was produced and the effects it
claimed; a separate gate decides allow, block, escalate, or require-review; Crucible
decides MATCH, DRIFT, or UNVERIFIABLE. These are never collapsed into one another.

## Implementation bricks (no big bang; dogfood)

1. **Format + envelope + JSON adapter** in the buildlang toolchain, with round-trip
   golden tests (mirroring the MIR interlingua proof): for representative payloads,
   value -> BDF -> value is structurally identical, and BDF <-> canonical JSON round-trips
   losslessly. Reuse the lossless-float module from the MIR work.
2. **Flagship-action bridge**: a thin adapter so `project-telos.flagship-action/v1` JSON
   and a BdfMessage round-trip losslessly. Existing flagships keep working; BDF becomes
   available without rewriting them.
3. **First native exchange**: port one data-suite tool (qkv or qjq, already buildlang) to
   read and write BDF with effect-typed file-system boundaries, in dual JSON and BDF mode.
   Prove two tools exchange BDF, not JSON, with compile-time capability rows. This is the
   demonstrable "tools speak buildlang natively" milestone.
4. **Broaden**: index, gather, forum, crucible, and telos gain native BDF emit and consume
   behind the same adapter; JSON remains the host-neutral fallback.

## Why this is foundation, not a side feature

- It makes the effects system load-bearing across the whole portfolio: every exchange is
  capability-typed, which is what an enterprise or lab needs in order to leave an agent
  loop running and later know exactly what touched the disk, the network, or paid compute.
- It makes the receipt model the default interchange anchor: every message hashes, every
  derivation is recorded.
- It is the data layer the local model foundry consumes: rollouts, rewards, verifier
  verdicts, and compute leases all become BdfMessages with declared effects, per the
  rl-scaling receipt spine.

## Honest bounds

Replacing JSON everywhere is a multi-step migration. v0 is the format, the envelope, the
JSON adapter, and one ported tool. "All tools speak it" is earned tool by tool, dual mode
during the transition. No claim that the silos are unified until two real tools exchange
BDF with verified effect rows.

## Success criteria

- A versioned BDF codec with a canonical JSON projection and lossless round-trip golden
  tests, green in CI.
- A flagship-action/v1 <-> BdfMessage bridge proven by round-trip tests.
- At least one pair of tools exchanging BDF (not JSON) with declared, checked capability
  rows.
- Unknown schema versions fail closed (UNVERIFIABLE or ERROR), never silently.
