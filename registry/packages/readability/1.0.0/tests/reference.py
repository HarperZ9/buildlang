"""Reference driver: prints what `readability FILE...` prints, using Flywheel's
original harness/writing_lint/readability.py loaded by path.

Usage: python -I tests/reference.py PATH/TO/readability.py FILE...

Per file one line: PATH: words W sentences S syllables Y ease E. W is the
number of _WORD_RE matches, S the number of _sentences, Y the sum of
syllables() over the words. E is the Flesch formula from reading_ease()
before its round(..., 1), or `none` where reading_ease() returns None.
Each file is decoded as UTF-8 with no newline translation. E is written as
BuildLang's `{}` writes an f64: the shortest round-trip digits, positional.
"""
import importlib.util
import sys
from decimal import Decimal

orig_path, files = sys.argv[1], sys.argv[2:]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("readability_original", orig_path)
rd = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rd)


def bl_float(x: float) -> str:
    return format(Decimal(repr(x)).normalize(), "f")


out = []
for path in files:
    with open(path, "rb") as f:
        text = f.read().decode("utf-8")
    words = rd._WORD_RE.findall(text)
    sents = rd._sentences(text)
    syl = sum(rd.syllables(w) for w in words)
    rounded = rd.reading_ease(text)
    if rounded is None:
        ease = "none"
    else:
        x = (206.835 - 1.015 * (len(words) / len(sents))
             - 84.6 * (syl / len(words)))
        # The unrounded value must be the one the original rounds.
        assert round(x, 1) == rounded, (path, x, rounded)
        ease = bl_float(x)
    out.append(f"{path}: words {len(words)} sentences {len(sents)} "
               f"syllables {syl} ease {ease}")
sys.stdout.buffer.write(("\n".join(out) + "\n").encode("utf-8"))
