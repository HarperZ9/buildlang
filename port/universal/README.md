# Universal Hourglass Lifter Pack (`port/universal/`)

Pure-BuildLang lifters implementing the top half of the Universal Hourglass Migration Architecture (`docs/design/UNIVERSAL-HOURGLASS-MIGRATION.md`).

## Lifters (all written in pure BuildLang and compiled by `buildc`)

1. **`wasm_lift.bld` (`wasm_lift input.wat output.bld`):** Deterministic WebAssembly Text (`.wat`) lifter covering `i32`/`i64`/`f64` typed functions, locals, arithmetic, comparisons, structured `if`/`else`/`while`, direct `call`, and `$print_*` host imports mapped to `~ Console`. Refuses linear `memory`, `load`/`store`, `table`, `call_indirect`, and `br_table` at exact line numbers (`exit 2`, nothing written).
2. **`asm_riscv_lift.bld` (`asm_riscv_lift input.s output.bld`):** Deterministic RISC-V `RV64IM` structured assembly lifter (`li`, `mv`, `add`, `sub`, `mul`, `div`, `rem`, `addi`, `beq`, `bne`, `blt`, `bge`, `j`, `ecall` print, `ret`) lowered to a zero-`goto` PC state machine. Refuses raw memory loads/stores (`ld`/`sd`/`lw`/`sw`) and privileged CSR ops (`csrrw`, `mret`) at exact line numbers.
3. **`multi_lang_lift.bld` (`multi_lang_lift <js|rs|zig|go|rb|spvasm> input output.bld`):** Unified token-and-line lifter across JavaScript/WebCode, Rust, Zig, Go, Ruby (with **3-Tier Staged Capability-Bounded Metaprogramming**: Tier 1 `define_method` boot-time partial evaluation, Tier 2 `send` closed-world dispatch table, Tier 3 fuel-metered zero-IO `eval` sandbox), and SPIR-V assembly (`OpConstant`/`OpIAdd`/`OpISub`/`OpIMul`/`OpSDiv`/`OpReturnValue`), reserving outright line-numbered refusals for unmodeled native C-ABI / ambient host escapes (`active_record`, `ObjectSpace`, `Kernel.system`, `Proxy`, `unsafe`, `macro_rules!`, `comptime`, `@ptrCast`, `go`/`chan`, `OpImageSample*`, `OpAtomic*`).

## Running the Verification Suite

```bash
bash port/universal/check.sh path/to/buildc
```
