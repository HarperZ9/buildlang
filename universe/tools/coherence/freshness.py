"""Build-state coherence adjudicator -- the Build/Compiler organ of the coherence
membrane. Mirrors the GPU organ adjudicate: the claim "the verified artifact
reflects current source" is checked against a witness, returning CONFIRMED /
CONTRADICTED(stale) / UNRESOLVABLE(unrecorded), with codes 0/1/2 matching the C
ABI ft_adjudicate.

The witness is a sha256 over source CONTENT, independent of any build tool mtime
fingerprint -- so it catches the exact failure where an IO layer preserves mtimes
and cargo replays a stale binary while reporting "up to date". It turns "did my
edit take effect" into a hash compare instead of an assertion.

  python tools/coherence/freshness.py record <component>
  python tools/coherence/freshness.py adjudicate <component>   # exit 0/1/2
  python tools/coherence/freshness.py adjudicate-all
"""
import sys, os, json, glob, hashlib, tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
MANIFEST = os.path.join(REPO, "tools", "components.toml")
WITNESS = os.path.join(HERE, "witness")
SEP = bytes([0])

DEFAULT_SOURCES = {
    "rust": ["src/**/*.rs", "Cargo.toml"],
    "c": ["c_demo/*.c", "include/*.h", "src/**/*.rs", "Cargo.toml"],
    "cpp": ["hook/*.cpp", "include/*.h", "src/**/*.rs", "Cargo.toml"],
    "python": ["**/*.py"],
    "build": ["lib.bld"],
}

def components():
    with open(MANIFEST, "rb") as f:
        return {c["name"]: c for c in tomllib.load(f).get("component", [])}

def source_globs(c):
    return c.get("sources") or DEFAULT_SOURCES.get(c.get("language"), ["**/*"])

def source_files(c):
    base = os.path.join(REPO, c["path"].replace("/", os.sep))
    files = []
    for g in source_globs(c):
        for p in glob.glob(os.path.join(base, g.replace("/", os.sep)), recursive=True):
            if os.path.isfile(p) and (os.sep + "target" + os.sep) not in p:
                files.append(p)
    return sorted(set(files))

def source_hash(c):
    h = hashlib.sha256()
    n = 0
    for p in source_files(c):
        rel = os.path.relpath(p, REPO).replace(os.sep, "/")
        h.update(rel.encode("utf-8")); h.update(SEP)
        with open(p, "rb") as f:
            h.update(f.read())
        h.update(SEP)
        n += 1
    return h.hexdigest(), n

def witness_path(name):
    return os.path.join(WITNESS, name + ".json")

def record(name, comps):
    digest, n = source_hash(comps[name])
    os.makedirs(WITNESS, exist_ok=True)
    json.dump({"component": name, "source_hash": digest, "file_count": n},
              open(witness_path(name), "w"), indent=2)
    return digest, n

def adjudicate(name, comps):
    digest, n = source_hash(comps[name])
    wp = witness_path(name)
    if not os.path.isfile(wp):
        return 2, "UNRESOLVABLE no recorded build (record after a known-good verify); " + str(n) + " files"
    recorded = json.load(open(wp))["source_hash"]
    if digest == recorded:
        return 0, "CONFIRMED source matches verified build (" + digest[:16] + ")"
    return 1, "CONTRADICTED source " + digest[:16] + " != verified build " + recorded[:16] + " -- artifact is STALE"

def main():
    if len(sys.argv) < 2:
        sys.exit("usage: freshness.py record|adjudicate|adjudicate-all [component]")
    comps = components()
    cmd = sys.argv[1]
    if cmd == "record":
        d, n = record(sys.argv[2], comps)
        print("recorded " + sys.argv[2] + ": " + d[:16] + " (" + str(n) + " files)")
    elif cmd == "adjudicate":
        code, w = adjudicate(sys.argv[2], comps)
        print(w)
        sys.exit(code)
    elif cmd == "adjudicate-all":
        worst = 0
        for name, c in comps.items():
            if not os.path.isdir(os.path.join(REPO, c["path"].replace("/", os.sep))):
                continue
            code, w = adjudicate(name, comps)
            print(name.ljust(20), w)
            if code == 1:
                worst = 1
        sys.exit(worst)
    else:
        sys.exit("unknown command: " + cmd)

if __name__ == "__main__":
    main()
