# QuantaLang Semantic Corpus

This corpus contains small QuantaLang programs used as stable semantic vectors.
Each program should represent behavior that matters across backend boundaries:
control flow, mutation, aggregate data, ownership, effects, or resource-shaped
state.

The first slice is intentionally narrow. The Rust backend includes these
programs directly in executable smoke tests, and the corpus is suitable for
extension into C/Rust/LLVM/WASM cross-backend receipts.

## Current Programs

- `scalar_branch.quanta`: function call, branch selection, stdout.
- `references_mutation.quanta`: mutable reference update, immutable readback,
  stdout.
- `structs_arrays.quanta`: struct fields, fixed arrays, function call, stdout.
- `tuple_ownership_reuse.quanta`: tuple aggregate lowering and by-value reuse,
  stdout.
- `struct_aggregate_reuse.quanta`: struct aggregate reuse through multiple
  fields, stdout.
- `field_assignment_reuse.quanta`: struct field assignment and post-assignment
  reuse, stdout.
