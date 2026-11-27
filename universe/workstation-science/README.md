# BuildLang Workstation Science, Aerospace, and Audio Kernels

Pure-BuildLang deterministic physics, biomechanics, orbital dynamics, robotics, geometry, geophysics, and acoustics kernels licensed under MIT and verified on buildc 1.15.0+.

- `daw_dsp.bld` (`PHY-066`, `E7`): Fractional-delay waveguide plucked string and 4-mode Kirchhoff C260 brass plate synthesizer with Goertzel spectral checks and planted passivity guard.
- `tensegrity.bld` (`PHY-067`, `B3`): 18-DOF 3-strut Snelson tensegrity prism nonlinear static equilibrium and prestress solver with planted slack-cable collapse trap.
- `tree_biomech.bld` (`PHY-068`, `C1`): Coupled Mattheck hollow-stem shell buckling boundary, 3-DOF multi-branch TMD sway, and gale root-plate overturn moment model.
- `orbital_assembly.bld` (`PHY-069`, `N04`): Symmetric Strang Lie-Poisson Dzhanibekov intermediate-axis flip integrator and Clohessy-Wiltshire 3-module orbital rendezvous docking kernel.
- `bio_flight_swarm.bld` (`PHY-070`, `N01`): Weis-Fogh clap-and-fling flapping-wing lift augmentation and 12-agent 3D Reynolds swarm collision-free dynamics.
- `passive_uav_track.bld` (`SIG-061`, `N03`): Passive 4-microphone acoustic TDOA and 2-state azimuth Kalman tracker with urban car-horn distractor rejection.
- `cad_eng.bld` (`GEO-024`, `E21`): Parametric involute spur gear tooth and optical lens barrel generator exporting verified unit-normal ASCII STL facets.
- `earth_subsurface.bld` (`GEO-025`, `N13`): Nonlinear Gauss-Newton Bouguer gravity buried-sphere inversion and 2D bare-earth LiDAR morphological ground filter.
- `transducers.bld` (`SIG-062`, `D9`): Coupled acoustic, magnetic, and thermal transducer impedance and sensitivity verifier with planted clipping and saturation controls.
- `instruments_fusion.bld` (`SIG-063`, `N08`): Multi-modal acoustic and stress-wave sensor fusion estimator for non-destructive structural hollow detection.

## Run

Run any kernel directly via `buildc check`, `buildc lint`, and `buildc run` in release mode on `universe/workstation-science/<kernel>.bld`.
