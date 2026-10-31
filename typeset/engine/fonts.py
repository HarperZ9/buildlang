"""Font registry: theme font specs to loaded fonts."""
import os

from .sfnt import Font
from .metricfont import MetricFont

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
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
