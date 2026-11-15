# Build Family — Lineage and Interlacing (Canonical)

Last verified: 2026-06-05. Maps the related workspaces, how the mixed languages
hand off to each other, and the foundational-to-derivative direction. Single
source of truth for the family tree.

## Successor to quanta-universe

This repo, `build-universe`, is the canonical successor to the earlier
`quanta-universe` repo, renamed in June 2026 as part of the ecosystem-wide
`quanta` -> `build` rename. `quanta-universe` retains earlier commit history
only; it is deprecated for new work, and all development continues here. Do not
open changes against `quanta-universe`; treat it as a read-only history record.

## Roster

| Workspace | Path | Language(s) | Remote | Role |
|---|---|---|---|---|
| BUILD-UNIVERSE | local checkout | BuildLang (.bld), Rust | HarperZ9/build-universe | Foundational ecosystem + spec |
| buildlang | nested, git-ignored in UNIVERSE | Rust | HarperZ9/buildlang | The compiler (separate repo) |
| APPS | local checkout | Python | HarperZ9/APPS (+7 submodules) | Derivative product layer |
| build-color | APPS submodule | Python | HarperZ9/build-color | Color product (published) |
| calibrate-pro | APPS submodule | Python + vendored C/C++ | HarperZ9/calibrate-pro | Display-calibration product (shipped exe) |
| build-finance / -oracle / -engine / -ui / -ecosystem | APPS submodules | Python | HarperZ9 | Finance / forecast / engine / UI / meta |
| aurora | referenced only | Lua (intended) | — | No source present locally |

## Lineage direction (evidence-backed)

BUILD-UNIVERSE (foundational) -> APPS (derivative productization).

- UNIVERSE commit history predates the APPS initial commit.
- The dwm_lut native wrapper exists in UNIVERSE first; APPS carries a
  near-identical derived copy (same algorithms, ~1,190 vs ~1,185 LOC).
- Every APPS subproject re-implements concepts expressed first in a UNIVERSE
  .bld module. No UNIVERSE module depends on APPS.
- All .bld modules depend on the buildlang compiler; APPS has zero .bld
  dependency (hand-written Python, not generated from .bld).

Correction to a common misstatement: APPS does NOT import the .bld modules
(Python cannot). APPS is a parallel, hand-written re-implementation in Python of
the same algorithms — a productization, not a binding.

## How the languages interlace (handoff chain)

    Rust (buildlang compiler)
       |  compiles
       v
    BuildLang (.bld + programs/) --transpile--> C --> MSVC --> native .exe
       |                                                  (56 verified programs)
       |  same algorithms re-expressed by hand
       v
    Python (APPS products) --ctypes--> vendored C/C++ DLLs
       |                               (dwm_lut: MinHook + HDE) --> Windows DWM
       v
    Lua (aurora): intended scripting layer, not yet present

Module correspondence (UNIVERSE .bld  to  APPS Python):

| UNIVERSE module | APPS product |
|---|---|
| calibrate (+ vendored dwm_lut) | calibrate-pro |
| spectrum, chromatic | build-color |
| delta, entropy, field-tensor | build-finance |
| oracle | build-oracle |
| neutrino | build-engine |
| nova, lumina | build-ui |

## Verified APPS test counts (vs prior ENGINEERING.md claims)

| Product | Claimed | Measured | Note |
|---|---|---|---|
| calibrate-pro | 297 | 228 | over-claimed |
| build-color | 457 | 281 | over-claimed |
| build-finance | 142 | 142 | match |
| build-oracle | 187 | 187 | match |
| build-engine | 96 | 173 | under-claimed |
| build-ui | 17 | 17 | match |
| aurora | Lua | 0 | no source present |

## Combining the family — recommendation

Do NOT physically merge git histories or collapse the repos yet. The pieces are
intentionally separated by language and release cadence (UNIVERSE = spec,
buildlang = compiler, APPS = shippable Python with its own submodules). A blind
merge would shred the submodule structure and the .bld/Python boundary.

Recommended combination mechanism, safest first:

1. Lineage manifest (this file) — one authoritative map. Done.
2. Shared golden-vector tests — extract the verifiable kernels (color, options,
   SARIMA, SHA-256) into language-neutral test vectors that BOTH the .bld and
   the Python implementations must satisfy. This is the real combine: it binds
   the two language families by behavior, not by moving files.
3. Optional umbrella repo — add UNIVERSE and APPS as submodules of a thin parent
   (or a git subtree) only if a single clone is required. Reversible.

Physical history merge is out of scope until explicitly requested.
