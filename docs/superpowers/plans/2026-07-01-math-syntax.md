# Math Syntax (Pillar B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to
> implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Julia-flavored math syntax in buildlang: Unicode operator aliases, the `**` power
operator, a `linalg` stdlib over `Vec<f64>`, and broadcasting `.+ .- .* ./` over `Array<T,N>`.

**Architecture:** Additive wiring inside the verified lexer to parser to HM to MIR to C path.
Broadcasting desugars to per-element scalar MIR ops, so no backend or MIR change is needed.
Design record: `docs/superpowers/specs/2026-07-01-math-syntax-design.md`.

**Tech Stack:** Rust compiler crate `buildlang` (bin `buildc`) under `compiler/`.

## Global Constraints

- All numeric math here is **f64 only**. Do NOT touch f32 (Vec<f32> silently aliases f64
  handles; a partial f32 path corrupts reads).
- **Backward compatibility is load-bearing.** Every existing program's emitted C must stay
  byte-identical (additive syntax only). Prove with the differential sweep before each merge.
- **fmt policy:** run `cargo fmt` and COMMIT the result. Do NOT revert the recurring drift
  files `compiler/src/lib.rs`, `compiler/src/types/check.rs`, `compiler/src/types/infer.rs`,
  `compiler/tests/cli.rs`. CI lint gate = `cargo fmt --check` (exit 0) AND
  `cargo clippy -- -D clippy::correctness` (exit 0).
- **No semantic-corpus program** for these features (avoids 8/8 receipt churn). Verify via
  `compiler/tests/cli.rs` e2e (the `c_backend_run` helper at `cli.rs:12586`, pattern at
  `cli.rs:12658`) and optional `tests/programs/*.bld` + `.expected`.
- Each e2e test guards with `if !c_backend_ready() { return; }`.
- No em-dashes in any operator-facing text or docs.
- Commit after each task with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

---

### Task I1: Unicode arithmetic-operator aliases

**Files:**
- Modify: `compiler/src/lexer/scanner.rs` (`scan_token`, insert arms before the
  `c if is_id_start(c)` fallback at `scanner.rs:403`)
- Test: `compiler/tests/lexer/mod.rs` (unit) and `compiler/tests/cli.rs` (e2e)

**Interfaces:**
- Produces: nothing new for later tasks. Aliases map to existing `TokenKind::Star`,
  `TokenKind::Slash`, `TokenKind::Minus`.

- [ ] **Step 1: Write the failing e2e test** in `compiler/tests/cli.rs`, a `#[test] fn
  unicode_math_operators_run_end_to_end()`: inline `.bld` source
  `fn main() { let a = 6 × 7; let b = 84 ÷ 2; let c = 10 − 3; println!("{} {} {}", a, b, c); }`
  written to a temp file, run via `c_backend_run`, assert stdout `"42 42 7\n"`. Guard with
  `c_backend_ready()`.
- [ ] **Step 2: Run it, expect FAIL** (`UnexpectedChar` at the lexer). Command:
  `cargo test --manifest-path compiler/Cargo.toml unicode_math_operators_run_end_to_end`.
- [ ] **Step 3: Implement** the alias arms in `scan_token`. Read `scanner.rs:344-415` to match
  the exact arm style (single-char arms return a `TokenKind`, e.g. mapping to
  `self.scan_star()`-style or a direct kind). Add: `'\u{00D7}' | '\u{00B7}' | '\u{2219}' =>
  TokenKind::Star`, `'\u{00F7}' => TokenKind::Slash`, `'\u{2212}' => TokenKind::Minus`. Place
  before the `is_id_start` fallback. (These are `×`, `·`, `∙`, `÷`, `−`.)
- [ ] **Step 4: Add a lexer unit test** in `compiler/tests/lexer/mod.rs` asserting `×` lexes to
  `TokenKind::Star` (follow the existing `lex_one`-style helpers there).
