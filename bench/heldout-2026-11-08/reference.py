"""Reference solutions for bench5 (M3 probe, 2026-11-08) in Python. They define
the expected outputs.

Usage: python reference.py            print expected.json to stdout
       python reference.py --check    exit 1 if expected.json differs

Fixture files given to every task live in FIXTURES; the grader writes them
into each task's working directory.
"""
import json
import math
import sys
from collections import Counter
from pathlib import Path

FIXTURES = {
    "scores.txt": "88\n92\n101\n75\n140\n",
    "words.txt": "lamp\nriver\nquay\nrope\n",
    "units.csv": "name,hp,atk,def\nbrute,40,9,2\nscout,18,6,1\nwarden,30,5,6\nmage,15,12,0\n",
    "map.txt": "#########\n#S..#...#\n#.#.#.#.#\n#.#...#.#\n#.#####.#\n#...#..E#\n#########\n",
    "events.txt": "pickup rope 2\npickup lamp 1\nuse rope 1\ndrop lamp 2\npickup salt 5\nuse salt 5\npickup rope 4\ndrop key 1\nuse rope 3\n",
}


def fizzsum():
    return str(sum(n for n in range(1, 1001) if (n % 3 == 0) != (n % 5 == 0)))


def primes():
    ps = [n for n in range(2, 5000) if all(n % d for d in range(2, int(n ** 0.5) + 1))]
    return f"{len(ps)} {ps[-1]}"


def gcd_lcm():
    out = []
    for a, b in [(12, 18), (17, 5), (100, 75), (270, 192)]:
        g = math.gcd(a, b)
        out.append(f"{a} {b} {g} {a * b // g}")
    return "\n".join(out)


def wordfreq():
    c = Counter("the cat and the hat and the bat sat on the mat".split())
    return "\n".join(f"{w} {n}" for w, n in sorted(c.items(), key=lambda kv: (-kv[1], kv[0])))


def reverse_words():
    ws = "one two three four five".split()
    return " ".join(reversed(ws)) + "\n" + " ".join(w[::-1] for w in ws)


def transpose():
    m = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]]
    return "\n".join(" ".join(str(m[r][c]) for r in range(3)) for c in range(4))


def bubble():
    xs = [29, 3, 17, 8, 42, 1, 15]
    swaps = 0
    for i in range(len(xs)):
        for j in range(len(xs) - 1 - i):
            if xs[j] > xs[j + 1]:
                xs[j], xs[j + 1] = xs[j + 1], xs[j]
                swaps += 1
    return " ".join(map(str, xs)) + f"\nswaps {swaps}"


def fib():
    f = [0, 1, 1]
    while len(f) <= 90:
        f.append(f[-1] + f[-2])
    return f"{f[90]}\n{sum(f[1:21])}"


def brackets():
    pairs = {")": "(", "]": "[", "}": "{"}
    out = []
    for s in ["([]{})", "([)]", "((", "{[()()]}", ""]:
        st, ok = [], True
        for ch in s:
            if ch in "([{":
                st.append(ch)
            elif not st or st.pop() != pairs[ch]:
                ok = False
                break
        out.append("yes" if ok and not st else "no")
    return "\n".join(out)


def binary():
    return "\n".join(bin(n)[2:] for n in [37, 255, 1024, 0])


def hex_parse():
    return "\n".join(str(int(h, 16)) for h in ["ff", "1A3", "0", "7fff"])


def josephus():
    people = list(range(1, 11))
    out, i = [], 0
    while people:
        i = (i + 2) % len(people)
        out.append(people.pop(i))
    return " ".join(map(str, out))


def shapes():
    shs = [("R", 3, 4), ("S", 5), ("R", 2, 10), ("S", 1)]
    out, total = [], 0
    for s in shs:
        if s[0] == "R":
            a, p = s[1] * s[2], 2 * (s[1] + s[2])
        else:
            a, p = s[1] * s[1], 4 * s[1]
        total += a
        out.append(f"{a} {p}")
    return "\n".join(out) + f"\ntotal area {total}"


def stock():
    items = {"apple": [10, 50], "pear": [4, 75], "plum": [0, 120]}
    refused = 0
    for op, name, n in [("sell", "apple", 3), ("restock", "plum", 6), ("sell", "pear", 5),
                        ("sell", "plum", 2), ("restock", "apple", 1)]:
        if op == "sell":
            if items[name][0] < n:
                refused += 1
            else:
                items[name][0] -= n
        else:
            items[name][0] += n
    out = [f"{k} {q} {q * p}" for k, (q, p) in items.items()]
    return "\n".join(out) + f"\nrefused {refused}"


