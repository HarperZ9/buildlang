# bench5 results (M3 probe, 2026-11-08)

Graded on 2026-11-09 with `grade.py` against `buildlang` 1.11.0 installed from crates.io
(`cargo install --version 1.11.0 --locked`, `BUILDLANG_STDLIB` unset). Sealed before generation:
manifest sha256 `4c3762c4ae92d3864c0ffadf835f0fbdb62e16d4045680a55911bbebd4e06b64`, commitment
3bd69e9 on `probe/heldout-commitment-20261108`, on GitHub at 2026-11-09T00:23:40Z. Answer hashes
are in `gen-A.sha256` and `gen-B.sha256`.

| Sample | pass@1 | Wilson 95% | check_pass_but_fail | check_pass_not_pass | Bar (29/34, CPBF 0) |
|---|---|---|---|---|---|
| A | 34 of 34 | 0.90 to 1.00 | 0 | 0 | met |
| B | 34 of 34 | 0.90 to 1.00 | 0 | 0 | met |

On tasks 1 to 30 alone: 30 of 30 for each sample, against 25. The 8 game-shaped tasks: 8 of 8
for each sample.

**M3 passes on 1.11.0.**

## Predictions, scored

- pass@1 between 24 and 31 per sample: missed, both samples scored above the range.
- check_pass_but_fail 0 or 1: held (0).
- game-shaped tasks fail more often: missed, none failed.

## Checks on the check

- The grader controls ran first on 1.11.0 and gave all 6 predicted outcomes (pass, wrong,
  check_fail and crash among them), so the grader can fail an answer.
- No answer prints a hard-coded result: a search of both samples for distinctive expected values
  (201003, 2880067194370816120, 4610, 17710, 8640, the josephus order, the particle positions,
  the duel result) found none. `duel_trace` and `dice` compute their output; a spot run of
  sample B's `duel_trace` wrote the expected trace file.
- Both generators reported reading only the five allowed files and running nothing. That rests
  on their reports, as in bench4.

## Limits beside the result

- The two samples are two draws of one model (Claude Opus 5.5) with one prompt, not two
  independent models. That is the route bench4 used; a second model family would be a stronger
  test.
- The same agent wrote the tasks, the 1.11.0 compiler changes and this grading. The tasks were
  written before generation and sealed, but the author knows the language well, so the tasks may
  sit closer to what the docs teach than tasks from an outside writer would.
- Both generators listed points the docs leave open (writing `v[i] = x`, the index type, the
  signature of `exit`, the argument order of `write_file`, calls through `fn(T) -> U` values, how
  to write a literal brace in `format!`). Every guess they made happened to be right. These are
  documentation gaps for the next spec pass (LANGUAGE-COMPLETE row F1), not failures here.
