"""BuildTeX: one source, laid out once, written as PDF and HTML, checked, with a receipt.

    python -m buildtex SOURCE OUT_BASE [--theme NAME] [--set key.path=value ...] [--no-html]

SOURCE is one of:
  .bld   a BuildLang document program; `buildc run` executes it and it saves buildlang.doc/v0
  .tsv   buildlang.doc/v0 directly (what a .bld program saves)
  .tex   a LaTeX subset, imported one way by buildtex.texin; TeX itself never runs

Writes OUT_BASE.pdf, OUT_BASE.html (and per-page SVG), OUT_BASE.layout.json and
OUT_BASE.buildtex.json, the receipt: source and output SHA-256, theme, the layout
facts from buildtex.check, and the external programs the build started.
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

from engine import docir  # noqa: E402
from engine.render import render  # noqa: E402
from engine.theme import load_theme  # noqa: E402

from . import check, texin  # noqa: E402
from .guard import no_tex_or_lean  # noqa: E402

SCHEMA = "buildlang.buildtex-receipt/v0"


def sha256(path):
    with open(path, "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()


def set_path(theme, dotted, raw):
    """--set emboss.dpi=72 style override; the value is read as JSON when it parses."""
    try:
        value = json.loads(raw)
    except ValueError:
        value = raw
    node = theme
    keys = dotted.split(".")
    for k in keys[:-1]:
        node = node.setdefault(k, {})
    node[keys[-1]] = value


def to_ir(source, work, theme_name):
    """Return (path of buildlang.doc/v0, import notes)."""
    ext = os.path.splitext(source)[1].lower()
    if ext == ".tsv":
        return source, []
    if ext == ".tex":
        imp = texin.Importer()
        with open(source, encoding="utf-8") as fh:
            meta, blocks = imp.convert(fh.read())
        out = os.path.join(work, "imported.doc.tsv")
        with open(out, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(texin.to_ir(meta, blocks, theme_name or "press"))
        notes = [f"tex import: {len(blocks)} blocks"]
        if imp.unknown:
            notes.append("tex import: commands not in the subset, left out: " + ", ".join(sorted(imp.unknown)))
        return out, notes
    if ext == ".bld":
        buildc = os.environ.get("BUILDC") or shutil.which("buildc")
        if not buildc:
            raise SystemExit("buildtex: a .bld source needs buildc (set BUILDC or put it on PATH)")
        src_dir = os.path.dirname(os.path.abspath(source))
        before = set(os.listdir(src_dir))
        subprocess.run([buildc, "run", os.path.basename(source)], cwd=src_dir, check=True)
        made = sorted(f for f in set(os.listdir(src_dir)) - before if f.endswith(".doc.tsv"))
        stem = os.path.splitext(os.path.basename(source))[0] + ".doc.tsv"
        found = stem if os.path.exists(os.path.join(src_dir, stem)) else (made[0] if made else None)
        if not found:
            raise SystemExit(f"buildtex: {source} ran but saved no .doc.tsv")
        return os.path.join(src_dir, found), [f"buildc run {os.path.basename(source)} -> {found}"]
    raise SystemExit(f"buildtex: unknown source type {ext!r}")


def build(source, out_base, theme_name=None, overrides=(), html=True):
    os.makedirs(os.path.dirname(os.path.abspath(out_base)) or ".", exist_ok=True)
    work = tempfile.mkdtemp(prefix="buildtex-")
    try:
        with no_tex_or_lean() as started:
            ir, notes = to_ir(source, work, theme_name)
            doc = docir.load(ir)
            theme = load_theme(theme_name or doc.theme)
            for item in overrides:
                key, _, raw = item.partition("=")
                set_path(theme, key, raw)
            fmts = ("pdf", "svg", "html") if html else ("pdf",)
            out, lay, pages = render_with(ir, out_base, theme, fmts)
        facts = check.facts(out_base + ".pdf", {"pages": out["pages"], "layout": lay}, doc)
        receipt = {
            "schema": SCHEMA,
            "source": {"path": os.path.basename(source), "sha256": sha256(source)},
            "ir_sha256": sha256(ir),
            "theme": theme.get("name", theme_name),
            "overrides": list(overrides),
            "outputs": {os.path.basename(f): sha256(f) for f in out["files"]},
            "facts": facts,
            "failed": check.passed(facts),
            "notes": notes + [d for d in out["diagnostics"] if not d.startswith("overfull")],
            "external_programs": started,
        }
        with open(out_base + ".buildtex.json", "w", encoding="utf-8", newline="\n") as fh:
            json.dump(receipt, fh, indent=1, ensure_ascii=False)
            fh.write("\n")
        return receipt
    finally:
        shutil.rmtree(work, ignore_errors=True)


def render_with(ir, out_base, theme, fmts):
    """engine.render.render with an already loaded (and overridden) theme."""
    path = os.path.join(tempfile.mkdtemp(prefix="buildtex-theme-"), theme.get("name", "theme") + ".json")
    try:
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(theme, fh)
        return render(ir, out_base, path, formats=fmts)
    finally:
        shutil.rmtree(os.path.dirname(path), ignore_errors=True)


def main(argv=None):
    ap = argparse.ArgumentParser(prog="buildtex")
    ap.add_argument("source")
    ap.add_argument("out")
    ap.add_argument("--theme")
    ap.add_argument("--set", action="append", default=[], metavar="KEY=VALUE")
    ap.add_argument("--no-html", action="store_true")
    a = ap.parse_args(argv)
    r = build(a.source, a.out, a.theme, a.set, html=not a.no_html)
    f = r["facts"]
    print(f"{f['pages']} page(s), {len(f['overfull'])} overfull, {len(f['fonts'])} fonts embedded="
          f"{all(x['embedded'] for x in f['fonts'])}, reading order {'ok' if f['reading_order'] else 'BROKEN'}; "
          f"external programs: {r['external_programs'] or 'none'}")
    for note in r["notes"]:
        print("note:", note, file=sys.stderr)
    for bad in r["failed"]:
        print("FAILED:", bad, file=sys.stderr)
    return 1 if r["failed"] else 0
