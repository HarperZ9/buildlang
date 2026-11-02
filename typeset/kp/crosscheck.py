"""Cross-check: the BuildLang Knuth-Plass program against engine/breaker.py on real paragraphs."""
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))
from dump_items import item_lists, to_text  # noqa: E402
from engine.breaker import Params, break_lines  # noqa: E402


def run_bld(exe, text):
    with tempfile.TemporaryDirectory() as d:
        with open(os.path.join(d, "items.txt"), "w") as fh:
            fh.write(text)
        subprocess.run([exe], cwd=d, check=True, capture_output=True)
        lines = open(os.path.join(d, "breaks.txt")).read().split()
    dem = int(lines[-1])
    return [int(x) for x in lines[:-2]], dem


def main(doc, theme, exe):
    agree, total, rows = 0, 0, []
    for idx, width, items in item_lists(doc, theme):
        res = break_lines(items, width, Params(pretolerance=-1, tolerance=200))
        bld, dem = run_bld(exe, to_text(width, items, 200))
        same = bld == res.breaks and dem == res.demerits
        agree += same
        total += 1
        rows.append((idx, len(res.breaks), res.demerits, dem, same))
    for r in rows:
        print("block %3d lines %2d python %8d buildlang %8d %s" % (r[0], r[1], r[2], r[3], "same" if r[4] else "DIFF"))
    print(f"{agree}/{total} paragraphs identical (breaks and total demerits)")
    return agree, total


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
