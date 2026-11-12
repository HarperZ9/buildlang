def launch():
    import sys

    from PyQt6.QtWidgets import QApplication

    from build_finance.gui.app import BuildFinanceWindow

    app = QApplication(sys.argv)
    window = BuildFinanceWindow()
    window.show()
    return app.exec()
