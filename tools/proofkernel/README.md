# proofkernel: mmk, a Metamath verifier written in BuildLang

`mmk` reads a Metamath database and checks every proof in it. All state sits in flat `i32` vectors indexed by token or statement id, proof stack entries are slices of one arena, and the language bounds-checks every index; the source holds no raw pointers.

```
compiler/target/release/buildc tools/proofkernel/mmk.bld -o mmk.c --target c
gcc -O1 -w -o mmk mmk.c
./mmk set.mm        # ERR lines for each fault, then one summary line; exit 0 means no fault
```

Build from inside `tools/proofkernel`, or copy its `.bld` files together, since `mmk.bld` names its modules by file.

| Module | Holds |
|---|---|
| `mmk.bld` | the entry point and the summary line |
| `state.bld` | the flat tables, growth helpers and fault reporting with line numbers |
| `tokens.bld` | the tokenizer and label lookup |
| `table.bld` | the arena, disjoint-variable pairs as a counted set, statements and frames |
| `decl.bld` | `$c`, `$v`, `$d`, `$f` and labelled statements, scopes, and the parse loop |
| `machine.bld` | the proof machine: substitution, the proof stack and disjoint-variable checks |
| `proof.bld` | normal and compressed proofs |

## What was checked

On 2026-10-31, built from BuildLang at 4ce772b, mmk accepted `set.mm`, `iset.mm`, `nf.mm`, `ql.mm` and `hol.mm` from the set.mm repository at 8f3fc60, with `set.mm` in about 2.1 s on the first run. A planted-fault run gave 0 false accepts and 0 false rejects. A mutant control of the kernel itself caught 6 of 9 mutants under its locked rule; a targeted check added afterwards catches the other 3. That study's records are private.

`tests/proofkernel/test_mmk.py` builds mmk and checks a small database it must accept and four planted faults it must refuse: a proof of the wrong statement, a missing hypothesis, an undeclared label and a swapped hypothesis order. CI runs it on Linux.

## Limits

Building mmk found four compiler gaps: a loop tail in a value function emits `return void`, an `as_bytes` slice type mismatches across functions, pointer arithmetic is missing, and `s.len` on `&str` fails. The kernel works around them. Under heavy memory pressure the Windows build crashed intermittently with an access violation, inferred to come from an unchecked `realloc` in the runtime; treat a crash as no verdict.
