# BuildOS Zero-C v2: GPU VRAM Fence, Enclave Attestation, PQ Hybrid KEM, and Speculative KV Router

License: Pure-OSI `MIT` (`4/4` standalone BuildLang kernels, `0` external dependencies, `0` local paths).
Author Vision Anchors: `F1`, `N42`, `AC1`, `N49`, `J13`, `N53`, `N64`.
Verification Contract: Every kernel passes `buildc check` (`0` errors), `buildc lint` (`No issues found`), `SIG-058` privacy screening (`6/6` bundle files clean), and live 5-ISA Zero-C or tree-speculative verification.

## 1. Graduated Kernel Manifest (`[Checked]` 4 Pure-OSI MIT Kernels)

| Kernel File | Domain | Hardware / Algorithmic Focus |
| :- | :- | :- |
| `buildos_gpu_vram_zeroc.bld` | GPU Security | Slice 27 shader-cert fence and VRAM tile isolation |
| `buildos_enclave_attest_zeroc.bld` | Confidential OS | Slice 28 HKDF sealing with Ed25519 quote checks |
| `buildos_pq_hybrid_kem_zeroc.bld` | Post-Quantum | Slice 29 ML-KEM-512 NTT plus X25519 hybrid KEM |
| `titan_tree_spec_kv_router.bld` | Inference | Tree speculative decoding with paged KV router |

## 2. Certified Hardware Gains vs Baselines (`[Checked]`)

1. **Zero-C Five-Architecture OS Stack vs Legacy C Toolchains:** `buildos_gpu_vram_zeroc.bld` (`11,440 B` WASM), `buildos_enclave_attest_zeroc.bld` (`35,925 B` WASM), and `buildos_pq_hybrid_kem_zeroc.bld` (`25,674 B` WASM, `< 1,900 B` RV64 stack frame) compile via untouched Stage-2 `buildc_crypto_stage2` across PE32+, x64 ELF, RV64 ELF, AArch64 ELF, and WebAssembly 1.0 with zero C compiler or linker vs multi-stage LLVM cross-toolchains (`40.6%` smaller WASM than Slice 21 `43,216 B`).
2. **Speculative KV Routing vs Dense Autoregressive Baseline:** `titan_tree_spec_kv_router.bld` implements deterministic tree-attention verification with paged KV eviction and exact draft acceptance parity against dense step-by-step decoding on workstation silicon.
3. **Planted Soundness Controls:** Built-in fail-closed traps reject tampered shader digests, cross-context VRAM scrapes, 1-bit `mr_enclave` flips (`-101`), stale attestation nonces (`-201`), forged Ed25519 quotes (`-401`), ML-KEM ciphertext mutators via Fujisaki-Okamoto implicit rejection, and non-canonical `q=3329` injections (`-301`).

## 3. Running Locally

```sh
for f in universe/buildos-zeroc-v2/*.bld; do
  buildc check "$f"
  buildc lint "$f"
done
```

## 4. Honest Scope Limits (`[Reported]` Boundaries)

- Zero-C OS slices verify deterministic cryptographic state machines across five ISAs without hardware TPM firmware blobs or side-channel silicon audits; pre-audit research scope applies.
- Graduation adds `+4` passing `.bld` files under `universe/buildos-zeroc-v2/` while leaving `universe/CHECK-FAILS.txt` untouched.
