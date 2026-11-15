# Contributing

Build Universe is part of the Project Telos public surface and is an **alpha**
compiler ecosystem. Keep changes small, tested against the real gates, and honest.
Accuracy is the hard rule here: every public claim must match [STATUS.md](STATUS.md)
and the actual code. Where a doc and STATUS.md disagree, STATUS.md wins, and the doc
gets corrected, never the other way around.

Before sending a change:

- Read [README.md](README.md), [ARCHITECTURE.md](ARCHITECTURE.md), [STATUS.md](STATUS.md),
  and any local `AGENTS.md`.
- Run the narrowest verification command that covers what you touched (below).
- Keep examples, module docs, and public claims aligned with current behavior.
- Do not commit secrets, `.env` files, or generated caches (`target/`, `.obj`, `.pdb`,
  release zips). The `buildlang/` compiler dir and the Cargo `target/` caches are
  already git-ignored.

## Build from source

There is **no** package-registry install. This is a from-source build; do not add or
imply `cargo install`, crates.io, PyPI, or any registry path.

The three layers build differently:

- **The compiler (`buildlang/`, Rust).** A separate repo, git-ignored inside this one.
  Clone it, then build and test it with Cargo:

  ```bash
  cargo build --release          # produces buildc
  cargo test                     # 612 pass / 0 fail expected
  cargo test -- --skip spirv     # 3 SPIR-V tests need spirv-val; skip if absent
  ```

  Cargo incremental builds are unreliable in this environment (source mtimes are
  preserved by the IO layer). Run `cargo clean -p buildlang` before a rebuild if a
  change does not seem to take.

- **A `.bld` module.** Compile a module to C with `buildc`, then build the C with a
  local toolchain (MSVC `cl` here). Only the C backend runs end to end.

  ```bash
  buildc your_module/lib.bld --target c -o out.c
  ```

- **The OS kernel (`buildos/`, Rust).** Its own Rust project with its own toolchain,
  independent of `buildc`. See [buildos/STATUS.md](buildos/STATUS.md).

## Add a `.bld` program

1. Write the program in `programs/`.
2. Verify it transpiles: `buildc your_program.bld --target c -o out.c`.
3. If it uses `pow()`, `sqrt()`, or float division, verify `1.0/3.0` produces `0.333`,
   not `0`.
4. If it calls methods defined later in the same `impl` block, move the callee above
   the caller (forward references within an `impl` are a known compiler limitation).
5. Confirm the emitted C actually compiles with your C toolchain; "transpiles cleanly"
   does not by itself prove "emits compilable C" (see [ARCHITECTURE.md](ARCHITECTURE.md)).

## Add a domain module

1. Create `your_module/lib.bld`.
2. Use `pub mod name {`, not `pub module std::name {` (the latter parses as `mod std`).
3. Transpile it: `buildc your_module/lib.bld --target c -o out.c`.
4. Common blockers, all in [ENGINEERING.md](ENGINEERING.md): Unicode box-drawing
   characters in comments break macro expansion; forward references in `impl` blocks;
   DefId misresolution in very large files; type-variable scope leaks across `impl`
   blocks.
5. Register the module in `tools/components.toml` so it joins the organism health
   check, then confirm `python tools/verify_organism.py` still passes.

## Test and lint gates

Run the slice that covers your change first; reserve full runs for shared-base changes
or a release.

| Change | Gate to run |
|---|---|
| Compiler (`buildlang/`) | `cargo test` (and `cargo fmt` + `cargo clippy` before pushing) |
| A `.bld` module or program | `buildc ... --target c` transpile, then compile the C, then `python tools/verify_organism.py` |
| Kernel (`buildos/`) | the kernel's own Rust build/test; see `buildos/STATUS.md` |
| Docs / public claims | re-check against STATUS.md; `git grep -niE "all written in buildlang|self-hosted"` should return nothing presented as achieved |

CI enforces two workflows:

- **`.github/workflows/ci.yml`** (`ubuntu-latest`): repository validation (100+ `.bld`
  files present, required module dirs exist, no committed secrets) and `.bld` lint
  (UTF-8 encoding, trailing whitespace).
- **`.github/workflows/organism.yml`** (`windows-latest`, MSVC available): runs
  `python tools/verify_organism.py --json`. The compiler and `.bld` modules are a
  separate repo absent in CI, so they skip honestly; frametrace is verified there.

## PR expectations

- One bounded change per PR, with the verification command and its output in the
  description.
- No new capability claim without a test or a compiled artifact behind it. If something
  is partial, say so and point at STATUS.md; do not describe a stub as shipped.
- Do not label a Tier 3 sketch (lumina, refract, forge tooling, neutrino, nexus,
  wavelength) as implemented engineering.
- Do not weaken the honesty surface: self-hosting and whole-ecosystem cross-module
  compilation are goals, and must stay described as goals until they actually build.
- The maintainer (see [AUTHORS.md](AUTHORS.md)) reviews, tests, and owns all public
  release, security, and claim decisions.
