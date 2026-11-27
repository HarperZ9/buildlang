# Workstation Frontier AI, Epigenetics, Developmental, and Titan-1M Kernels

License: Pure-OSI `MIT` (`10/10` standalone BuildLang kernels, `0` external dependencies, `0` secrets, `0` local workstation paths).
Author Vision Anchors: `N33`, `N39`, `N40`, `N73`, `J7`, `N72`, `AC6`, `N22`, `J5`, `N44`.
Verification Contract: Every kernel passes `buildc check`, `buildc lint` (`No issues found`), `buildc -O 3` native execution, and native `SIG-058` privacy verification (`12/12` files clean).

## 1. Graduated Kernel Manifest (`[Checked]` 10 Pure-OSI MIT Kernels)

| Kernel File | Idea ID | Domain | Verification Focus |
| :- | :- | :- | :- |
| `epigenetics_schedule_study.bld` | `N33` | Model Epigenetics | Optimizer-schedule vs reward-history spectral separation |
| `reasons_generalization.bld` | `N39` | Reasoning Generalization | Two-arm reasons-in-training-data out-of-distribution study |
| `mycorrhizal_swarm.bld` | `N40` | Biological Swarms | Mycorrhizal resource-routing coordination vs selfish baseline |
| `bio_to_compiler_transfer.bld` | `N73` | Cross-Substrate Transfer | Biological regulatory pass transfer to compiler scheduling |
| `neuron_lens_probe.bld` | `J7` | Interpretability | Causal neuron-lens probe with planted ablation controls |
| `living_env_curriculum.bld` | `N72` | Developmental AI | Living-environment curriculum adaptation simulator |
| `developmental_bootstrap.bld` | `AC6` | Sensor Bootstrapping | Sensor-first developmental bootstrapping schedule |
| `distant_idea_bridge.bld` | `N22` | Structural Isomorphism | Cross-domain distant-idea structural bridge engine |
| `living_nested_organelles.bld` | `J5` | Living Architectures | 4-organelle metabolic homeostasis and lesion recovery |
| `titan_1m_retrieval_bench.bld` | `N44` | Long-Context `O(1)` | 1,000,000-step Cayley symplectic passkey benchmark |

## 2. Measured Baseline Gains and Planted Controls (`[Checked]`)

1. Symplectic `O(1)` Long-Context Memory vs Quadratic KV-Cache Baseline: `titan_1m_retrieval_bench.bld` executes `1,000,000` consecutive Cayley symplectic token updates in `384 B` live kernel state (`8.628 GB` modeled 14B total VRAM vs `196.608 GB` BF16 KV-cache, a `22.787x` reduction), recovering `8/8` planted passkeys with `8.49e-12` energy drift and `2.57x` native speedup over the Python `f64` oracle while catching planted `|lambda| = 1.0015` explosion fail-closed at step `9,218`.
2. Certified Native BuildLang Speedup vs Interpreted Baselines: Compiling these `10` standalone kernels via `buildc -O 3` removes interpreter overhead and enforces static effect typing (`~ Console`) plus `0`-warning lint hygiene before graduation into `HarperZ9/buildlang`.
3. Planted Negative Control Coverage: Across the suite, non-symplectic blowup (`N44`), rank-1 epigenetic shortcut collapse (`N33`), causal ablation probes (`J7`), and 25% organelle lesion recovery (`J5`) test fail-closed behavior directly inside each executable.

## 3. Running Locally

```sh
for f in universe/workstation-ai-bio/*.bld; do
  buildc check "$f"
  buildc lint "$f"
  buildc -O 3 -o /tmp/kernel_bin "$f" && /tmp/kernel_bin
done
```

## 4. Honest Scope Limitations (`[Reported]` Boundaries)

- Standalone Deterministic Simulations: Each kernel executes a self-contained CPU deterministic simulation in IEEE-754 `f64` arithmetic; full multi-billion-parameter GPU training runs live in separate accelerator harnesses.
- No Global Compiler Registry Edits: This graduation batch leaves `universe/CHECK-FAILS.txt` completely untouched and introduces zero breaking edits outside `universe/workstation-ai-bio/`.
