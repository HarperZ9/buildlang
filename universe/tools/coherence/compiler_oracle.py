"""Compiler organ of the coherence membrane: adjudicate a .bld module against
the actual compiler (buildc) and the actual C compiler (cl) -- not against
reasoning about the lowering (the doctrine section 10 sensor: read the generated
C, do not reason about it). Two invariants, most-expensive-last:
  transpile           -- buildc emits C (exit 0)
  codegen well-formed -- the EMITTED C compiles (cl /c), catching the emergent
                         "type-checks but emits invalid C" failure (e.g. return Self;).
Verdict mirrors the GPU organ ft_adjudicate and the Build organ freshness:
  CONFIRMED (0) / CONTRADICTED (1, witness) / UNRESOLVABLE (2, toolchain absent).

  python tools/coherence/compiler_oracle.py adjudicate <module.bld|component>
  python tools/coherence/compiler_oracle.py adjudicate-all
"""
import sys, os, glob, subprocess, tempfile, tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
MANIFEST = os.path.join(REPO, "tools", "components.toml")
CRLF = chr(13) + chr(10)


def find_buildc():
    for sub in ("debug", "release"):
        p = os.path.join(REPO, "buildlang", "compiler", "target", sub, "buildc.exe")
        if os.path.isfile(p):
            return p
    return None


def find_vcvars():
    vswhere = os.path.join("C:" + os.sep, "Program Files (x86)", "Microsoft Visual Studio",
                           "Installer", "vswhere.exe")
    if not os.path.isfile(vswhere):
        return None
    try:
        vs = subprocess.check_output([vswhere, "-latest", "-property", "installationPath"],
                                     text=True).strip()
    except Exception:
        return None
    vcvars = os.path.join(vs, "VC", "Auxiliary", "Build", "vcvars64.bat")
    return vcvars if os.path.isfile(vcvars) else None


def first_error(text):
    for line in text.splitlines():
        if "error" in line.lower():
            return line.strip()[:200]
    nonempty = [l for l in text.splitlines() if l.strip()]
    return nonempty[-1].strip()[:200] if nonempty else "(no output)"


def transpile(buildc, src, out_c):
    r = subprocess.run([buildc, src, "--target", "c", "-o", out_c],
                       capture_output=True, text=True)
    return r.returncode, (r.stdout + r.stderr)


def compile_c(vcvars, c_file):
    workdir = os.path.dirname(c_file)
    bat = CRLF.join(["@echo off",
                     'call "' + vcvars + '" >nul',
                     'cl /c /nologo "' + c_file + '"']) + CRLF
    batpath = os.path.join(workdir, "coc.bat")
    open(batpath, "w", encoding="utf-8", newline="").write(bat)
    r = subprocess.run(["cmd", "/c", batpath], capture_output=True, text=True, cwd=workdir)
    return r.returncode, (r.stdout + r.stderr)


def module_path(name, comps):
    if os.path.isfile(name):
        return name
    if name in comps:
        return os.path.join(REPO, comps[name]["path"].replace("/", os.sep), "lib.bld")
    return None


def adjudicate(name, comps):
    buildc = find_buildc()
    if not buildc:
        return 2, "UNRESOLVABLE no buildc built"
    src = module_path(name, comps)
    if not src or not os.path.isfile(src):
        return 2, "UNRESOLVABLE module source not found: " + str(name)
    tmp = tempfile.mkdtemp(prefix="coc_")
    out_c = os.path.join(tmp, "module.c")
    rc, out = transpile(buildc, src, out_c)
    if rc != 0:
        return 1, "CONTRADICTED transpile fails: " + first_error(out)
    lines = sum(1 for _ in open(out_c, encoding="utf-8", errors="replace")) if os.path.isfile(out_c) else 0
    vcvars = find_vcvars()
    if not vcvars:
        return 0, "CONFIRMED transpiles to " + str(lines) + " lines C (no C compiler; codegen unchecked)"
    crc, cout = compile_c(vcvars, out_c)
    if crc != 0:
        return 1, "CONTRADICTED emitted C fails to compile: " + first_error(cout)
    return 0, "CONFIRMED transpiles to " + str(lines) + " lines C, compiles clean (cl /c)"


def components():
    with open(MANIFEST, "rb") as f:
        return {c["name"]: c for c in tomllib.load(f).get("component", [])}


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: compiler_oracle.py adjudicate <module|component> | adjudicate-all")
    comps = components()
    cmd = sys.argv[1]
    if cmd == "adjudicate":
        code, w = adjudicate(sys.argv[2], comps)
        print(w)
        sys.exit(code)
    if cmd == "adjudicate-all":
        worst = 0
        for name, c in comps.items():
            if c.get("language") != "build":
                continue
            code, w = adjudicate(name, comps)
            print(name.ljust(16), w)
            if code == 1:
                worst = 1
        sys.exit(worst)
    sys.exit("unknown command: " + cmd)


if __name__ == "__main__":
    main()
