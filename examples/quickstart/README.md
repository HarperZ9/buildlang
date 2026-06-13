# QuantaLang Quickstart Examples

These examples are the adoption smoke path for `quantac`. They stay wired into
the compiler CLI tests so the first programs a new user sees do not drift out
of sync with the compiler.

Run the local readiness check first:

```bash
quantac doctor
```

Then run the CPU examples:

```bash
quantac run examples/quickstart/hello.quanta
quantac run examples/quickstart/ledger.quanta
quantac run examples/quickstart/effects_greeting.quanta
```

Compile the shader example to HLSL:

```bash
quantac examples/quickstart/vignette_shader.quanta --target hlsl -o vignette_shader.hlsl
```

The examples cover the practical baseline:

- `hello.quanta`: binary setup and formatted output.
- `ledger.quanta`: functions, mutable locals, loops, and formatted output.
- `effects_greeting.quanta`: algebraic effect declaration, perform, handle, and resume-free dispatch.
- `vignette_shader.quanta`: shader-oriented math and a fragment entry point for HLSL output.
