# Workstation Creativity Kernels (`universe/workstation-creativity/`)

Pure-OSI `MIT` native BuildLang kernels graduated from Wave 76C into `HarperZ9/buildlang` (`universe/workstation-creativity/`), covering typography, audio telemetry, procedural grammar synthesis, visual distillation, retro console shaders, pen-plotter path optimization, divergence-free flow fields, perceptual ASCII rendering, smooth explainer animation timelines, and midcentury graphic grid layout.

Author vision anchors: `E5`, `B9`, `E16`, `E8`, `E4`, `N36`, `E9`, `E10`, `E17`, and `K3`.

## Kernel Inventory (10 Verified Native BuildLang Programs)

- `living_type_bezier.bld` (`E5`): Optical-size cubic Bezier typography and kerning solver (`kern_av=-0.260`, `0.15 ms`); catches planted self-intersecting bowtie Bezier trap fail-closed (`intersections=1`).
- `ear_telemetry.bld` (`B9`): Deterministic audio tempo and key telemetry kernel (`max_bpm_err=0.00174`, `1.03 ms`); catches planted silent-null input (`status=1`, `key_id=-1`).
- `procgen_grammar.bld` (`E16`): Deterministic procedural split-grammar layout generator (`xor_h0=251600908`, `0.63 ms`); catches planted empty-spec control (`status=1`).
- `distill_extract.bld` (`E8`): Deterministic palette and layout feature distillation kernel (`xor_hash=1933852425`, `0.13 ms`); catches planted out-of-bounds and NaN corruption traps (`oob_status=1`, `nan_status=1`).
- `console_shader_stack.bld` (`E4`): Physical retro console CRT scanline and phosphor energy shader kernel (`e_ratio=0.8109`, `max_c=0.8807`, `0.005 ms`); catches planted energy-conservation violation (`traps_caught=1`).
- `plotter_path_writer.bld` (`N36`): Two-opt pen-plotter travel optimizer cutting pen-up air travel by `81.35%` with `0.00 mm` pen-down error (`0.016 ms`) compared to naive unoptimized stroke order; catches planted out-of-bounds and dropped-stroke traps (`oob_caught=1`, `drop_caught=1`).
- `plotter_flow_studio.bld` (`E9`): Divergence-free curl-noise flow-field plotter integrator (`max_div=1.54e-13`, `len=305.10 mm`, clearance `22.8 mm`, `0.23 ms`) compared to noisy Euler streamlines; catches planted sink-divergence trap (`sink_caught=1`).
- `ascii_render_engine.bld` (`E10`): Perceptual glyph-density ASCII renderer (`mean_err=0.00087`, `digest=3058441201`, `0.009 ms`) compared to flat linear-ramp thresholding; catches planted ramp-inversion trap (`traps_caught=1`).
- `explainer_timeline.bld` (`E17`): Arc-length reparameterized C1 cubic Hermite explainer timeline (`c1_err=2.22e-16`, `reparam_err=7.04e-7`, `0.040 ms`) compared to unnormalized parameter stepping; catches planted non-monotonic keyframe trap (`mono_caught=1`).
- `midcentury_synthesis_grid.bld` (`K3`): Swiss and midcentury harmonic layout grid solver (`elems=12`, `collisions=0`, `ratio_err=0.00`, `0.003 ms`) compared to manual unvalidated bounding boxes; catches planted overlap collision trap (`traps_caught=1`).

## Verification and License

Every `.bld` file in this folder passes `buildc check`, `buildc lint` (`0` warnings), and `buildc run` in release mode with zero failures, alongside `SIG-058` privacy verification (`0` flags). All files in this directory are licensed under the pure-OSI `MIT` license (`LICENSE`).

## Scope Limits

These kernels run deterministic CPU mathematical and geometric verification workloads with self-contained synthetic fixtures; interactive canvas rendering, live audio device capture, and serial pen-plotter hardware I/O live in downstream host adapters.
