# Build UI Enterprise Readiness

Build UI is the shared presentation layer of the Build/Project Telos family: a
small, binding-neutral Qt 6 theme and widget set that gives every Build
application one consistent visual language.

## Enterprise role

- Provide one source of truth for color (`build_ui.theme.C`) and Qt stylesheet
  generation (`create_stylesheet`).
- Provide reusable, pre-styled widgets (`Card`, `Sidebar`, `NavButton`, `Stat`,
  `Heading`, `StatusDot`, `ToastNotification`) without application logic.
- Keep source portable across PyQt6 and PySide6 through QtPy while requiring
  exactly one selected binding per process.

## Operator surface

- The importable Python API (`build_ui.theme`, `build_ui.widgets`) embeds in a
  supported Qt 6 application. There is no CLI or standalone GUI.
- Operators with multiple bindings installed set `QT_API=pyqt6` or
  `QT_API=pyside6` before the first QtPy or Build UI import.

## Reproducibility and provenance

- The same color class produces the same stylesheet string, and widgets render
  deterministically from the same constructor arguments and Qt environment.
- The release workflow builds one wheel and sdist, runs Twine validation,
  records SHA-256 hashes, and performs clean installed-wheel checks outside the
  source checkout.
- Manual runs remain non-publishing. A published GitHub release may use OIDC
  trusted publishing only after the candidate job and all three isolated wheel
  lanes pass.

## Dependencies and boundary

- **Core runtime:** QtPy. Plain installation provides no Qt binding.
- **Binding extras:** PyQt6 or PySide6, installed independently. Their upstream
  licenses and advisories apply; Build UI does not relicense them.
- **Application boundary:** Build UI has no network, code evaluation, business
  logic, or application file I/O.

## Quality gates

- Ruff check and format check, QtPy-derived mypy flags, and pytest run in CI.
- Linux and Windows compatibility lanes exercise both bindings at a 70%
  coverage floor, plus binding-selection and fail-closed mismatch probes.
- Binding-free Python 3.10-3.13 lanes verify the pure theme and packaging
  surfaces.
- The candidate wheel is installed in core-only, PyQt6-only, and PySide6-only
  environments and rejected if it resolves from the source checkout.

## Honest limits

- Local acceptance does not replace downstream GUI acceptance. Every consuming
  application must validate its own screens before updating its Build UI
  constraint.
- A process cannot safely mix Qt objects from two bindings, and changing
  `QT_API` after import is unsupported.
- Calibrate Pro accepted the final clean-runner 2.0.0 wheel by exact hash; the
  receipt and immutable GitHub Actions run are recorded in `MIGRATING.md`.
  Other consumers retain their own application-level acceptance gates.
