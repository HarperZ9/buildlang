# Clean-Room Open-World Engine and Graphics Kernels (`universe/cre-cleanroom`)

**License:** Pure OSI `MIT` (`LICENSE`)
**Author Idea Anchors:** `N17`, `E2`, `E3`, `N18`
**Clean-Room Provenance:** Every kernel in this package implements public binary format specifications, textbook computer graphics, and fixed-timestep mechanics verified solely on synthetic in-memory fixtures. Zero external game files, proprietary code, or third-party assets are included or required.

## Included Pure-BuildLang Modules

| Module | Formula ID | Core Capability | Verified Benchmark & Controls |
| :- | :- | :- | :- |
| `cre_format_reader.bld` | `SIG-065` | Clean-room `BSA` v105 header/hash, `ESM` `TES4`/`GRUP`/`CELL`/`REFR` walker, and `PEX` `0xFA57C0DE` verifier | `207.75M` BSA lookups/s, `0.0` `f64` 6-DOF parity, `6/6` planted traps caught |
| `cre_pex_aot.bld` | `SIG-068` | Clean-room 11-opcode `PEX` bytecode interpreter plus zero-warning `-O2` C99 AOT emitter | `5/5` bit-exact scripts, `56.09x` AOT speedup (`5M` ticks), `3/3` traps caught |
| `cre_cell_batcher.bld` | `OPT-054` | 6-plane frustum culling, 16-bucket indirect-draw batching, and 120 Hz decoupled physics tick | `625.0x` draw reduction (`0.0988 ms`), `0.0` 60/144/240 FPS drift, `819%` Havok trap |
| `ntc610_decoder.bld` | `OPT-052` | 4K PBR neural texture latent pyramid (`16->16->8` MLP) material decoder | `10.67 MiB`/4K (`5.9995x` vs BC7), `42.55 dB` PSNR, `3/3` fail-closed guards |
| `gsplat_keepsake.bld` | `COL-040` | Deterministic tile-based 3D Gaussian-Splatting memory keepsake rasterizer | `100k` splats tile-binned, multi-view `PSNR >= 32 dB`, `3/3` fail-closed guards |

## Verification

Run each standalone module directly with `buildc` (`1.15.0` or newer):

```sh
buildc check universe/cre-cleanroom/cre_format_reader.bld
buildc lint universe/cre-cleanroom/cre_format_reader.bld
buildc run -O 3 universe/cre-cleanroom/cre_format_reader.bld
```

Every module executes its built-in self-verification suite and planted fail-closed control checks with zero external data dependencies.
