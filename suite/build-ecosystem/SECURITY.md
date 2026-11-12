# Security Policy

## Supported

Build Ecosystem follows a rolling release aligned with its pinned members.
Only the latest release on the default branch is supported for fixes.

## Reporting a vulnerability

Report suspected vulnerabilities privately via GitHub Security Advisories —
the "Security" tab of this repository, then "Report a vulnerability". Do NOT
open a public issue for an unfixed vulnerability.

## Attack surface (the honest part)

Build Ecosystem is a pure aggregator: `build_ecosystem/__init__.py` contains
no logic beyond a version string. It performs no network access, no file
I/O, and no code evaluation of its own.

Its entire security surface is inherited from the packages it pins:
**build-color**, **build-finance**, **build-oracle**, **build-engine**,
**calibrate-pro**, and **build-ui**. A vulnerability in any member should be
reported against that member's repository. If the issue is specifically
about which version this package pins (e.g. it pins a known-vulnerable
minimum version), report it here.
