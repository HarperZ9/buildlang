# Status: codegen/backend/

Last audited: 2026-06-15

2026-06-15 wind-down assessment: see
`docs/COMPILER_WIND_DOWN_ASSESSMENT_2026-06-15.md` for the current product
posture. This backend-local file is retained as an implementation inventory.
Non-C targets are selectable through `buildc build --target`, but C remains
the only production-backed end-to-end target. This inventory describes apparent
backend coverage from source and unit tests; it is not a substitute for
end-to-end executable/module receipts.

2026-06-13 update: the Rust backend now has a narrow executable stdout smoke
slice in addition to compile-only metadata validation. Eight semantic corpus
programs are lowered to Rust, compiled with `rustc`, executed, and checked for
deterministic stdout. The semantic corpus manifest also drives a Rust execution
test so corpus path and expected-output drift is caught by `cargo test`;
manifest contract, receipt consistency, and metadata tests keep the execution
receipt aligned with the manifest.

2026-06-13 C update: function-style `println("{}", value)` now lowers to C
`printf` format specifiers instead of printing the literal format string. The
current 8-program semantic corpus was run through `buildc run`, parallel-probed
with per-run temp build directories, and recorded in
`semantic-corpus/receipts/c-execution-2026-06-13.json`.

## Hello-World Viability Assessment

Each backend was evaluated against a simple hello-world-equivalent program:
a function that takes two i32 values, adds them, and returns the result.
This exercises: function definition, parameter handling, binary operations,
return values, and basic type mapping.

---

## 1. C Backend (`c.rs`) -- WORKING

- **Implements Backend trait:** Yes (line 820)
- **Output format:** C99 source text (OutputFormat::CSource)
- **MIR operations handled:**
  - Use, BinaryOp (all variants), UnaryOp (Neg, Not): fully handled
  - Ref, AddressOf, Cast, Aggregate, Repeat, Discriminant, Len, NullaryOp: fully handled
  - FieldAccess, VariantField, IndexAccess: fully handled
  - All terminators (Goto, If, Switch, Call, Return, Unreachable, Abort, Drop, Assert): fully handled
- **Could produce a working hello-world:** Yes, verified through the current C execution path. This is the only backend with the current product execution claim.
- **Work needed:** None for basic programs. Already end-to-end functional.
- **Tests:** 11 unit tests.

## 2. x86-64 Backend (`x86_64.rs`) -- EXPERIMENTAL (assembly/object mode)

- **Implements Backend trait:** Yes (line 578)
- **Output format:** GNU-syntax x86-64 assembly (OutputFormat::Assembly) or raw machine code (OutputFormat::Object)
- **MIR operations handled:**
  - Use, BinaryOp (Add, Sub, Mul, Div, Rem, BitAnd, BitOr, BitXor, Shl, Shr, comparisons): handled
  - UnaryOp (Neg, Not): handled
  - Other rvalues (Ref, Aggregate, FieldAccess, etc.): fallthrough to `# TODO` comment in output
  - Terminators: Goto, If, Return, Call (System V ABI with 6 register args), Unreachable, Abort: handled
  - Switch, Drop, Assert: fallthrough to `# TODO` comment
- **Could produce a working hello-world:** Plausible for a constrained assembly subset from source inspection and unit tests, but not a current release claim. It still needs assembler/linker execution proof. Machine code mode produces raw bytes without ELF headers.
- **Work needed before promotion:**
  1. Promote assembler/linker integration from best-effort guidance to a
     verified cross-platform build path
  2. Add an x86/x64 semantic execution corpus
  3. Add proper register allocation (currently uses rax as a single accumulator
     with push/pop spilling) -- correct for covered cases but slow
  4. Complete struct/enum field access and array indexing (currently emits TODO
     comments for unsupported MIR)
- **Tests:** 22 unit tests.

## 3. ARM64 Backend (`arm64.rs`) -- EXPERIMENTAL (assembly/object mode)

- **Implements Backend trait:** Yes (line 619)
- **Output format:** ARM64 assembly (OutputFormat::Assembly) or raw machine code (OutputFormat::Object)
- **MIR operations handled:**
  - Use, BinaryOp (Add, Sub, Mul, Div, Rem, BitAnd, BitOr, BitXor, Shl, Shr, comparisons): handled
  - UnaryOp (Neg via `neg`, Not via `mvn`): handled
  - Other rvalues: fallthrough to `// TODO` comment
  - Terminators: Goto (`b`), If (`cmp`/`b.ne`/`b`), Return (with stack restore + `ldp`/`ret`), Call (AAPCS64 with x0-x7), Unreachable (`brk #0`), Abort (`bl abort`): handled
  - Switch, Drop, Assert: fallthrough to `// TODO` comment
