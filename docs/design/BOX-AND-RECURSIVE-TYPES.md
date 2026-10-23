# Box<T> and recursive types (issue #83)

Status: implemented for 1.8.0 (2026-10-22). Field access through a box needs `(*b).x`.

## What 1.7.0 does

Measured on a build of main at bd994c1:

- `Box::new(x)` passes the type checker and is then refused by the backend gate with BL0901
  ("`Box_new` cannot be compiled"). A binary search tree cannot be written.
- A type annotation `Box<T>` is not resolved by the checker. It becomes a fresh type variable, so
  `let b: Box<i64> = 5;` passes `buildc check`, builds, and segfaults when printed
  (`tests/reject/box_annotation_mismatch.bld`). That is a check-pass-but-fail.
- An enum that contains itself by value (`enum T { Leaf, Node(T) }`) passes `buildc check` and then
  fails in gcc with "field has incomplete type" (`tests/reject/recursive_without_box.bld`). Another
  check-pass-but-fail.
- MIR lowering already maps the annotation `Box<T>`, `Rc<T>` and `Arc<T>` to a thin C pointer
  (`codegen/lower/types.rs`), and `*p` on a pointer lowers to a C dereference.

## Decision

Support `Box<T>` on the C backend; documenting a workaround would leave the probe's `bst` task
unwritable.

1. Checker: `Box<T>` resolves to an owned pointer type to `T` (the checker's pointer kind, mutable).
   `Box::new` has type `fn(T) -> Box<T>`. `*b` on a `Box<T>` has type `T`. With the annotation
   resolved, `let b: Box<i64> = 5` is a type mismatch.
2. Infinite-size check: after item collection, a struct or enum that reaches itself through
   by-value fields (directly, or through tuples, arrays, `Option` or `Result`) is rejected with a new
   code and an edit that wraps the field in `Box<...>`. `Box`, `Vec`, `HashMap`, references and raw
   pointers break the cycle.
3. Lowering: `Box::new(x)` lowers to a call to `Box_new` whose destination is `Ptr(type of x)`.
4. C backend: `Box_new` emits `malloc(sizeof(T))`, aborts on a null result, stores the argument and
   assigns the pointer. Enum and struct typedefs are forward-declared so a payload can point at its
   own type.

## Limits in 1.8.0

- Boxed memory is not freed. A program that builds many boxes grows until it exits. A drop pass is
  future work and needs the ownership rules for moves out of `*b`.
- `Rc<T>` and `Arc<T>` lower the same way, with no reference count. Until counting exists the checker
  refuses `Rc::new` and `Arc::new` with an explicit message, so nobody relies on sharing semantics
  that are not there.
- `Box<dyn Trait>` (a fat pointer) stays unsupported and is refused by the checker.
- Field access through a box needs an explicit dereference, `(*b).x`, unless the auto-deref test in
  the implementation shows otherwise.

## Checks

- `tests/heldout/box_scalar`, `box_list`, `box_bst`: check, build, run and print the expected
  output (`compiler/tests/heldout_regressions.rs`, and `check_implies_compile.rs`).
- `tests/reject/*.bld`: `buildc check` must fail with a BL code. Each failed to be rejected on 1.7.0.
