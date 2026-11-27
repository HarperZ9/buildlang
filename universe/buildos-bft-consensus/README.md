# BuildOS Zero-C 4-Node Ed25519 BFT Consensus and Transparency Log (Slice 33)

License: Pure-OSI `MIT` (`1/1` standalone BuildLang BFT consensus kernel, `0` external dependencies, `0` local paths).
Author Vision Anchors: `F1`, `N42`, `AC1`, `N11`, `N49`, `N53`.
Verification Contract: Passes `buildc check` (`0` errors), `buildc lint` (`No issues found`), `SIG-058` privacy screening (`3/3` bundle files clean), and `25/25` live five-architecture Zero-C builds (`5` clean binaries plus `20` Byzantine attack rejections).

## 1. Graduated Kernel Manifest (`[Checked]` Slice 33 BFT Consensus Kernel)

| Kernel File | Domain | Hardware / Algorithmic Focus |
| :- | :- | :- |
| `buildos_bft_consensus_ledger_zeroc.bld` | Zero-C BFT | `1,408`-line `3f+1` (`f=1`) Ed25519 BFT quorum engine with double-vote slashing, 3-block Merkle chain, and 4-leaf RFC 6962 log |

## 2. Certified Hardware Gains vs Baselines (`[Checked]`)

1. **Five-Architecture Binary Footprint vs Slice 28 Baseline (`-14.8%` WASM):** Compiling `buildos_bft_consensus_ledger_zeroc.bld` (`63,502 B`, every function frame `< 1,900 B < 2,048 B` RV64 limit) via untouched Stage-2 `buildc_crypto_stage2` yields `30,622 B` WASM (`14.8%` smaller than Slice 28 `35,925 B`) and `86,528 B` PE32+ (`14.2%` smaller than `100,864 B`), plus `89,228 B` x64 ELF, `106,680 B` RV64 ELF, and `101,484 B` AArch64 ELF in `9.9 ms` to `38.4 ms` with zero C compiler or linker vs incumbent LLVM cross-toolchains.
2. **Bit-Exact `3/4` Quorum Ed25519 Finality Across Five ISAs:** All five targets match Stage-0 `buildc` and an independent Python oracle bit-for-bit across three live RFC 8032 Ed25519 validator signatures (`V0`, `V1`, `V2`) over `blk3` (`84ea7900...afc7`), sealing RFC 6962 four-leaf transparency root `5a5ab5b24408b84df9c2e941d34e23bcb3eb3291aa09f80dec3a91bc27552b60`.
3. **Four Planted Byzantine Controls (`20/20` Rejections):** Built-in adversarial modes halt fail-closed with `committed=0` on equivocating validator double votes (`-101`), insufficient `2/4` sub-quorums (`-201`), forged Ed25519 scalar `S` quorum signatures (`-301`), and tampered Block 3 parent digests (`-401`).

## 3. Running Locally

```sh
buildc check universe/buildos-bft-consensus/buildos_bft_consensus_ledger_zeroc.bld
buildc lint universe/buildos-bft-consensus/buildos_bft_consensus_ledger_zeroc.bld
```

## 4. Honest Scope Limits (`[Reported]` Boundaries)

- This zero-C binary checks deterministic BFT safety rules, live quorum Ed25519 math, and RFC 6962 log chaining over fixed view schedules without asynchronous gossip sockets, view-change timeouts, or external side-channel audits; pre-audit research scope applies.
- Graduation adds `+1` passing `.bld` file under `universe/buildos-bft-consensus/` while leaving `universe/CHECK-FAILS.txt` untouched.
