# Held-out probe commitment, 2026-11-08 (bench5, M3)

This commit fixes a held-out agent-writability probe before any agent sees its tasks.
`MANIFEST.sha256` lists the SHA-256 of every file in the probe: 34 tasks (8 game-shaped), the
Python reference that defines expected outputs, the bench4 grader and analysis with the recorded
harness diff, the generator prompt, the preregistration (bar 29 of 34 per sample and zero
check-pass-but-fail, predictions, analysis), the grader controls with their 1.11.0 results, and
the frozen v1.11.0 docs the agents receive. The manifest itself hashes to

    4c3762c4ae92d3864c0ffadf835f0fbdb62e16d4045680a55911bbebd4e06b64

The files are published after the run, next to this commitment, so anyone can recompute the
hashes and confirm the tasks and the bar were not changed after the results were seen.