- [ ] **Step 5: Run tests, expect PASS.** Then `cargo fmt --manifest-path compiler/Cargo.toml`
  and `cargo clippy --manifest-path compiler/Cargo.toml -- -D clippy::correctness`.
- [ ] **Step 6: Commit** (`feat(lexer): Unicode arithmetic-operator aliases (× ÷ − · ∙)`).

### Task I2: `**` power operator

**Files:**
- Modify: `compiler/src/lexer/token.rs` (declare `StarStar` after `Star` ~`token.rs:191`;
  Display arm near `token.rs:391`)
- Modify: `compiler/src/lexer/scanner.rs` (`scan_star`, `scanner.rs:439`)
- Modify: `compiler/src/parser/expr.rs` (`mod bp` ~`expr.rs:18`, `infix_binding_power`
  `expr.rs:776`, `parse_binary_op` `expr.rs:829`, and a prefix arm for `StarStar`)
- Test: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: existing `BinOp::Pow` (already typed at `infer.rs:3428`, lowered at `c.rs:3557`).
- Produces: `TokenKind::StarStar`; `**` parses to `ExprKind::Binary { op: BinOp::Pow, .. }` in
  infix position and to `Deref(Deref(x))` in prefix position.

- [ ] **Step 1: Confirm no code `**` in compiled trees.** Run
  `grep -rn '\*\*' tests/programs examples demos stdlib semantic-corpus --include='*.bld'` and
  eyeball that any hits are inside strings/comments (safe). Record the result in the report.
- [ ] **Step 2: Write the failing e2e tests** in `cli.rs`, `#[test] fn
  power_operator_runs_end_to_end()`: `2 ** 10` prints `1024`; `2 ** 3 ** 2` prints `512`
  (right-assoc). Add `#[test] fn double_deref_still_works_after_starstar()` proving
  `**ptr`-style double-deref still compiles to the same value (construct via a pointer-to-
  pointer if the language supports it; otherwise a parser-level test that `**x` yields two
  `Deref`s). Guard e2e with `c_backend_ready()`.
- [ ] **Step 3: Run, expect FAIL** (parse error on `**`).
- [ ] **Step 4: Lexer** — declare `TokenKind::StarStar` + Display `` `**` ``; in `scan_star`
  eat a second `*` (before the `=` check) to emit `StarStar`.
- [ ] **Step 5: Parser** — add `bp::POWER` const to `mod bp` (value above `PRODUCT`, chosen so
  `-2 ** 2` matches the documented semantics; pin with the test). In `infix_binding_power` add
  `TokenKind::StarStar => Some((bp::POWER, bp::POWER))` (right-assoc). In `parse_binary_op` add
  `TokenKind::StarStar => BinOp::Pow`. Add a **prefix arm** for `StarStar`: parse the operand at
  prefix binding power and wrap in two `UnaryOp::Deref` (equivalent to `*(*x)`), so prefix
  `**ptr` stays double-deref. Read the existing `Star` prefix-deref arm at `expr.rs:288` for the
  exact `ExprKind::Unary` construction.
- [ ] **Step 6: Run tests, expect PASS.** Document the chosen `-2 ** 2` semantics in the test.
- [ ] **Step 7:** `cargo fmt` + `cargo clippy -- -D clippy::correctness`. Run the differential
  sweep (baseline binary at `main`, feature binary; `buildc check` over every `.bld`; assert 0
  new failures). Record counts in the report.
- [ ] **Step 8: Commit** (`feat(parser): wire ** to BinOp::Pow with prefix double-deref split`).

### Task I3: `linalg` stdlib module over `Vec<f64>`

**Files:**
- Create: `stdlib/linalg.bld` (free functions only; template `stdlib/algorithms.bld:6-35`)
- Test: `compiler/tests/cli.rs`; optional `compiler/tests/programs/NNN_linalg.bld` + `.expected`

