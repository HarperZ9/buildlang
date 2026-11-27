# BuildOS Zero-C Fuel-Metered Sandbox Bytecode VM (Slice 32)

License: Pure-OSI `MIT` (`1/1` standalone BuildLang sandbox VM kernel, `0` external dependencies, `0` local paths).
Author Vision Anchors: `F1`, `N42`, `AC1`, `N11`, `N49`, `N53`.
Verification Contract: Passes `buildc check` (`0` errors), `buildc lint` (`No issues found`), `SIG-058` privacy screening (`3/3` bundle files clean), and `25/25` live five-architecture Zero-C builds (`5` clean binaries plus `20` sandbox-escape rejections).

## 1. Graduated Kernel Manifest (`[Checked]` Slice 32 Sandbox Bytecode VM)

| Kernel File | Domain | Hardware / Algorithmic Focus |
| :- | :- | :- |
| `buildos_wasm_cap_vm_zeroc.bld` | Zero-C VM | `524`-line fuel-metered stack bytecode interpreter with 32-word linear memory, W^X read-only code guard, SHA-256 hostcall, and `cap_mask` checks |

## 2. Certified Hardware Gains vs Baselines (`[Checked]`)

1. **Five-Architecture Binary Footprint vs Slice 31 Baseline (`-3.9%` WASM):** Compiling `buildos_wasm_cap_vm_zeroc.bld` (`23,110 B`, every function frame `< 1,350 B < 2,048 B` RV64 limit) via untouched Stage-2 `buildc_crypto_stage2` yields `12,343 B` WASM (`3.9%` smaller than Slice 31 `12,839 B`) and `34,304 B` PE32+ (`6.9%` smaller than `36,864 B`), plus `36,989 B` x64 ELF, `44,112 B` RV64 ELF, and `40,564 B` AArch64 ELF in `12.1 ms` to `21.1 ms` with zero C compiler or linker vs incumbent LLVM cross-toolchains.
2. **Bit-Exact RFC 6962 Execution Trace Across Five ISAs:** All five targets match Stage-0 `buildc` and an independent Python oracle bit-for-bit over a 42-instruction guest program (`fuel_used=447` of `512`, `cap_mask=3` permitting `CAP_CRYPTO=1` and `CAP_LOG=2`, `host_out=4256336022`), committing RFC 6962 trace root `a22ee7e6e2af59885b7a7de106f325ffb500136895cc3524bd0f5e668053575b`.
3. **Four Planted Sandbox-Escape Controls (`20/20` Rejections):** Built-in adversarial modes halt fail-closed with `committed=0` on infinite-loop fuel exhaustion at `512` ticks (`-101`), out-of-bounds linear-memory stores (`addr=40 >= 32`, `-201`), unauthorized `HOST_NET_EGRESS` hostcalls outside `cap_mask=3` (`-301`), and W^X writes to read-only code segment `1024` (`-401`).

## 3. Running Locally

```sh
buildc check universe/buildos-cap-vm/buildos_wasm_cap_vm_zeroc.bld
buildc lint universe/buildos-cap-vm/buildos_wasm_cap_vm_zeroc.bld
```

## 4. Honest Scope Limits (`[Reported]` Boundaries)

- This zero-C prototype executes a compact 15-opcode stack bytecode with a 32-word linear memory without WebAssembly floating-point opcodes, indirect call tables, or dynamic `memory.grow`; pre-audit research scope applies.
- Graduation adds `+1` passing `.bld` file under `universe/buildos-cap-vm/` while leaving `universe/CHECK-FAILS.txt` untouched.
