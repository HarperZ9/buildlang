# BuildLang as a Fin-Sec / Blockchain Host — Direction & Spike

> Status: **exploratory spike** (2026-06-30). This documents the direction, an
> honest fit assessment, a working first demonstration, and the dependency-ordered
> brick plan. Nothing here is production. The framing mirrors `QUANTUM-HOST.md`:
> the same effect-typed, multi-target language hosts financial and on-chain logic,
> with the parts that need rigor (resources, determinism, exact money) enforced by
> the type system and lowered to C/LLVM.

## Why the fit is real (and where it is not)

**Architectural advantages that map onto fin-sec and blockchain:**

1. **Linear types are the shared keystone.** A `#[linear]` value is consumable at
   most once. That IS no-double-spend for an on-chain asset and resource-handle
   safety for a settlement obligation — the *same* feature already built for
   quantum no-cloning (see `docs/LINEAR-TYPES.md`). One type-system property,
   three domains. `examples/linear/coin.bld` is the no-double-spend demo today.
2. **Effects are the audit boundary.** Both domains live or die on *what touched
   what*: a settlement that performs IO, a contract that reads chain state. The
   existing capability-effect system (`~ Console`, `~ FileSystem`, accountability
   receipts with SHA-256 source digests) is the natural place to make money-moving
   and state-changing effects explicit and re-checkable.
3. **Determinism + the C anchor.** Blockchain execution must be bit-for-bit
   reproducible across nodes; fin-sec must be exact and reproducible for audit.
   BuildLang's C backend is the production-verified target, and the language has
   no hidden float in integer paths.

**The load-bearing gaps (this is what the brick plan exists to close):**

- **Overflow is unspecified.** Integer arithmetic has no defined overflow
  behavior yet. For money and consensus that is a critical-bug class: a silent
  wrap is a minted-coin or a mispriced trade. **Deterministic checked / wrapping
  / saturating arithmetic is the decisive first brick** for both domains.
  *(Related foundation fix landed 2026-06-30: unsuffixed integer literals over
  i32 range were being silently truncated to 32 bits in both the type checker and
  the MIR lowering — e.g. a 64-bit value printed as `-808`. Now widened to i64 /
  i128, so wide money/hash literals are exact. Also fixed: an `Option<i64>`-return
  + match miscompiled the 64-bit payload as `int32_t` (the if-expression result
  local took the `None` branch's i32 default); `lower_if` now retypes the result
  local to the aggregate branch type, so `examples/finance/checked.bld` returns
  `Option<i64>` directly and runs end-to-end.)*
- **No native decimal / fixed-point.** Money must be exact in minor units; float
  is forbidden. Today you hand-roll integer cents (the spike does). A native
  `decimal` / fixed-point type is the fin-sec brick.
- **No `u256` / big integers.** The EVM word is 256-bit; balances and hashes need
  wide integers. BuildLang has `i8..i128` / `u8..u128` but nothing wider.
- **No cryptographic hashing / signatures.** A real chain needs keccak256 /
  sha256 and signature verification. These are reachable through the existing
  native C-ABI FFI (`extern` + `header`/`link`), not by reimplementation — the
  same on-ramp the FFI subsystem already provides.

## The spike — exact money + a tamper-evident chain, running today

`examples/finance/ledger.bld` expresses both cores with only existing primitives
and runs end-to-end (BuildLang -> C -> MSVC -> run):

```
balance cents 97401
genesis 1582441088
b1 726415890
b2 1038788747
tamper-evident (1=yes, changing an amount changes the hash) 1
```

- **Fin-sec core:** money as integer cents. `$1,000.00 - $25.99 = $974.01` lands
  *exactly* as `97401` cents — no float, fully reproducible.
- **Blockchain core:** three blocks linked by a deterministic hash of
  `(prev_hash, amount, nonce)`; recomputing block 1 with a tampered amount
  (`2599 -> 9999`) yields a different hash, so the chain is tamper-evident.

The hash is a deterministic *non-cryptographic* mix with explicit modular
reduction — chosen precisely so it does **not** rely on the unspecified-overflow
gap. That honesty is the point: the spike proves expressibility + determinism
today, and names exactly what each brick adds.

## The path (in dependency order)

1. **Deterministic checked / wrapping / saturating arithmetic** — the decisive
   first brick, shared by both domains. `checked_add(a, b) -> Option<T>` (None on
   overflow), `wrapping_*`, `saturating_*`, lowered to C overflow-checked
   intrinsics. Builds on the existing Option lowering and the builtin/runtime
   pattern. *This is the next concrete piece of work for this prong.*
2. **`decimal` / fixed-point type** — exact money, fin-sec. A scaled-integer type
   with checked arithmetic from brick 1.
3. **`u256` / big integers** — EVM-width integers for balances and hashes.
4. **Cryptographic primitives via FFI** — keccak256 / sha256 / signature verify,
   linked through the existing C-ABI FFI, not reimplemented.
5. **Linear assets + effect-typed settlement** — compose `#[linear]` (no
   double-spend) with capability effects (audited money movement) into a small
   end-to-end ledger / settlement example once bricks 1–2 land.

Bricks 1–3 are language + runtime work; brick 4 reuses the FFI machinery that
already exists; brick 5 composes features already present.

## Bottom line

Today there is no native `decimal`, `u256`, checked arithmetic, or crypto *in the
compiler* — but the **decisive shared keystone (linear no-cloning) exists**, the
spike shows the language already expresses and runs exact money and a
tamper-evident chain deterministically, and the design (effects + linear types +
C/LLVM determinism + native FFI) is a well-suited foundation. The next brick is
deterministic checked/wrapping/saturating arithmetic — it closes the single
gap (unspecified overflow) that both fin-sec and blockchain cannot tolerate.