**Interfaces:**
- Consumes: builtins `vec_new_f64`, `vec_push_f64`, `vec_get_f64`, `vec_len` (all wired,
  `runtime.rs:1663-1776`); `sqrt` from `stdlib/math` or `stdlib/core`.
- Produces (bare-name after `mod linalg;`): `vec_add`, `vec_sub`, `vec_mul`, `vec_div`
  (elementwise `Vec<f64>,Vec<f64> -> Vec<f64>`), `vec_scale(Vec<f64>,f64)`,
  `vec_scalar_add(Vec<f64>,f64)`, `vec_dot(Vec<f64>,Vec<f64>) -> f64`, `vec_sum(Vec<f64>) -> f64`,
  `vec_norm(Vec<f64>) -> f64`.

- [ ] **Step 1: Read the template** `stdlib/algorithms.bld:6-35` and `stdlib/core.bld:8,38`
  (constant/fn style) and `stdlib/math.bld:20` (`lerp_f64`). Confirm `sqrt` availability.
- [ ] **Step 2: Write the failing e2e test** in `cli.rs`, `#[test] fn
  linalg_module_runs_end_to_end()`: a `.bld` with `mod core; mod math; mod linalg;` building two
  Vec<f64> `[1.0,2.0,3.0]` and `[4.0,5.0,6.0]` (via `vec_new_f64`+`vec_push_f64`), asserting
  `vec_dot` prints `32`, `vec_sum([1,2,3])` prints `6`, `vec_norm([3,4])` prints `5`. Guard with
  `c_backend_ready()`.
- [ ] **Step 3: Run, expect FAIL** (unresolved `mod linalg`).
- [ ] **Step 4: Write `stdlib/linalg.bld`** with the free functions above. Elementwise builds a
  fresh result via `vec_new_f64` + a `vec_len` loop of `vec_get_f64`/`vec_push_f64`. `vec_dot`
  accumulates `vec_get_f64(a,i) * vec_get_f64(b,i)`. `vec_norm(v) = sqrt(vec_dot(v,v))`. Do NOT
  name any function `dot`/`length`/`lerp`/`normalize` (clash with graphics-vec builtins).
- [ ] **Step 5: Run tests, expect PASS.** Optional: add `tests/programs/NNN_linalg.bld` +
  `.expected`.
- [ ] **Step 6:** `cargo fmt` + clippy correctness. Differential sweep is trivially clean (new
  file, no existing importer) but run `buildc corpus verify` to confirm 8/8.
- [ ] **Step 7: Commit** (`feat(stdlib): linalg module — elementwise/dot/sum/norm over Vec<f64>`).

### Task I4: Broadcasting `.+ .- .* ./` over `Array<T,N>`

**Files:**
- Modify: `compiler/src/lexer/token.rs` (`DotPlus/DotMinus/DotStar/DotSlash` after `Caret`
  ~`token.rs:196`; Display arms after `token.rs:394`)
- Modify: `compiler/src/lexer/scanner.rs` (`scan_dot`, `scanner.rs:555`, the `else` arm)
- Modify: `compiler/src/ast/operators.rs` (`BinOp::DotAdd/DotSub/DotMul/DotDiv` after `Pow`
  `operators.rs:32`; extend `precedence`/`associativity`/`as_str`/`is_arithmetic`)
- Modify: `compiler/src/parser/expr.rs` (`infix_binding_power` `expr.rs:776`, `parse_binary_op`
  `expr.rs:829`)
- Modify: `compiler/src/types/infer.rs` (`infer_binary` `expr.rs:3339`, new broadcast arms)
- Modify: `compiler/src/codegen/lower/expr.rs` (broadcast arm at top of `lower_binary`
  `expr.rs:786`)
- Modify: every non-exhaustive `ast::BinOp` match surfaced by `cargo build` (e.g.
  `ast/expr.rs:498 Expr::precedence`, `codegen/mir_representation.rs:573`)
