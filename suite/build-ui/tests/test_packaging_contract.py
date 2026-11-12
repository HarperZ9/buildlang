from __future__ import annotations

import pathlib

import pytest
import tomli
from packaging.licenses import canonicalize_license_expression

import build_ui

ROOT = pathlib.Path(__file__).resolve().parents[1]


def _project() -> dict:
    return tomli.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))


def test_version_is_dynamic_and_authoritative() -> None:
    data = _project()
    assert build_ui.__version__ == "2.0.0"
    assert data["project"]["dynamic"] == ["version"]
    assert "version" not in data["project"]
    assert data["tool"]["setuptools"]["dynamic"]["version"] == {"attr": "build_ui.__version__"}


def test_pep639_license_payload_is_declared() -> None:
    data = _project()
    assert data["build-system"]["requires"] == ["setuptools>=77.0.0"]
    license_expression = data["project"]["license"]
    assert license_expression == "LicenseRef-FSL-1.1-MIT"
    assert canonicalize_license_expression(license_expression) == license_expression
    assert data["project"]["license-files"] == [
        "LICENSE",
        "THIRD_PARTY_NOTICES.md",
    ]
    notice = (ROOT / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
    for token in ("QtPy", "MIT", "PyQt6", "GPL", "PySide6", "LGPLv3"):
        assert token in notice


def test_qtpy_core_and_binding_extras_are_disjoint() -> None:
    project = _project()["project"]
    assert project["dependencies"] == ["QtPy>=2.4.3,<3"]
    extras = project["optional-dependencies"]
    assert extras["pyqt6"] == ["PyQt6>=6.5,<7"]
    assert extras["pyside6"] == ["PySide6>=6.11.1,<7"]
    assert not any("PyQt" in item or "PySide" in item for item in project["dependencies"])
    for name, requirements in extras.items():
        has_pyqt = any("PyQt" in item for item in requirements)
        has_pyside = any("PySide" in item for item in requirements)
        assert not (has_pyqt and has_pyside)
        if name in {"test", "dev"}:
            assert not has_pyqt and not has_pyside


def test_package_source_has_no_direct_qt_binding_imports() -> None:
    forbidden = ("from PyQt", "import PyQt", "from PySide", "import PySide")
    for path in sorted((ROOT / "build_ui").rglob("*.py")):
        text = path.read_text(encoding="utf-8")
        assert not any(token in text for token in forbidden), path


def test_package_source_uses_binding_neutral_signal_names() -> None:
    text = (ROOT / "build_ui" / "widgets.py").read_text(encoding="utf-8")
    assert "pyqtSignal" not in text
    assert "pyqtSlot" not in text
    assert "pyqtProperty" not in text
    assert "from build_ui._qt import" in text
    assert "Signal" in text


def test_qt_facade_rejects_unsupported_and_fallback_selection() -> None:
    from build_ui._qt import require_supported_api

    with pytest.raises(ImportError, match="requires PyQt6 or PySide6"):
        require_supported_api("PyQt5", None)
    with pytest.raises(ImportError, match="requires PyQt6 or PySide6"):
        require_supported_api("PySide2", None)
    with pytest.raises(ImportError, match="requested PyQt6.*selected PySide6"):
        require_supported_api("PySide6", "PyQt6")


def test_qt_facade_reads_but_never_mutates_qt_api() -> None:
    source = (ROOT / "build_ui" / "_qt.py").read_text(encoding="utf-8")
    assert 'os.environ.get("QT_API")' in source
    forbidden = (
        'os.environ["QT_API"] =',
        "os.environ['QT_API'] =",
        "os.environ.setdefault",
        "os.environ.update",
        "os.environ.pop",
        "del os.environ",
        "os.putenv",
    )
    assert not any(token in source for token in forbidden)
