# Security Policy

## Supported

Build UI follows a rolling release. Only the latest release on the supported
2.x line receives security fixes.

## Reporting a vulnerability

Report suspected vulnerabilities privately through GitHub Security Advisories:
open the repository's **Security** tab and choose **Report a vulnerability**.
Do not open a public issue for an unfixed vulnerability.

Include the affected module and version, a minimal reproduction, and the
impact. The maintainer will acknowledge the report, establish a response
window, and coordinate disclosure.

## Attack surface

Build UI is a UI-only support library with a deliberately small surface:

- **No network.** The library performs no network access. Nothing is fetched or
  sent.
- **No code evaluation.** The library never evaluates input as code; it builds
  Qt stylesheet strings from color constants and constructs widgets.
- **No file I/O.** `theme.py`, `_qt.py`, and `widgets.py` do not read or write
  application files. Rendering goes through Qt's widget and paint APIs.
- **Explicit binding selection.** The core package installs QtPy but no Qt
  binding. PyQt6 and PySide6 are independent optional extras with their own
  upstream advisories and license terms. `QT_API` must be selected before the
  first QtPy or Build UI widget import when multiple bindings are installed.

## What does not count

- A style regression is a correctness issue, not a security vulnerability; use
  the normal issue tracker.
- A crash caused by constructing a widget without a running `QApplication` is
  standard Qt behavior, not a Build UI vulnerability.
- Mixing objects from PyQt6 and PySide6 in one process violates the documented
  process invariant and is unsupported.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for dependency licensing.
