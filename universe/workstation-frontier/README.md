# BuildLang Workstation Frontier Kernels

Pure-BuildLang deterministic aerospace GNC, fundamental-constants orbital dynamics, SPICE noise budgeting, checksummed columnar database aggregation, fail-closed DAG pipelines, statistical outlier hunting, 4D polytope projections, nuclear shell stability, and biological quantum-limit kernels licensed under MIT and verified on buildc 1.15.0+.

- `flight_6dof.bld` (`N02`): Six-DOF quaternion quadrotor and ballistic flight dynamics with ISA atmosphere, LQR attitude and altitude control, and planted unnormalized-quaternion guard.
- `const_sandbox.bld` (`E1`): Tunable fundamental-constants symplectic orbital verifier checking period scaling with gravitational constant and unbound escape thresholds.
- `vbench_spice.bld` (`E12`): Johnson-Nyquist thermal, shot, and flicker SPICE noise-density budget evaluator across 28 sealed instrument specs.
- `bdb_query_bench.bld` (`F6`): Checksummed columnar hash-join and group-by aggregation engine over 68,500 rows with 500 planted 1-bit page-corruption checks.
- `dag_pipeline.bld` (`E18`): Declarative 5-stage topological DAG runner proving fail-closed downstream halt on stage-3 verification failure.
- `contrary_scanner.bld` (`A7`): Assumption-mutating MAD and Huber 4-sigma anomaly scanner across 1,000 physical-law residual rows.
- `ndim_slice4d.bld` (`A20`): Four-dimensional tesseract SO(4) rotation and hyperplane slicer preserving 16 vertices and 32 edge lengths.
- `coxeter_24cell.bld` (`N14`): Regular 24-cell F4 Coxeter-plane projection checking 24 vertices, 96 edges, and exact 8-fold and 12-fold rotational symmetry.
- `nuclide_shells.bld` (`N15`): Semi-empirical Bethe-Weizsacker plus shell-correction nuclear binding energy model reproducing Fe-56 peak and magic-number stability.
- `bio_sensor_limits.bld` (`B6`): Dark-adapted retinal rod Poisson photon threshold, Weber-Fechner compression, and cochlear thermal-Brownian floor verifier.

## Run

Run any kernel directly via `buildc check`, `buildc lint`, and `buildc run` in release mode on `universe/workstation-frontier/<kernel>.bld`.
