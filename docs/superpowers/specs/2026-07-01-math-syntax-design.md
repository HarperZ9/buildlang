# Design: Math Syntax (Pillar B)

Status: draft for execution (2026-07-01), branch `feat/math-syntax`. Governed by
`docs/UNIVERSAL_SUBSTRATE_DIRECTIVE_2026-06-30.md` (the honest scientific language: Julia
ergonomics + the effects/linear wedge). Second Julia pillar after multiple dispatch.

## Summary

Add Julia-flavored mathematical syntax to buildlang inside the only fully verified path
(lexer to parser to HM type-check to MIR to C backend to native exe), in four independent,
independently mergeable increments:

- **I1** Unicode arithmetic-operator aliases (`×`, `÷`, `−`, `·`) to existing tokens.
- **I2** the `**` power operator wired to the already-complete `BinOp::Pow` backend.
- **I3** a `linalg` stdlib module: elementwise, dot, sum, norm, scale over dynamic `Vec<f64>`.
- **I4** broadcasting operators `.+ .- .* ./` over fixed-size `Array<T,N>` (incl. scalar
  broadcast), with compile-time length checking.

Honest scope wording: this ships **elementwise broadcasting over fixed-size arrays plus a
1-D vector-algebra library over dynamic `Vec<f64>` plus a power operator and Unicode
aliases**. It is NOT "Julia-parity linear algebra." A true dynamic 2-D `Matrix{T}` with
matmul/transpose/solve is a tracked follow-on (there is no N-D MIR type today).

## Motivation

Julia's defining surface for scientific computing is mathematical notation: broadcasting
(`a .+ b`), a power operator, and Unicode math symbols, over real array algebra. buildlang
today has:

- `BinOp::Pow` fully defined (`ast/operators.rs:32`), typed (`infer.rs:3428`), and lowered by
  every backend (C `rvalue_to_c` early-returns `pow(l,r)` at `c.rs:3557`), but **never
  produced by the parser** (no `**` token; `parse_binary_op` has no arm). It is orphaned
  dead plumbing.
- No broadcasting operators of any kind. No elementwise vector algebra beyond the fixed
  graphics structs `build_vec2/3/4` / `build_mat4`.
- Unicode LETTERS already work as identifiers (`cursor.rs:222` uses `unicode_xid`), but
  Unicode operator SYMBOLS raise `LexerErrorKind::UnexpectedChar` (`scanner.rs:406`).

Everything needed exists and is verified by five subsystem maps (see the SDD progress
ledger). The work is additive wiring, not new infrastructure.

## Scope

**In this branch:** I1, I2, I3, I4.

**Deferred (tracked follow-ons, do not build here):**
- Dynamic-`Vec<f64>` broadcasting operators. I4 targets fixed `Array<T,N>` (see the design
  choice below); extending `.+` to dynamic `Vec` needs a runtime length check and the
  Vec-loop lowering, a separate increment.
- True dynamic 2-D `Matrix{T}` and linear algebra (matmul, transpose, solve). No N-D MIR type
  exists (`ir.rs:976-1014`; `MirType::Vector` is fixed-lane SIMD, `mat4` is a hardcoded
  struct). This is a branch of its own.
- `f32` / `Float32` element parity. `Vec<f32>` silently routes through the f64 runtime
  handles today (`macros.rs:1171`), so a partial f32 path corrupts reads via element-size
  mismatch. Everything here is f64.
- Broadcast comparisons (`.== .< .>`), `.^`, and Unicode operators with no ASCII `BinOp`
  equivalent (`⊗`, `∘`). Additive later once I4 proves the dot-op lexing and precedence.

**Honest parity assessment:** I1 and I2 are largely ergonomic sugar (familiar, low-substance).
I3 and I4 are the load-bearing content: real elementwise vector algebra on the language's
array types. That is the reason to ship the branch.

## Architecture per increment

### I1 — Unicode arithmetic-operator aliases

