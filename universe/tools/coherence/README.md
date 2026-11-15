# Coherence-membrane organs (cross-domain)

The Build/Compiler organ of the coherence membrane (docs/COHERENCE-MEMBRANE.md),
the same gate the GPU organ (photon/frametrace) exposes via ft_adjudicate, here
for build state.

## freshness.py -- build-state coherence adjudicator

The claim "the verified artifact reflects current source" is adjudicated against
a content-hash witness:

- CONFIRMED   -- current source hash == the hash recorded at the last good verify;
- CONTRADICTED -- they differ: the artifact is STALE (your edit is not reflected);
- UNRESOLVABLE -- no recorded build: cannot adjudicate freshness.

The witness hashes source CONTENT, so it is independent of mtime-based build-tool
fingerprints -- it catches the exact 2026-06-05 failure where cargo replayed a
stale binary while reporting "up to date" because the IO layer preserved mtimes.
Method diversity (the content hash AND the build tool) is the point: when they
disagree, do not trust the green.

verify_organism.py maintains these witnesses automatically (records on PASS) and
shows a freshness column, so "did my edit get verified" is a witnessed verdict.
Witness records live in witness/ (git-ignored, per-machine build state).

## compiler_oracle.py -- Compiler organ adjudicator

The same gate, a third domain. Adjudicates a .bld unit against the actual
compiler AND the actual C compiler, not against reasoning about the lowering:
- transpile: buildc emits C (exit 0);
- codegen well-formed: the EMITTED C compiles (cl /c) -- catching "type-checks
  but emits invalid C".

  python tools/coherence/compiler_oracle.py adjudicate programs/color_test.bld
  python tools/coherence/compiler_oracle.py adjudicate spectrum   # a registry module

Ground-truth finding (2026-06-05): the deep check showed "transpiles" (the
shallow bar, buildc exit 0) is NOT "emits valid C". Self-contained programs
color_test/wc/base64 are CONFIRMED (transpile + cl clean); programs/calc is
CONTRADICTED at transpile (mutability mismatch, calc.bld:120); library modules
spectrum/delta transpile but their emitted C FAILS cl -- a name-prefixing codegen
bug (typedef hdr_ColorPrimaries vs a bare ColorPrimaries field; BTreeMap in
delta). The organism gate keeps "transpiles" as the module bar; this organ is the
deeper, separate witness. buildc runs only locally, so this organ is local-only
(absent buildc in CI -> UNRESOLVABLE).