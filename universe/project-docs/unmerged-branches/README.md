# Work from build-universe branches that never reached its main

Checked on 2026-11-15, before the `HarperZ9/build-universe` repository was archived. Every
remote branch was compared with `main` (commit `1633fc3`). Forty-five branches had nothing
`main` lacked. Nine had commits ahead of it; the table says what happened to each.

Each `.patch` file is the diff from the branch's merge base to its tip, written with
`git diff`. The patches are records. They were made against the old layout, and the three from
June name `.quanta` files, which are the `.bld` files of today.

| Branch | Tip | Commits | What it holds | State here |
|---|---|---|---|---|
| `recon/corpus-effects-2026-09-05` | `96aa3d3` | 11 | Parser and effect-row repairs across 98 `.bld` files | Applied to this tree. `buildc check` passes went from 62 to 69 of 267 with buildc 1.14.0 |
| `docs/live-pointers-20261114` | `8d6f354` | 1 | A README note saying where the compiler lives (build-universe#38) | Carried in `universe/README.md`, reworded for the new home |
| `art/brand-refresh-20261004` | `300f1a1` | 1 | README art | Already on `main` through build-universe#31 (squash merge) |
| `design/canon-docs-page` | `86f9d50` | 1 | The Pages site on the design canon | Already on `main` through build-universe#34 (squash merge) |
| `feat/builddb-completion` | `c4aaaac` | 1 | `programs/BUILDDB-COMPLETE.md` | Lives on as `bdb/COMPLETE.md` in this repository. Patch kept: `feat-builddb-completion.patch` |
| `bdb/buildc-1.4.0-verified` | `af1b647` | 1 | A STATUS note and a script that reproduces a qdb REPL crash under bulk input | Not applied. qdb moved to `bdb/`, which has its own REPL tests. Patch kept: `bdb-buildc-1.4.0-verified.patch` |
| `chore/repackage-release-candidates` | `3064275` | 19 | STATUS notes on codegen passes, a de-duplication inventory, two renames that remove duplicate definitions in `spectrum` and `oracle`, `tools/coherence/compiler_oracle.py`, `.gitignore` rules | Not applied. Patch kept: `chore-repackage-release-candidates.patch` |
| `fix/codegen-prefixing` | `505b61e` | 15 | The first 14 commits of the branch above, plus a five-line Codex config | Not applied. Patch kept: `fix-codegen-prefixing.patch` |
| `feat/axiom-poc-operators` | `e875096` | 11 | Axiom subtree mutation operators (432 lines in `axiom/lib.quanta`), a release planning helper, a usage guide, package metadata | Not applied. Patch kept: `feat-axiom-poc-operators.patch` |

The two duplicate definitions named in `chore/repackage-release-candidates` are still in this
tree: `pub mod harmony` appears twice in `spectrum/lib.bld`, and `ConformalPredictor` is
defined twice in `oracle/lib.bld`.