Add match arms in `scanner.rs scan_token()` (among the single-char arms, before the
`c if is_id_start(c)` fallback at `scanner.rs:403`) that map a conservative set of non-XID
Unicode symbols to EXISTING `TokenKind`s: `×` (U+00D7) and `·` (U+00B7) and `∙` (U+2219) to
`Star`; `÷` (U+00F7) to `Slash`; `−` (U+2212 minus sign) to `Minus`. No enum change, no AST,
parser, type, or codegen change. These chars error at the lexer today, so acceptance is
purely additive. Do NOT alias any XID character (would shadow identifiers) or any symbol
without an ASCII `BinOp` equivalent.

### I2 — `**` power operator

Wire the front-end to the finished Pow plumbing.

- **Lexer:** declare `TokenKind::StarStar` (after `Star`, `token.rs:191`; Display arm near
  `token.rs:391`). In `scan_star()` (`scanner.rs:439`) recognize `**` by eating a second `*`
  before the `=` check.
- **Parser deref hazard (load-bearing):** `**ptr` is double-deref today because `Star` is a
  prefix deref operator (`parser/expr.rs:288`). To preserve it, add a **prefix arm for
  `StarStar`** that parses the operand and wraps it in two `Deref` unary ops (equivalent to
  `*(*x)`). This keeps `**ptr` meaning double-deref in prefix position while `**` in infix
  position means power. This split is what makes I2 backward-safe regardless of any existing
  `**ptr`.
- **Parser infix:** in `infix_binding_power()` (`parser/expr.rs:776`) add
  `TokenKind::StarStar => Some((bp::POWER, bp::POWER))` right-associative, where `bp::POWER`
  is a NEW const in `mod bp` (`parser/expr.rs:18`). Placement: at/above `bp::PRODUCT` and
  consistent with unary-minus binding so `-2 ** 2` matches the documented choice (pin the
  chosen semantics with a test; Julia's `-a^b == -(a^b)`). In `parse_binary_op()`
  (`parser/expr.rs:829`) add `TokenKind::StarStar => BinOp::Pow`.
- **No type/codegen change.** `Pow` is already typed and lowered.

Semantics note (documented, not a blocker): C `pow()` is double-based, so `2 ** 10` yields
`1024.0` then converts to the expression's type; integer exactness beyond 2^53 is lossy.

### I3 — `linalg` stdlib module over `Vec<f64>`

buildc links the repo-root `stdlib/` (verified via `buildc doctor`). Stdlib modules are
source-level AST merges where only `ItemKind::Function` items are prefix-mangled and callable
by bare name after `mod linalg;` (`main.rs:6540-6601`). Struct/impl/generic items are
second-class, so this module is **free functions only**.

New file `stdlib/linalg.bld` over the existing dynamic `Vec<f64>` (fully wired:
`vec_new_f64`/`vec_push_f64`/`vec_get_f64`/`vec_len`, `runtime.rs:272-284,1663-1776`), template
`stdlib/algorithms.bld:6-35` with `Vec<i32>` to `Vec<f64>` and `vec_get` to `vec_get_f64`:

- `vec_add`, `vec_sub`, `vec_mul`, `vec_div` (elementwise, build a fresh result `Vec<f64>`),
- `vec_scale(v, s)`, `vec_scalar_add(v, s)` (scalar broadcast),
- `vec_dot(a, b)` (accumulate `a[i]*b[i]`), `vec_sum(v)`, `vec_norm(v)` (`sqrt(dot(v,v))`).

Names are distinct (`vec_dot`, `vec_norm`, NOT bare `dot`/`length`/`lerp`) to avoid clashing
with the graphics-vec builtins the lowerer dispatches (`runtime.rs:1818-1823`) and the
bare-name import model. `sqrt` comes from `stdlib/math` or `core`. Zero compiler/runtime
change; a new file no existing program imports.

### I4 — Broadcasting `.+ .- .* ./` over `Array<T,N>`

The flagship surface. **Target `Array<T,N>`, not dynamic `Vec`.** Rationale:

- Array literals `[1.0,2.0,3.0]` type as `Array<f64,3>` (`infer_array`, `infer.rs:3207` returns
  `Ty::array(elem, len)`) and lower to `MirType::Array` (`lower_array`, `expr.rs:5943`). So the
  natural user syntax `[1,2,3] .+ [4,5,6]` has `Array` operands; a Vec-guarded arm would never
  fire on it.
- The element count is in the type, so `[1,2] .+ [1,2,3]` is a **compile-time** length error,
  no runtime `DimensionMismatch`.
- Lowering is the simplest possible: an unrolled aggregate reusing the exact mechanism
  `lower_array` already uses (`AggregateKind::Array`).

Pieces:

- **Lexer:** declare `TokenKind::DotPlus/DotMinus/DotStar/DotSlash` (arithmetic block, after
  `Caret`, `token.rs:196`; Display arms after `token.rs:394`). In `scan_dot()`
  (`scanner.rs:555`), in the `else` arm after the failed second-`.` eat and before the
  is_digit float-dot fallback, peek `cursor.first()` for `+ - * /` and eat it, returning the
  new kind. This only promotes today's parse-errors (`.` then an operator char is never valid
  field access) to valid tokens; leading-dot floats (`.5`) and `a.method` field access are
  unaffected because those are not `.`-then-operator.
