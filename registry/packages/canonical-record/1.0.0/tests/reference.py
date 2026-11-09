"""Reference driver: prints what `canonical-record` prints, using the original Python.

Usage: python -I reference.py PATH/TO/canonical_record.py dumps FILE.json
       python -I reference.py PATH/TO/canonical_record.py sha256 FILE.json
       python -I reference.py PATH/TO/canonical_record.py array FILE.json

PATH/TO/canonical_record.py is Flywheel's harness/canonical_record.py
(FSL-1.1-MIT). The module's own CLI writes refusals to stderr; this driver and
the BuildLang program write the same line to stdout so the goldens capture it.
`array` reads {"dtype": str, "shape": [int, ...], "values": [...]} and calls
array_sha256(dtype, shape, values). The golden files in this folder were
written by this driver from that module.
"""
import importlib.util
import json
import struct
import sys

USAGE = "usage: canonical-record {dumps|sha256|array} FILE.json"
BAD_ARRAY_FILE = 'error: array file needs {"dtype": str, "shape": [int, ...], "values": [...]}'


def load(path):
    spec = importlib.util.spec_from_file_location("canonical_record_ref", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def array_line(cr, value):
    if not isinstance(value, dict):
        return BAD_ARRAY_FILE
    dtype, shape, values = value.get("dtype"), value.get("shape"), value.get("values")
    if not isinstance(dtype, str) or not all(0x20 <= ord(c) <= 0x7E for c in dtype):
        return BAD_ARRAY_FILE
    if not isinstance(shape, list) or not all(type(d) is int for d in shape):
        return BAD_ARRAY_FILE
    if not isinstance(values, list):
        return BAD_ARRAY_FILE
    try:
        return cr.array_sha256(dtype, shape, values)
    except cr.CanonicalError:
        raise
    except (struct.error, TypeError, OverflowError):
        return f"error: values do not pack as {dtype}"


def run(cr, argv):
    if len(argv) != 2 or argv[0] not in ("dumps", "sha256", "array"):
        return USAGE, 2
    cmd, path = argv
    try:
        value = cr._strict_load(path)
        if cmd == "dumps":
            return cr.dumps(value).decode("utf-8"), 0
        if cmd == "sha256":
            return cr.sha256(value), 0
        line = array_line(cr, value)
        return line, 0 if not line.startswith("error:") else 1
    except cr.CanonicalError as exc:
        return f"no canonical form: {exc}", 1
    except RecursionError:
        return "error: nesting too deep", 1
    except OSError:
        return f"error: cannot read {path}", 1
    except UnicodeDecodeError:
        return "error: input is not UTF-8", 1
    except json.JSONDecodeError:
        return "error: invalid JSON", 1
    except ValueError as exc:
        if "Exceeds the limit" in str(exc):
            return "error: integer has more than 4300 digits", 1
        raise


def main():
    cr = load(sys.argv[1])
    line, code = run(cr, sys.argv[2:])
    sys.stdout.buffer.write(line.encode("utf-8") + b"\n")
    sys.stdout.flush()
    sys.exit(code)


if __name__ == "__main__":
    main()
