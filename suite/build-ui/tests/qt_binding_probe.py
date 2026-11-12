from __future__ import annotations

import json
import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import qtpy
from qtpy.QtWidgets import QApplication

from build_ui import __version__
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


def main() -> int:
    app = QApplication.instance() or QApplication([])
    card, layout = Card.with_layout()
    sidebar = Sidebar(["One", "Two"], app_version=__version__)
    seen: list[int] = []
    sidebar.page_changed.connect(seen.append)
    sidebar._buttons[1].click()
    app.processEvents()
    public_names = (
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
    payload = {
        "api": qtpy.API_NAME,
        "qt_version": qtpy.QT_VERSION,
        "build_ui_version": __version__,
        "card": type(card).__name__,
        "card_base_module": type(card).__mro__[1].__module__,
        "layout": type(layout).__name__,
        "signal_roundtrip": seen == [1],
        "exclusive_sidebar": [button.isChecked() for button in sidebar._buttons] == [False, True],
        "public_api": all(public_names),
    }
    print(json.dumps(payload, sort_keys=True))
    passed = all(payload[name] for name in ("signal_roundtrip", "exclusive_sidebar", "public_api"))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
