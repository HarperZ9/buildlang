# Build organism tooling

The Build repo is a mixed-language organism: a Rust compiler, BuildLang
modules, native components (frametrace -- Rust core + C ABI + C++ D3D11 hook +
Python adapter), and more. These tools keep it cohesive and honest as it grows,
so modules integrate into one whole by ground truth rather than by assertion.

## components.toml

The registry of REAL, verifiable components -- the load-bearing tier, not the
sketches in STATUS.md Tier 3. Each entry has a verify command and an expectation
(tested = runtime-verified, build = compile/link only). A new real module joins
the organism by adding an entry here.

## verify_organism.py

Runs every component verify command and prints what actually builds and passes on
this machine right now -- one command, no claims:

    python tools/verify_organism.py            # all components
    python tools/verify_organism.py --quick    # skip heavy (e.g. the compiler)
    python tools/verify_organism.py --json      # CI-friendly summary

Exit code is the number of failures, so it doubles as a CI gate. This is the
observe-ground-truth doctrine made operational: the organism reports its own
health instead of anyone reasoning about whether the modules still fit together.
