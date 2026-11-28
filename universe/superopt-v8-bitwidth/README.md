# BuildLang Superopt V8 Bit-Width OR-Reduction Rule

License: Pure-OSI `MIT` (`2/2` standalone BuildLang modules, `0` external dependencies, `0` local paths).
Verification Contract: Passes `buildc check` (`0` errors), `buildc lint` (`No issues found`), `publish_metadata_gate.py` (`0` findings), native C + WebAssembly execution (`100,000/100,000` random 8-lane unsigned blocks exact match + `2/2` planted fail-closed controls), and 5-backend certified GPU shader generation (`WGSL`, `HLSL`, `MSL`, `SPIR-V`, `GLSL`).

## 1. Identity and Module Manifest

For any non-empty vector of unsigned integers `x_0, ..., x_{n-1}`:

`bit_width(max_i x_i) == bit_width(or_i x_i)`

Replacing unsigned block-max comparisons with associative bitwise OR removes compare-and-select dependencies before leading-zero counting.

| Module File | Target Scope | Role |
| :- | :- | :- |
| `superopt_v8_bitwidth_or.bld` | C + WASM | `bit_width_u32`, `block_bit_width_max`, `block_bit_width_or`, `100,000`-block verifier (`800,000` lanes), and `2` planted fail-closed controls |
| `superopt_v8_bitwidth_or_compute.bld` | 5-Backend GPU | `#[compute(workgroup_size = [64, 1, 1])]` branchless 8-lane OR-reduction and bit-width compute shader |

## 2. Verification and Planted Controls

1. **Exact Equivalence Across 100,000 Random Unsigned Blocks:** `verify_100k_blocks(539365671)` sweeps `100,000` deterministic 8-lane unsigned blocks (`800,000` values spanning `0..32` bit-widths) and checks `block_bit_width_max == block_bit_width_or` on every block with `0` mismatches, producing identical rotate-XOR digests across native C and WebAssembly.
2. **Planted Control 1 (Signed Negative Guard):** `validate_signed_block_guard(-1, 42)` returns `-101`, rejecting negative two's-complement inputs where bitwise OR would spuriously raise the sign bit.
3. **Planted Control 2 (Mutated Mask Detection):** `verify_mutated_mask_control(0, 1, 2, 3, 4, 5, 6, 7, 2147483648)` returns `-201`, catching any accumulator corruption that changes the highest set bit.

## 3. Running Locally

```sh
buildc check universe/superopt-v8-bitwidth/superopt_v8_bitwidth_or.bld
buildc lint universe/superopt-v8-bitwidth/superopt_v8_bitwidth_or.bld
buildc check universe/superopt-v8-bitwidth/superopt_v8_bitwidth_or_compute.bld
buildc lint universe/superopt-v8-bitwidth/superopt_v8_bitwidth_or_compute.bld
buildc universe/superopt-v8-bitwidth/superopt_v8_bitwidth_or.bld --target wasm -o /tmp/bitwidth.wasm
buildc universe/superopt-v8-bitwidth/superopt_v8_bitwidth_or_compute.bld --target wgsl --superopt --emit-gpu-cert /tmp/bitwidth.cert.json -o /tmp/bitwidth.wgsl
```

## 4. Scope Boundaries

- The identity applies strictly to unsigned non-negative integers; signed inputs require explicit non-negative gating (`validate_signed_block_guard`).
- This bundle adds `+2` passing `.bld` files under `universe/superopt-v8-bitwidth/` while leaving `universe/CHECK-FAILS.txt` untouched.
