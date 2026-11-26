# Universal Hourglass Migration Architecture

Status: Active implementation slice (`port/universal/`, 2026-11-26). Complements `docs/design/LANGUAGE-MIGRATION.md` (which defines the C-subset `port c` frontend and differential verification contract).

## 1. Why an Hourglass Beats an N x M Rewrite Matrix

Migrating software across `N` source ecosystems (C, Python, JavaScript/TypeScript, Rust, Zig, Go, Ruby/Rails, WebAssembly, LLVM IR, SPIR-V, RISC-V, ARM64, x86-64, Plain-English) onto `M` hardware targets (x86-64, ARM64/Apple Silicon, RISC-V, WebAssembly, SPIR-V, HLSL, GLSL, Metal, WGSL) by hand requires `N * M` fragile translators.

BuildLang resolves this with a **Typed Effectful Hourglass**:

```text
  Funnel A (Source Frontends)       Funnel B (Universal IRs)        Funnel C (Native ISA Lifters)
  C | Py | JS/TS | Rust | Zig       WASM (.wat/.wasm) | LLVM (.ll)  RISC-V | x86-64 | ARM64 (Mach-O)
  Go | Ruby/Rails | Plain-English   SPIR-V (.spvasm) | HLSL/GLSL    (explicit dynamic-runtime boundary)
                  \                           |                           /
                   +--------------------------+--------------------------+
                                              v
                       ===============================================
                       |   BuildLang Typed Effectful Waist (.bld)    |
                       |   Explicit types + explicit effects (~ IO)  |
                       |   Fail-closed line-numbered refusal report  |
                       |   Byte-for-byte differential verification   |
                       ===============================================
                                              v
                  +---------------------------+---------------------------+
                  |                           |                           |
            CPU Native Targets          Portable / Web Targets      GPU Compute & Shader Targets
            C99 | LLVM IR | x86-64      WebAssembly (--target wasm) SPIR-V | HLSL | GLSL
            ARM64 | RISC-V (--target    WASI                        Metal (--target msl)
            riscv64)                                                WGSL (--target wgsl)
```

Every incoming language or binary enters the narrow waist (`BuildLang` source with explicit scalar/aggregate types and explicit algebraic effects such as `~ Console`, `~ FileSystem`, `~ Network`). Once verified at the waist, **all** downstream targets inherit the migrated program with zero per-language backend work.

## 2. Top Half: Three Verified Entry Funnels

### 2.1 Funnel A: Source Frontends (`port/` and `port/universal/multi_lang_lift.bld`)

Source frontends preserve developer intent, names, comments, and readable control flow:

- **C (`port c`):** C99 numeric/array/control subset verified byte-for-byte against `gcc` (`port/check.sh`).
- **Python & Plain-English (`port py`, `port plain`):** Typed/inferred Python subset and structured English specifications lowered to BuildLang with explicit capability rows.
- **Unified Multi-Language Lifter (`port/universal/multi_lang_lift.bld`):** Pure BuildLang token-and-structure lifter covering clean subsets of:
  - **JavaScript / WebCode (`js`):** `function` declarations, `let`/`const`, arithmetic, `while`/`if`/`else`, `console.log` -> `~ Console`.
  - **Rust (`rs`):** `fn` signatures with explicit scalar types, `let`/`let mut`, loops, `println!` -> `~ Console`.
  - **Zig (`zig`):** `fn` signatures, `var`/`const`, loops, `print` -> `~ Console`.
  - **Go (`go`):** `func` signatures, `:=` bindings, `for` loops, `fmt.Println` -> `~ Console`.
  - **Ruby (`rb`):** `def`..`end` functions, assignments, `while`/`if`/`else`/`end`, `puts` -> `~ Console`.

### 2.2 Funnel B: Universal IR and Bytecode Lifters

One IR lifter captures **every** upstream language that already compiles to that IR:

- **WebAssembly Text / Binary (`port/universal/wasm_lift.bld`, `.wat`/`.wasm`):** Any Rust, Zig, Go/TinyGo, C/C++, Swift, or AssemblyScript module compiled to WebAssembly enters BuildLang through a single deterministic stack-to-typed-expression lifter. Handles `i32`/`i64`/`f64` typed functions, locals, arithmetic, comparisons, structured `if`/`else`/`loop`, direct `call`, and explicit `$print_*` host imports mapped to `~ Console`.
- **SPIR-V Assembly (`multi_lang_lift.bld spvasm`, `.spvasm`/`.spv`):** Lifts SSA GPU compute/math kernels (`OpFunction`, `OpConstant`, `OpIAdd`/`OpISub`/`OpIMul`/`OpSDiv`, `OpReturnValue`) directly into BuildLang so shaders from GLSL/HLSL/WGSL/Metal cross-compile through the waist.
- **LLVM IR (`.ll`) Roadmap:** Structured SSA basic blocks + typed GEP/phi lowering follow the exact same basic-block state-machine lowering pattern proven in `asm_riscv_lift.bld`.

### 2.3 Funnel C: Native ISA Binary Lifters

- **RISC-V Structured Assembly (`port/universal/asm_riscv_lift.bld`, `RV64IM`):** Lifts register-file functions (`li`, `mv`, `add`, `sub`, `mul`, `div`, `rem`, `addi`, `beq`, `bne`, `blt`, `bge`, `j`, `call`, `ecall` print, `ret`) into deterministic BuildLang using an explicit `pc` basic-block state machine (`while pc != -1`) with zero `goto` and zero undefined register aliasing.
- **x86-64 and ARM64 / Apple Silicon (Mach-O) Extension Path:** Decode ELF/PE/Mach-O text sections into normalized 3-address register instructions and feed the identical SSA/state-machine lifter.

## 3. Staged Capability-Bounded Dynamic Metaprogramming (3-Tier Model)