- Test: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: array typing `Ty::array(elem, len)` (`infer.rs:3207`), `MirType::Array`,
  `AggregateKind::Array` and `lower_array` (`expr.rs:5943`).
- Produces: `.+ .- .* ./` tokens and `BinOp::DotAdd/DotSub/DotMul/DotDiv`; a broadcast lowering
  that emits per-element scalar MIR ops into a fresh `MirType::Array`.

- [ ] **Step 1: Write the failing e2e tests** in `cli.rs`, `#[test] fn
  array_broadcast_runs_end_to_end()`: `a=[1.0,2.0,3.0]; b=[10.0,20.0,30.0];` then `a .+ b`
  prints `11 22 33`; `a .* 2.0` prints `2 4 6`; `2.0 .+ a` prints `3 4 5` (print each element).
  Add `#[test] fn array_broadcast_length_mismatch_is_rejected()`: `[1.0,2.0] .+ [1.0,2.0,3.0]`
  fails `buildc check` with a length error (use the check-only invocation pattern in `cli.rs`).
- [ ] **Step 2: Run, expect FAIL** (parse error on `.+`).
- [ ] **Step 3: Lexer** — declare the four `Dot*` tokens + Display. In `scan_dot`, in the `else`
  arm after the failed second-`.` eat and before the is_digit fallback, peek `cursor.first()`
  for `'+' '-' '*' '/'`; if matched, eat it and return the matching kind.
- [ ] **Step 4: AST** — add the four `BinOp::Dot*` variants; extend `precedence` (`.+/.-` at the
  Add/Sub level, `.*/./` at the Mul/Div level), `associativity` (Left), `as_str` (`.+` etc.),
  `is_arithmetic` (true). Run `cargo build` and fix EVERY resulting non-exhaustive-match error
  across the crate (record the list in the report).
- [ ] **Step 5: Parser** — `infix_binding_power`: `.+/.-` at `bp::SUM`, `.*/./` at `bp::PRODUCT`,
  left-assoc `(bp, bp+1)`. `parse_binary_op`: map the four tokens to the four `Dot*` BinOps.
- [ ] **Step 6: Type checker** — in `infer_binary`, add a match arm for the four `Dot*` ops (do
  NOT fall into the scalar arithmetic arm). Using `apply()`d operand types:
  `Array<T,N> . Array<U,M>` unify `T`,`U` and require `N==M` (else a length-mismatch
  `TypeError`), result `Array<T,N>`; `Array<T,N> . S` unify `T`,`S`, result `Array<T,N>`;
  `S . Array<T,N>` unify `S`,`T`, result `Array<T,N>`; else `InvalidBinaryOp`.
- [ ] **Step 7: Codegen** — at the top of `lower_binary` (`expr.rs:786`, after string cases,
  before the `build_vecN` arm at 787) intercept the four `Dot*` variants. For a compile-time
  length `N` from the operand `MirType::Array(elem, N)`, build a fresh `MirType::Array(elem, N)`
  result whose element `k` is a scalar `MirRValue::BinaryOp` with the mapped scalar op
  (`DotAdd -> Add`, etc.) over `left[k]` and `right[k]` (scalar broadcast reuses the scalar for
  all `k`). Model construction on `lower_array` (`expr.rs:5943`, `AggregateKind::Array`). Return
  before reaching the `match op` at `expr.rs:885` so the `Dot*` variants never become MIR ops.
- [ ] **Step 8: Run tests, expect PASS** (all e2e + the length-mismatch rejection).
- [ ] **Step 9:** `cargo fmt` + `cargo clippy -- -D clippy::correctness`. Run the FULL
  differential sweep (baseline at `main` vs feature over tests/programs + examples + demos +
  stdlib + registry + semantic-corpus; 0 new failures) and `buildc corpus verify` 8/8. Record
  counts.
- [ ] **Step 10: Commit** (`feat: broadcasting operators .+ .- .* ./ over Array<T,N>`).
