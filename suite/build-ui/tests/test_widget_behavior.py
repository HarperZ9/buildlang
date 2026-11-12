from __future__ import annotations

import gc
import os

import pytest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from qtpy.QtGui import QImage
from qtpy.QtWidgets import QApplication, QVBoxLayout

from build_ui.widgets import (
    Card,
    Heading,
    NavButton,
    Sidebar,
    Stat,
    StatusDot,
    ToastNotification,
)


@pytest.fixture(scope="session")
def app() -> QApplication:
    return QApplication.instance() or QApplication([])


def test_card_with_layout_returns_attached_layout(app: QApplication) -> None:
    card, layout = Card.with_layout(QVBoxLayout, margins=(1, 2, 3, 4), spacing=5)
    gc.collect()
    assert card.layout() is layout
    margins = layout.contentsMargins()
    assert (margins.left(), margins.top(), margins.right(), margins.bottom()) == (
        1,
        2,
        3,
        4,
    )
    assert layout.spacing() == 5
    assert card.graphicsEffect() is not None


def test_status_dot_mutates_and_paints(app: QApplication) -> None:
    dot = StatusDot(size=12)
    dot.set_color("#112233")
    dot.show()
    app.processEvents()
    image = QImage(dot.size(), QImage.Format.Format_ARGB32)
    image.fill(0)
    dot.render(image)
    assert dot._color == "#112233"
    assert any(image.pixelColor(x, y).alpha() > 0 for x in range(image.width()) for y in range(image.height()))
    dot.close()


def test_sidebar_emits_selected_page(app: QApplication) -> None:
    sidebar = Sidebar(["One", "Two"], app_name="Probe", app_version="2.0.0")
    seen: list[int] = []
    sidebar.page_changed.connect(seen.append)
    sidebar._buttons[1].click()
    app.processEvents()
    assert seen == [1]
    assert [button.isChecked() for button in sidebar._buttons] == [False, True]


def test_text_and_selection_widgets_preserve_behavior(app: QApplication) -> None:
    heading = Heading("Section", level=2)
    assert heading.text() == "Section"
    assert "font-size: 16px" in heading.styleSheet()
    stat = Stat("Value", "1")
    stat.set_value("2", color="#112233")
    assert stat._value_label.text() == "2"
    assert "#112233" in stat._value_label.styleSheet()
    button = NavButton("Page")
    assert not button.isChecked()
    button.nextCheckState()
    assert button.isChecked()


def test_toast_timer_and_animations_are_owned(app: QApplication) -> None:
    toast = ToastNotification("Saved", level="success")
    try:
        gc.collect()
        assert toast.graphicsEffect() is not None
        assert toast._auto_timer.isSingleShot()
        assert toast._auto_timer.isActive()
        assert toast._auto_timer.interval() == 3000
        final_position = toast.pos()
        toast.slide_in()
        assert toast._slide_anim.duration() == 250
        assert toast._slide_anim.endValue() == final_position
        toast._fade_out()
        assert toast._fade_anim.duration() == 300
        assert toast._fade_anim.endValue() == 0.0
    finally:
        toast._auto_timer.stop()
        toast.hide()
        toast.deleteLater()
        app.processEvents()
