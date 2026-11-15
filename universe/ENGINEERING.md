# Build Ecosystem — Engineering Runbook

Last verified: 2026-06-05. Each .bld module transpiles to C individually; whole-ecosystem cross-module resolution is incomplete and self-hosting is not yet achieved. 56 native programs verified MSVC-clean (65 sources in programs/). Compiler: 612 tests pass / 0 fail on cargo test (755 #[test] annotations across binaries, incl. ignored/multi-bin). See STATUS.md for per-module maturity. A local MSVC toolchain (Visual Studio 2022) is available: the 56 native programs compile with MSVC `cl` and run (per STATUS.md); whole-ecosystem cross-module C is still not built as one unit.

## Quick Reference

| Repo | Tests | CI | Release |
|------|-------|----|---------|
| buildlang | 612 pass / 0 fail | clippy + fmt + test + e2e color self-check | v1.0.0 (buildc.exe) |
| calibrate-pro | 228 Python | ruff + pytest (Ubuntu + Windows) | v1.0.0 (standalone exe) |
| build-color | 281 Python | ruff + pytest | v1.0.0 |
| build-universe | — | file validation | v1.0.0 |
| build-finance | 142 Python | ruff + pytest | — |
| build-oracle | 187 Python | ruff + pytest | — |
| build-engine | 173 Python | ruff + pytest (stubs for cross-deps) | — |
| build-ui | 17 Python | ruff + pytest (needs libEGL on Linux) | — |
| build-ecosystem | — | sdist build validation | — |
| aurora | no source | referenced only; no .lua source present locally | — |

## When CI Fails

### buildlang
- **clippy fails**: Check `-D clippy::correctness`. Style/perf/complexity are allowed.
- **fmt fails**: Run `cargo fmt` locally, commit.
- **test fails**: Run `cargo test -- --skip spirv` locally. 3 SPIR-V tests are skipped (need spirv-val).
- **e2e compilation fails**: A .bld program doesn't compile. Check if a type system change broke it.
- **color_test fails**: The self-verifying binary disagrees with CIE 1976. Check float codegen.

### calibrate-pro
- **build-color not found**: It's installed via `git+https://github.com/HarperZ9/build-color.git`. If build-color CI is red, calibrate-pro will fail too.
- **Windows-only test fails on Linux**: Tests using `ctypes.windll` are skipped on Linux. If new Windows tests are added, mark them `@pytest.mark.skipif(sys.platform != "win32")`.

### build-engine
- **Cross-project imports fail**: Tests that import `build_finance` or `build_oracle` are skipped in CI (private repos). Run locally with all packages installed.
- **Timing tests fail on Windows**: Rate-limiting tests are skipped on Windows CI.

### aurora
- **Lua module not found**: Ensure `LUA_PATH="./?.lua;./?/init.lua;;"` is set.
- **Lua 5.1 syntax error**: Don't use `or` as standalone expression statements. Use `if not ... then` chains.

## Adding a New .bld Program

1. Write the program in `programs/`
2. Verify it compiles: `buildc your_program.bld --target c -o /dev/null`
3. If it uses `pow()`, `sqrt()`, or float division — verify `1.0/3.0` produces `0.333`, not `0`
4. If it calls methods defined later in the same impl block — move the callee above the caller
5. Don't use `&str` as function parameters (codegen bug: value vs pointer)
6. Commit and push — CI will verify

## Adding a New BUILD-UNIVERSE Module

1. Create `your_module/lib.bld`
2. Use `pub mod name {` not `pub module std::name {`
3. Test compilation: `buildc your_module/lib.bld --target c -o /dev/null`
4. Common blockers:
   - `pub module std::X` → parsed as `mod std`, use `pub mod X`
   - Unicode box characters in comments → cause macro expansion errors
   - Forward references in impl blocks → define helpers before callers
   - `f64::INFINITY` → works, `i32::MAX` → works
   - `HashMap::values().find()` → deterministic (fixed, uses DefId ordering)

## Known Limitations

### Compiler — Active Limitations (not re-tested this pass)
- **Forward references**: Methods must be defined before they're called within the same impl block.
- **DefId misresolution in large files**: In files with 1000+ types, the type checker occasionally resolves names to the wrong DefId. Root cause of many foundation cascading errors.
- **Type variable scope leaks**: Generic type parameters (K, V) from one impl block can leak into unrelated functions in the same module.
- **Struct literals in some contexts**: `Foo { field: value }` inside deeply nested expressions may be parsed as a block instead of a struct literal.

### Compiler — Verified resolved / docs were stale (2026-06-05)
Empirically re-tested by compiling minimal repros with `buildc --target c`. Each item below, previously listed as an active limitation, now compiles:
- ~~&str parameters~~ — `&str` params compile with correct value semantics.
- ~~Infinite type false positive~~ — a fn taking `&StructType` and returning a struct literal of that type compiles.
- ~~Trait method dispatch on self~~ — default trait methods calling `self.method()` compile.
- ~~Self as unit struct constructor~~ — **FIXED this session**: `fn new() -> Self { Self }` now type-checks and lowers to a valid unit construction `(T){0}` (previously emitted invalid C: `return Self;`). buildlang commit 8d83d74 (branch feat/phase1-generics); regression test `tests/programs/132_unit_self_constructor`.
- Also verified to type-check and emit C (native run NOT verified — no local C toolchain): generic methods returning `Self`, generic enum methods, and heap types (`String`) in enum variants.

Caveat: "compiles" here means type-check + C emission succeed; with no local C compiler, native execution of the emitted C was not verified.

### Compiler — RESOLVED (2026-04-04)
- ~~Enum forward declarations~~ — Enums now forward-declared as `typedef struct` (MSVC compatible).
- ~~Enum typedef redefinition~~ — Enum struct body uses `struct X {}` not `typedef struct X {} X`.
- ~~Intrinsic_ prefix in C output~~ — `intrinsic::trunc` etc. now mapped to C stdlib (trunc, exp2, asin...).
- ~~Vec method dispatch~~ — `.push()/.len()/.pop()/.get()` emit typed runtime calls.
- ~~Vec subscript on BuildVecHandle~~ — `vec[i]` emits `build_hvec_get_<type>()`.
- ~~Missing ctype.h~~ — toupper/tolower/isspace now compile.
- **64/65 programs compile with MSVC 19.50 zero errors**
- **7 native executables verified** (color_test 12/12 pass, wc, calc, base64, basename, cmp, grep)

### Compiler — RESOLVED (2026-04-03)
- ~~No module imports~~ — `mod foo;` and `use foo::bar;` fully implemented.
- ~~No trait dispatch~~ — Trait resolution, vtables, dynamic dispatch all work.
- ~~Generic impls skipped~~ — Monomorphization fully implemented.
- ~~Macro expansion bug~~ — Brace depth + synthetic span detection fixed.
- ~~Field-tensor blocked~~ — Now compiles: 6,108 LOC → 30,882 lines C.
- ~~Entropy blocked~~ — Now compiles: 6,681 LOC → 33,760 lines C (ML trading models).
- ~~Unsafe blocks eating match arms~~ — Parser fixed: `unsafe`/`async` no longer treated as items.
- ~~Cross-module type names~~ — type_module_map + DefId-keyed inherent methods.
- ~~Array-to-slice coercion~~ — `[T; N]` ↔ `[T]` unification added.
- ~~Ref patterns in closures~~ — `|&s|` and `|&&r|` fixed (parse_pattern_primary + AndAnd).
- ~~Missing math builtins~~ — Added tanh, sinh, cosh, asin, acos, atan, exp2.
- ~~UTF-8 boundary panics~~ — Added is_char_boundary checks in codegen span extraction.
- ~~Nexus blocked~~ — Now compiles: 6,025 LOC → 23,893 lines C.
- ~~Delta blocked~~ — Now compiles: 7,084 LOC → 32,746 lines C (options pricing).
- ~~Wavelength blocked~~ — Now compiles: 8,791 LOC → 38,811 lines C (audio/video).
- ~~Chromatic blocked~~ — Now compiles: 5,948 LOC → 32,119 lines C (color science).
- ~~Nova blocked~~ — Now compiles: 8,007 LOC → 32,724 lines C (preset engine).
- ~~Calibrate blocked~~ — Now compiles: 6,822 LOC → 25,755 lines C (display calibration).
- ~~Oracle blocked~~ — Now compiles: 11,491 LOC → 64,859 lines C (AI forecasting).
- ~~Lumina blocked~~ — Now compiles: 10,246 LOC → 44,817 lines C (rendering pipeline).
- ~~Refract blocked~~ — Now compiles: 6,227 LOC → 17,461 lines C (ENB/ReShade engine).
- ~~Prism blocked~~ — Now compiles: 6,873 LOC → 28,338 lines C (shader pipeline).
- ~~Entangle blocked~~ — Now compiles: 3,047 LOC → 7,389 lines C (device mesh). Stripped Arc/Mutex/Duration.
- ~~Foundation blocked~~ — Now compiles: 1,335 LOC → 4,615 lines C (stdlib). Trimmed to supported subset (math/string/collections/io/ai/graph).
- ~~Neutrino blocked~~ — Now compiles: 5,914 LOC → 33,171 lines C (deep learning). Removed SIMD/raw pointers, simplified Tensor.
- ~~Occurs-check false positive~~ — Apply substitution before check in unifier bind().

### C Codegen
- **Tuple typedefs**: `(f32, f32)` emits `Tuple_f32_f32` but the typedef may appear after first use.
- **Rectangle collision**: User type `Rectangle` collides with Windows API `wingdi.h`. Avoid this name.

### Python APPS
- **build-color not on PyPI**: Upload failed. Install from git.
- **calibrate-pro monolithic files**: 3 files exceed 300 lines (2,736 / 2,334 / 2,036). Refactoring planned.
- **GUI tests on Linux**: PyQt6 needs `libegl1 libxkbcommon0` on Ubuntu CI.

## 7-Gate Checklist (run before every release)

1. **Every claim has a test** — verify locally, not just CI
2. **No fabricated content** — grep for patent/production-ready/enterprise-grade
3. **Error handling is a strategy** — no bare `except Exception: pass`
4. **Incomplete is documented** — STATUS.md for anything partial
5. **Git hygiene** — no .bak, no (1).md, LICENSE present
6. **AI writes data, you write architecture** — design decisions documented
7. **GitHub pages** — descriptions, topics, CI badges, releases
