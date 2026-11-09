"""Reference driver: prints what `art-receipts [ROOT | RECEIPTS.json]` prints,
by running BuildLang's original scripts/check_art_receipts.py loaded by path.

Usage: python -I tests/reference.py PATH/TO/check_art_receipts.py [ROOT | RECEIPTS.json]

A folder argument is a repository root and becomes ROOT/docs/art/receipts.json,
as in the port; the original takes only the receipts path. Output is written
with LF line ends. The exit code is the original's.
"""
import contextlib
import importlib.util
import io
import os
import sys

orig, rest = sys.argv[1], sys.argv[2:]
if rest and os.path.isdir(rest[0]):
    rest = [rest[0] + "/docs/art/receipts.json"]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("check_art_receipts_original", orig)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
sys.argv = [orig] + rest
buf = io.StringIO()
with contextlib.redirect_stdout(buf):
    code = mod.main()
sys.stdout.buffer.write(buf.getvalue().encode("utf-8"))
sys.exit(code)
