"""
Calibrate Pro - Fullscreen Test Pattern Display

A zero-dependency (beyond tkinter) fullscreen test pattern viewer.

Features
--------
- Displays patterns fullscreen on a selected monitor.
- Keyboard navigation: Left/Right arrows cycle patterns, Escape exits,
  number keys 1-9 jump to specific patterns.
- Overlay label shows the current pattern name and fades after 2 seconds.

Patterns
--------
1. Grayscale Ramp      - Horizontal gradient, 256 steps black to white
2. RGB Primaries       - Three vertical bars: R, G, B
3. RGBCMY              - Six vertical bars: R, G, B, C, M, Y
4. Gray Steps          - 11 uniform patches 0% to 100% in 10% increments
5. White               - Full white (uniformity check)
6. Black               - Full black (black-level check)
7. Checkerboard        - Alternating black/white squares
8. Color Gradient      - Full hue sweep at constant lightness

All patterns are rendered with tkinter Canvas -- no PIL or other image
library required.
"""

import math
import colorsys
import tkinter as tk
from typing import Callable, List, Tuple, Optional


# ---------------------------------------------------------------------------
# Pattern drawing functions
# ---------------------------------------------------------------------------
# Each function has the signature ``(canvas, width, height) -> None``.
# It should draw directly on the supplied canvas, which has already been
# cleared to black.

def _draw_grayscale_ramp(canvas: tk.Canvas, w: int, h: int) -> None:
    """Horizontal gradient from black to white (256 steps)."""
    steps = 256
    strip_w = max(w / steps, 1.0)
    for i in range(steps):
        v = int(i * 255 / (steps - 1))
        colour = f"#{v:02x}{v:02x}{v:02x}"
        x0 = i * w / steps
        x1 = (i + 1) * w / steps
        canvas.create_rectangle(x0, 0, x1, h, fill=colour, outline="")


def _draw_rgb_primaries(canvas: tk.Canvas, w: int, h: int) -> None:
    """Three vertical bars: red, green, blue."""
    colours = ["#ff0000", "#00ff00", "#0000ff"]
    bar_w = w / len(colours)
    for i, c in enumerate(colours):
        canvas.create_rectangle(i * bar_w, 0, (i + 1) * bar_w, h,
                                fill=c, outline="")


def _draw_rgbcmy(canvas: tk.Canvas, w: int, h: int) -> None:
    """Six vertical bars: R, G, B, C, M, Y."""
    colours = [
        "#ff0000", "#00ff00", "#0000ff",
        "#00ffff", "#ff00ff", "#ffff00",
    ]
    bar_w = w / len(colours)
    for i, c in enumerate(colours):
        canvas.create_rectangle(i * bar_w, 0, (i + 1) * bar_w, h,
                                fill=c, outline="")


def _draw_gray_steps(canvas: tk.Canvas, w: int, h: int) -> None:
    """11 uniform patches from 0% to 100% in 10% increments."""
    steps = 11
    patch_w = w / steps
    for i in range(steps):
        v = int(i * 255 / (steps - 1))
        colour = f"#{v:02x}{v:02x}{v:02x}"
        canvas.create_rectangle(i * patch_w, 0, (i + 1) * patch_w, h,
                                fill=colour, outline="")


def _draw_white(canvas: tk.Canvas, w: int, h: int) -> None:
    """Full white screen."""
    canvas.create_rectangle(0, 0, w, h, fill="#ffffff", outline="")


def _draw_black(canvas: tk.Canvas, w: int, h: int) -> None:
    """Full black screen."""
    canvas.create_rectangle(0, 0, w, h, fill="#000000", outline="")


def _draw_checkerboard(canvas: tk.Canvas, w: int, h: int) -> None:
    """Alternating black/white squares."""
    # Aim for roughly 32-pixel squares
    cell = max(int(min(w, h) / 32), 8)
    cols = int(math.ceil(w / cell))
    rows = int(math.ceil(h / cell))
    for row in range(rows):
        for col in range(cols):
            if (row + col) % 2 == 0:
                colour = "#ffffff"
            else:
                colour = "#000000"
            x0 = col * cell
            y0 = row * cell
            x1 = x0 + cell
            y1 = y0 + cell
            canvas.create_rectangle(x0, y0, x1, y1, fill=colour, outline="")


def _draw_color_gradient(canvas: tk.Canvas, w: int, h: int) -> None:
    """Full hue sweep at constant saturation and lightness."""
    steps = max(int(w), 256)
    strip_w = max(w / steps, 1.0)
    for i in range(steps):
        hue = i / steps
        r, g, b = colorsys.hls_to_rgb(hue, 0.5, 1.0)
        ri, gi, bi = int(r * 255), int(g * 255), int(b * 255)
        colour = f"#{ri:02x}{gi:02x}{bi:02x}"
        x0 = i * w / steps
        x1 = (i + 1) * w / steps
        canvas.create_rectangle(x0, 0, x1, h, fill=colour, outline="")


# ---------------------------------------------------------------------------
# Pattern registry
# ---------------------------------------------------------------------------

PATTERNS: List[Tuple[str, Callable]] = [
    ("Grayscale Ramp", _draw_grayscale_ramp),
    ("RGB Primaries", _draw_rgb_primaries),
    ("RGBCMY", _draw_rgbcmy),
    ("Gray Steps", _draw_gray_steps),
    ("White", _draw_white),
    ("Black", _draw_black),
    ("Checkerboard", _draw_checkerboard),
    ("Color Gradient", _draw_color_gradient),
]


# ---------------------------------------------------------------------------
# Fullscreen pattern viewer
# ---------------------------------------------------------------------------

