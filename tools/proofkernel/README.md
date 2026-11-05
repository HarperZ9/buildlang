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

## A memory-model litmus test

`tests/proofkernel/ptx_mp.mm` encodes message passing under a simplified PTX memory model: thread 0 writes `x`, fences and writes `y`; thread 1 reads `y`, fences and reads `x`. Three axioms carry the model: a fence between two events in program order orders them, base causality runs from a release pattern through an observation to an acquire pattern, and the causality axiom makes a read see any write that precedes it in causality order (or a coherence-later one). The theorem `mp-fenced` proves that the read of `x` sees the write of `x`, which forbids the outcome where it returns the initial value.

The test holds mmk to five cases. It accepts the fenced proof. It refuses a proof of the unfenced program that passes program order where a fence is required (`hypothesis-mismatch`), a proof that cites an axiom the model does not have (`bad-proof-label`), the fenced proof once the causality axiom is removed, and the fenced proof once the reader's fence is gone.

The encoding follows the shape of the PTX causality axiom; it is not a full PTX model. It has one scope, treats every access as strong, collapses coherence order to "this write or a later one", and leaves out the other axioms. A refused unfenced proof shows that this one attempt fails, and the model offers no other route: `ax-fence` is the only axiom that concludes `fenced`, and it needs an `isfence` fact the unfenced program does not have. The test follows a lead from Koska, Koska and Koska, "Formalizing PTX in Lean to Prove GPU Kernel Correctness" (2026), which models the full PTX memory consistency model in Lean.

## Limits

Building mmk found four compiler gaps: a loop tail in a value function emits `return void`, an `as_bytes` slice type mismatches across functions, pointer arithmetic is missing, and `s.len` on `&str` fails. The kernel works around them. Under heavy memory pressure the Windows build crashed intermittently with an access violation, inferred to come from an unchecked `realloc` in the runtime; treat a crash as no verdict.
