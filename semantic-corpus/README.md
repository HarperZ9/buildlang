# BuildLang Semantic Corpus

This corpus contains small BuildLang programs used as stable semantic vectors.
Each program should represent behavior that matters across backend boundaries:
control flow, mutation, aggregate data, ownership, effects, or resource-shaped
state.

The first slice is intentionally narrow. The Rust backend includes these
programs directly in executable smoke tests, and the corpus is suitable for
extension into C/Rust/LLVM/WASM cross-backend receipts.

`manifest.json` is part of the executable contract. Compiler tests validate
its schema, unique program IDs, source paths, expected stdout, declared
surfaces, and named Rust execution tests before trusting receipt metadata.
The current receipt set includes Rust executable tests and a C execution
receipt for all 8 programs. `buildc run` uses per-run temp build directories,
so C receipt generation can be parallel-probed without shared C/PDB collisions.
Run `buildc corpus verify` from the repository to validate `manifest.json`,
the C/Rust receipts, and real C-backend stdout against the manifest. Use
`buildc corpus verify --root <DIR>` for copied corpus fixtures. Add `--write`
only after C stdout validation should refresh that copy's C execution receipt;
Rust receipt changes remain covered by the Rust backend test suite.

## Capability Metadata

Execution receipts record the capability gate posture for the corpus:

- `declared_effects`: built-in effects that corpus programs declare in source.
- `observed_capabilities`: capabilities expected from manifest surfaces.
- `capability_gate`: `passed` when receipt verification includes capability
  metadata.
- `capability_gate_test`: the compiler test proving capability enforcement.

The current corpus declares and observes `Console` because every program owns a
stdout contract. `buildc corpus verify` derives the expected capability list
from `manifest.json` surfaces and rejects receipt drift if the metadata is
missing or inconsistent.

## Current Programs

- `scalar_branch.bld`: function call, branch selection, stdout.
- `references_mutation.bld`: mutable reference update, immutable readback,
  stdout.
- `structs_arrays.bld`: struct fields, fixed arrays, function call, stdout.
- `tuple_ownership_reuse.bld`: tuple aggregate lowering and by-value reuse,
  stdout.
- `struct_aggregate_reuse.bld`: struct aggregate reuse through multiple
  fields, stdout.
- `field_assignment_reuse.bld`: struct field assignment and post-assignment
  reuse, stdout.
- `nested_field_reuse.bld`: nested struct field access and reuse, stdout.
- `deref_reuse.bld`: reference dereference and post-deref reuse, stdout.