class PatternViewer:
    """Fullscreen tkinter window that displays calibration test patterns."""

    def __init__(
        self,
        display: int = 0,
        patterns: Optional[List[Tuple[str, Callable]]] = None,
    ):
        self.display = display
        self.patterns = patterns or PATTERNS
        self.index = 0
        self._fade_after_id: Optional[str] = None

        # Create root
        self.root = tk.Tk()
        self.root.title("Calibrate Pro - Test Patterns")
        self.root.configure(bg="black")

        # Position on the requested display
        self._position_on_display()

        # Go fullscreen
        self.root.attributes("-fullscreen", True)
        self.root.attributes("-topmost", True)

        # Hide cursor
        self.root.config(cursor="none")

        # Canvas fills the entire window
        self.canvas = tk.Canvas(
            self.root, bg="black", highlightthickness=0,
        )
        self.canvas.pack(fill=tk.BOTH, expand=True)

        # Overlay label for pattern name
        self.label = tk.Label(
            self.root,
            text="",
            font=("Segoe UI", 20),
            fg="white",
            bg="#333333",
            padx=16,
            pady=8,
        )
        # Place top-centre
        self.label.place(relx=0.5, y=30, anchor=tk.N)

        # Key bindings
        self.root.bind("<Left>", self._prev_pattern)
        self.root.bind("<Right>", self._next_pattern)
        self.root.bind("<Escape>", self._quit)
        for n in range(1, 10):
            self.root.bind(str(n), self._jump_to)

        # Draw the first pattern after the window is mapped
        self.root.after(100, self._draw_current)

    # -- positioning --------------------------------------------------------

    def _position_on_display(self) -> None:
        """Move the window to the requested display index."""
        try:
            # Try to use screeninfo (optional) for multi-monitor support
            from screeninfo import get_monitors  # type: ignore
            monitors = get_monitors()
            if self.display < len(monitors):
                m = monitors[self.display]
                self.root.geometry(f"{m.width}x{m.height}+{m.x}+{m.y}")
                return
        except ImportError:
            pass

        # Fallback: try ctypes on Windows to enumerate monitors
        try:
            import ctypes
            from ctypes import wintypes

            monitors_info: list = []

            def _callback(hMonitor, hdcMonitor, lprcMonitor, dwData):
                rct = lprcMonitor.contents
                monitors_info.append({
                    "x": rct.left,
                    "y": rct.top,
                    "w": rct.right - rct.left,
                    "h": rct.bottom - rct.top,
                })
                return True

            MONITORENUMPROC = ctypes.WINFUNCTYPE(
                wintypes.BOOL,
                wintypes.HMONITOR,
                wintypes.HDC,
                ctypes.POINTER(wintypes.RECT),
                wintypes.LPARAM,
            )
            ctypes.windll.user32.EnumDisplayMonitors(
                None, None, MONITORENUMPROC(_callback), 0,
            )

            if self.display < len(monitors_info):
                m = monitors_info[self.display]
                self.root.geometry(f"{m['w']}x{m['h']}+{m['x']}+{m['y']}")
                return
        except Exception:
            pass

        # Final fallback: just use the full screen of the default display
        sw = self.root.winfo_screenwidth()
        sh = self.root.winfo_screenheight()
        self.root.geometry(f"{sw}x{sh}+0+0")

    # -- drawing ------------------------------------------------------------

    def _draw_current(self) -> None:
        """Clear the canvas and draw the current pattern."""
        self.canvas.delete("all")
        w = self.canvas.winfo_width()
        h = self.canvas.winfo_height()

        if w < 2 or h < 2:
            # Widget not mapped yet -- retry shortly
            self.root.after(50, self._draw_current)
            return

        name, draw_fn = self.patterns[self.index]
        draw_fn(self.canvas, w, h)
        self._show_label(name)

    def _show_label(self, text: str) -> None:
        """Show the overlay label and schedule it to fade after 2 seconds."""
        if self._fade_after_id is not None:
            self.root.after_cancel(self._fade_after_id)
            self._fade_after_id = None

        number = self.index + 1
        total = len(self.patterns)
        self.label.config(text=f"{number}/{total}  {text}")
        self.label.lift()
        self.label.place(relx=0.5, y=30, anchor=tk.N)

        self._fade_after_id = self.root.after(2000, self._hide_label)

    def _hide_label(self) -> None:
        """Hide the overlay label."""
        self.label.place_forget()
        self._fade_after_id = None

    # -- navigation ---------------------------------------------------------

    def _next_pattern(self, event=None) -> None:
        self.index = (self.index + 1) % len(self.patterns)
        self._draw_current()

    def _prev_pattern(self, event=None) -> None:
        self.index = (self.index - 1) % len(self.patterns)
        self._draw_current()

    def _jump_to(self, event) -> None:
        try:
            n = int(event.char)
        except (ValueError, TypeError):
            return
        if 1 <= n <= len(self.patterns):
            self.index = n - 1
            self._draw_current()

    def _quit(self, event=None) -> None:
        self.root.destroy()

    # -- run ----------------------------------------------------------------

    def run(self) -> None:
        """Enter the tkinter main loop."""
        self.root.mainloop()


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def show_patterns(display: int = 0) -> None:
    """
    Open the fullscreen pattern viewer on the specified display.

    Args:
        display: Zero-based monitor index (0 = primary).
    """
    viewer = PatternViewer(display=display)
    viewer.run()


# ---------------------------------------------------------------------------
# Direct invocation
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(
        description="Calibrate Pro - Fullscreen Test Patterns",
    )
    parser.add_argument(
        "--display", "-d",
        type=int,
        default=0,
        help="Display index (0-based, default: 0)",
    )
    args = parser.parse_args()
    show_patterns(display=args.display)
