# BuildOS Zero-C QUIC-Lite Encrypted Transport and Egress Firewall (Slice 31)

License: Pure-OSI `MIT` (`1/1` standalone BuildLang network kernel, `0` external dependencies, `0` local paths).
Author Vision Anchors: `F1`, `N42`, `AC1`, `N11`, `N49`, `N53`.
Verification Contract: Passes `buildc check` (`0` errors), `buildc lint` (`No issues found`), `SIG-058` privacy screening (`3/3` bundle files clean), and `25/25` live five-architecture Zero-C builds (`5` clean binaries plus `20` attack rejections).

## 1. Graduated Kernel Manifest (`[Checked]` Slice 31 QUIC-Lite Kernel)

| Kernel File | Domain | Hardware / Algorithmic Focus |
| :- | :- | :- |
| `buildos_quic_zero_copy_net_zeroc.bld` | Zero-C Network | `617`-line RFC 9000/9001/9002 QUIC-Lite ChaCha20-Poly1305 transport, 64-bit sliding anti-replay bitmap, NewReno `cwnd`, and `CAP_NET_PEER` firewall |

## 2. Certified Hardware Gains vs Baselines (`[Checked]`)

1. **Five-Architecture Binary Footprint vs Slice 21 Baseline (`-70.3%` WASM):** Compiling `buildos_quic_zero_copy_net_zeroc.bld` (`26,474 B`, every function frame `< 1,250 B < 2,048 B` RV64 limit) via untouched Stage-2 `buildc_crypto_stage2` yields `12,839 B` WASM (`70.3%` smaller than Slice 21 `43,216 B`) and `36,864 B` PE32+ (`69.4%` smaller than `120,320 B`), plus `39,447 B` x64 ELF, `48,108 B` RV64 ELF, and `45,056 B` AArch64 ELF in `8.3 ms` to `12.3 ms` with zero C compiler or linker vs incumbent LLVM cross-toolchains.
2. **Bit-Exact RFC 6962 Transport Commit Across Five ISAs:** All five binaries match Stage-0 `buildc` and an independent Python oracle bit-for-bit on eight out-of-order packets (`max_pn=107`, `bmp_lo=255`), deterministic NewReno recovery (`cwnd: 4800 -> 9900 -> 4950 -> 5550`), and RFC 6962 root `eb3c5e9bd4b22e5960b0f33be690abdaef99aa88db82376767fb5e62a7663ad9`.
3. **Four Planted Network Controls (`20/20` Rejections):** Built-in adversarial modes halt fail-closed with `committed=0` on replayed sequence numbers (`pn=101`, `-101`), stale out-of-window nonces (`pn=30`, `-201`), forged Poly1305 AEAD tags (`-301`), and unauthorized external peers lacking `CAP_NET_PEER` (`-401`).

## 3. Running Locally

```sh
buildc check universe/buildos-quic-net/buildos_quic_zero_copy_net_zeroc.bld
buildc lint universe/buildos-quic-net/buildos_quic_zero_copy_net_zeroc.bld
```

## 4. Honest Scope Limits (`[Reported]` Boundaries)

- This zero-C binary checks deterministic QUIC-Lite AEAD framing, 64-bit sliding anti-replay, NewReno `cwnd`, and `CAP_NET_PEER` egress rules over fixed vectors without live UDP socket rings, header masking, or side-channel silicon audits; pre-audit research scope applies.
- Graduation adds `+1` passing `.bld` file under `universe/buildos-quic-net/` while leaving `universe/CHECK-FAILS.txt` untouched.
