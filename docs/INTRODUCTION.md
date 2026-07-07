# An Introduction to BuildLang

BuildLang is a systems programming language whose type system tells you what a
program touches. Every ambient capability, file IO, network, process control,
environment, clock, GPU, console, foreign code, is a typed effect in a
function's signature, checked by the compiler, and recordable in a receipt you
can re-verify later. Programs compile to native binaries through a C backend,
to HLSL or GLSL for shader work, and experimentally to SPIR-V, LLVM IR, WASM,
Rust, x86-64, and ARM64. The whole toolchain is one binary, `buildc`.

## Why it exists

Most languages treat IO as invisible: any function can open a socket and
nothing in its type says so. BuildLang makes ambient access part of the
contract, and then goes one step further: the compiler can seal what it
observed into a machine-readable receipt bound to source digests, so a
reviewer, a CI gate, or a future you can re-check the claim instead of
trusting it. Capability effects are the language feature; receipts are the
paper trail.

## Core concepts

### Capability effects

A function that reads a file must say so:

```build
fn load_config() ~ FileSystem {
    read_file("ops.toml");
}
```

The `~ FileSystem` clause is a typed effect. Calling `tcp_connect` requires
`~ Network`, calling into an `extern` block requires `~ Foreign`, `println!`
requires `~ Console`, and compile-time macros like `include_str!` and `env!`
are gated too. Effects propagate: a caller of `load_config` must declare or
handle `FileSystem` itself. The checker follows function values through
closures, struct fields, enum payloads, control flow, and async blocks, so a
callback stored in a struct cannot erase its capability row.

### Types

Inference is Hindley-Milner: you rarely write types on locals. The language
has structs, enums (sum types), traits with dynamic dispatch, generics,
pattern matching, closures, and iterators. An experimental `#[linear]`
attribute marks a type as no-cloning, a value that should be moved at most
once. That is the shared discipline beneath qubits, no-double-spend ledger
entries, and unique resource handles. It is honest about its maturity:
a large set of escapes is rejected under regression tests, full soundness is
still open. See [LINEAR-TYPES.md](LINEAR-TYPES.md).

### Compilation model

`buildc` lowers source through a typed AST into MIR (an SSA mid-level IR),
then into a backend. C is the production path: generated C compiles with gcc,
clang, or MSVC. Shader entry points marked `#[fragment]` emit HLSL or GLSL
directly. `#[compute]` kernels emit dispatchable SPIR-V, and a build with
`--features gpu` can run them on a physical Vulkan device with a CPU
cross-check (`buildc run --gpu`, experimental). MIR itself has a versioned
JSON form (`buildc mir emit`).

### Receipts

Two independent families:

- **Check receipts** (`buildc check --receipt`): what the type checker
  observed, declared effects, capability boundaries, propagated callers,
  SHA-256 digests of every source input. `buildc receipt verify` re-runs the
  check and fails on any drift. Policy profiles (`pure`, `console-only`,
  `offline`, `ci-review`, `strict-accountability`) turn the receipt into a CI
  gate.
- **Scientific-runtime receipts** (`buildc run --emit-receipt`): a numeric
  program's output series checked against a stated invariant (conservation,
  boundedness, monotone energy, and four more), re-verified by re-running the
  program. Each invariant ships a negative fixture that must fail, because a
  verifier that cannot fail proves nothing.

## Your first ten minutes

Install (Rust toolchain plus a C compiler required):

```bash
cargo install buildlang
buildc doctor
```

`doctor` reports compiler version, C-backend readiness, stdlib discovery, and
optional backend tools. Then write `hello.bld`:

```build
fn main() ~ Console {
    println!("Hello from BuildLang!");
}
```

Run it:

```bash
buildc run hello.bld
# Hello from BuildLang!
```

Delete the `~ Console` clause and run again: the checker rejects the program
and names the capability and the call that requires it. That error is the
language's core idea in one message.

Now check the program against a policy and print a receipt:

```bash
buildc check hello.bld --profile console-only --receipt -
```

You get the human summary, then JSON with `"status": "passed"` and
`"declared_effects": { "main": ["Console"] }`. Save it to a file and verify:

```bash
buildc check hello.bld --profile console-only --receipt r.json
buildc receipt verify r.json --expect-profile console-only
```

Edit the file, verify again, and watch it fail: the receipt is bound to the
exact source bytes. Finally, try a shader. The repository's
`examples/quickstart/vignette_shader.bld` defines a `#[fragment]` entry point:

```bash
buildc examples/quickstart/vignette_shader.bld --target hlsl -o vignette.hlsl
```

The generated HLSL is readable, commented, and ready for ReShade.

## Where to go next

- [USAGE.md](../USAGE.md): the full command reference with verified output
- [GETTING_STARTED.md](GETTING_STARTED.md): a longer tutorial, install to shaders
- [EFFECTS_GUIDE.md](EFFECTS_GUIDE.md): the capability-effect system in depth
- [SHADER_GUIDE.md](SHADER_GUIDE.md): HLSL and GLSL output reference
- [SCIENTIFIC-RECEIPT.md](SCIENTIFIC-RECEIPT.md): invariants, exit codes, failure classes
- [DESIGN.md](../DESIGN.md): pipeline architecture and rationale
- `examples/`: quickstart programs, FFI, linear types, GPU kernels, numeric
  invariant kernels, all runnable with the commands above