- **Could produce a working hello-world:** Plausible for a constrained assembly subset on ARM64 hardware or emulation, but not a current release claim. It still needs assembler/linker execution proof and platform coverage.
- **Work needed before promotion:**
  1. Promote CLI-selected ARM64 output to a verified assembler/linker path
  2. Invoke assembler and linker (or cross-compiler toolchain)
  3. Callee-saved register save/restore is stubbed (`// TODO: Save X19-X28`)
  4. No register allocator (uses x0 as accumulator with x9 temp)
  5. Struct/enum field access and array indexing not implemented
  6. Only testable on ARM64 hardware or via QEMU emulation
- **Tests:** 21 unit tests plus machine code tests.

## 4. WASM Backend (`wasm.rs`, `wasm_flow.rs`, `wasm_bin.rs`) -- EXPERIMENTAL (binary module, Node execution check)

Updated 2026-11-13.

- **Output format:** `--target wasm` / `wasm32` writes a binary `.wasm` module (OutputFormat::Wasm), encoded by the compiler itself in `wasm_bin.rs`. `--target wat` (or `-o NAME.wat`) writes WebAssembly text from the same instruction selection.
- **Control flow:** every function body is a dispatch loop (`loop` with one `block` per MIR block, entered through `br_table` on a `$__bb` local), so any MIR control-flow graph lowers correctly, including loops whose header is not block 0. Structured reconstruction is not done yet.
- **Output:** host imports `env.print_i64`, `env.print_u64`, `env.print_f64`, `env.print_bool` and `env.print_str(ptr, len)`; the module exports `main` and `memory`. No WASI is needed. `println`/`print` with a literal format and plain `{}` holes lower to these imports.
- **Execution check:** `compiler/tests/wasm_node.rs` compiles three programs (a call with integer arithmetic, a `while` loop summing 1 to 100, an `if/else`), runs each in Node's built-in `WebAssembly` and requires its stdout to equal the C backend's `buildc run` stdout. The test skips with a message when `node` is not on PATH.
- **Refused:** there is no linear-memory model yet. Aggregates, field and variant access, index access, references, deref and every store kind fail with an "unsupported" error in binary output instead of emitting code. Globals, indirect calls, string values other than print formats, and runtime builtins other than `println`/`print` are also refused.
- **Work needed before promotion:**
  1. Linear-memory model: shadow stack frames for address-taken locals, a bump allocator, struct layout from `MirTypeDef`
  2. Aggregate, field, index and deref lowering, with typed stores in the correct operand order
  3. A Vec/String runtime inside the module, then globals
  4. Package builds to wasm and a page loader; float formatting that matches C
- **Tests:** 11 unit tests in `wasm.rs`, plus the Node execution test.

## 5. LLVM Backend (`llvm.rs`) -- EXPERIMENTAL (nearest non-C candidate)

- **Implements Backend trait:** Yes (line 1705)
- **Output format:** LLVM IR text (OutputFormat::LlvmIr)
- **MIR operations handled:**
  - Use, BinaryOp (full mapping to LLVM `add`/`sub`/`mul`/`sdiv`/`fadd`/etc.): handled
  - UnaryOp (Neg: `fneg` for float, `sub 0` for int; Not: `xor true`/`xor -1`): handled
  - Ref, AddressOf: generates `store ptr` correctly
  - Cast: full mapping to LLVM cast instructions (sext, zext, trunc, fpext, fptrunc, etc.)
  - Aggregate: generates GEP + store for each element
  - Repeat: generates loop of GEP + store
  - Discriminant, Len, NullaryOp: handled with proper GEP
  - FieldAccess, VariantField: emits `; TODO` comment (not implemented)
  - IndexAccess: emits `; TODO` comment (not implemented)
  - Terminators: Goto (`br label`), If (`br i1`), Switch (`switch`), Call (with proper signatures), Return (`ret`), Unreachable, Drop, Assert: all handled
