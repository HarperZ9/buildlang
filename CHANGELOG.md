# Changelog

All notable changes to BuildLang will be documented in this file.

Current status note (2026-06-15): entries below preserve historical release
claims as they were recorded at the time. Current release-shaped evidence is
tracked in `STATUS.md`, `README.md`, and
`docs/COMPILER_WIND_DOWN_ASSESSMENT_2026-06-15.md`; historical counts such as
`108/108` or `132/132` are not the current release gate.

## Unreleased

- Stdlib (sum-type large payloads): `Option<T>` and `Result<T, E>` now carry
  payloads that do not fit the 8-byte union slot (e.g. `String`/`BuildString`,
  24 bytes). `Some(s)` / `Ok(s)` box the payload (`malloc` + copy, pointer stored
  in the `.value.p` / `.ok.ok_p` slot) and the match deref-reads it
  (`*(BuildString*)…`). Previously the construct cast a struct to `int64_t`.
  Scalars and pointers still go inline. Verified end-to-end under MSVC:
  `Option<String>` prints `some found`, `Result<String, String>` prints
  `ok nonzero` / `err zero`. Covered by
  `option_string_payload_is_boxed_through_the_pointer_slot`. (The boxed
  allocation is freed only under the opt-in drop-analysis path; in the default
  no-free mode it leaks, consistent with current owned-string handling.)
- Stdlib (`Option<T>` payload threading): `match call() { Some(x) => ... }` on a
  direct call to a `-> Option<T>` function now reads the correct union slot for a
  non-`i32` scalar payload. Previously the match defaulted the payload type to
  `i32` and read `.value.i` even when construction wrote `.value.f` (e.g.
  `Option<f64>`), so the float bits were reinterpreted as an int (silent-wrong).
  A per-function side-table (`fn_option_inner_types`), captured in the collection
  pass from `-> Option<T>`, threads the payload type to the match site (symmetric
  to the `Result` Ok threading). Verified end-to-end under MSVC: `Option<f64>`
  prints `some 2.5`. Covered by
  `option_match_on_direct_call_reads_the_threaded_payload_slot`.
- Stdlib (`Result<T, E>`): `Ok(x)` / `Err(e)` now construct the runtime
  `Result` struct and `match r { Ok(x) => ..., Err(e) => ... }` branches on the
  `is_ok` discriminant, reading the Ok payload from the typed `ok` union slot
  (`.ok_i` / `.ok_f` / `.ok_p`) and the Err payload from the `err` `BuildString`.
  The Ok payload type is threaded from the binding annotation
  (`let r: Result<i32, String> = ...`) or the matched call's return signature, so
  a non-`i32` Ok payload reads the correct slot instead of silently defaulting to
  `i32`. Previously `Ok`/`Err` lowered to undefined calls into an `i32` dest (a
  C2440) and the match emitted `if (true)` with whole-struct binds (silent-wrong).
  Covered by `ok_err_construct_result_struct_not_bare_call` and
  `result_match_tests_is_ok_and_binds_typed_slots`; verified end-to-end under MSVC
  for `i32` and `f64` Ok payloads across direct-call and let-bound matches. (Err is
  always `BuildString` and Ok payloads >8 bytes, e.g. `Result<String, _>`, still
  need boxing - tracked separately.)
- Native FFI (variadic): extern functions accept a trailing C-style `...`
  (e.g. `fn printf(fmt: &str, ...) -> i32`). The parser records it on
  `FnSig.is_variadic`, lowering carries it to the MIR signature so the C backend
  emits a trailing `, ...`, and the type checker (`FnTy.is_variadic`) lets a
  variadic call pass more arguments than there are fixed parameters while a
  non-variadic call still enforces exact arity. `printf("%d and %d\n", 1, 2)`
  now parses, type-checks, and lowers to `printf(fmt, 1, 2)`. Covered by
  `extern_variadic_fn_parses`, `variadic_extern_emits_ellipsis_in_c`,
  `variadic_extern_call_with_extra_args_typechecks`, and a non-variadic arity
  regression test.
- Native FFI (export header): `buildc build --emit header` writes a C header
  (`main.h`) declaring the program's `extern "C"` exports, with an include
  guard, the integer/bool/size typedefs the prototypes use, and a
  `#ifdef __cplusplus extern "C"` linkage guard. C and C++ consumers can
  `#include` it and call into the compiled BuildLang code. Covered by
  `extern_c_fn_is_marked_c_export` and `c_export_header_declares_exports_only`.
