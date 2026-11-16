"""Take our route registry and append the routes only their side added."""
import json
import re
import subprocess
import sys

repo = sys.argv[1]
PAT = re.compile(r'ROUTE_REGISTRY_JSON = ("(?:[^"\\]|\\.)*");')


def show(stage, path):
    return subprocess.run(
        ["git", "-C", repo, "show", ":%d:%s" % (stage, path)],
        capture_output=True, encoding="utf8", check=True,
    ).stdout


for path in sys.argv[2:]:
    ours_text = show(2, path)
    base = json.loads(json.loads(PAT.search(show(1, path)).group(1)))
    ours = json.loads(json.loads(PAT.search(ours_text).group(1)))
    theirs = json.loads(json.loads(PAT.search(show(3, path)).group(1)))
    added = 0
    for fam in theirs["families"]:
        bfam = next((f for f in base["families"] if f["label"] == fam["label"]), {"routes": []})
        ofam = next(f for f in ours["families"] if f["label"] == fam["label"])
        have_base = {r["href"] for r in bfam["routes"]}
        have_ours = {r["href"] for r in ofam["routes"]}
        for r in fam["routes"]:
            if r["href"] not in have_base and r["href"] not in have_ours:
                ofam["routes"].append(r)
                added += 1
    raw = PAT.search(ours_text).group(1)
    indent = 2 if '\\n  \\"' in raw else None
    enc = json.dumps(json.dumps(ours, indent=indent, ensure_ascii=False), ensure_ascii=False)
    # Check our own encoding reproduces our side before writing the merged one.
    check = json.dumps(
        json.dumps(json.loads(json.loads(raw)), indent=indent, ensure_ascii=False), ensure_ascii=False
    )
    print(path, "added", added, "round trip exact:", check == raw)
    out = ours_text.replace(raw, enc)
    open(repo + "/" + path, "w", encoding="utf8", newline="").write(out)
