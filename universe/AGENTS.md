# Build Universe Agent Instructions

## Scope

Build Universe is the compiler-and-language substrate of Project Telos: the BuildLang
language and its `.bld` standard library and domain modules, a Rust compiler
(`buildlang/`) that transpiles `.bld` to C, and a Rust OS kernel (`buildos/`). It is
an **alpha** research compiler. Changes should improve the compiler, a `.bld` module,
the kernel, or the honesty of the docs, without ever inflating a claim.

## Where source lives

- **`buildlang/`** (Rust, git-ignored, separate repo): the compiler. ~231K LOC, 612
  passing cargo tests. Produces `buildc`.
- **`foundation/`** (`.bld`): the standard library.
- **Domain modules** (`.bld`): spectrum, chromatic, delta, oracle, entropy,
  field-tensor, photon, prism, axiom, and the rest. `programs/` holds the native
  example programs.
- **`buildos/`** (Rust): the hobby OS kernel, independent of `buildc`.
- **`STATUS.md`** is canonical for per-module reality. **`ARCHITECTURE.md`** maps the
  layers and the `.bld`-to-C pipeline. **`CONTRIBUTING.md`** has the build and gate
  steps. **`LINEAGE.md`** is the family tree.

## Developer contract

- Accuracy is the hard gate. Every public claim must match STATUS.md and the code. If
  a doc disagrees with STATUS.md, fix the doc, not STATUS.md.
- Self-hosting and whole-ecosystem cross-module compilation are **goals**, not
  achievements. Never describe them as done. Only the C backend runs end to end;
  HLSL/GLSL emit text; the other backends produce no runnable artifact yet.
- Do **not** claim any registry install (`cargo install`, crates.io, PyPI). This is a
  from-source build only.
- Do not label a Tier 3 sketch (lumina, refract, forge tooling, neutrino, nexus,
  wavelength) as implemented engineering.
- Keep README, ARCHITECTURE, STATUS, CONTRIBUTING, and examples consistent when
  behavior changes.

## What not to touch

- The `buildlang/` and `target/` caches are git-ignored; do not commit build waste
  (`target/`, `.obj`, `.pdb`, `.exe`, release zips).
- The `quanta` -> `build` rename is complete across this repo, including the buildos
  userspace crate (`libbuild`, formerly `libquanta`) and the `x86_64-buildos` target
  specs. Do not reintroduce `quanta` naming in new code, paths, or lock files.
- Do not attempt compiler work to "make a claim true" (cross-module linking,
  self-hosting, codegen fixes). Those are roadmap; document them as honest
  limitations.
- Do not touch the deprecated `quanta-universe` repo; it is a read-only history
  record.

## Verification

Run the narrowest slice for what you touched first:

```bash
# compiler
cargo test                          # in buildlang/, 612 pass / 0 fail expected
cargo fmt && cargo clippy           # before pushing

# a .bld module or program
buildc your_module/lib.bld --target c -o out.c    # then compile the C, then:
python tools/verify_organism.py                   # organism health gate

# honesty check on docs
git grep -niE "all written in buildlang|self-hosted"   # nothing presented as achieved
```

CI: `.github/workflows/ci.yml` (repo validation + `.bld` lint) and
`.github/workflows/organism.yml` (`verify_organism.py` on windows-latest with MSVC).
