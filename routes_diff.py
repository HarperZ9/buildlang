"""Show how the route registry differs between base, ours and theirs."""
import json
import re
import subprocess
import sys

repo, path = sys.argv[1], sys.argv[2]


def load(stage):
    text = subprocess.run(
        ["git", "-C", repo, "show", ":%d:%s" % (stage, path)],
        capture_output=True, encoding="utf8", check=True,
    ).stdout
    m = re.search(r'ROUTE_REGISTRY_JSON = ("(?:[^"\\]|\\.)*");', text)
    return json.loads(json.loads(m.group(1))), text


def flat(reg):
    out = {}
    for k, v in reg.items():
        if k != "families":
            out[("top", k)] = v
    for fam in reg["families"]:
        for k, v in fam.items():
            if k != "routes":
                out[("fam", fam["label"], k)] = v
        for i, r in enumerate(fam["routes"]):
            out[("route", fam["label"], r.get("href"))] = (i, r)
    return out


base, _ = load(1)
ours, _ = load(2)
theirs, _ = load(3)
fb, fo, ft = flat(base), flat(ours), flat(theirs)
for name, side in (("ours", fo), ("theirs", ft)):
    print("==", name)
    for k in sorted(set(fb) | set(side), key=str):
        if fb.get(k) != side.get(k):
            a, b = fb.get(k), side.get(k)
            print(" ", k, "| base:", json.dumps(a)[:150], "| now:", json.dumps(b)[:230])
