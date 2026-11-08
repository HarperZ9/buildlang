"""Reference driver: prints what `repo-art SPEC FACE-METRICS NAME` prints,
using BuildLang's original Python renderers imported by path.

Usage: python -I tests/reference.py PATH/TO/tools SPEC.json FACE-METRICS.json NAME

The SVG is written with the single trailing newline render_repo_art.py adds.
repo_card.py reads face-metrics.json from beside itself at import time; the
tables are rebuilt here from the FACE-METRICS argument so both programs
measure text from the same file.
"""
import json
import sys
from pathlib import Path

tools, spec_path, metrics_path, name = sys.argv[1:5]
sys.dont_write_bytecode = True  # read the tools folder, write nothing into it
sys.path.insert(0, str(Path(tools).resolve()))

import repo_card  # noqa: E402
from repo_flow import flow_svg  # noqa: E402

metrics = json.loads(Path(metrics_path).read_text(encoding="utf-8"))
repo_card._METRICS = metrics
repo_card.FIRST, repo_card.LAST = metrics["range"]
repo_card.SANS = repo_card._widest("sans", "regular")
repo_card.SANS_BOLD = repo_card._widest("sans", "bold")
repo_card.MONO_REG = repo_card._widest("mono", "regular")
repo_card.MONO_BOLD = repo_card._widest("mono", "bold")
repo_card._OFF_TABLE = max(max(repo_card.SANS), max(repo_card.SANS_BOLD),
                           max(repo_card.MONO_REG), max(repo_card.MONO_BOLD))
repo_card.ROLE = {
    "title": (repo_card.SANS_BOLD, 21.0, 0.0),
    "note": (repo_card.SANS, 11.5, 0.0),
    "foot": (repo_card.SANS, 11.5, 0.0),
    "kicker": (repo_card.MONO_REG, 11.0, 0.16),
    "head": (repo_card.MONO_REG, 11.0, 0.16),
    "source": (repo_card.MONO_REG, 11.5, 0.0),
    "key": (repo_card.MONO_BOLD, 13.0, 0.0),
    "value": (repo_card.MONO_REG, 12.0, 0.0),
}

spec = json.loads(Path(spec_path).read_text(encoding="utf-8"))
svg = None
for flow in spec.get("flows", []):
    if flow["file"] in (name, name + ".svg"):
        svg = flow_svg(flow)
for card in spec.get("cards", []):
    if svg is None and card["file"] in (name, name + ".svg"):
        svg = repo_card.card_svg(card)
if svg is None:
    sys.stdout.buffer.write(f"repo-art: no flow or card named `{name}` in the spec\n".encode())
    raise SystemExit(1)
sys.stdout.buffer.write((svg + "\n").encode("utf-8"))
