# Multiple Dispatch (`buildlang`)

> Status: **static multiple dispatch, shipped 2026-07-01.** Opt-in by simply defining
> more than one function with the same name. Backward-compatible: single-definition names
> and `extern "C"` FFI are byte-identical to before.

buildlang supports **Julia-style multiple dispatch**: several functions may share one name,
each with a different parameter-type signature, and a call selects the method whose parameter
types best match the **tuple of all argument types** — not just the first/receiver argument.

```
fn area(w: i64, h: i64) -> i64 { w * h }        // rectangle
fn area(r: i64) -> i64 { 3 * r * r }            // (rough) circle by radius
fn combine(a: i64, b: i64) -> i64 { a + b }
fn combine(a: i64, b: bool) -> i64 { if b { a } else { 0 } }  // dispatch on the 2nd arg

fn main() ~ Console {
    println!("{}", area(3, 4));      // -> area(i64,i64)  = 12
    println!("{}", area(5));         // -> area(i64)      = 75
    println!("{}", combine(7, true)); // -> combine(i64,bool), NOT combine(i64,i64)
}
```

## How it works (static resolution)

1. **Registry.** Multiple functions with the same name are all kept (previously a second
   definition silently overwrote the first). Each is a candidate with its parameter types.
2. **Resolution by the argument-type tuple.** At a call, buildlang infers the types of *all*
   arguments (Hindley-Milner) and selects the **unique most-specific** candidate via one
   shared resolver (`types/dispatch.rs`, used by both the type checker and codegen so they
   can never disagree). Specificity order: **exact type match > coercion/concrete > generic**
   (a concrete parameter is more specific than a generic `T`), Julia-style.
3. **Code generation.** A name with **one** definition keeps its plain C name (so existing
   code and FFI are unchanged). A name with **two or more** definitions emits each as a
   distinct mangled function (`name_<param-types>`, reusing the generic mangler), and each
   call emits a direct call to the selected one.
4. **Generics compose.** A generic `fn f<T>(x: T)` and a concrete `fn f(x: i64)` can share a
   name; a call picks the concrete when it matches (more specific) and otherwise
   monomorphizes the generic.

## Errors

- **No matching method** — no candidate's parameters match the argument tuple.
- **Ambiguous method** — two or more candidates are equally most-specific and incomparable;
  the error lists them so you can add a more specific method. (This is an *error*, never a
  silent arbitrary pick.)

## Not yet supported (deferred)

- **Operator overloading on both operands.** Operators (`+`, `*`, ...) still dispatch on the
  left operand's type only. Full both-operand operator dispatch through the resolver is a
  tracked follow-on.
- **Dynamic (runtime-type) dispatch.** Julia dispatches at runtime when argument types are
  not statically known. buildlang resolves dispatch **statically**; it has no runtime type
  descriptors yet. This covers nearly all calls (HM inference makes argument types static),
  but a value typed only as an abstract trait object uses single-dispatch vtables, not
  multiple dispatch. Runtime multiple dispatch is a separate future effort.
- **A generic-only name called with two different types in one scope** (`identity(42)` then
  `identity("x")`) can still fail the checker — a pre-existing monomorphic-generalization gap
  in the single-candidate path, unrelated to multiple dispatch. Overloaded names are
  unaffected.
