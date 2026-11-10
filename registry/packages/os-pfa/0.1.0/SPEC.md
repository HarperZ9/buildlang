# os-pfa 0.1.0: page-frame allocator core

A page frame is one fixed-size block of physical memory. An operating-system
kernel hands frames to the code that needs memory and takes them back later.
This package is the bookkeeping for that job: a bitmap with one bit per frame.

The core owns no memory and reads no hardware. The caller passes the bitmap
in as a slice of 64-bit words, so the same source runs inside a kernel and
inside an ordinary program on a development machine. Nothing here has run
inside a kernel yet.

Exactness class: `exact-int`. Every result is an integer, and two correct
implementations must agree on every return code and every bit of state.

## State

- `bits`: caller storage. Bit `f` is 1 when frame `f` is used or does not exist.
- `Pfa.nframes`: number of frames managed, 1 to 2^40.
- `Pfa.nfree`: number of free frames.
- `Pfa.hint`: a word index. Every bitmap word below it is full.

Invariants, true after every operation that returns:

| Code | Invariant |
|---|---|
| 11 | `nfree` equals the number of clear bits below `nframes` |
| 12 | every bit at or past `nframes` in the last word is set |
| 13 | every word below `hint` is full |

## Operations

Every function is total: it returns a code for every input and never traps.
Codes: `PFA_OK` 0, `PFA_NONE` -1, `PFA_RANGE` -2, `PFA_STATE` -3, `PFA_SMALL` -4.

| Function | Result |
|---|---|
| `pfa_init(p, bits, nframes)` | `PFA_RANGE` when `nframes` is 0 or above 2^40. `PFA_SMALL` when `bits` has fewer than `ceil(nframes / 64)` words. Otherwise every frame is free and the result is `PFA_OK`. |
| `pfa_alloc(p, bits)` | The lowest-numbered free frame, now used. `PFA_NONE` when no frame is free. |
| `pfa_free(p, bits, f)` | `PFA_RANGE` when `f >= nframes`. `PFA_STATE` when `f` is already free. Otherwise `f` is free and the result is `PFA_OK`. |
| `pfa_alloc_run(p, bits, n)` | `PFA_RANGE` when `n` is 0. Otherwise the lowest start of `n` free frames in a row (first fit), all now used, or `PFA_NONE` when no such run exists. |
| `pfa_reserve(p, bits, start, n)` | `PFA_RANGE` when `n` is 0 or the range leaves the map. `PFA_STATE` when any frame in the range is used. Otherwise the whole range is used and the result is `PFA_OK`. |
| `pfa_free_run(p, bits, start, n)` | `PFA_RANGE` as for reserve. `PFA_STATE` when any frame in the range is free. Otherwise the whole range is free and the result is `PFA_OK`. |

A call that returns `PFA_RANGE`, `PFA_STATE` or `PFA_SMALL` changes no bit and
no count. `pfa_alloc` may raise `hint` when it returns `PFA_NONE`; that is the
only state a refused call may change.

Cost: `pfa_alloc` reads at most `ceil(nframes / 64)` words. `pfa_free` reads
one word. The run operations visit each frame in the range once.

## How the contract is checked

`src/model.bld` is the reference model: one byte per frame in a growable
array, with no bitmap, no hint and no word arithmetic. It is slow and simple
on purpose.

| Check | Command | What it does |
|---|---|---|
| Golden cases | `os-pfa cases` | A fixed script whose results were worked out by hand. |
| Trace | `os-pfa trace N SEED STEPS`, `trace-ref` | Generated operations through the core and through the model. Both must print the same bytes. |
| Differential fuzz | `os-pfa fuzz SEED STEPS` | The same seeded operations go to the core and the model at six map sizes (1, 63, 64, 65, 200, 512 frames). Return codes, full state and invariants are compared after every step. |
| Properties | `os-pfa props SEED STEPS` | Properties 21 to 25 of `tests/props.txt`, judged without the model. |
| Controls | `os-pfa controls` | Four copies of core functions, each with one planted defect. Both checks must refuse every one. |

The generator is splitmix64 in wrapping 64-bit arithmetic, so one seed gives
one operation sequence on every host.

`tests/controls.txt` lists the planted defects and the code that refuses each.
A check that accepts a planted defect is a broken check, and `os-pfa controls`
exits with status 1 in that case.

## Limits

- The checks run the core as an ordinary hosted program. No kernel calls it yet.
- The fuzz sizes stop at 512 frames. Larger maps use the same code paths; they
  have not been run.
- One processor. Nothing here is safe to call from two processors at once
  without a lock held by the caller.
- No memory zones, no per-processor caches, no buddy merging. A run search is
  linear in the number of frames.