- **AST:** add `BinOp::DotAdd/DotSub/DotMul/DotDiv` (after `Pow`, `operators.rs:32`). Extend the
  method matches: `precedence()` (`.+/.-` reuse SUM level, `.*/./` reuse PRODUCT level),
  `associativity()` (left), `as_str()`, `is_arithmetic()`. Adding variants makes every
  exhaustive `ast::BinOp` match fail to compile; drive the full list with `cargo build` and
  fix each (`Expr::precedence()` at `ast/expr.rs:498`, `mir_representation.rs:573`, etc.).
- **Parser:** `infix_binding_power()` (`parser/expr.rs:776`) map `.+/.-` to `bp::SUM`, `.*/./`
  to `bp::PRODUCT`, left-assoc `(bp, bp+1)`. `parse_binary_op()` (`parser/expr.rs:829`) map the
  four dot tokens to the four broadcast `BinOp`s.
- **Type checker:** add broadcast arms to `infer_binary` (`infer.rs:3339`, do NOT reuse the
  scalar `unify`-and-return-left arm). Rules using `apply()`d operand types:
  - `Array<T,N> .+ Array<U,M>`: unify `T` with `U`; require `N == M` else emit a length-mismatch
    error; result `Array<T,N>`.
  - `Array<T,N> .+ S` (scalar): unify `T` with `S`; result `Array<T,N>`.
  - `S .+ Array<T,N>` (scalar-left): unify `S` with `T`; result `Array<T,N>`.
  - Anything else: `InvalidBinaryOp`.
- **Codegen:** insert a broadcast dispatch arm at the top of `lower_binary` (`expr.rs:786`,
  after the string special-cases, before the `build_vecN` struct arm at 787), guarded on the
  four `Dot*` `BinOp` variants. It reads both operands, and for a compile-time-known length `N`
  emits a fresh `MirType::Array(elem, N)` result whose elements are the per-element scalar MIR
  ops: for `k in 0..N`, `result[k] = left[k] <scalar-op> right[k]` (scalar broadcast uses the
  scalar for every `k`). Because the desugaring emits per-element **scalar** `MirRValue::BinaryOp`
  (Add/Sub/Mul/Div), the `Dot*` variants NEVER become MIR ops and NEVER reach any backend.
  This is why no backend file changes and the `_ => unreachable!` at `expr.rs:903` is never
  hit (the arm returns before that map). Model the aggregate construction on `lower_array`
  (`expr.rs:5943`); read array elements via the existing Array index path.

## Backward compatibility

Proven by the committed guards plus the manual sweep:
- `buildc corpus verify` re-runs all 8 semantic-corpus programs and byte-compares live stdout
  (`main.rs:3001-3045`); exercised in CI via `cargo test`.
