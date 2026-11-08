"""Reference driver: prints what `sha256 FILE...` prints, using hashlib.

Usage: python -I reference.py FILE...
"""
import hashlib
import sys

out = []
for path in sys.argv[1:]:
    with open(path, "rb") as f:
        out.append(f"{hashlib.sha256(f.read()).hexdigest()}  {path}")
sys.stdout.buffer.write(("\n".join(out) + "\n").encode())