def compose():
    fs = [lambda x: x * 2, lambda x: x + 3, lambda x: x * x]
    res = []
    for v in [1, 2, 3, 4, 5]:
        for f in fs:
            v = f(v)
        res.append(v)
    return " ".join(map(str, res))


def first_over():
    xs = [12, 7, 33, 51, 8, 77, 64]
    def first(t):
        for x in xs:
            if x > t:
                return x
        return None
    return "\n".join("none" if r is None else str(r) for r in [first(50), first(100)])


def parse_sum():
    import re
    total, errors = 0, 0
    for p in "12,x,7,,40,-3,5a".split(","):
        if re.fullmatch(r"-?[0-9]+", p):
            total += int(p)
        else:
            errors += 1
    return f"sum {total}\nerrors {errors}"


def score_check():
    xs = [int(l) for l in FIXTURES["scores.txt"].split()]
    for x in xs:
        if x > 100:
            return f"invalid {x}"
    return f"average {sum(xs) // len(xs)}"


def upper_file():
    return f"lines {len(FIXTURES['words.txt'].split())}"


def histogram():
    return "\n".join(f"{l} {'#' * v}" for l, v in zip("abcd", [3, 7, 2, 5]))


def prefix():
    import os
    a = os.path.commonprefix(["interstellar", "internet", "interval", "internal"])
    b = os.path.commonprefix(["dog", "cat"]) or "(none)"
    return f"{a}\n{b}"


def isbn():
    def ok(s):
        if len(s) != 10:
            return False
        tot = 0
        for i, ch in enumerate(s):
            if ch.isdigit():
                v = int(ch)
            elif ch == "X" and i == 9:
                v = 10
            else:
                return False
            tot += v * (10 - i)
        return tot % 11 == 0
    return "\n".join(f"{s} {'valid' if ok(s) else 'invalid'}"
                     for s in ["0306406152", "0306406153", "080442957X", "12345"])


def wrap():
    words = "the quick brown fox jumps over the lazy dog and keeps running far away".split()
    lines, cur = [], ""
    for w in words:
        if not cur:
            cur = w
        elif len(cur) + 1 + len(w) <= 16:
            cur += " " + w
        else:
            lines.append(cur)
            cur = w
    lines.append(cur)
    return "\n".join(lines)


def hanoi():
    moves = []
    def solve(n, a, c, b):
        if n == 0:
            return
        solve(n - 1, a, b, c)
        moves.append(f"{a} {c}")
        solve(n - 1, b, c, a)
    solve(3, "A", "C", "B")
    return f"{2 ** 10 - 1}\n" + "\n".join(moves)


def expr_tree():
    return f"{(2 + 3) * (4 - 1) + 10}\n(((2 + 3) * (4 - 1)) + 10)"


def colors():
    c = Counter("red blue red green blue red yellow".split())
    return "\n".join(f"{k} {v}" for k, v in c.items())


def projectile():
    h, v, steps, peak = 0, 1000, 0, 0
    while True:
        v -= 98
        h += v
        steps += 1
        if h < 0:
            break
        peak = max(peak, h)
    return f"landed {steps}\npeak {peak}"


def particles():
    pos = [5, 20, 50, 77, 95]
    vel = [7, -9, 13, 4, -6]
    bounces = 0
    for _ in range(50):
        for i in range(5):
            pos[i] += vel[i]
            if pos[i] < 0:
                pos[i] = -pos[i]
                vel[i] = -vel[i]
                bounces += 1
            elif pos[i] > 100:
                pos[i] = 200 - pos[i]
                vel[i] = -vel[i]
                bounces += 1
    return " ".join(map(str, pos)) + f"\nbounces {bounces}"


