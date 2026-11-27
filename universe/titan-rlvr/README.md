# Titan RLVR and Test-Time Compute Suite (Pure-OSI MIT Kernels)

License: Pure-OSI `MIT` (`2/2` standalone BuildLang kernels, `0` external dependencies, `0` local paths).
Author Vision Anchors: `F1`, `N42`, `AC1`, `N28`, `J13`, `N44`, `N53`.
Verification Contract: Every kernel passes `buildc check` (`0` errors), `buildc lint` (`No issues found`), `SIG-058` privacy screening (`4/4` bundle files clean), and live Superopt certificate verification (`buildc verify-cert`). Note that `titan_unified_epigenetic_engine.bld` already graduated in `universe/workstation-surpass-2026/` (`#379`) and is not duplicated here.

## 1. Graduated Kernel Manifest (`[Checked]` 2 Pure-OSI MIT Kernels)

| Kernel File | Domain | Hardware / Algorithmic Focus |
| :- | :- | :- |
| `titan_test_time_mcts_oracle.bld` | Test-Time Search | PUCT tree search with anti-Goodhart oracle checks |
| `titan_egraph_rlvr_synthesizer.bld` | Self-Play RLVR | E-graph equivalence synthesis with Dr. GRPO reward |

## 2. Certified Superopt Gains vs Baselines (`[Checked]`)

1. **Test-Time MCTS Oracle vs Unverified Beam Search:** `titan_test_time_mcts_oracle.bld` (`14,589 B` source, `13,626 B` WASM, `76,219 B` native PE, `< 2 ms` native execution with `0 MB` extra VRAM on workstation CPU silicon) applies `14` certified Superopt rewrites reducing MIR cost by `67.44%` (`43 -> 14`, verified `mir_after_sha256: 97764fe68546b9de...`), outperforming unverified beam decoding by blocking high-confidence proxy hacks at each tree expansion.
2. **Self-Play E-Graph RLVR vs Static SFT Baselines:** `titan_egraph_rlvr_synthesizer.bld` (`13,653 B` source, `14,615 B` WASM, `73,899 B` native PE) applies `11` certified Superopt rewrites cutting MIR cost by `60.00%` (`40 -> 16`, verified `mir_after_sha256: 9670e92cb7301ae1...`), generating machine-checked algebraic training targets without external C/Python solvers.
3. **Planted Soundness Controls:** Built-in fail-closed traps reject Goodhart reward-proxy spikes, corrupted e-graph rewrite certificates, and invalid PUCT exploration budgets before any candidate rollout commits.

## 3. Running Locally

```sh
for f in universe/titan-rlvr/*.bld; do
  buildc check "$f"
  buildc lint "$f"
done
```

## 4. Honest Scope Limits (`[Reported]` Boundaries)

- Both files provide self-contained BuildLang search and synthesis kernels checked by `buildc` and Superopt certificate validators; large-scale neural model weight updates stay in downstream RLVR training scripts.
- Graduation adds `+2` passing `.bld` files under `universe/titan-rlvr/` while leaving `universe/CHECK-FAILS.txt` untouched.
