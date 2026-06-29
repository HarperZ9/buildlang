# BuildLang Quickstart Examples

These examples are the adoption smoke path for `buildc`. They stay wired into
the compiler CLI tests so the first programs a new user sees do not drift out
of sync with the compiler.

Run the local readiness check first:

```bash
buildc doctor
```

Then run the CPU examples:

```bash
buildc run examples/quickstart/hello.bld
buildc run examples/quickstart/ledger.bld
buildc run examples/quickstart/effects_greeting.bld
```

Compile the shader example to HLSL:

```bash
buildc examples/quickstart/vignette_shader.bld --target hlsl -o vignette_shader.hlsl
```

The examples cover the practical baseline:

- `hello.bld`: binary setup and formatted output.
- `ledger.bld`: functions, mutable locals, loops, and formatted output.
- `effects_greeting.bld`: algebraic effect declaration, perform, handle, and resume-free dispatch.
- `vignette_shader.bld`: shader-oriented math and a fragment entry point for HLSL output.
