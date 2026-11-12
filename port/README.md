# port: bring a program into BuildLang in one step

`port` reads a program in another language and writes the same program in BuildLang. You run one
command and get a file that builds. If the program uses something the tool cannot carry over, it
lists each line and writes nothing, so a port is whole or it is refused.

The tool is itself a BuildLang program: `main.bld` and seven modules beside it (`types`, `lex`,
`state`, `exprs`, `decls`, `stmts`, `file`). It is deterministic: the same input gives the
same output, with no model in the loop.

## Use it

Build the tool once:

```bash
buildc build --release port
```

Port a C file and run the result:

```bash
port/target/release/main c hello.c hello.bld
```

```bash
buildc run hello.bld
```

## What it reads today

Slice one is a C subset:

| Carried over | Refused, with the line number |
|---|---|
| functions over `int`, `long`, `unsigned`, `char`, `float`, `double`, `void` | pointers and `&` |
| locals, fixed arrays with initialisers, file-level `const` numbers | `struct`, `union`, `enum`, `typedef` |
| `if`, `while`, `do`, `for`, `break`, `continue`, `return`, `?:` | `goto` |
| `switch` whose cases each end in `break` or `return` | a case that falls through, `continue` inside a `switch` |
| casts, and every implicit conversion written out as a cast | array parameters |
| `#define NAME number`, as a constant | macros with parameters, a `#define` of anything but one number |
| literal suffixes `L`, `u`, `f`, hex, character constants | strings in variables |
| `sqrt sin cos tan exp log pow floor ceil fabs abs` | other library calls |
| `printf` with a literal format: `%d %i %u %ld %f %.Nf %c %s %%` | other conversions, `main` returning non-zero |

Functions that print, or call something that prints, get the `Console` effect in their signature.

## How we know nothing was lost

`port/check.sh` runs on every change:

1. gcc builds and runs each program in `corpus/c`. That output is the reference.
2. `port` writes BuildLang, and `buildc` builds and runs it.
3. The two outputs must be equal byte for byte.

Programs in `corpus/c/refused` must be refused at exactly the lines recorded beside them. A control
compares one program's output with another program's reference and must find a difference.

Current result: 4 of 4 corpus programs identical to gcc, 2 of 2 refusals as recorded. The corpus is
small. It covers integer and floating arithmetic with mixed types, loops with `continue`,
recursion, arrays, `switch`, defined constants and formatted output.

Two differences are kept on purpose, both where C leaves the result undefined: signed overflow
stops the program in BuildLang, and so does a function with a value that runs off its end.

## Where this goes

The plan and the order of languages are in `docs/design/LANGUAGE-MIGRATION.md`. Next for C:
structs, pointers used as references or arrays, and strings. Then Python, JavaScript and TypeScript, Rust, Go, Java and Ruby, each with its own
corpus and the same check against the source language's own toolchain.

Every construct the tool has to refuse because BuildLang cannot express it yet is recorded as a
compiler gap in `tests/features`.
