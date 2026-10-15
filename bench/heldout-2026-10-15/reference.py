"""Reference solutions for bench4 in Python. They define the expected outputs.

Usage: python reference.py            print expected.json to stdout
       python reference.py --check    exit 1 if expected.json differs

Fixture files given to every task live in FIXTURES; the grader writes them
into each task's working directory.
"""
import json
import sys
from pathlib import Path

FIXTURES = {
    "app.conf": "# server settings\nhost = example.local\nport=8080\n\n# limits\nmax_users =  25\ntimeout= 30\n",
    "sales.csv": "item,qty,price\nwidget,4,250\ngadget,10,99\ndoohickey,1,1200\ngizmo,3,410\n",
}


def collatz():
    def steps(n):
        s = 0
        while n != 1:
            n = n // 2 if n % 2 == 0 else 3 * n + 1
            s += 1
        return s
    best = max(range(1, 31), key=lambda n: (steps(n), -n))
    return f"{best} {steps(best)}"


def caesar():
    def shift(t, k):
        return "".join(chr((ord(c) - 97 + k) % 26 + 97) if "a" <= c <= "z" else c for c in t)
    enc = shift("attack at dawn zebra", 3)
    return enc + "\n" + shift(enc, -3)


def ledger():
    bal = {1: 100, 2: 50, 3: 0}
    rej = 0
    for t in [("D", 3, 25), ("W", 2, 80), ("T", 1, 3, 60), ("W", 1, 50), ("T", 2, 1, 50), ("D", 2, 5)]:
        if t[0] == "D":
            bal[t[1]] += t[2]
        elif t[0] == "W":
            if bal[t[1]] < t[2]:
                rej += 1
            else:
                bal[t[1]] -= t[2]
        else:
            if bal[t[1]] < t[3]:
                rej += 1
            else:
                bal[t[1]] -= t[3]
                bal[t[2]] += t[3]
    return "\n".join(f"{i} {bal[i]}" for i in (1, 2, 3)) + f"\nrejected {rej}"


def matmul():
    a = [[1, 2, 3], [4, 5, 6]]
    b = [[7, 8], [9, 10], [11, 12]]
    rows = [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(2)] for i in range(2)]
    return "\n".join(" ".join(map(str, r)) for r in rows)