def units_table():
    rows = [l.split(",") for l in FIXTURES["units.csv"].strip().split("\n")[1:]]
    us = [(r[0], int(r[1]), int(r[2]), int(r[3])) for r in rows]
    out = []
    for a in us:
        best = None
        for d in us:
            if d is a:
                continue
            dmg = max(1, a[2] - d[3])
            hits = -(-d[1] // dmg)
            if best is None or hits < best[1] or (hits == best[1] and d[0] < best[0]):
                best = (d[0], hits)
        out.append(f"{a[0]} {best[0]} {best[1]}")
    return "\n".join(out)


def _duel():
    f = {"A": [30, 7, 3, 3], "B": [40, 5, 2, 2]}
    lines, t = [], 0
    while True:
        t += 1
        hits = []
        for k in "AB":
            f[k][3] -= 1
            if f[k][3] == 0:
                hits.append(k)
                f[k][3] = f[k][2]
        for k in hits:
            o = "B" if k == "A" else "A"
            f[o][0] -= f[k][1]
            lines.append(f'{{"t":{t},"by":"{k}","hp":{f[o][0]}}}')
        da, db = f["A"][0] <= 0, f["B"][0] <= 0
        if da or db:
            w = "draw" if da and db else ("B" if da else "A")
            return lines, f"winner {w} tick {t}"


def duel_trace():
    return _duel()[1]


def dice():
    x, counts = 42, [0] * 6
    for _ in range(1000):
        x = (1103515245 * x + 12345) % 2147483648
        counts[(x // 65536) % 6] += 1
    return "\n".join(f"{i + 1} {c}" for i, c in enumerate(counts))


def grid_path():
    from collections import deque
    g = FIXTURES["map.txt"].strip().split("\n")
    for r, row in enumerate(g):
        if "S" in row:
            s = (r, row.index("S"))
    dist = {s: 0}
    q = deque([s])
    while q:
        r, c = q.popleft()
        if g[r][c] == "E":
            return str(dist[(r, c)])
        for dr, dc in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            nr, nc = r + dr, c + dc
            if 0 <= nr < len(g) and 0 <= nc < len(g[nr]) and g[nr][nc] != "#" and (nr, nc) not in dist:
                dist[(nr, nc)] = dist[(r, c)] + 1
                q.append((nr, nc))
    return "unreachable"


def life():
    live = {(0, 1), (1, 2), (2, 0), (2, 1), (2, 2)}
    for _ in range(4):
        nxt = set()
        for r in range(8):
            for c in range(8):
                n = sum((r + dr, c + dc) in live for dr in (-1, 0, 1) for dc in (-1, 0, 1) if dr or dc)
                if n == 3 or (n == 2 and (r, c) in live):
                    nxt.add((r, c))
        live = nxt
    rows = ["".join("#" if (r, c) in live else "." for c in range(8)) for r in range(8)]
    return "\n".join(rows) + f"\nalive {len(live)}"


def inventory_log():
    inv, refused = {}, 0
    for line in FIXTURES["events.txt"].strip().split("\n"):
        act, item, n = line.split()
        n = int(n)
        if act == "pickup":
            inv[item] = inv.get(item, 0) + n
        elif inv.get(item, 0) < n:
            refused += 1
        else:
            inv[item] = inv.get(item, 0) - n
    out = [f"{k} {v}" for k, v in sorted(inv.items()) if v > 0]
    return "\n".join(out) + f"\nrefused {refused}"


TASKS = [fizzsum, primes, gcd_lcm, wordfreq, reverse_words, transpose, bubble, fib, brackets,
         binary, hex_parse, josephus, shapes, stock, compose, first_over, parse_sum, score_check,
         upper_file, histogram, prefix, isbn, wrap, hanoi, expr_tree, colors,
         projectile, particles, units_table, duel_trace, dice, grid_path, life, inventory_log]
GAME = ["projectile", "particles", "units_table", "duel_trace", "dice", "grid_path", "life",
        "inventory_log"]
EXIT = {"score_check": 3}
FILES_OUT = {
    "upper_file": {"upper.txt": "\n".join(w.upper() for w in FIXTURES["words.txt"].split())},
    "duel_trace": {"duel.trace": "\n".join(_duel()[0])},
}


def expected():
    return {
        f.__name__: {"stdout": f(), "exit": EXIT.get(f.__name__, 0), "files": FILES_OUT.get(f.__name__, {})}
        for f in TASKS
    }


if __name__ == "__main__":
    exp = expected()
    if "--check" in sys.argv:
        on_disk = json.loads((Path(__file__).parent / "expected.json").read_text(encoding="utf-8"))
        sys.exit(0 if on_disk == exp else 1)
    print(json.dumps(exp, indent=1, sort_keys=False))
