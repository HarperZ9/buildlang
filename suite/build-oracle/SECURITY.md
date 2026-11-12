# Security Policy

## Supported

Build Oracle follows a rolling release. Until a 2.0 line exists, only the
latest release on the default branch is supported for fixes.

## Reporting a vulnerability

Report suspected vulnerabilities privately via GitHub Security Advisories:
the "Security" tab of this repository, then "Report a vulnerability". Do NOT
open a public issue for an unfixed vulnerability.

Please include the affected module and version, a minimal reproduction, and
the impact. The maintainer will acknowledge within a stated window and agree
a disclosure date.

## Attack surface (the honest part)

Build Oracle is a deterministic, offline numeric library. Its surface is
small by design:

- **No network.** The core performs no network access. Nothing is fetched or
  sent. Forecasts are computed entirely from the series you pass in.
- **No code evaluation.** Inputs are numbers, arrays, and file paths; the
  library never `eval`s or executes input.
- **File I/O is limited.** Model save/load (see `test_persistence.py`) and
  CLI `--data` file loading read from disk. Treat untrusted model or data
  files as untrusted input: malformed files should raise, not corrupt state
  or execute code.
- **Forecasts are estimates, not guarantees.** Every model in this library
  (ARIMA, VAR, Prophet-style, neural, ensemble) produces a statistical
  estimate conditioned on the input series and its assumptions. Nothing in
  this library asserts a forecast is correct, and no output should be
  treated as a safety- or financial-critical guarantee without independent
  validation.
- **Optional dependencies carry their own surface.** pandas, scikit-learn,
  matplotlib, and PyQt6 are optional; when installed, their own advisories
  apply. The numpy/scipy-only core is unaffected by them.

## What does not count

- A malformed-input parse that raises a normal exception is expected
  behavior, not a vulnerability. A parse that reads out of bounds, hangs
  unboundedly, or corrupts memory in the pure-Python/numpy path is in scope.
- A forecast that turns out to be inaccurate, or a model that fails to
  converge on adversarial or degenerate input, is a correctness issue (open
  a normal issue with the reproduction), not a security vulnerability.