def roman():
    vals = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"),
            (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")]
    out = []
    for n in (1994, 2024, 3999, 4, 9, 40):
        s = ""
        for v, r in vals:
            while n >= v:
                s += r
                n -= v
        out.append(s)
    return "\n".join(out)


def bsearch():
    xs = [2, 5, 8, 12, 16, 23, 38, 56, 72, 91]
    return "\n".join(str(xs.index(t) if t in xs else -1) for t in (23, 2, 91, 7, 100))


def palindromes():
    return "\n".join(f"{w} {'yes' if w == w[::-1] else 'no'}" for w in "racecar level buildlang noon ab a".split())


def rle():
    def enc(s):
        out, i = "", 0
        while i < len(s):
            j = i
            while j < len(s) and s[j] == s[i]:
                j += 1
            out += f"{s[i]}{j - i}"
            i = j
        return out
    return enc("aaabccddddde") + "\n" + enc("z")


def rpn():
    def ev(e):
        st = []
        for tok in e.split(" "):
            if tok in "+-*":
                b, a = st.pop(), st.pop()
                st.append(a + b if tok == "+" else a - b if tok == "-" else a * b)
            else:
                st.append(int(tok))
        return st[0]
    return f"{ev('3 4 + 2 * 7 -')}\n{ev('5 1 2 + 4 * + 3 -')}"


def config():
    kv = []
    for line in FIXTURES["app.conf"].split("\n"):
        if not line.strip() or line.startswith("#"):
            continue
        k, v = line.split("=", 1)
        kv.append((k.strip(), v.strip()))
    d = dict(kv)
    return "\n".join(f"{k}: {v}" for k, v in kv) + f"\ntotal {int(d['port']) + int(d['timeout'])}"


def csv():
    rows = [l.split(",") for l in FIXTURES["sales.csv"].strip().split("\n")[1:]]
    rev = [(r[0], int(r[1]) * int(r[2])) for r in rows]
    top = max(rev, key=lambda x: x[1])
    return f"total {sum(r for _, r in rev)}\ntop {top[0]} {top[1]}"


def squares_file():
    sq = [i * i for i in range(1, 7)]
    return f"lines {len(sq)}\nlast {sq[-1]}"


def ring():
    buf = []
    for v in (1, 2, 3, 4, 5):
        buf.append(v)
        if len(buf) > 3:
            buf.pop(0)
    p = buf.pop(0)
    buf.append(6)
    return f"popped {p}\n" + " ".join(map(str, buf))


def bst():
    tree = None

    def ins(t, v):
        if t is None:
            return [v, None, None]
        if v < t[0]:
            t[1] = ins(t[1], v)
        else:
            t[2] = ins(t[2], v)
        return t

    def inorder(t):
        return [] if t is None else inorder(t[1]) + [t[0]] + inorder(t[2])

    def height(t):
        return 0 if t is None else 1 + max(height(t[1]), height(t[2]))

    for v in (50, 30, 70, 20, 40, 60, 80, 35):
        tree = ins(tree, v)
    return " ".join(map(str, inorder(tree))) + f"\nheight {height(tree)}"


def traffic():
    cyc = [("Red", 30), ("Green", 25), ("Yellow", 5)]
    states = [cyc[i % 3] for i in range(5)]
    return "\n".join(f"{s} {d}" for s, d in states) + f"\ntotal {sum(d for _, d in states)}"


def bits():
    return f"{2026:b}\n{bin(2026).count('1')}"


def leap():
    return str(sum(1 for y in range(1900, 2101) if (y % 4 == 0 and y % 100 != 0) or y % 400 == 0))


def digits():
    n = 1
    for _ in range(20):
        n *= 2
    s = sum(map(int, str(n)))
    r = n
    while r >= 10:
        r = sum(map(int, str(r)))
    return f"{n}\n{s}\n{r}"


def anagram():
    pairs = [("listen", "silent"), ("hello", "world"), ("evil", "vile"), ("aab", "abb")]
    return "\n".join(f"{a} {b} {'yes' if sorted(a) == sorted(b) else 'no'}" for a, b in pairs)


def pascal():
    rows, row = [], [1]
    for _ in range(6):
        rows.append(" ".join(map(str, row)))
        row = [1] + [row[i] + row[i + 1] for i in range(len(row) - 1)] + [1]
    return "\n".join(rows)


def inventory():
    stock = {"apple": (120, 3), "pear": (95, 0), "plum": (40, 12)}
    out = []
    for q in ("plum", "kiwi", "pear"):
        if q not in stock:
            out.append(f"{q} missing")
        elif stock[q][1] == 0:
            out.append(f"{q} out of stock")
        else:
            out.append(f"{q} {stock[q][0]} x{stock[q][1]}")
    return "\n".join(out)


def parse_fields():
    total = bad = 0
    for f in "12,x,7,30,-4,9z".split(","):
        body = f[1:] if f.startswith("-") else f
        if body.isdigit():
            total += int(f)
        else:
            bad += 1
    return f"sum {total}\ninvalid {bad}"


def higher_order():
    xs = range(1, 11)
    ev = sum(x * x for x in xs if x % 2 == 0)
    prod = 1
    for x in xs:
        if x % 2 == 1:
            prod *= x
    return f"{ev}\n{prod}"


def exit_status():
    xs = [3, 7, 7, 12, 5]
    for i in range(1, len(xs)):
        if xs[i] < xs[i - 1]:
            return f"unsorted at index {i}"
    return "sorted"


def stats():
    xs = [2, 3, 5, 7, 11, 13]
    return f"mean {sum(xs) / len(xs):.2f}\nmin {min(xs)} max {max(xs)}"


def stairs():
    return "\n".join(" " * (4 - k) + "#" * k for k in range(1, 5))


def largest():
    return f"{max([3, 9, 4])}\n{max(['pear', 'apple', 'plum'])}"


TASKS = [collatz, caesar, ledger, matmul, roman, bsearch, palindromes, rle, rpn, config, csv,
         squares_file, ring, bst, traffic, bits, leap, digits, anagram, pascal, inventory,
         parse_fields, higher_order, exit_status, stats, stairs, largest]
EXIT = {"exit_status": 3}
FILES_OUT = {"squares_file": {"squares.txt": "1\n4\n9\n16\n25\n36"}}


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
