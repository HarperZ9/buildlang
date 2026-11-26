# BuildLang Workstation Dogfood Modules

Compiled BuildLang programs verifying Layer 1 deterministic parallel kernels and Layer 3 native workstation MCP governance directly on `buildc 1.15.0+`:

- `kcore_fast.bld`: 12 verified `FORMULAS.tsv` equations (`HASH-FNV`, `COL-001`, `COL-003`, `COL-005`, `COL-030`, `OPT-001`, `OPT-002`, `OPT-004`, `OPT-008`, `OPT-014`, `OPT-033`, `SIG-013`), each with known-answer bounds (`<= 1e-12`) and planted-wrong-control tests, plus a `1,048,576`-point `par_for` bit-exact multi-thread benchmark timed via `clock_ns()`.
- `procgen_forge.bld` + `bench_procgen_forge.py`: Pure-BuildLang procedural geometry, material, and foliage kernel suite (`sdf_rbox`, `sdf_rcone`, `sdf_torus`, `sdf_smin_cubic`, `sdf_displace`, finite-difference normals, `256x256` tileable periodic `GEO-005` fBm + Worley PBR fields with `seam_delta == 0.0`, `OPT-034` GGX + Smith G2 + `OPT-003` complex conductor Fresnel, biomechanical Da Vinci pipe model, and `64v/126t` meshlet bounding-sphere/normal-cone verifier), paired with a bit-exact Python parity benchmark and 5 planted-failure controls.
- `workstation_mcp.bld`: Pure-BuildLang MCP JSON-RPC 2.0 stdio server (`~ Console + FileSystem + Clock + Environment`, zero Network/Process/Foreign authority) implementing `initialize`, `ping`, `tools/list`, `workstation_health`, `check_em_dash_and_secret`, `anti_silo_preflight`, and `audit_public_hygiene`, with `--self-test` known-answer and planted-failure controls.

## Run

```bash
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/kcore_fast.bld
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/procgen_forge.bld
python universe/workstation/bench_procgen_forge.py
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/workstation_mcp.bld -- --self-test
```
