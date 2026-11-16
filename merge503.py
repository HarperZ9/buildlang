import io, re
R = "E:/wt/site-all/"
H = re.compile(r"<<<<<<< HEAD\r?\n(.*?)=======\r?\n(.*?)>>>>>>> [^\n]*\n", re.S)
def rw(p, fn):
    s = io.open(R + p, encoding="utf8", newline="").read()
    n = [0]
    def sub(m):
        n[0] += 1
        return fn(n[0], m.group(1), m.group(2))
    s2 = H.sub(sub, s)
    assert "<<<<<<<" not in s2
    io.open(R + p, "w", encoding="utf8", newline="").write(s2)
    print(p, n[0], "hunks")
rw(".github/workflows/ci.yml", lambda i, o, t: o.rstrip("\r\n") + " tests/findings-build.test.mjs tests/findings-charts.test.mjs tests/findings-live.test.mjs" + o[len(o.rstrip("\r\n")):])
def bw(i, o, t):
    if i == 1:
        return ("//   3. build-studies.mjs, build-cmmns.mjs, build-experiments.mjs, record-findings.mjs and build-findings.mjs (from\n"
                "//      findings/src/ and findings/data/, which scripts/import-findings.mjs fills from the workshop's papers)\n"
                + "".join(o.splitlines(keepends=True)[1:]))
    return o.replace('("world/", "studies/")', '("world/", "studies/", "findings/")')
rw("scripts/build-world.mjs", bw)
rw("scripts/world-discovery.mjs", lambda i, o, t: o.replace('"experiments/", ', '"experiments/", "findings/", '))
rw("world/site/depth.css", lambda i, o, t: o)
