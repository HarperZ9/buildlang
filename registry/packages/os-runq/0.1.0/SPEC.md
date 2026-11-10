# os-runq 0.1.0: round-robin run queue core

A run queue is the list of tasks that are ready to use the processor. Round
robin is the scheduling rule where each ready task gets the processor in turn
for a fixed number of timer ticks, called the quantum, and then goes to the
back of the queue.

The core owns no memory and reads no hardware. The caller passes two pieces of
storage in: `ring`, a circular buffer of task numbers, and `state`, one byte
per task. The same source runs inside a kernel and inside an ordinary program
on a development machine. Nothing here has run inside a kernel yet.

Exactness class: `exact-int`. Every result is an integer, and two correct
implementations must agree on every return code, every task state and the
order of the queue.

## State

- `state[t]`: `RQ_ABSENT` 0, `RQ_READY` 1, `RQ_RUNNING` 2 or `RQ_BLOCKED` 3.
- `ring`, `Runq.head`, `Runq.len`: the ready tasks, front first.
- `Runq.current`: the running task, or -1 when the processor is idle.
- `Runq.quantum`: ticks per turn. `Runq.left`: ticks the running task still has.

Invariants, true after every operation that returns:

| Code | Invariant |
|---|---|
| 31 | the ring holds exactly the ready tasks, each one once |
| 32 | at most one task runs; it is `current` and holds 1 to `quantum` ticks; an idle processor holds none |

## Operations

Every function is total: it returns a code for every input and never traps.
Codes: `RQ_OK` 0, `RQ_IDLE` -1, `RQ_RANGE` -2, `RQ_STATE` -3, `RQ_SMALL` -4.
"Dispatch" below means: the task at the front of the queue becomes the running
task with a full quantum and its number is the result; with an empty queue the
processor is idle and the result is `RQ_IDLE`.

| Function | Result |
|---|---|
| `rq_init(q, ring, state, cap, quantum)` | `RQ_RANGE` when `cap` is 0 or above 65536, or `quantum` is 0. `RQ_SMALL` when either slice has fewer than `cap` entries. Otherwise every task is absent, the processor is idle and the result is `RQ_OK`. |
| `rq_add(q, ring, state, t)` | `RQ_RANGE` when `t >= cap`. `RQ_STATE` unless `t` is absent. Otherwise `t` is ready at the back and the result is `RQ_OK`. |
| `rq_next(q, ring, state)` | The running task, if any, goes to the back. Then dispatch. |
| `rq_tick(q, ring, state)` | Idle processor: dispatch. Running task with more than one tick left: it loses one tick and stays; the result is its number. Running task on its last tick: as `rq_next`. |
| `rq_block(q, ring, state)` | `RQ_STATE` when no task runs. Otherwise the running task becomes blocked. Then dispatch. |
| `rq_wake(q, ring, state, t)` | `RQ_RANGE` when `t >= cap`. `RQ_STATE` unless `t` is blocked. Otherwise `t` is ready at the back and the result is `RQ_OK`. |
| `rq_exit(q, ring, state)` | `RQ_STATE` when no task runs. Otherwise the running task becomes absent. Then dispatch. |
| `rq_remove(q, ring, state, t)` | `RQ_RANGE` when `t >= cap`. `RQ_STATE` when `t` is absent. Otherwise `t` becomes absent and the result is `RQ_OK`. The order of the other ready tasks does not change. A removed running task leaves the processor idle until the next `rq_next` or `rq_tick`. |

A call that returns `RQ_RANGE`, `RQ_STATE` or `RQ_SMALL` changes nothing.

Two consequences follow from the table and are checked as properties. Every
dispatch picks the task that has been ready the longest (property 34). A task
that is not blocked, removed or ended keeps the processor for exactly
`quantum` ticks per turn (property 35).

Cost: every operation is constant time except `rq_remove` of a ready task,
which moves at most `len` ring entries.

## How the contract is checked

`src/model.bld` is the reference model. It has no queue at all: it stamps each
task with a counter when the task becomes ready, and a dispatch picks the
ready task with the lowest stamp. The core and the model share no data
structure, so a defect in the ring arithmetic cannot hide in both.

| Check | Command | What it does |
|---|---|---|
| Golden cases | `os-runq cases` | A fixed script whose results were worked out by hand. |
| Trace | `os-runq trace CAP SEED STEPS`, `trace-ref` | Generated operations through the core and through the model. Both must print the same bytes, including a digest of the final state. |
| Differential fuzz | `os-runq fuzz SEED STEPS` | The same seeded operations go to the core and the model at five queue sizes (1, 2, 3, 8, 16 tasks) with quantum 1 to 4. Return codes, full state, queue order and invariants are compared after every step. |
| Properties | `os-runq props SEED STEPS` | Properties 33 to 35 of `tests/props.txt`, judged without the model. |
| Controls | `os-runq controls` | Four copies of core functions, each with one planted defect. Both checks must refuse every one. |

`tests/controls.txt` lists the planted defects and the code that refuses each.
A check that accepts a planted defect is a broken check, and `os-runq
controls` exits with status 1 in that case.

One refusal has a thin margin. The property check first refuses control 3
(remove fills the gap with the last entry) at step 2901 of a 3000-step run,
because the defect only shows when a later dispatch reaches the misplaced
task. The differential check compares queue order directly and refuses it at
step 1339. A stronger order property is future work.

## Limits

- The checks run the core as an ordinary hosted program. No kernel calls it yet.
- The fuzz sizes stop at 16 tasks.
- One processor and one priority level. No sleep queues, no priority
  inheritance, no load balancing between processors.
- The caller must hold interrupts off, or a lock, around every call.
