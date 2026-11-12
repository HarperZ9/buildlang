"""
Build Engine GUI — professional trading dashboard.

Launch with::

    from build_engine.gui import launch
    launch()
"""


def launch():
    """Launch the Build Engine GUI application."""
    import sys

    from PyQt6.QtWidgets import QApplication

    from build_engine.gui.app import BuildEngineWindow

    app = QApplication(sys.argv)
    w = BuildEngineWindow()
    w.show()
    return app.exec()