Outright refusal of dynamic metaprogramming (`define_method`, `method_missing`, `send`, `eval`, `getattr`, JS `Proxy`) leaves too much real-world software stranded outside the hourglass. Conversely, granting ambient OS authority or silently dropping dynamic behavior violates BuildLang's soundness guarantees.

BuildLang's universal lifters use a **3-Tier Staged Capability-Bounded Model**:

### Tier 1: Boot-Time Partial Evaluation (Futamura Specialization)

Constant-known metaprogramming is evaluated once at lift/compile time and lowered into **100% static, zero-overhead BuildLang functions**:

- Ruby `define_method(:name) { |x| expr }`, Rails `has_many` / `belongs_to` over a known schema, and static Python/TS decorator registrations are specialized directly into typed BuildLang `fn` definitions.
- Zero runtime interpreter overhead; full `buildc` type and effect verification applies identically to hand-written code.

### Tier 2: Closed-World Dispatch Table (`~ DynamicDispatch`)

Finite dynamic lookups (`send(sym, arg)`, `method_missing` over a known method/schema set, dynamic route tables, `getattr` over known struct fields) are lowered into a **verified closed-world dispatch table**:

- The lifter collects every statically defined and Tier-1-specialized symbol and emits an explicit deterministic dispatch function (`__dynamic_dispatch`) over that closed set.
- Unknown selectors hit a deterministic trap or explicit fallback branch rather than performing unbounded open-world reflection.

### Tier 3: Fuel-Metered Capability-Bounded Sandbox (`~ DynamicEval`)

For runtime-computed expression strings (`eval(expr)` over arithmetic/DSL strings), the lifter lowers into a **pure, deterministic, fuel-metered mini-evaluator** (`__sandbox_eval_i64`):

- **Strict effect attenuation:** The evaluator declares zero `FileSystem`, zero `Network`, and zero `Process` capabilities (pure computation only).
- **Bounded termination:** Every token/step decrements an explicit fuel counter (`fuel`), failing cleanly if fuel is exhausted so untrusted input cannot trigger unbounded loops or ambient host escapes.

### Outright Refusal Boundary (Unmodeled Native Extensions & Undeclared Host Escapes)

Outright fail-closed refusal (`exit 2`, exact line numbers, zero output written) is reserved strictly for constructs with **no declared capability bound or unmodeled native C-ABI state**:

| Ecosystem / Construct | Reason for Outright Refusal | Hourglass Policy |
|---|---|---|
| Ruby/Rails opaque C-extensions & host escapes (`require 'active_record'`, `ObjectSpace`, `Kernel.system`) | Mutates external C-ABI heap or spawns ambient host shells outside capability bounds | Fail closed at exact line numbers; migrate domain logic via Tiers 1-3 and declare explicit `~ Database` / `~ Process` shims |
| JavaScript (`new Proxy`, direct `document.` DOM host mutation, `async` event-loop runtime) | Unmodeled browser host state or unbounded proxy traps | Refuse at exact line numbers; lift pure WebCode and bounded expressions cleanly |
| Rust (`unsafe`, `macro_rules!`, inline `asm!`) | Bypasses safe ownership invariants or requires pre-expansion | Refuse at exact lines; run `cargo expand` or lift compiled WASM via Funnel B |
| Zig (`@ptrCast`, `comptime` reflection, inline `asm`) | Raw pointer aliasing and compile-time AST reflection | Refuse at exact lines; lift compiled WASM or pure scalar kernels |
| Go (`go` goroutines, `chan`, `unsafe.Pointer`) | M:N scheduler runtime and raw memory casts | Refuse at exact lines until structured effect channels attach |
| WebAssembly (`memory`, `i32.load`/`store`, `call_indirect`, `br_table`) | Untyped linear memory requires bounds-checked slice lowering | Refuse at exact lines in Slice 1 |
| RISC-V (`ld`/`sd`/`lw`/`sw`, `csrrw`, `mret`) | Raw physical memory and privileged CSR state | Refuse at exact lines; pure register compute and `ecall` print lift cleanly |
| SPIR-V (`OpImageSampleImplicitLod`, `OpAtomicIAdd`, ray query ops) | Texture sampler hardware state and cross-lane atomics | Refuse at exact lines in Slice 1 |

## 4. Bottom Half: Universal Hardware Targets

Once a program passes `buildc check` at the BuildLang waist, the compiler lowers it across:

- **CPU Native:** C99 backend (`Windows`, `Linux`, `macOS`, `Apple Silicon`), LLVM IR, `x86-64`, `ARM64`, `RISC-V` (`--target riscv64`).
- **Portable Sandbox / Web:** WebAssembly (`--target wasm`).
- **GPU Compute & Graphics:** SPIR-V (`--target spirv`), HLSL, GLSL, Apple Metal (`--target msl`), WebGPU (`--target wgsl`).

## 5. Differential Verification Standard (`port/universal/check.sh`)

Every lifter in `port/universal/` is **written in pure BuildLang**, compiled with `buildc build --release`, and verified by `port/universal/check.sh`:

1. **Corpus Equivalence:** Every accepted source program across `wat`, `riscv`, `js`, `rs`, `zig`, `go`, `rb`, and `spvasm` is lifted to `.bld`, compiled and executed via `buildc run`, and compared **byte-for-byte** against its golden reference output (plus live `node` execution for JavaScript when `node` is present).
2. **Exact Refusal Line Verification:** Every refused fixture across all 8 ecosystems must exit with status `2`, write **no** output file, and report the exact expected line numbers recorded in its `.lines` file.
3. **Planted-Mismatch Control:** Two distinct lifted programs' outputs are cross-compared and **must** differ, proving the comparison harness cannot silently pass on broken outputs.
