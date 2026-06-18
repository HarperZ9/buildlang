# QuantaLang Demo

> Best-effort demo - not runtime-verified by author.

A small, self-contained QuantaLang program (`temperature.quanta`) that
exercises the practical CPU surface of `quantac`:

- a pure helper function (`to_fahrenheit`),
- mutable locals and a `while` loop,
- the `Console` capability effect carried by `println!`.

The output shown below was captured from a local debug build of `quantac`
1.0.0; treat the commands and output as best-effort and re-run them against
your own build to confirm.

## Run it

From the repository root, after building `quantac` (see [USAGE.md](../../USAGE.md)):

```bash
quantac run examples/demo/temperature.quanta
```

Output:

```
0C = 32F
10C = 50F
20C = 68F
30C = 86F
```

## Type-check it under a capability policy

`println!` makes `main` carry the `Console` effect. The `console-only`
built-in policy permits exactly that capability and denies the rest:

```bash
quantac check examples/demo/temperature.quanta --profile console-only
```

Output:

```
Lexing... OK (65 tokens)
Parsing... OK (2 items)
Type checking... OK

No errors found in 'examples/demo/temperature.quanta'
```

Add `--receipt -` to print a machine-readable accountability receipt, or
`--receipt receipt.json` to save one for `quantac receipt verify`.

## Compile to C

To emit C and build it with your system C compiler instead of `quantac run`:

```bash
quantac examples/demo/temperature.quanta -o temperature.c
cc temperature.c -o temperature
./temperature
```

See [USAGE.md](../../USAGE.md) for the full command and backend reference.
