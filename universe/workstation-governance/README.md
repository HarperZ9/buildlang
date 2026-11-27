# BuildLang Workstation Accountability and Epistemics Kernels

Ten verified governance and epistemics kernels written in native BuildLang under the MIT license. Every module builds cleanly on buildc 1.15.0+, carries an explicit planted negative control, and executes with zero external dependencies.

- `honest_exit_valve.bld` (`N25`): Zero-penalty honest-exit reseating valve separating unguarded reward hacking from verified abstention across 300 reseats, catching faulty arms fail-closed.
- `three_control_negatives.bld` (`N28`): Three-control negative verification gate (false-pass, false-fail, permuted-null).
- `anti_self_audit_chain.bld` (`N49`): Independent out-of-band FIPS 180-4 SHA-256 audit chain across 16 entries that blocks self-certification and catches single-bit digest tampering.
- `isomorphism_entropy.bld` (`D6`): Bijection and conditional entropy check rejecting lossy collapse.
- `uncheatable_search_forge.bld` (`N43`): Deterministic retrieval and forge integrity verifier checking SHA-256 source grounding and blocking hallucinated citations or poisoned ranks.
- `personal_voice_twin.bld` (`N51`): Cadence and stylistic fingerprint gate rejecting synthetic drift and impersonation.
- `universal_articulate_pipe.bld` (`N52`): Streaming prose-hygiene audit checking punctuation and slop thresholds across four profiles.
- `typed_second_channel.bld` (`N27`): Typed out-of-band provenance channel rejecting empty, circular, copied, or misdated envelopes.
- `confidence_word_calib.bld` (`N30`): Confidence-word calibration check mapping numeric evidence bands 3 down to 0 while rejecting overclaiming or missing controls.
- `gather_refit_graph.bld` (`N32`): Deterministic evidence-gathering and dependency-refit graph solver verifying topological updates while catching orphan nodes and planted cycles.

## Run

Check, lint, and execute any module directly in release mode:

```
buildc check universe/workstation-governance/<kernel>.bld
buildc lint universe/workstation-governance/<kernel>.bld
buildc run --release universe/workstation-governance/<kernel>.bld
```
