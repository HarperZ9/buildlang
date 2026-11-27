# Workstation Physics, Signals, and Optics Kernels

Ten verified pure BuildLang numerical physics, signal processing, optics, biomechanics, and stress-testing kernels with deterministic verification harnesses and fail-closed planted controls.

## Kernels

1. `symmetry_audit.bld`: Automated symmetry invariant auditor testing translation, rotation, scaling, and reflection laws against planted broken-symmetry controls.
2. `analogy_contract.bld`: Structural-law analogy verifier enforcing dimensional and dynamical equivalence while rejecting planted non-isomorphic analogies fail-closed.
3. `materials_visco.bld`: Maxwell and Kelvin-Voigt viscoelastic-plastic creep-recovery and homeostatic setpoint adaptation kernel separating elastic rebound from plastic yield.
4. `digics_adc.bld`: Analog-to-digital quantization and transfer kernel reproducing theoretical SQNR scaling across bit depths with planted quantization-error controls.
5. `cross_modal_chirp.bld`: Deterministic bidirectional chirp spectrogram to audio round-trip transduction kernel with planted phase-scramble rejection.
6. `aperture_gate.bld`: Optical aperture photon SNR versus Airy diffraction resolution stop-sweep simulator with planted diffraction-bound checks.
7. `bio_pareto.bld`: Pareto optimality-gap scorer comparing benchmark biological mechanisms against Carnot, Shannon, Airy diffraction, and shot-noise physical limits.
8. `law_shapes.bld`: Symplectic undamped Duffing double-well oscillator separatrix integrator verifying homoclinic saddle-point accuracy against analytic invariants.
9. `marbling_advect.bld`: Two-dimensional streamfunction curl-noise incompressible fluid marbling advection kernel verifying divergence-free area preservation across 500 steps.
10. `ood_log_sweep.bld`: Six-decade out-of-distribution log-sweep stress harness flagging planted NaN, overflow, and symmetry breakdown fail-closed.

## Verification

Run `buildc check`, `buildc lint`, and `buildc run` in release mode on each `.bld` file in this directory. All ten programs compile with zero type errors, zero lint warnings, and exit with status 0.

## License

Licensed under the MIT License. See `LICENSE` in this directory.
