# BuildLang Workstation Dogfood Modules

Compiled BuildLang programs verifying Layer 1 deterministic parallel kernels and Layer 3 native workstation MCP governance directly on `buildc 1.15.0+`:

- `kcore_fast.bld`: 12 verified `FORMULAS.tsv` equations (`HASH-FNV`, `COL-001`, `COL-003`, `COL-005`, `COL-030`, `OPT-001`, `OPT-002`, `OPT-004`, `OPT-008`, `OPT-014`, `OPT-033`, `SIG-013`), each with known-answer bounds (`<= 1e-12`) and planted-wrong-control tests, plus a `1,048,576`-point `par_for` bit-exact multi-thread benchmark timed via `clock_ns()`.
- `procgen_forge.bld` + `bench_procgen_forge.py`: Pure-BuildLang procedural geometry, material, and foliage kernel suite (`sdf_rbox`, `sdf_rcone`, `sdf_torus`, `sdf_smin_cubic`, `sdf_displace`, finite-difference normals, `256x256` tileable periodic `GEO-005` fBm + Worley PBR fields with `seam_delta == 0.0`, `OPT-034` GGX + Smith G2 + `OPT-003` complex conductor Fresnel, biomechanical Da Vinci pipe model, and `64v/126t` meshlet bounding-sphere/normal-cone verifier), paired with a bit-exact Python parity benchmark and 5 planted-failure controls.
- `aerospace_ai_kernels.bld` + `bench_aerospace_ai_kernels.py`: Pure-BuildLang SpaceX GNC and Frontier-AI kernel suite (Tsiolkovsky multi-stage delta-V, symplectic Velocity-Verlet two-body + J2 oblateness orbital propagator, 6-DOF powered-descent ZEM/ZEV optimal landing controller, multiply-free BitNet b1.58 packed ternary GEMV, and Flash tiled online-softmax + Factorized Context Attention), paired with a bit-exact Python parity benchmark and 5 planted-failure controls.
- `sota_ml_physics_kernels.bld` + `bench_sota_ml_physics_kernels.py`: Pure-BuildLang SOTA ML (`rmsnorm`, `rope_rotate_pair`, `swiglu_gate`, tiled `flash_online_softmax`, scaled `bitnet_b158_ternary_gemv`) and 6-DOF symplectic Velocity-Verlet + Rodrigues unit-quaternion + Tsiolkovsky powered-descent physics suite with 6 planted-failure controls and bit-exact Python parity benchmark.
- `vault_verifier.bld`: Pure-BuildLang Antigravity Lossless Vault Verifier kernel checking canonical FNV-1a manifest digests, deduplication byte conservation, and planted-tamper rejection.
- `null_cone_gravity_check.bld`: Pure-BuildLang deterministic f64 verification kernel for null-cone gravity equations (Raychaudhuri Simpson quadrature, polylog crossover, corner-boost shift, clock-entropy log) with 4 planted-failure controls.
- `wave15_online_rkl_noh4_oracle.bld`: Pure-BuildLang deterministic f64 1-Pass Online-Softmax Reverse-KL oracle over Qwen2.5 151,936-vocab logits (`par_for` associative chunk merge matching Python `dense_exact_reverse_kl` to `< 1e-13`, overflow-proof under `+800/-800` shift) plus NOH-4 proactive + reseating CODIT immune valve (`MACE4IRmol` 4-stream spread, energy residual, `K_settle=3` hysteresis, cosine-ramp re-admission, and 4 planted controls).
- `workstation_mcp.bld` + `test_agent_sdk_mcp.py`: Pure-BuildLang Model- & Agent-Native Compiled SDK Server v2 (`~ Console + FileSystem + Clock + Environment`, zero Network/Process/Foreign authority) implementing JSON-RPC 2.0 MCP over stdio (`initialize`, `ping`, `tools/list`, `tools/call`) with 8 compact tools (`sdk_manifest`, `anti_silo_preflight`, `audit_public_hygiene`, `kcore_eval`, `procgen_sdf_eval`, `aerospace_gnc_eval`, `bitnet_ternary_dot`, `receipt_verify_hash`) returning `{ ok, verdict, result, next_valid_actions, fnv1a_digest, elapsed_ns }`, `--self-test`, and stdio latency/parity harness.

## Run

```bash
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/kcore_fast.bld
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/procgen_forge.bld
python universe/workstation/bench_procgen_forge.py
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/aerospace_ai_kernels.bld
python universe/workstation/bench_aerospace_ai_kernels.py
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/sota_ml_physics_kernels.bld
python universe/workstation/bench_sota_ml_physics_kernels.py
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/vault_verifier.bld
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/null_cone_gravity_check.bld
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/wave15_online_rkl_noh4_oracle.bld
BUILDLANG_STDLIB=stdlib buildc run --release universe/workstation/workstation_mcp.bld -- --self-test
python universe/workstation/test_agent_sdk_mcp.py
```