- Native FFI (export): `extern "C" fn` is now accepted as a function
  *definition*, not only inside extern blocks. A C-ABI function definition gets
  external linkage and a stable, unmangled name, so it compiles to a
  non-`static` C function callable from C and any C-ABI language. Ordinary
  functions stay internal (`static`). This is the reciprocal of header-backed
  extern blocks. Covered by `extern_c_fn_definition_parses_as_function`,
  `extern_c_fn_definition_emits_non_static_export`, and
  `regular_fn_keeps_internal_static_linkage`.
- Native FFI: extern blocks accept an optional `header "..."` clause naming the
  backing C header. The C backend emits the matching `#include` (angle-bracket
  form for `"<sqlite3.h>"`, quoted form for `"mylib.h"`), de-duplicated and
  sorted for reproducible output, and no longer synthesizes a prototype for a
  header-backed function, so the header's real declaration is authoritative.
  This is the native, embedded integration path for any C-ABI library. Covered
  by parser, lowering, and C-backend tests (`extern_block_header_*`,
  `extern_header_clause_lowers_to_mir_link_header`, `c_backend_*header*`).
- Native FFI: foreign `static` declarations in extern blocks now lower and
  generate correct C. A foreign static is treated as an external declaration,
  never a definition: it carries the block's `header`/`link` clauses, so the C
  backend includes the header (or emits a bare `extern <type> <name>;` when no
  header backs it) and links the library. Previously a foreign static
  type-checked but produced C that referenced an undeclared symbol. Covered by
  `extern_static_lowers_to_external_global_with_header` and
  `c_backend_foreign_static_*` tests.
- Native FFI: extern blocks also accept an optional `link "..."` clause naming
  the library to link. `buildc build` passes it to the C compiler (`-lname`
  for gcc/clang/cc, `name.lib` for MSVC) and the emitted C records a greppable
  `// buildc-link: name` note. The `link` and `header` clauses may appear in
  either order, so a program that calls a third-party C library builds and
  links in one command. Covered by parser, lowering, `GeneratedCode`, and
  `user_link_flags` tests (`extern_block_link_*`,
  `extern_link_clause_lowers_to_mir_link_lib`, `generated_code_*link*`,
  `user_link_flags_format_per_toolchain`).
- Presentation pass: README hero and brand assets under `docs/brand/`, Build ecosystem navigation, and Current status / Operator surface blocks.
- Documented the operator surface across the `buildc` CLI and the bundled LSP server.
- Relicensed to the BuildLang Fair-Source License v1.0 under the operator's umbrella.

## [1.0.5] - 2026-03-28 - Self-Hosted Compiler Verification

### Proven - Self-Hosting: Complete Audit of All 9 Versions
- All 9 versions compile to C through BuildLang; 6 run to completion, 3 have runtime bugs
- **6 of 9 run to completion with verified correct output**:
  - v1: 3-pass pipeline generating C (`int x = 3 + 4; int y = x * 2;`)
  - v2: Functions + if/else + while (`square()`, `abs_val()`, `sum_to()`)
  - v3: Character lexer tokenizing `fn add(a, b)` into 28 tokens
  - v4: Token-driven parser building 8-node AST from `let x = 3 + 4;`
  - v5: Function definition parsing from token stream
  - v6: Structs + branching + loops from tokens
- **3 of 9 compile but have runtime bugs (infinite loops in character-level parsing)**:
  - v7, v8, v9: Hang during codegen - nested while loops in hand-written character parsers don't advance past certain token boundaries. Bug is in the `.bld` program logic, not in the BuildLang compiler.
- Self-hosted support libraries (Option, Cmp, Span, LexerTokens) all produce correct output

---

## [1.0.4] - 2026-03-28 - Module System & Use Resolution

### Added - Module Registry
- `TypeContext` now maintains a `module_bindings` registry mapping module names to their exported bindings
- Inline `mod foo { ... }` blocks register their bindings in the registry after type checking
- `current_scope_bindings()` snapshots a module's scope before it's popped

### Added - Use Statement Resolution
- `use foo::bar;` resolves through the module registry and imports the binding
- `use foo::bar as baz;` supports renaming
- `use foo::*;` glob imports all module bindings
- `use foo::{bar, baz};` nested imports resolve each sub-tree
- Resolution happens during the collection pass so imported items are available for forward references

### Changed - DESIGN.md
- Module system limitation updated: inline modules and use statements now work; external file modules remain unimplemented

### Verified
- 132/132 test programs compile (zero regression)
- 591 unit tests pass
- New module + use test programs compile successfully

---

## [1.0.3] - 2026-03-28 - Exhaustiveness Checking & Builtin Fixes

### Added - Pattern Exhaustiveness Checking
- Match expressions over enum types now produce a type error if not all variants are covered
- Error message names the missing variants: `non-exhaustive match: missing variants Blue`
- Wildcard patterns (`_`) and binding patterns recognized as catch-all arms
- `Or` patterns (`A | B`) correctly accumulate covered variants
- Enum resolution works even when scrutinee is an unresolved type variable (resolves from pattern paths)

