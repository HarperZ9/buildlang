#!/usr/bin/env python3
"""Verify docs/art/receipts.json: each file SHA-256 must match its stored bytes.

Usage: python scripts/check_art_receipts.py [receipts.json]
Exit 1 on any digest mismatch, malformed receipt, or if no listed file could be checked.
A listed file that is absent from the checkout is reported and skipped: receipts also
list renders (PNGs, extra SVGs) that are never committed. Checking is on committed bytes
only, so it shows the bytes match the receipt, not that the scene produced them.
"""

import hashlib
import json
import sys
from pathlib import Path


def main():
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "docs/art/receipts.json")
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        want = {}
        for rec in data["receipts"].values():
            for name, digest in rec["outputs"].items():
                if want.setdefault(name, digest) != digest:
                    print(f"FAIL {name}: receipts disagree on its digest")
                    return 1
    except (OSError, ValueError, KeyError, AttributeError, TypeError) as exc:
        print(f"FAIL cannot read {path}: {exc!r}")
        return 1
    ok, bad, skipped = 0, 0, []
    for name, digest in sorted(want.items()):
        f = path.parent / name
        if not f.is_file():
            skipped.append(name)
            continue
        got = hashlib.sha256(f.read_bytes()).hexdigest()
        if got == digest:
            ok += 1
        else:
            bad += 1
            print(f"FAIL {name}: receipt {digest} stored bytes {got}")
    print(f"art receipts: {ok} match, {bad} mismatch, {len(skipped)} not committed")
    if ok == 0 and not bad:
        print("FAIL no listed file was checked")
        return 1
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
