"""Font registry: theme font specs to loaded fonts."""
import os

from .sfnt import Font
from .metricfont import MetricFont

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
SHIPPED = os.path.join(ROOT, "data", "fonts")
SEARCH = [os.path.join(ROOT, "data", "fonts"), os.path.join(os.environ.get("SystemRoot", ""), "Fonts"),
          "/usr/share/fonts/truetype"] + os.environ.get("TYPESET_FONTS", "").split(os.pathsep)


def resolve(spec):
    if spec.startswith("lm:"):
        return os.path.join(ROOT, "data", "lm", spec[3:] + ".json")
    if os.path.isabs(spec) and os.path.exists(spec):
        return spec
    for d in SEARCH:
        p = os.path.join(d, spec)
        if os.path.exists(p):
            return p
    raise FileNotFoundError(f"font {spec!r} not found in {SEARCH}")


class Registry(dict):
    """Maps a role key (rm, it, bf, ...) to a font object."""

    def __init__(self, specs):
        super().__init__()
        self.paths = {}
        cache = {}
        for key, spec in specs.items():
            if isinstance(spec, dict):
                spec, index = spec["file"], spec.get("index", 0)
            else:
                index = 0
            path = resolve(spec)
            if (path, index) not in cache:
                if path.endswith(".json"):
                    cache[(path, index)] = MetricFont(path)
                else:
                    f = Font(path, index)
                    f.kind = "ttf"
                    cache[(path, index)] = f
            self[key] = cache[(path, index)]
            self.paths[key] = path

    def glyph_fallback(self, ch, tried=()):
        """The registry key of a font shipped in data/fonts that has ch, loading it under "fallback-<name>" on
        first use; None when no shipped font has it. Fonts are tried in file-name order, so builds repeat."""
        shipped = getattr(self, "_shipped", None)
        if shipped is None:
            names = sorted(n for n in os.listdir(SHIPPED) if n.lower().endswith((".ttf", ".otf")))
            shipped = self._shipped = [(n, os.path.join(SHIPPED, n)) for n in names]
        skip = {os.path.normcase(os.path.abspath(self.paths[k])) for k in tried if k in self.paths}
        for name, path in shipped:
            if os.path.normcase(os.path.abspath(path)) in skip:
                continue
            key = "fallback-" + os.path.splitext(name)[0]
            if key not in self:
                font = Font(path, 0)
                font.kind = "ttf"
                self[key], self.paths[key] = font, path
            if self[key].glyph(ch):
                return key
        return None
