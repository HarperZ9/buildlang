# Math Syntax (`buildlang`)

> Status: **shipped 2026-07-01** (Pillar B). Four additive features inside the verified
> lexer to parser to type-check to MIR to C path. Backward-compatible: existing programs
> compile to byte-identical C (verified by a per-feature differential sweep).

buildlang gains Julia-flavored mathematical notation in four independent pieces. Two are
ergonomic sugar (`**`, Unicode aliases); two are the substantive scientific-computing content
(array broadcasting, a vector-algebra library). Read the scope note below before assuming
Julia parity: this is **elementwise broadcasting over fixed-size arrays plus a 1-D vector
library over dynamic `Vec<f64>`**, not full linear algebra.

## 1. Broadcasting operators `.+ .- .* ./` (over fixed-size `Array<T,N>`)

Elementwise operators apply a scalar operation position-by-position over fixed-size arrays,
and broadcast a scalar over every element.

```
fn main() ~ Console {
    let a = [1.0, 2.0, 3.0];
    let b = [10.0, 20.0, 30.0];
    let sum    = a .+ b;      // [11, 22, 33]  elementwise
    let scaled = a .* 2.0;    // [2, 4, 6]     scalar broadcast (right)
    let shifted = 2.0 .+ a;   // [3, 4, 5]     scalar broadcast (left)
    println("{} {} {}", sum[0], sum[1], sum[2]);
}
```

**Operand type.** These operate on `Array<T,N>`, the type of an array literal `[..]` (fixed
length known at compile time). They do **not** operate on the dynamic `Vec<T>`; for dynamic
vectors use the `linalg` library (below).

**Length checking is at compile time.** `[1.0, 2.0] .+ [1.0, 2.0, 3.0]` is a type error
(`buildc check` rejects it), because the length is part of the `Array<T,N>` type. There is no
runtime dimension check and no runtime failure mode.

**How it works.** `.+ .- .* ./` lex to distinct tokens and parse to `BinOp::DotAdd/DotSub/
DotMul/DotDiv`, sharing the precedence of their scalar counterparts (`.+/.-` like `+/-`,
`.*/./` like `*//`). The type checker (`infer_binary`) requires equal lengths for
array-with-array, unifies the element types, and types a scalar operand against the element
type. Codegen (`lower_broadcast`) desugars the operator into an unrolled array of per-element
**scalar** MIR ops, so the broadcast operators never reach any backend as a distinct
operation. Element types other than `f64` (for example integer arrays) broadcast too, using
the ordinary scalar operation for that element type.

**Element type must be numeric.** Broadcasting is defined only for integer and floating-point
element types. `["a"] .+ ["b"]` and `[true] .* [false]` are type errors, not string
concatenation or boolean arithmetic.

**Whitespace with a leading numeric literal.** Write scalar-left broadcast with a space:
`2.0 .+ a` or `2 .+ a`. The space-free spelling `2.+a` lexes the `.` as a decimal point
(`2.0 + a`, a scalar-plus-array type error), because the number scanner claims the `.` before
the broadcast lexer sees it. Array-left broadcast (`a .+ b`, `a .* 2.0`) has no such ambiguity.

**Fixed-size arrays through function signatures.** `Array<T,N>` is a first-class type: it can
be a function parameter type *and* a return type, so a real numerical kernel can be written as
an ordinary function. The compile-time length is carried across the call boundary — a
shape-mismatched argument or an inconsistent declared return shape is a `buildc check` error,
never a runtime failure.

```
fn axpy(a: f64, x: [f64; 3], y: [f64; 3]) -> [f64; 3] {
    let scaled = a .* x;      // scalar-left broadcast, still [f64; 3]
    scaled .+ y               // elementwise add, [f64; 3]
}

fn main() ~ Console {
    let r = axpy(2.0, [1.0, 2.0, 3.0], [10.0, 20.0, 30.0]);   // [12, 24, 36]
    println("{} {} {}", r[0], r[1], r[2]);
}
```

