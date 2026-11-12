from __future__ import annotations

import os

SUPPORTED_REQUESTS = {"pyqt6": "PyQt6", "pyside6": "PySide6"}


def requested_api_name(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = value.lower()
    if normalized not in SUPPORTED_REQUESTS:
        raise ImportError(f"Build UI 2 requires QT_API=pyqt6 or QT_API=pyside6; got {value!r}.")
    return SUPPORTED_REQUESTS[normalized]


REQUESTED_API_NAME = requested_api_name(os.environ.get("QT_API"))

try:
    import qtpy
except ImportError as exc:
    if type(exc).__name__ == "QtBindingsNotFoundError":
        raise ImportError(
            'Build UI widgets require one Qt 6 binding; install "build-ui[pyqt6]>=2,<3" or "build-ui[pyside6]>=2,<3".'
        ) from exc
    raise


SUPPORTED_APIS = frozenset({"PyQt6", "PySide6"})


def require_supported_api(
    api_name: str,
    requested_name: str | None,
) -> str:
    if api_name not in SUPPORTED_APIS:
        raise ImportError(
            f"Build UI 2 requires PyQt6 or PySide6; QtPy selected {api_name!r}. "
            "Set QT_API=pyqt6 or QT_API=pyside6 before importing Build UI."
        )
    if requested_name is not None and api_name != requested_name:
        raise ImportError(
            f"Build UI explicitly requested {requested_name}, but QtPy selected "
            f"{api_name}. Install the requested Build UI binding extra and import "
            "Build UI before any other Qt binding."
        )
    return api_name


QT_API_NAME = require_supported_api(qtpy.API_NAME, REQUESTED_API_NAME)

from qtpy import QtCore, QtGui, QtWidgets

__all__ = [
    "QT_API_NAME",
    "QtCore",
    "QtGui",
    "QtWidgets",
    "require_supported_api",
    "requested_api_name",
]
