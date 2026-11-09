#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Write a staticlib project for qdb: db.bld without main(), plus exports.bld.

  python bdb/lib/make_lib.py OUT_DIR
  buildc build --emit staticlib OUT_DIR      # OUT_DIR/target/debug/libmain.a, main.h

The library is the same source as the CLI: everything in db.bld above
`fn main()` is copied unchanged, so the two cannot drift.
"""
import os
import sys

here = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(here, "..", "db.bld"), encoding="utf-8").read()
cut = src.index("\nfn main() ~")
exports = open(os.path.join(here, "exports.bld"), encoding="utf-8").read()
out = sys.argv[1]
os.makedirs(out, exist_ok=True)
with open(os.path.join(out, "main.bld"), "w", encoding="utf-8", newline="\n") as f:
    f.write(src[:cut] + "\n\n" + exports)
print("wrote %s" % os.path.join(out, "main.bld"))
