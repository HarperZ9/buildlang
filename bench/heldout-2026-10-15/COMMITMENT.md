# Held-out probe commitment, 2026-10-15

This commit fixes a held-out agent-writability probe before any agent sees
its tasks. `MANIFEST.sha256` lists the SHA-256 of every file in the probe:
27 tasks, the Python reference that defines expected outputs, the grader,
the analysis script, the generator prompt, the preregistration (pass bar,
predictions, analysis), the grader controls, and the frozen v1.6.0 docs the
agents receive. The manifest itself hashes to

    6f9de2cdc162c60d2f86e029c969fbd1070c2f5189ec13cbb58bca15fc322f14

The files are published after the run, next to this commitment, so anyone can
recompute the hashes and confirm the tasks and pass bar were not changed
after the results were seen.
