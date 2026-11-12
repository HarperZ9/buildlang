"""Build ecosystem shared widgets -- Card, StatusDot, Heading, Stat, NavButton, Sidebar, ToastNotification."""

from build_ui._qt import QtCore, QtGui, QtWidgets

QEasingCurve = QtCore.QEasingCurve
QPoint = QtCore.QPoint
QPropertyAnimation = QtCore.QPropertyAnimation
Qt = QtCore.Qt
QTimer = QtCore.QTimer
Signal = QtCore.Signal
QColor = QtGui.QColor
QPainter = QtGui.QPainter
QFrame = QtWidgets.QFrame
QGraphicsDropShadowEffect = QtWidgets.QGraphicsDropShadowEffect
QGraphicsOpacityEffect = QtWidgets.QGraphicsOpacityEffect
QHBoxLayout = QtWidgets.QHBoxLayout
QLabel = QtWidgets.QLabel
QPushButton = QtWidgets.QPushButton
QSizePolicy = QtWidgets.QSizePolicy
QVBoxLayout = QtWidgets.QVBoxLayout
QWidget = QtWidgets.QWidget

from build_ui.theme import C


class Card(QFrame):
    """A surface card with consistent styling."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setStyleSheet(f"""
            Card {{
                background: {C.SURFACE};
                border: 1px solid {C.BORDER};
                border-radius: 14px;
            }}
        """)
        shadow = QGraphicsDropShadowEffect(self)
        shadow.setBlurRadius(18)
        shadow.setXOffset(0)
        shadow.setYOffset(3)
        shadow.setColor(QColor(180, 160, 140, 30))
        self.setGraphicsEffect(shadow)

    @staticmethod
    def with_layout(layout_cls=QVBoxLayout, margins=(20, 16, 20, 16), spacing=10):
        card = Card()
        layout = layout_cls(card)
        layout.setContentsMargins(*margins)
        layout.setSpacing(spacing)
        return card, layout


class StatusDot(QWidget):
    """Small colored circle indicator."""

    def __init__(self, color: str = C.TEXT3, size: int = 10, parent=None):
        super().__init__(parent)
        self._color = color
        self.setFixedSize(size, size)

    def set_color(self, color: str):
        self._color = color
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        # Soft glow ring
        glow = QColor(self._color)
        glow.setAlpha(40)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(glow)
        p.drawEllipse(0, 0, self.width(), self.height())
        # Inner dot
        p.setBrush(QColor(self._color))
        inset = max(2, self.width() // 4)
        p.drawEllipse(inset, inset, self.width() - inset * 2, self.height() - inset * 2)
        p.end()


class Heading(QLabel):
    """Section heading with consistent typography."""

    def __init__(self, text: str, level: int = 1, parent=None):
        super().__init__(text, parent)
        sizes = {1: 21, 2: 16, 3: 14}
        weights = {1: "500", 2: "500", 3: "400"}
        colors = {1: C.TEXT, 2: C.TEXT, 3: C.TEXT2}
        self.setStyleSheet(
            f"font-size: {sizes.get(level, 14)}px; "
            f"font-weight: {weights.get(level, '400')}; "
            f"color: {colors.get(level, C.TEXT)};"
        )


class Stat(QWidget):
    """Compact stat display: value + label."""

    def __init__(self, label: str, value: str = "\u2014", color: str = C.TEXT, parent=None):
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        self._value_label = QLabel(value)
        self._value_label.setStyleSheet(f"font-size: 22px; font-weight: 600; color: {color};")
        layout.addWidget(self._value_label)

        desc = QLabel(label)
        desc.setStyleSheet(f"font-size: 11px; color: {C.TEXT2};")
        layout.addWidget(desc)

    def set_value(self, value: str, color: str | None = None):
        self._value_label.setText(value)
        if color:
            self._value_label.setStyleSheet(f"font-size: 22px; font-weight: 600; color: {color};")


class NavButton(QPushButton):
    """Sidebar navigation button."""

    def __init__(self, text: str, parent=None):
        super().__init__(text, parent)
        self.setCheckable(True)
        self.setFixedHeight(42)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self._update_style(False)

    def _update_style(self, checked: bool):
        if checked:
            self.setStyleSheet(f"""
                QPushButton {{
                    background: {C.ACCENT};
                    border: none;
                    border-radius: 10px;
                    color: white;
                    text-align: left;
                    padding-left: 18px;
                    font-weight: 600;
                    font-size: 13px;
                }}
            """)
        else:
            self.setStyleSheet(f"""
                QPushButton {{
                    background: transparent;
                    border: none;
                    border-radius: 10px;
                    color: {C.TEXT2};
                    text-align: left;
                    padding-left: 18px;
                    font-size: 13px;
                }}
                QPushButton:hover {{
                    background: {C.SURFACE2};
                    color: {C.TEXT};
                }}
            """)

    def setChecked(self, checked: bool):
        super().setChecked(checked)
        self._update_style(checked)

    def nextCheckState(self):
        self.setChecked(True)


class Sidebar(QWidget):
    """Left sidebar with navigation buttons."""

    page_changed = Signal(int)

    def __init__(
        self,
        pages: list[str],
        app_name: str = "Build",
        app_version: str = "1.0.0",
        parent=None,
    ):
        super().__init__(parent)
        self.setFixedWidth(190)
        self.setStyleSheet(f"background: {C.BG_ALT}; border-right: 1px solid {C.BORDER};")

        layout = QVBoxLayout(self)
        layout.setContentsMargins(10, 14, 10, 14)
        layout.setSpacing(6)

        # Logo / title
        title = QLabel(app_name)
        title.setStyleSheet(f"font-size: 15px; font-weight: 600; color: {C.ACCENT_TX}; padding: 8px 8px 20px 8px;")
        layout.addWidget(title)

        self._buttons = []
        for i, name in enumerate(pages):
            btn = NavButton(name)
            btn.clicked.connect(lambda checked, idx=i: self._on_click(idx))
            layout.addWidget(btn)
            self._buttons.append(btn)

        layout.addStretch()

        # Bottom: version
        ver = QLabel(f"v{app_version}")
        ver.setStyleSheet(f"color: {C.TEXT3}; font-size: 10px; padding: 8px;")
        layout.addWidget(ver)

        self._buttons[0].setChecked(True)

    def _on_click(self, index: int):
        for i, btn in enumerate(self._buttons):
            btn.setChecked(i == index)
        self.page_changed.emit(index)


class ToastNotification(QFrame):
    """Slide-in toast notification with auto-dismiss and fade-out."""

    _ICONS = {
        "info": "\u2139\ufe0f",
        "success": "\u2705",
        "warning": "\u26a0\ufe0f",
    }
    _BORDER_COLORS = {
        "info": C.CYAN,
        "success": C.GREEN,
        "warning": C.YELLOW,
    }

    def __init__(self, message: str, level: str = "info", parent=None):
        super().__init__(parent)
        self.setFixedWidth(340)
        self.setFixedHeight(52)

        border_color = self._BORDER_COLORS.get(level, C.CYAN)
        self.setStyleSheet(
            f"ToastNotification {{  background: {C.SURFACE};  border: 1px solid {border_color};  border-radius: 12px;}}"
        )

        shadow = QGraphicsDropShadowEffect(self)
        shadow.setBlurRadius(24)
        shadow.setXOffset(0)
        shadow.setYOffset(4)
        shadow.setColor(QColor(120, 100, 80, 50))
        self.setGraphicsEffect(shadow)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(14, 8, 10, 8)
        layout.setSpacing(10)

        # Icon
        icon_text = self._ICONS.get(level, self._ICONS["info"])
        icon_label = QLabel(icon_text)
        icon_label.setStyleSheet("font-size: 16px; background: transparent;")
        icon_label.setFixedWidth(22)
        layout.addWidget(icon_label)

        # Message
        msg_label = QLabel(message)
        msg_label.setStyleSheet(f"font-size: 12px; color: {C.TEXT}; background: transparent;")
        msg_label.setWordWrap(True)
        layout.addWidget(msg_label, stretch=1)

        # Close button
        close_btn = QPushButton("\u2715")
        close_btn.setFixedSize(22, 22)
        close_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        close_btn.setStyleSheet(
            f"QPushButton {{ background: transparent; border: none; "
            f"font-size: 12px; color: {C.TEXT3}; border-radius: 11px; }}"
            f"QPushButton:hover {{ background: {C.SURFACE2}; color: {C.TEXT}; }}"
        )
        close_btn.clicked.connect(self._fade_out)
        layout.addWidget(close_btn)

        # Auto-hide timer
        self._auto_timer = QTimer(self)
        self._auto_timer.setSingleShot(True)
        self._auto_timer.timeout.connect(self._fade_out)
        self._auto_timer.start(3000)

    def slide_in(self):
        """Animate the toast sliding up from below its final position."""
        final_pos = self.pos()
        start_pos = QPoint(final_pos.x(), final_pos.y() + 60)
        self.move(start_pos)
        self.show()

        self._slide_anim = QPropertyAnimation(self, b"pos")
        self._slide_anim.setDuration(250)
        self._slide_anim.setStartValue(start_pos)
        self._slide_anim.setEndValue(final_pos)
        self._slide_anim.setEasingCurve(QEasingCurve.Type.OutCubic)
        self._slide_anim.start()

    def _fade_out(self):
        """Fade out and remove the toast."""
        self._auto_timer.stop()
        try:
            effect = QGraphicsOpacityEffect(self)
            self.setGraphicsEffect(effect)
            effect.setOpacity(1.0)

            self._fade_anim = QPropertyAnimation(effect, b"opacity")
            self._fade_anim.setDuration(300)
            self._fade_anim.setStartValue(1.0)
            self._fade_anim.setEndValue(0.0)
            self._fade_anim.setEasingCurve(QEasingCurve.Type.InCubic)
            self._fade_anim.finished.connect(self._remove)
            self._fade_anim.start()
        except Exception:
            self._remove()

    def _remove(self):
        """Remove this toast from the parent."""
        self.hide()
        self.setParent(None)
        self.deleteLater()
