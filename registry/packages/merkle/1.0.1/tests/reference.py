"""Reference driver: prints what `merkle` prints, using the original Python.

Usage: python -I reference.py PATH/TO/merkle.py table N
       python -I reference.py PATH/TO/merkle.py root FILE
       python -I reference.py PATH/TO/merkle.py prove FILE INDEX

PATH/TO/merkle.py is Flywheel's harness/merkle.py (FSL-1.1-MIT). The golden
files in this folder were written by this driver from that module.
"""
import importlib.util
import sys


def load(path):
    spec = importlib.util.spec_from_file_location("merkle_ref", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def file_leaves(path):
    data = open(path, "rb").read()
    parts = data.split(b"\n")
    if parts and parts[-1] == b"":
        parts.pop()
    return parts


def tamper_first(proof):
    if not proof:
        return proof
    first = bytes([proof[0][0] ^ 1]) + proof[0][1:]
    return [first] + proof[1:]


def flip(b):
    return bytes([b[0] ^ 1]) + b[1:]


def cons_ok(mk, m, n, old, new, proof):
    try:
        return 1 if mk.verify_consistency(m, n, old, new, proof) else 0
    except mk.MerkleError:
        return -1


def table(mk, max_n, out):
    for n in range(max_n + 1):
        leaves = [f"leaf-{i}".encode() for i in range(n)]
        root = mk.merkle_root(leaves)
        out.append(f"root {n} {root.hex()}")
        for i in range(n):
            proof = mk.inclusion_proof(leaves, i)
            ok = mk.verify_inclusion(leaves[i], i, n, proof, root)
            bad_leaf = mk.verify_inclusion(b"x", i, n, proof, root)
            bad_proof = "-"
            if proof:
                bad_proof = str(int(mk.verify_inclusion(leaves[i], i, n, tamper_first(proof), root)))
            joined = ",".join(p.hex() for p in proof) or "-"
            out.append(f"incl {n} {i} {joined} ok={int(ok)} bad_leaf={int(bad_leaf)} bad_proof={bad_proof}")
        for m in range(1, n + 1):
            old = mk.merkle_root(leaves[:m])
            proof = mk.consistency_proof(leaves, m)
            ok = cons_ok(mk, m, n, old, root, proof)
            if proof:
                bad = cons_ok(mk, m, n, old, root, tamper_first(proof))
            else:
                bad = cons_ok(mk, m, n, flip(old), root, proof)
            joined = ",".join(p.hex() for p in proof) or "-"
            out.append(f"cons {m} {n} {joined} ok={ok} bad={bad}")
        zero = cons_ok(mk, 0, n, root, root, [])
        shrink = cons_ok(mk, n + 1, n, root, root, [])
        out.append(f"refuse {n} zero={zero} shrink={shrink}")


def main():
    mk = load(sys.argv[1])
    cmd = sys.argv[2]
    out = []
    if cmd == "table":
        table(mk, int(sys.argv[3]), out)
    elif cmd == "root":
        out.append(mk.root_hex(file_leaves(sys.argv[3])))
    elif cmd == "prove":
        for p in mk.inclusion_proof(file_leaves(sys.argv[3]), int(sys.argv[4])):
            out.append(p.hex())
    sys.stdout.buffer.write(("\n".join(out) + "\n").encode())


if __name__ == "__main__":
    main()