Passing a `[f64; 4]` where a `[f64; 3]` parameter is declared, or returning a `[f64; 3]` body
from a `-> [f64; 2]` signature, is rejected at compile time with a length/shape diagnostic. C
cannot return or assign a bare array by value, so an array-returning function lowers to a
`void` function with a caller-allocated pointer-to-array out-parameter (the result is
`memcpy`d into the caller's array); this is an internal codegen detail with no surface syntax.

## 2. `linalg` vector-algebra library (over dynamic `Vec<f64>`)

For runtime-sized numeric vectors, the `linalg` stdlib module provides free functions over the
dynamic `Vec<f64>`. Opt in with `mod linalg;` and call by bare name.

```
mod core;
mod math;
mod linalg;

fn main() ~ Console {
    let mut a = vec_new_f64();
    vec_push_f64(a, 3.0); vec_push_f64(a, 4.0);
    let n = vec_norm(a);          // 5.0
    println("{}", n);
}
```

Functions: `vec_add`, `vec_sub`, `vec_mul`, `vec_div` (elementwise), `vec_scale` and
`vec_scalar_add` (scalar broadcast), `vec_dot`, `vec_sum`, `vec_norm` (`sqrt(dot(v,v))`). These
are ordinary buildlang functions built on the existing f64 vector builtins; there is no
compiler or runtime change behind them. Binary elementwise functions iterate to the first
operand's length (callers pass equal-length vectors).

## 3. `**` power operator

`a ** b` is exponentiation, right-associative, binding tighter than `*` and inside a leading
unary minus (so `-2 ** 2` is `-(2 ** 2) = -4`, the Julia/Python convention). It wires the
front-end to buildlang's pre-existing `BinOp::Pow` (already lowered to C `pow(l, r)`), so the
value is computed in double precision; integer exactness beyond 2^53 is lossy. Prefix `**x`
remains double dereference `*(*x)`, so no pointer code changes meaning.

One deliberate change: an adjacent, space-free `a**b` now means `a` to the power `b`. Before
this feature it lexed as `a * *b` (multiply by a dereference). If you mean multiply-by-deref,
write `a * *b` with the space. No program in the tree used the adjacent spelling.

## 4. Unicode arithmetic-operator aliases

`×` (and `·`, `∙`) alias `*`; `÷` aliases `/`; `−` (U+2212, the minus sign, not the ASCII
hyphen) aliases `-`. These are pure lexer aliases to existing operators, so `6 × 7` is exactly
`6 * 7`. Unicode letters already work as identifiers; this only adds the symbol operators.

## Scope (read this before claiming parity)

Shipped: elementwise broadcasting over fixed-size `Array<T,N>`, `Array<T,N>` as a first-class
function parameter and return type (so numerical kernels can be written as ordinary functions
with compile-time shape checking across the call boundary), a 1-D vector-algebra library over
dynamic `Vec<f64>`, the `**` operator, and Unicode operator aliases. That is an honest "math
syntax" surface, not "Julia-parity linear algebra": this closes the array-ergonomics gap, not
the GPU general-compute gap, which remains a separate effort.

Two distinct surfaces, on purpose: the broadcast **operators** work on fixed-size array
literals; the **library** works on dynamic `Vec<f64>`. A dynamic `Vec` cannot yet use `.+`.

Deferred (tracked follow-ons):

- **Dynamic-`Vec` broadcasting operators.** Extending `.+` to `Vec<T>` needs a runtime length
  check and a runtime-length loop lowering.
- **A true dynamic 2-D `Matrix{T}` and linear algebra** (matmul, transpose, solve). There is
  no N-D array/matrix type in the MIR yet (`MirType::Vector` is fixed-lane SIMD; `mat4` is a
  hardcoded graphics struct), so this is a substantial separate effort.
- **`f32` element parity.** Everything here is `f64`. `Vec<f32>` currently routes through the
  f64 runtime handles, so a real `f32` path is a coordinated runtime, lowering, and
  type-checker change.
- **Broadcast comparisons (`.== .< .>`), `.^`, and Unicode operators with no ASCII operator
  equivalent (`⊗`, `∘`).**
