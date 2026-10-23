# Held-out probe regressions

Each program here reproduces one failure class from the held-out
agent-writability probe of 1.6.0 (`bench/heldout-2026-10-15/RESULTS.md`).
`compiler/tests/heldout_regressions.rs` builds and runs every program and
compares its standard output with the `.expected` file beside it, and
`check_implies_compile.rs` includes this directory.

| Program | Issue |
|---|---|
| match_unit_arm.bld | #75 |
| split_empty.bld | #76 |
| index_borrowed_vec.bld | #77 |
| generic_vec.bld, generic_body.bld | #78 |
| vec_char_display.bld | #79 |
| format_escapes.bld, println_escapes.bld | #80 |
| char_leak.bld | #81 |
| generic_twice.bld | #82 |
| box_scalar.bld, box_list.bld, box_bst.bld | #83 |
| unsafe_block_runs.bld, deref_field_store.bld | M4 (found while building the C ABI export) |
| match_vec_new_arm.bld | #98 |
| diverging_arm_later_fn.bld | #99 |
| format_inline_char_cast.bld | #100 |
| module_macro_args.bld (and `tests/modmacro/`) | module calls inside macro arguments (M4 pilot) |
| float_sqrt_constants.bld | f64 `sqrt` rounding on MinGW, and the f64 constants (M4 pilot) |
| struct_literal_fields.bld | struct literal fields out of order, and `Vec::new()` in a field (M4 pilot) |
| vec_element_stores.bld | stores into vector element fields and through `&mut Vec` (M4 pilot) |