- **Could produce a working hello-world:** Plausible for constrained LLVM IR inspection and external `clang`/`llc` use, but not a current release claim. This is the closest non-C candidate after the C backend.
- **Work needed before promotion:**
  1. Promote CLI-selected LLVM output to a verified clang/llc path
  2. Invoke `clang` or `llc` on generated .ll file to produce executable
  3. Implement FieldAccess and IndexAccess (currently emit TODO comments)
  4. The `target()` method returns `Target::X86_64` instead of `Target::LlvmIr` (line 1735) -- should be fixed
- **Tests:** 11 unit tests.

## 6. SPIR-V Backend (`spirv.rs`) -- SPECIALIZED (GPU compute only)

- **Implements Backend trait:** Yes (line 1749)
- **Output format:** SPIR-V binary (OutputFormat::SpirV) -- raw u32 words converted to little-endian bytes
- **MIR operations handled:**
  - Use: loads via OpLoad, constants via OpConstant/OpConstantTrue/etc.
  - BinaryOp: maps to SPIR-V opcodes (OpIAdd, OpFAdd, OpISub, OpIMul, OpSDiv, OpFDiv, etc.)
  - UnaryOp (Neg: OpSNegate/OpFNegate, Not: OpNot): handled
  - Cast: maps to OpConvertFToS/OpConvertSToF/OpUConvert/OpSConvert/etc.
  - Aggregate, Repeat, Ref, AddressOf, FieldAccess, IndexAccess, etc.: returns zero constant (default fallback)
  - Terminators: Goto (OpBranch), If (OpSelectionMerge + OpBranchConditional), Switch (OpSwitch), Return (OpReturn/OpReturnValue), Call (OpFunctionCall), Unreachable (OpUnreachable): handled
- **Could produce a working hello-world:** No, not in the traditional stdout sense. SPIR-V is a GPU shader/compute format, not a CPU program format. Any execution claim requires a Vulkan/OpenCL host program and validation for the specific module.
- **Work needed before promotion:**
  1. Promote CLI-selected SPIR-V output to a validated shader path
  2. Validate output with `spirv-val` from the Vulkan SDK
  3. Global variables emit zero (not wired to storage buffers)
  4. No descriptor set/binding decorations for buffer I/O
  5. Need a host-side Vulkan program to dispatch the shader and read results
  6. Implement buffer bindings so the shader can read input / write output
- **Tests:** 7 unit tests.

---

## Summary Table

| Backend | Backend Trait | Output Format | Hello-World Viable | Biggest Blocker |
|---------|-------------|---------------|-------------------|-----------------|
| C | Yes | C99 source | **Yes (working)** | None |
| x86-64 | Yes | Assembly / machine code | Experimental subset only | No assembler/linker integration |
| ARM64 | Yes | Assembly / machine code | Experimental subset only | No assembler/linker integration, needs ARM hardware/emulation |
| WASM | Yes | Binary .wasm and WAT text | Experimental; three programs run in Node and match C | No linear-memory model yet |
| LLVM | Yes | LLVM IR text | Experimental, nearest non-C candidate | No clang/llc invocation, wrong target() return |
| SPIR-V | Yes | SPIR-V binary | No (GPU-only format) | Needs Vulkan host, no buffer I/O |

## Historical Priority Order for Promotion Work

Current wind-down posture is preservation and receipt accuracy, not broad
productionization. If backend promotion resumes, this older order remains a
reasonable technical sequence:

1. **LLVM** -- Lowest effort. Output is already valid LLVM IR. Harden clang/llc invocation.
2. **x86-64** -- Medium effort. Assembly output is reasonable. Need assembler + linker.
3. **WASM** -- Binary output and dispatch-loop control flow landed 2026-11-13. Next: the linear-memory model.
4. **ARM64** -- Medium effort but hardware-dependent. Same as x86-64 but for ARM.
5. **SPIR-V** -- High effort. Fundamentally different target (GPU). Needs buffer I/O, Vulkan host.

## What All Non-C Backends Share

- All implement `Backend` trait with `generate()` returning `CodegenResult<GeneratedCode>`
- All handle the core MIR operation set (Use, BinaryOp, UnaryOp, basic terminators)
- None handle FieldAccess or VariantField (struct/enum member access)
- Non-C backends are selectable through `buildc build --target`, but they do
  not carry the C backend's production claim
- Rust now has a narrow executable stdout smoke slice; other non-C backends do
  not yet have end-to-end tests that produce and run an executable/module
