# Our tools as BuildLang packages: survey and first wave

Survey of 2026-11-07, against buildc 1.10.0. Each port is BuildLang source in
`.bld` files, compiled by `buildc pkg build` into a native program. None calls
the original or wraps it. Each one passes a differential test against the
original on the same inputs (`tests/cases.txt`, run by
`compiler/tests/registry_packages.rs`).

## First wave

| Package | Original | Licence | .bld lines | Differential result |
|---|---|---|---|---|
| `sha256` 1.0.0 | Python `hashlib` (building block) | AGPL-3.0-only | 163 | FIPS 180-4 vectors and a 1,280-byte file of every byte value: identical |
| `merkle` 1.0.0 | Flywheel `harness/merkle.py` | FSL-1.1-MIT | 424 | every root, inclusion proof and consistency proof, with tampered-proof checks, for trees of 0 to 40 leaves (1,722 lines): identical |
| `canonical-record` 1.0.0 | Flywheel `harness/canonical_record.py` (RFC 8785) | FSL-1.1-MIT; vectors CC0-1.0 | 1,622 | 190 of 190 cases identical (canonical text, SHA-256, array digest, refusals); 75 of 75 published vector fields; 8,000 random documents identical |
| `color` 1.0.0 | build-color `difference.py` and `spaces.py` | FSL-1.1-MIT | 606 | 34 Sharma CIEDE2000 pairs identical to the original and to the published 4 decimals; 608 random pairs within 2.84e-14; 103 sRGB rows of 17 conversions within 6.42e-12 (bounds 2 to 4 times the measured maximum) |
| `repo-art` 1.0.0 | BuildLang `tools/repo_flow.py` and `tools/repo_card.py` | AGPL-3.0-only | 727 | the three committed artworks and 7 stress artworks: identical bytes |
| `readability` 1.0.0 | Flywheel `harness/writing_lint/readability.py` | FSL-1.1-MIT | 234 | 9 of 9 cases identical, Unicode whitespace included; 400 random texts identical |
| `art-receipts` 1.0.0 | BuildLang `scripts/check_art_receipts.py` | AGPL-3.0-only | 356 | 10 of 10 cases identical with equal exit codes (7 kept in `cases.txt`; 3 print a platform path separator); this repository's art: `2 match, 0 mismatch, 22 not committed` from both |

Each package keeps its source's licence: its `Build.toml` names it, and the
FSL packages carry the licence text in `LICENSE`.

Why these: each is pure computation over text, bytes or numbers, and each has
a known answer outside the port (published vectors, committed goldens, or the
original run on the same inputs).

## Everything considered

| Tool | Verdict | Reason |
|---|---|---|
| Flywheel `merkle.py` | ported | above |
| Flywheel `canonical_record.py` | ported | above; two gaps: a JSON string inside a float array is refused, and CPython's recursion limit is modelled only near its edge |
| build-color `difference.py`, `spaces.py` | ported (most of both) | JzAzBz, ICtCp, HSV and wide-gamut matrices left for a later wave; none was blocked |
| BuildLang `tools/repo_flow.py`, `repo_card.py` | ported | above |
| BuildLang `tools/repo_art.py` `header_svg` | later | needs Python's MT19937 `random`, `colorsys`, and libm `sin`/`cos` bit-equal with the CRT Python uses |
| Flywheel writing linter (`harness/writing_lint/*`, `scripts/check_writing.py`) | blocked | 38 `re` calls with lookbehind, lookahead and flags; BuildLang has no regular expressions |
| Flywheel `writing_lint/readability.py` | ported | hand-written tokenizer in place of `re` |
| Flywheel `hash_chain.py`, `canonical_bytes.py`, `byte_witness*.py` | next wave | build on `canonical-record`; need their own byte-form spec |
| Flywheel `ed25519_verify.py` | later | needs SHA-512 and 255-bit field arithmetic; u128 multiply not probed |
| BuildLang `scripts/check_art_receipts.py` | ported | malformed-JSON messages are the port's own wording after the shared `FAIL cannot read` prefix |
| Low Water hash, trace codec, cbegin, move tables (2026-10-23 pilot) | ported | `lw-hash`, `lw-codec`, `lw-cbegin`, `lw-moves` 0.1.0 under AGPL-3.0-only (author decision 2026-11-09); fixtures from Low Water origin/dev 8b49bdd, listed with their hashes in each package's `tests/SOURCES.txt` |
| Low Water DETMATH | done earlier | already in `tests/detmath` |
| build-color adaptation, appearance, tonemap, spectral | next wave | numpy matrix code; float bounds as in `color` |
| build-color ICC, LUT and image I/O | later | binary I/O through `str`-typed bytes |
| Kernel `yield_spares` (units_cpu.py) | partial later | `dies_per_wafer` and `yield_noparts` fit; the rest needs `lgamma` and incomplete gamma/beta; the source has no licence file |
| Kernel `excitable_wave` (exp_waves.py) | blocked | torch and its RNG; the known answer is statistical |
| Kernels `selfcal`, `topo` | blocked | numpy RNG, least squares, SVD and sparse solves; no linear algebra in the stdlib |

## Language gaps found

Each is an issue on HarperZ9/buildlang.

| Issue | Gap | Hit by |
|---|---|---|
| #139 | f64 `{}` and `to_json()` round an exact decimal tie to the wrong digit (`204366253598380.125` prints `...380.13`; ECMAScript and Python give `...380.12`) | canonical-record (own exact formatter) |
| #140 | `char.to_string()` returns the decimal code point | avoided in the text ports (`format!` instead) |
| #141 | `{:.N}` follows the host printf (MinGW rounds 0.125 to 0.13); `{:e}` is refused | repo-art (own formatter) |
| #142 | string literals and `format!` drop U+0000 | canonical-record |
| #143 | a local named `argc` breaks the C build | sha256 |
| #144 | `.len()` on a `&Vec<T>` parameter fails in the C backend; the error names the wrong line | all ports (`vec_len` instead) |
| #145 | `Vec<str>` has no `sort`, `pop` or `truncate` in the C backend | canonical-record, repo-art |
| #146 | `read_bytes` is typed `str`; no bytes-to-string constructor | sha256, merkle |
| #147 | no `rotate_left`/`rotate_right`, free `cbrt`/`fma`, `lgamma`, `erf`, incomplete gamma or beta | sha256; blocks `yield_spares` |
| #148 | no regular expressions | blocks the writing linter |
| #149 | `str.lines()` keeps a trailing empty line | merkle (splits bytes by hand) |
| #150 | deep recursion exits with code 127 and no message | canonical-record (iterative walk) |
| #151 | `HashMap<str, i64>` is a C-string-keyed f64 map; `.get(k).unwrap()` does not compile | canonical-record |
| #152 | package sources share one namespace (imports are joined text) | every package (name prefixes) |
| #153 | executables from `pkg build` are not byte-reproducible with MinGW gcc; the C artifact is | the package manager |
| #157 | `s.as_bytes()` passed to a `Vec<u8>` parameter checks, then fails in gcc | art-receipts |
