"""Themes are data: JSON files in typeset/themes, with single inheritance."""
import json
import os

from .layout import merged

THEMES = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "themes"))


def load_theme(name_or_path):
    path = name_or_path if name_or_path.endswith(".json") else os.path.join(THEMES, name_or_path + ".json")
    with open(path, encoding="utf-8") as fh:
        t = json.load(fh)
    if "extends" in t:
        base = load_theme(t["extends"])
        t = merged(base, {k: v for k, v in t.items() if k != "extends"})
    return t
