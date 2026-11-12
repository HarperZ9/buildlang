"""
Build Oracle — GUI Package

Launch the PyQt6 application window.
"""


def launch():
    import sys

    from PyQt6.QtWidgets import QApplication

    from build_oracle.gui.app import BuildOracleWindow

    app = QApplication(sys.argv)
    window = BuildOracleWindow()
    window.show()
    return app.exec()
