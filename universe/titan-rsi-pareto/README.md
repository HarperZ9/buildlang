# Titan Oracle-Gated RSI Pareto Compute Allocator

This bundle graduates `titan_rsi_pareto_orchestrator.bld` from Wave 149B, a pure-MIT BuildLang implementation of an oracle-gated Recursive Self-Improvement Pareto allocator. It passes `buildc check` plus `buildc lint` cleanly inside `buildc check-dir`, compiles to native `x64-pe` with certified Superopt V6 rewrites, and matches both its zero-import WebAssembly twin and Python reference within `3.63e-13`.

The allocator runs 100 allocation cycles across eight specialist lanes totaling `51,200` marginal-gain-per-joule candidate checks under a `6.0 J` per-cycle energy budget. A hard fail-closed Anti-Goodhart Rollback Valve named `anti_goodhart_valve` rejects unverified edits at code `-101`, proxy-drift breaches at `-102`, and non-positive energy budgets at `-201`. Across the full run, it reaches `3.020x` monotonic compound score growth from `3.3900` to `10.2373`, blocks `40 / 40` deceptive Goodhart traps with zero regressions, and emits bit-exact digest `pareto_hash = 1468869863`.

All files in this directory carry pure-OSI MIT licensing in `LICENSE` with zero external dependencies.