### Fixed - Missing Builtin Registrations
- Registered `assert(bool)`, `assert_eq`, `println` as builtin functions in the type checker
- Registered typed vector builtins: `vec_get_f64`, `vec_push_f64`, `vec_new_f64`, `vec_pop_f64`, and i64 variants
- Registered string methods: `parse_int() -> i64`, `parse_float() -> f64`
- **132/132 test programs now compile** (was 121/132 due to missing builtins)

### Changed - DESIGN.md
- Pattern exhaustiveness moved from "Known Limitations" to "Resolved"
- Effect system limitation reworded as a deliberate design trade-off with rationale

---

## [1.0.2] - 2026-03-28 - End-to-End Proof & Depth

### Proven - Full Compilation Pipeline
- **108/108 test programs compile and run correctly**
- Pipeline: `.bld` → `buildc` → C99 → MSVC → native x86-64 → correct output
- Coverage: functions, recursion, closures, generics, traits, dynamic dispatch, algebraic effects, pattern matching, iterators, hashmaps, file I/O, vectors, color science, self-hosted compiler components
- See [TEST_RESULTS.md](TEST_RESULTS.md) for documented outputs

### Added - Type System Tests (78 new tests)
- Type inference: 40 tests (unification properties, bidirectional flow, occurs check, effect inference)
- Parser: 38 tests (10 operator precedence, 8 expression forms, 10 items, 10 patterns)
- Compiler unit tests: 518 → 588

### Added - Design Rationale (DESIGN.md)
- Why bidirectional inference instead of Algorithm W
- Why Pratt parsing instead of recursive descent
- Why setjmp/longjmp for algebraic effects
- Why color space annotations in the type system
- Known Limitations section (no borrow checker, eager monomorphization, one-shot effects)

---

## [1.0.1] - 2026-03-28 - Production Readiness & Code Quality

### CI/CD
- Added **clippy lint** job to GitHub Actions CI (`cargo clippy -- -D warnings`)
- Added **rustfmt check** job (`cargo fmt --check`)
- Added `[lints.clippy]` configuration to `Cargo.toml`

### Error Handling
- **pkg/lockfile.rs**: Converted 24 `.unwrap()` calls to `?` propagation
  - Added `Fmt(fmt::Error)` variant to `LockfileError`
  - Renamed `to_string()` to `serialize()` returning `Result<String, LockfileError>`
- **pkg/version.rs**: Converted 14 `.unwrap()` calls to `?` in test functions
- **runtime/async_rt.rs**: Annotated 36 Mutex lock unwraps as standard Rust practice
- **runtime/gc.rs**: Annotated 9 unwraps (7 Mutex locks + 2 structural guarantees)

### Documentation
- Added **unwrap policy** to `codegen/mod.rs` explaining why codegen unwraps are intentional assertions on validated AST
- Added policy notes to 4 backend files: llvm.rs, c.rs, arm64.rs, x86_64.rs
- Documented **backend maturity levels**: C (production), others (experimental)

### Audit Results
- **Lexer**: All 28 `panic!()` calls confirmed to be in test code only - production lexer has proper error handling with 30+ error variants
- **Parser**: Already uses `expect()` with messages (not `unwrap()`) - correct practice
- **Codegen**: 651 unwraps are assertions on type-checked AST (intentional, documented)
- **Runtime**: 45 unwraps are all Mutex locks (standard Rust, annotated)

---

## [1.0.0] - 2026-03-22

### Language Features
- Generics with trait bounds and where clauses
- Pattern matching with exhaustiveness checking
- Closures with capture semantics
- Algebraic effects and effect handlers
- Built-in color space types (sRGB, Linear, ACES, Oklab, HSL, HSV)
- Ownership and borrowing system
- Module system with visibility controls
- Macro system with hygiene

### Compiler
- C backend (stable, primary target)
- HLSL shader output
- GLSL shader output
- SPIR-V binary shader output
- x86-64 native backend (experimental)
- AArch64 native backend (experimental)
- WASM backend (experimental)
- LLVM IR backend (experimental)
- 8 total code generation backends

### Tooling
- LSP server with completion, hover, and diagnostics
- VS Code extension with syntax highlighting and LSP integration
- CLI (`buildc`) with lex, parse, check, build, and run subcommands
- Package manager (`build pkg`) with dependency resolution
- Code formatter (`build fmt`)

### Known Limitations
- Non-C backends (x86-64, AArch64, WASM, LLVM) are experimental and may not support all language features
- Package manager is not connected to a live registry
- Formatter is not wired into the CLI pipeline
