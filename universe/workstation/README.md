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
- `kcore_64_primitives.bld` (`OPT-053`, Author Idea `F3`): 64-primitive mathematical core powered by native 106-bit Dekker/Shewchuk Double-Double arithmetic (`64/64 <= 0.66 ULP`, `61/64 <= 0.5 ULP` vs 100-digit `mpmath`) plus planted cancellation trap.
- `registry_of_everything.bld` (`SIG-066`, Author Ideas `A5`/`D5`): Deterministic unit-verified 1,024-entry Registry of Everything across colors, Schott/Ohara glasses, materials, waveforms, and CODATA 2022 constants (`0.44 us` range queries, 4/4 planted schema/duplicate traps caught).
- `scale_algebra.bld` (`PHY-064`, Author Idea `A3`/`AC4`): Buckingham Pi dimensional scaling & exact SI unit-algebra verifier with planted dimensional-mismatch trap.
- `conservation.bld` (`PHY-065`, Author Idea `A24`): Symplectic Velocity-Verlet vs explicit Euler energy/momentum/angular-momentum conservation verifier.
- `kcore_color.bld` (`COL-039`, Author Idea `B2`): Exact Sharma 2005 CIEDE2000 + Oklab perceptual color space round-trip kernel.
- `signal_kcore.bld` (`SIG-059`, Author Idea `D1`): Matched-filter SNR, Cramér-Rao lower bound, and Wiener optimal deconvolution kernel.
- `optics_kcore.bld` (`PHY-071`, Author Ideas `B1`/`N09`): Snell refraction, paraxial thin-lens ABCD ray trace, Newton Airy disk, and 2x2 complex Transfer Matrix Method 10-pair quarter-wave TiO2/SiO2 Bragg mirror.
- `cfd_kcore.bld` (`PHY-072`, Author Idea `C8`): 1D compressible Euler Sod shock-tube solver (MUSCL-Minmod + HLL, `< 0.41%` L2 error vs exact Toro Riemann) plus laminar Hagen-Poiseuille pipe flow and planted `CFL > 1` stability guard.
- `data_forge_core.bld` (`SIG-058`): Pure-BuildLang FIPS 180-4 SHA-256, RFC 6962 Merkle tree sealing, holdout 6-gram decontamination, and fail-closed privacy/PII/secret/HIPAA firewall (`6/6` planted controls caught).
- `articulate_core.bld` (`SIG-057`): Pure-BuildLang zero-slop prose auditor (9 detectors + cadence CV) catching `6/6` planted AI-tell traps while passing `6/6` clean human baselines.

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
