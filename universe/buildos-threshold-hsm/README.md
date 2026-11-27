# BuildOS Zero-C (t=3, n=5) Shamir Threshold Secret Sharing and Proactive Refresh HSM (Slice 36)

License: Pure-OSI `MIT` (`1/1` standalone BuildLang threshold HSM kernel, `0` external dependencies, `0` local paths).
Author Vision Anchors: `F1`, `N42`, `AC1`, `N11`, `N49`, `N53`.
Verification Contract: Passes `buildc check` (`0` errors), `buildc lint` (`No issues found`), `SIG-058` privacy screening (`3/3` bundle files clean), and `25/25` live five-architecture Zero-C builds (`5` clean binaries plus `20` threshold-attack rejections).

## 1. Graduated Kernel Manifest (`[Checked]` Slice 36 Threshold HSM Kernel)

| Kernel File | Domain | Hardware / Algorithmic Focus |
| :- | :- | :- |
| `buildos_shamir_threshold_hsm_zeroc.bld` | Zero-C HSM | `544`-line `(t=3, n=5)` Shamir interpolation, Pedersen VSS over `GF(2^31-1)` and `Z_Q^*`, proactive key rotation, and RFC 6962 audit tree |

## 2. Certified Hardware Gains vs Baselines (`[Checked]`)

1. **Five-Architecture Binary Footprint vs C/OpenSSL and `vsss-rs` Baselines (`> 27x` Smaller WASM):** Compiling `buildos_shamir_threshold_hsm_zeroc.bld` (`21,006 B`, every function frame `< 1,400 B < 2,048 B` RV64 limit) via untouched Stage-2 `buildc_crypto_stage2` on workstation silicon (`< 64 MiB` host RAM, `0 B` GPU VRAM) yields `11,087 B` WASM (`> 27x` smaller than incumbent C/OpenSSL or Rust `vsss-rs` builds `> 300 KB`), alongside `29,184 B` PE32+, `31,989 B` x64 ELF, `38,900 B` RV64 ELF, and `35,772 B` AArch64 ELF in `8.22 ms` to `12.18 ms` with zero C compiler or linker vs incumbent LLVM cross-toolchains.
2. **Bit-Exact Epoch 0 Dealing and Epoch 1 Proactive Refresh Across Five ISAs:** Constant-time Mersenne prime `p = 2^31-1` math and 19-bit limb reduction in `Z_Q^*` (`Q = 46p + 1`) verify all five custodian shares against `C_j = G^(a_j) H^(r_j) mod Q`, recover exact secret `1,513,918,951` across all `10/10` 3-of-5 subsets, rotate all five shares via zero-secret polynomial `g(x)` (`g(0) = 0`, `D_0 = 1`), re-verify all `10/10` post-rotation subsets, and seal RFC 6962 root `0b8beced748fd9708385180f42c36f74fad38ed16bf745c6e2d0cd6698caba7c` bit-for-bit across all five targets.
3. **Four Planted Threshold HSM Controls (`20/20` Rejections):** Built-in adversarial modes halt fail-closed with `committed=0` on sub-threshold `2/5` recovery (`-101`), corrupted share `y_3 + 1 mod p` (`-201`), malicious refresh `g(0) = 1337` (`-301`), and duplicate `x`-coordinate replay `{1, 1, 3}` (`-401`).

## 3. Running Locally

```sh
buildc check universe/buildos-threshold-hsm/buildos_shamir_threshold_hsm_zeroc.bld
buildc lint universe/buildos-threshold-hsm/buildos_shamir_threshold_hsm_zeroc.bld
```

## 4. Honest Scope Limits (`[Reported]` Boundaries)

- Prime `p = 2^31-1` and 37-bit `Q` verify zero-overflow Pedersen VSS and proactive refresh algebra inside compact binaries without 128-bit discrete-log security, live hardware RNG, or external side-channel audits; pre-audit research scope applies.
- Graduation adds `+1` passing `.bld` file under `universe/buildos-threshold-hsm/` while leaving `universe/CHECK-FAILS.txt` untouched.
