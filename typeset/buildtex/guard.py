"""Refuse and record any attempt to start a TeX or Lean binary during a build.

BuildTeX replaces LaTeX and Lean; it does not call them. The guard wraps
every way Python starts a process (subprocess.Popen, os.system, the
os.spawn* and os.exec* families) for the length of a build. A forbidden
program raises ForbiddenBinary; every other start is recorded, so a build
receipt can list exactly which external programs ran (normally none, or
the BuildLang compiler `buildc` for a .bld source).
"""
import contextlib
import os
import shlex
import subprocess

FORBIDDEN = frozenset({
    # TeX engines, formats, drivers and helpers
    "tex", "etex", "pdftex", "xetex", "luatex", "luahbtex", "latex", "pdflatex", "xelatex", "lualatex",
    "latexmk", "context", "mtxrun", "dvips", "dvipdfm", "dvipdfmx", "xdvipdfmx", "bibtex", "bibtex8", "biber",
    "makeindex", "xindy", "kpsewhich", "mktexpk", "mktextfm", "tlmgr", "tectonic", "pandoc",
    # Lean and its toolchain
    "lean", "lake", "elan", "leanc", "leanmake",
})


class ForbiddenBinary(RuntimeError):
    pass


def program(argv):
    """The bare program name of an argv list or command string, lower case, without .exe."""
    if isinstance(argv, (bytes, bytearray)):
        argv = argv.decode("utf-8", "replace")
    if isinstance(argv, str):
        parts = shlex.split(argv, posix=os.name != "nt") or [""]
    else:
        parts = [os.fsdecode(a) for a in argv] or [""]
    name = os.path.basename(parts[0].strip('"')).lower()
    for ext in (".exe", ".cmd", ".bat", ".com"):
        if name.endswith(ext):
            name = name[: -len(ext)]
    return name


@contextlib.contextmanager
def no_tex_or_lean():
    """Context manager yielding the list of programs started inside it."""
    started = []

    def check(argv):
        name = program(argv)
        if name in FORBIDDEN:
            raise ForbiddenBinary(f"BuildTeX does not run {name!r}; typesetting and proof checking are native")
        started.append(name)

    real_popen_init = subprocess.Popen.__init__
    real_system = os.system
    patched = {}

    def popen_init(self, args, *a, **kw):
        check(kw.get("executable") or args)
        return real_popen_init(self, args, *a, **kw)

    def system(cmd):
        check(cmd)
        return real_system(cmd)

    def wrap(fn, argv_at):
        def inner(*a, **kw):
            check(a[argv_at] if len(a) > argv_at else a[argv_at - 1])
            return fn(*a, **kw)
        return inner

    subprocess.Popen.__init__ = popen_init
    os.system = system
    for name in dir(os):
        if name.startswith("spawn") or name.startswith("exec"):
            fn = getattr(os, name)
            if callable(fn):
                patched[name] = fn
                # spawn*(mode, file, args...) names the program second; exec*(file, args...) first
                setattr(os, name, wrap(fn, 1 if name.startswith("spawn") else 0))
    try:
        yield started
    finally:
        subprocess.Popen.__init__ = real_popen_init
        os.system = real_system
        for name, fn in patched.items():
            setattr(os, name, fn)
