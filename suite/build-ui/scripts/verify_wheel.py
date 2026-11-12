from __future__ import annotations

import argparse
import importlib
import importlib.metadata
import importlib.util
import json
import os
from pathlib import Path

from packaging.requirements import Requirement
from packaging.specifiers import SpecifierSet
from packaging.utils import canonicalize_name
from packaging.version import Version

EXPECTED_ENV = {"PyQt6": "pyqt6", "PySide6": "pyside6"}


def _requirements() -> dict[str, list[Requirement]]:
    result: dict[str, list[Requirement]] = {}
    for value in importlib.metadata.requires("build-ui") or []:
        requirement = Requirement(value)
        name = canonicalize_name(requirement.name)
        result.setdefault(name, []).append(requirement)
    return result


def _one_requirement(
    requirements: dict[str, list[Requirement]],
    name: str,
) -> Requirement:
    matches = requirements[name]
    assert len(matches) == 1, (name, matches)
    return matches[0]


def _assert_metadata() -> None:
    assert importlib.metadata.version("build-ui") == "2.0.0"
    requirements = _requirements()
    qtpy_requirement = _one_requirement(requirements, "qtpy")
    pyqt_requirement = _one_requirement(requirements, "pyqt6")
    pyside_requirement = _one_requirement(requirements, "pyside6")
    assert qtpy_requirement.specifier == SpecifierSet(">=2.4.3,<3")
    assert qtpy_requirement.marker is None
    assert pyqt_requirement.specifier == SpecifierSet(">=6.5,<7")
    assert str(pyqt_requirement.marker) == 'extra == "pyqt6"'
    assert pyside_requirement.specifier == SpecifierSet(">=6.11.1,<7")
    assert str(pyside_requirement.marker) == 'extra == "pyside6"'
    assert Version(importlib.metadata.version("QtPy")) in qtpy_requirement.specifier
    distribution = importlib.metadata.distribution("build-ui")
    license_names = {
        Path(str(path)).name
        for path in distribution.files or ()
        if ".dist-info/licenses/" in str(path).replace("\\", "/")
    }
    assert {"LICENSE", "THIRD_PARTY_NOTICES.md"} <= license_names


def _assert_installed_origin(source_root: Path | None) -> None:
    import build_ui

    if source_root is None:
        return
    origin = Path(build_ui.__file__).resolve()
    try:
        origin.relative_to(source_root.resolve())
    except ValueError:
        return
    raise AssertionError(f"build_ui resolved from source checkout: {origin}")


def _verify_no_binding(source_root: Path | None) -> dict[str, object]:
    _assert_installed_origin(source_root)
    assert importlib.util.find_spec("PyQt6") is None
    assert importlib.util.find_spec("PySide6") is None
    from build_ui.theme import STYLE

    assert STYLE
    try:
        importlib.import_module("build_ui.widgets")
    except ImportError as exc:
        message = str(exc)
        assert "build-ui[pyqt6]>=2,<3" in message
        assert "build-ui[pyside6]>=2,<3" in message
    else:
        raise AssertionError("build_ui.widgets imported without a Qt binding")
    return {"api": None, "theme_import": True, "widget_error": True}


def _verify_binding(expected_api: str, source_root: Path | None) -> dict[str, object]:
    expected_env = EXPECTED_ENV[expected_api]
    assert os.environ.get("QT_API") == expected_env
    other_api = "PySide6" if expected_api == "PyQt6" else "PyQt6"
    assert importlib.util.find_spec(expected_api) is not None
    assert importlib.util.find_spec(other_api) is None
    assert os.environ.get("QT_QPA_PLATFORM") == "offscreen"

    import qtpy
    from qtpy.QtWidgets import QApplication

    import build_ui
    from build_ui.theme import STYLE, C, create_stylesheet
    from build_ui.widgets import (
        Card,
        Heading,
        NavButton,
        Sidebar,
        Stat,
        StatusDot,
        ToastNotification,
    )

    _assert_installed_origin(source_root)
    assert expected_api == qtpy.API_NAME
    assert build_ui.__version__ == "2.0.0"
    assert all(
        (
            C,
            STYLE,
            create_stylesheet,
            Card,
            StatusDot,
            Heading,
            Stat,
            NavButton,
            Sidebar,
            ToastNotification,
        )
    )
    app = QApplication.instance() or QApplication([])
    card, layout = Card.with_layout()
    sidebar = Sidebar(["One", "Two"], app_version=build_ui.__version__)
    seen: list[int] = []
    sidebar.page_changed.connect(seen.append)
    sidebar._buttons[1].click()
    app.processEvents()
    checked = [button.isChecked() for button in sidebar._buttons]
    assert type(card).__mro__[1].__module__.startswith(expected_api)
    assert layout.parent() is card
    assert seen == [1]
    assert checked == [False, True]
    return {
        "api": qtpy.API_NAME,
        "build_ui_version": build_ui.__version__,
        "exclusive_sidebar": True,
        "other_binding_absent": True,
        "signal_roundtrip": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--expected-api", choices=("PyQt6", "PySide6"))
    mode.add_argument("--expect-no-binding", action="store_true")
    parser.add_argument("--source-root", type=Path)
    args = parser.parse_args()
    _assert_metadata()
    payload = (
        _verify_no_binding(args.source_root)
        if args.expect_no_binding
        else _verify_binding(args.expected_api, args.source_root)
    )
    print(json.dumps(payload, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
