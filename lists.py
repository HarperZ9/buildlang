import io, re, sys
repo = sys.argv[1]
def fix(path, extra):
    s = io.open(repo + "/" + path, encoding="utf8", newline="").read()
    m = re.search(r"<<<<<<< HEAD\r?\n(.*?\r?\n)=======\r?\n.*?>>>>>>> [^\n]*\n", s, re.S)
    ours = m.group(1)
    close = ours.rstrip("\r\n")[-2:] if ours.rstrip("\r\n").endswith("];") else ours.rstrip("\r\n")[-1:]
    body = ours.rstrip("\r\n")[: -len(close)]
    new = body + "".join(', "%s"' % e for e in extra) + close + ours[len(ours.rstrip("\r\n")):]
    assert s.count("<<<<<<< ") == 1
    io.open(repo + "/" + path, "w", encoding="utf8", newline="").write(s[: m.start()] + new + s[m.end():])
    print(path, new.strip()[-90:])
fix("scripts/build-world.mjs", ["buildlang.html", "low-water.html"])
fix("scripts/world-asset-revision.mjs", ["buildlang.html", "low-water.html"])
fix("scripts/world-discovery.mjs", ["low-water.html"])
s = io.open(repo + "/sitemap.xml", encoding="utf8", newline="").read()
s2 = re.sub(r"<<<<<<< HEAD\r?\n(.*?)=======\r?\n(.*?)>>>>>>> [^\n]*\n", lambda m: m.group(1) + m.group(2), s, flags=re.S)
io.open(repo + "/sitemap.xml", "w", encoding="utf8", newline="").write(s2)
print("sitemap markers left", s2.count("<<<<<<<"))
