"""Resolve merge conflicts whose two sides differ only by a revision stamp.

Usage: python resolve_stamps.py <repo> <list-of-conflicted-files>
A hunk whose sides are equal after stamps are masked takes our side; the
stamper recomputes the value afterwards. Files with any other hunk are left
alone and listed.
"""
import io
import re
import sys

repo, listing = sys.argv[1], sys.argv[2]
STAMP = re.compile(r"v=world-[0-9a-f]{6,}|\?v=[0-9a-f]{8,}|world-[0-9a-f]{10}")


def mask(lines):
    return [STAMP.sub("V", ln) for ln in lines]


real = []
done = 0
for name in io.open(listing, encoding="utf8").read().split():
    path = repo + "/" + name
    text = io.open(path, encoding="utf8", newline="").read()
    lines = text.splitlines(keepends=True)
    out, i, ok = [], 0, True
    while i < len(lines):
        if lines[i].startswith("<<<<<<< "):
            a = lines.index(next(x for x in lines[i:] if x.startswith("=======")), i)
            b = lines.index(next(x for x in lines[a:] if x.startswith(">>>>>>> ")), a)
            ours, theirs = lines[i + 1 : a], lines[a + 1 : b]
            if any(x.startswith("|||||||") for x in ours):
                ok = False
                break
            if mask(ours) == mask(theirs):
                out += ours
            else:
                ok = False
                break
            i = b + 1
        else:
            out.append(lines[i])
            i += 1
    if ok:
        io.open(path, "w", encoding="utf8", newline="").write("".join(out))
        done += 1
    else:
        real.append(name)
print("resolved", done)
print("left", len(real))
for r in real:
    print(r)
