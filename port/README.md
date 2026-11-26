# port: bring a program into BuildLang in one step

`port` reads a program in another language (or Controlled Plain-English) and writes the same program in BuildLang. You run one command and get a file that builds. If the program uses something the tool cannot carry over, it lists each line and writes nothing, so a port is whole or it is refused.

The tool is itself a BuildLang program: `main.bld` plus modules (`types`, `lex`, `state`, `exprs`, `decls`, `stmts`, `file`, `py_expr`, `py_front`, `plain_front`). It is deterministic: the same input gives the same output, with zero LLM in the loop.

## Use it

Build the tool once:

```bash
buildc build --release port
```

Port a C, Python, or Controlled Plain-English file and run the result:

```bash
port/target/release/main c hello.c hello.bld
port/target/release/main py script.py script.bld
port/target/release/main plain spec.plain spec.bld
buildc run hello.bld
```

## What it reads today

### Frontend 1: C subset (`port c`)

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

### Frontend 2: Python subset (`port py`)

| Carried over | Refused, with the line number |
|---|---|
| typed/inferred functions over `int` (`i64`), `float` (`f64`), `bool`, `str` (`String`) | `class`, `lambda`, decorators `@`, `yield`, `async`/`await` |
| locals, compound assignments (`+= -= *= /=`), top-level constants | `try`/`except`/`finally`, `with`, `raise`, `global`/`nonlocal`, `del` |
| `if`/`elif`/`else`, `while`, `for i in range(n / a,b / a,b,step)`, `break`, `continue`, `return`, `pass` | list/dict comprehensions, arbitrary iterables |
| `and`/`or`/`not`, `//`, `/`, `**`, `import math` (`sqrt`, `sin`, `cos`, `tan`, `exp`, `log`, `floor`, `ceil`, `fabs`, `pow`, `abs`, `min`, `max`, `int`, `float`, `len`) | non-`math` imports (`import json`, etc.), unknown calls |
| `print(...)` and `f"...{expr:.Nf}..."` (auto-inferred `~ Console` transitively) | arbitrary dynamic formatting or unknown side effects |

### Frontend 3: Controlled Plain-English (`port plain`, Aurora-inspired)

Zero-LLM deterministic entry grammar inspired by the Aurora vision to open programming to newcomers, non-programmers, and analogical thinkers without syntax walls or silent bugs:
- `note:` comments, `constant NAME is <expr>`
- `define add(a: int, b: int) returning int:` / `define main:`
- `set x to a plus b`, `add <expr> to x`, `subtract <expr> from x`, `multiply x by <expr>`
- `if <cond>:`, `otherwise if <cond>:`, `otherwise:`, `while <cond>:`, `repeat i from 0 to 5:` (`0..5`), `repeat i from 1 through 5:` (`1..=5`), `stop`, `skip`, `return <expr>`, `print <expr>` / `say <expr>`, `call <fn>(...)`
- Natural operators: `plus`, `minus`, `times`, `divided by`, `modulo`, `square root of`, `equals`, `is not`, `is greater than`, `is at least`, `is less than`, `is at most`, `and`, `or`, `not`
- **Plain-English Mirror (`// MIRROR:`):** Every generated `.bld` file opens with a deterministic plain-language header naming every defined function, every capability/effect requested (for example `Console` only, zero `FileSystem`/`Network`/`Process`), and every active safety trap (checked overflow, bounds checks) so intent can be verified before running (the seed of `buildc spec explain` / `S2`).
- Unrecognized imperative verbs (such as `fetch ... from network` or `delete ...`) are refused with exact line numbers and exit code 2.

Functions that print, or call something that prints, automatically get the `Console` effect in their signature.

## How we know nothing was lost

`port/check.sh` runs on every change:

1. **C subset (`corpus/c`):** `gcc -O1 -lm` builds and runs each program; `port c` writes BuildLang and `buildc run` executes it; outputs must match byte for byte.
2. **Python subset (`corpus/py`):** `python3` runs each program; `port py` writes BuildLang and `buildc run` executes it; outputs must match byte for byte.
3. **Controlled Plain-English (`corpus/plain`):** `port plain` writes BuildLang and `buildc run` executes it; outputs must match each `.want` reference byte for byte.
4. **Refusals (`corpus/*/refused`):** Every refused fixture must exit `2`, write no `.bld` file, and report the exact line numbers in its `.lines` file.
5. **Planted controls (3 of 3):** One control per frontend compares two different programs' outputs and must detect a difference.

Current result:
- C: 5 of 5 corpus programs identical to `gcc`, 2 of 2 refusals as recorded.
- Python: 5 of 5 corpus programs identical to `python3`, 2 of 2 refusals as recorded.
- Controlled Plain-English: 3 of 3 corpus programs identical to golden `.want`, 1 of 1 refusal as recorded.

## Equivalence boundary and surpass roadmap

`port` verifies **byte-for-byte observable equivalence** (stdout, exit status, IEEE-754 floating-point values, and deterministic state hashes) against the source toolchain. It intentionally does **not** chase raw ELF/PE executable byte identity with `gcc` or CPython: compiled BuildLang binaries enforce explicit effect rows, checked signed overflow traps, and deterministic math lowering.

To surpass `-O2` C and hand-written assembly on verified hot loops while preserving proven equivalence, `port` feeds directly into BuildLang's verification and optimization roadmap in `docs/STRATEGY-2026-10-SURPASS.md`:
- **V4 (Translation validation):** Per-function equivalence certificates checking lowered C/native code against MIR semantics.
- **V5 (E-graph superoptimization):** Equality saturation with equivalence certificates to rewrite hot numerical kernels beyond standard peephole passes.
- **Next frontends (`docs/design/LANGUAGE-MIGRATION.md`):** JavaScript/TypeScript, Rust, and universal IR lifters (WASM `.wat`, RISC-V/assembly, SPIR-V) feeding the same differential check harness.
