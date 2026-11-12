from __future__ import annotations

import argparse
import importlib.util
import json
import os
import subprocess
import sys
import tempfile

MODULE_BY_API = {"pyqt6": "PyQt6", "pyside6": "PySide6"}
OPPOSITE_API = {"pyqt6": "pyside6", "pyside6": "pyqt6"}
CHILD = """
import json

try:
    import build_ui.widgets
except ImportError as exc:
    print(json.dumps({"failed_closed": True, "error": str(exc)}, sort_keys=True))
    raise SystemExit(0)
print(json.dumps({"failed_closed": False}, sort_keys=True))
raise SystemExit(1)
"""


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--installed-api", choices=tuple(MODULE_BY_API), required=True)
    args = parser.parse_args()
    installed_module = MODULE_BY_API[args.installed_api]
    absent_module = MODULE_BY_API[OPPOSITE_API[args.installed_api]]
    assert importlib.util.find_spec(installed_module) is not None
    assert importlib.util.find_spec(absent_module) is None
    requested_api = OPPOSITE_API[args.installed_api]
    env = os.environ.copy()
    env["QT_API"] = requested_api
    env["QT_QPA_PLATFORM"] = "offscreen"
    completed = subprocess.run(
        [sys.executable, "-c", CHILD],
        cwd=tempfile.gettempdir(),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert completed.returncode == 0, (completed.stdout, completed.stderr)
    payload = json.loads(completed.stdout.strip().splitlines()[-1])
    assert payload["failed_closed"] is True
    print(
        json.dumps(
            {
                "installed_api": args.installed_api,
                "requested_api": requested_api,
                "failed_closed": True,
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