- `transpile_preservation_c_and_rust_backends_agree_on_stdout` (`cli.rs:12688`) asserts C and
  Rust backends agree byte-for-byte on every Rust-capable corpus program.
- CI end-to-end compile proof over 10 fixed programs on ubuntu+windows, plus 4 negative tests
  that must stay rejected (`ci.yml:55-125`).
- The manual baseline-vs-feature differential sweep (build a baseline binary at the pre-branch
  commit in a throwaway worktree; run both binaries over every `.bld` in tests/programs +
  examples + demos + stdlib + registry + semantic-corpus; a regression is baseline-pass but
  new-fail). Additive syntax must leave every existing program's emitted C byte-identical.

Traps (all mitigated): `**ptr` double-deref (mitigated by the prefix-split); `.`-then-operator
adjacency (only promotes current errors); Unicode symbol aliases (only add acceptance for
chars that error today).

We do NOT add a semantic-corpus program for these features (that would churn the 8/8 receipt
set and the count assertions at `cli.rs:10670/10762`). Verification is via `cli.rs` e2e tests
and `tests/programs/*.bld` + `.expected` snapshots.

## Testing

Per increment, all f64:
- **I1:** lexer unit test (`×` lexes to `Star`) + `cli.rs` e2e (`6 × 7` prints `42`).
- **I2:** `cli.rs` e2e: `2 ** 10` prints `1024`; `2 ** 3 ** 2` prints `512` (right-assoc);
  `-2 ** 2` matches the documented choice; a parser test that `**ptr` still means double-deref.
- **I3:** `cli.rs` e2e: `mod core; mod linalg;` then `vec_dot([1,2,3],[4,5,6])` prints `32`,
  `vec_norm([3,4])` prints `5`. Optional `tests/programs/NNN_linalg.bld` + `.expected`.
- **I4:** `cli.rs` e2e: `a=[1.0,2.0,3.0]; b=[10.0,20.0,30.0]; a .+ b` prints `11,22,33`;
  `a .* 2.0` prints `2,4,6`; `2.0 .+ a` prints `3,4,5`; a length-mismatch program FAILS
  `buildc check` with a length error.
- **Regression each increment:** `cargo test` green; `cargo fmt --check` (run `cargo fmt` and
  commit; do not revert the drift files `lib.rs`, `types/check.rs`, `types/infer.rs`,
  `tests/cli.rs`); `cargo clippy -- -D clippy::correctness` green; `buildc corpus verify` 8/8;
  the differential sweep 0 regressions.

## Decomposition (each: impl to task-review gate; buildc-run verifiable)

- **I1** Unicode aliases. Files: `lexer/scanner.rs`, `tests/lexer/mod.rs`, `tests/cli.rs`.
- **I2** `**` power. Files: `lexer/token.rs`, `lexer/scanner.rs`, `parser/expr.rs`, `tests/cli.rs`.
- **I3** `linalg.bld`. Files: `stdlib/linalg.bld`, `tests/cli.rs`, optional `tests/programs/`.
- **I4** broadcasting over `Array<T,N>`. Files: `lexer/token.rs`, `lexer/scanner.rs`,
  `ast/operators.rs`, `parser/expr.rs`, `types/infer.rs`, `codegen/lower/expr.rs`, and every
  non-exhaustive `ast::BinOp` match surfaced by `cargo build`, `tests/cli.rs`.

## Risks

- **I4 blast radius.** New `ast::BinOp` variants break exhaustive matches. Mitigation: let
  `cargo build` enumerate them; the broadcast arm intercepts before the AST-to-MIR map so no
  backend needs a real Pow-style lowering.
- **`**ptr`.** Mitigated by the prefix-split; the differential sweep confirms no existing
  program changes.
- **fmt drift.** I3 and I4 touch `infer.rs`/`check.rs`/`cli.rs`. Run `cargo fmt` and commit.
- **Array vs Vec confusion for users.** Documented: operators work on fixed-size array
  literals; the `linalg` library works on dynamic `Vec<f64>`. Dynamic-Vec operators are a
  tracked follow-on.
