#!/usr/bin/env python3
"""Head-to-head Python reference parity and speedup benchmark for procgen_forge.bld.

Runs the exact same workload as universe/workstation/procgen_forge.bld:
  1. 256x256 tileable procedural PBR+spectral material field (65,536 texels)
  2. 65,536 composite SDF + finite-difference normal + GGX/Smith/Conductor evals
  3. 10,000 biomechanical Da Vinci pipe-model + meshlet bounding-sphere checks

Asserts bit-exact FNV-1a hash parity across all 3 stages between Python and the
compiled BuildLang binary, and prints exact measured speedup ratios.
Pure stdlib Python.
"""
from __future__ import annotations

import json
import math
import os
import re
import struct
import subprocess
import sys
import time
from pathlib import Path

PI = 3.141592653589793
FNV_OFFSET = 1469598103934665603
FNV_PRIME = 1099511628211
MASK64 = 0xFFFFFFFFFFFFFFFF


def fnv_u8(h0: int, b: int) -> int:
    return ((h0 ^ b) * FNV_PRIME) & MASK64


def fnv_u64(h0: int, v: int) -> int:
    h = h0
    v &= MASK64
    for k in range(8):
        h = fnv_u8(h, (v >> (8 * k)) & 255)
    return h


def fnv_f32(h0: int, v: float) -> int:
    bits = struct.unpack("<I", struct.pack("<f", v))[0]
    h = h0
    for k in range(4):
        h = fnv_u8(h, (bits >> (8 * k)) & 255)
    return h


def reduce_hashes(parts: list[int]) -> int:
    h = FNV_OFFSET
    for p in parts:
        h = fnv_u64(h, p)
    return h


def clamp(x: float, lo: float, hi: float) -> float:
    return min(max(x, lo), hi)


def v3_norm(x: float, y: float, z: float) -> tuple[float, float, float]:
    l = math.sqrt(x * x + y * y + z * z)
    if l <= 1e-15:
        return (0.0, 1.0, 0.0)
    return (x / l, y / l, z / l)


def sdf_rbox(px: float, py: float, pz: float, bx: float, by: float, bz: float, r: float) -> float:
    qx, qy, qz = abs(px) - bx, abs(py) - by, abs(pz) - bz
    ox, oy, oz = max(qx, 0.0), max(qy, 0.0), max(qz, 0.0)
    return math.sqrt(ox * ox + oy * oy + oz * oz) + min(max(qx, max(qy, qz)), 0.0) - r


def sdf_rcone(px: float, py: float, pz: float, h: float, r1: float, r2: float) -> float:
    b = (r1 - r2) / h
    a = math.sqrt(max(1.0 - b * b, 0.0))
    qx = math.sqrt(px * px + pz * pz)
    qy = py
    k = qx * (-b) + qy * a
    if k < 0.0:
        return math.sqrt(qx * qx + qy * qy) - r1
    if k > a * h:
        dy = qy - h
        return math.sqrt(qx * qx + dy * dy) - r2
    return qx * a + qy * b - r1


def sdf_torus(px: float, py: float, pz: float, maj: float, min_r: float) -> float:
    qx = math.sqrt(px * px + pz * pz) - maj
    return math.sqrt(qx * qx + py * py) - min_r


def sdf_smin_cubic(a: float, b: float, k: float) -> float:
    h = max(k - abs(a - b), 0.0) / k
    return min(a, b) - h * h * h * k / 6.0


def sdf_composite(px: float, py: float, pz: float) -> float:
    d_box = sdf_rbox(px, py, pz, 0.6, 0.5, 0.4, 0.08)
    d_tor = sdf_torus(px - 0.25, py, pz, 0.55, 0.14)
    d_cone = sdf_rcone(px, py + 0.4, pz, 0.9, 0.22, 0.09)
    u0 = sdf_smin_cubic(d_box, d_tor, 0.24)
    u1 = sdf_smin_cubic(u0, d_cone, 0.18)
    ripple = math.sin(6.0 * px) * math.cos(6.0 * py) * math.sin(6.0 * pz)
    return (u1 + 0.02 * ripple) / (1.0 + 0.02 * 10.4)


def sdf_normal(px: float, py: float, pz: float) -> tuple[float, float, float]:
    e = 0.0001
    dx = sdf_composite(px + e, py, pz) - sdf_composite(px - e, py, pz)
    dy = sdf_composite(px, py + e, pz) - sdf_composite(px, py - e, pz)
    dz = sdf_composite(px, py, pz + e) - sdf_composite(px, py, pz - e)
    return v3_norm(dx, dy, dz)


def lattice_hash_01(ix: int, iy: int, seed: int) -> float:
    h = fnv_u64(FNV_OFFSET, seed)
    h = fnv_u64(h, ix)
    h = fnv_u64(h, iy)
    return (h & 16777215) / 16777216.0


def smoothstep5(t: float) -> float:
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0)


def periodic_value_noise(u: float, v: float, period: int, seed: int) -> float:
    sx, sy = u * period, v * period
    ix, iy = math.floor(sx), math.floor(sy)
    fx, fy = smoothstep5(sx - ix), smoothstep5(sy - iy)
    x0, x1 = ix % period, (ix + 1) % period
    y0, y1 = iy % period, (iy + 1) % period
    c00 = lattice_hash_01(x0, y0, seed)
    c10 = lattice_hash_01(x1, y0, seed)
    c01 = lattice_hash_01(x0, y1, seed)
    c11 = lattice_hash_01(x1, y1, seed)
    top = c00 + fx * (c10 - c00)
    bot = c01 + fx * (c11 - c01)
    return top + fy * (bot - top)


def periodic_fbm(u: float, v: float, base_period: int, seed: int) -> float:
    total = 0.0
    amp = 0.5
    norm = 0.0
    per = base_period
    for oct_idx in range(4):
        total += amp * periodic_value_noise(u, v, per, seed + oct_idx * 31)
        norm += amp
        amp *= 0.5
        per *= 2
    return total / norm


def periodic_worley(u: float, v: float, period: int, seed: int) -> float:
    sx, sy = u * period, v * period
    cx, cy = math.floor(sx), math.floor(sy)
    min_d2 = 9.0
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            nx, ny = (cx + dx) % period, (cy + dy) % period
            px = (cx + dx) + lattice_hash_01(nx, ny, seed)
            py = (cy + dy) + lattice_hash_01(nx, ny, seed + 97)
            rx, ry = px - sx, py - sy
            d2 = rx * rx + ry * ry
            if d2 < min_d2:
                min_d2 = d2
    return math.sqrt(min_d2)


def material_height_at(u: float, v: float) -> float:
    fbm = periodic_fbm(u, v, 8, 1337)
    wor = min(periodic_worley(u, v, 8, 4242), 1.0)
    return 0.65 * fbm + 0.35 * (1.0 - wor)


def ggx_d(ndoth: float, alpha: float) -> float:
    a2 = alpha * alpha
    denom = ndoth * ndoth * (a2 - 1.0) + 1.0
    return a2 / (PI * denom * denom)


def smith_g1_ggx(ndotv: float, alpha: float) -> float:
    a2 = alpha * alpha
    return (2.0 * ndotv) / (ndotv + math.sqrt(a2 + (1.0 - a2) * ndotv * ndotv))


def fresnel_conductor(cos_i: float, eta: float, k: float) -> float:
    cos2 = cos_i * cos_i
    sin2 = max(1.0 - cos2, 0.0)
    eta2, k2 = eta * eta, k * k
    t0 = eta2 - k2 - sin2
    a2_plus_b2 = math.sqrt(t0 * t0 + 4.0 * eta2 * k2)
    t1 = a2_plus_b2 + cos2
    a = math.sqrt(max(0.5 * (a2_plus_b2 + t0), 0.0))
    t2 = 2.0 * a * cos_i
    rs = (t1 - t2) / (t1 + t2)
    t3 = cos2 * a2_plus_b2 + sin2 * sin2
    t4 = t2 * sin2
    rp = rs * (t3 - t4) / (t3 + t4)
    return 0.5 * (rs + rp)


def run_python_workload() -> dict[str, object]:
    t0 = time.perf_counter_ns()
    n = 256
    inv_n = 1.0 / n
    mat_parts: list[int] = []
    for row in range(n):
        v = row * inv_n
        vm = ((row - 1) % n) * inv_n
        vp = ((row + 1) % n) * inv_n
        h = FNV_OFFSET
        for col in range(n):
            u = col * inv_n
            um = ((col - 1) % n) * inv_n
            up = ((col + 1) % n) * inv_n
            fbm = periodic_fbm(u, v, 8, 1337)
            wor = min(periodic_worley(u, v, 8, 4242), 1.0)
            ht = 0.65 * fbm + 0.35 * (1.0 - wor)
            du = (material_height_at(up, v) - material_height_at(um, v)) * 2.0
            dv = (material_height_at(u, vp) - material_height_at(u, vm)) * 2.0
            _, _, nz = v3_norm(-du, -dv, 1.0)
            rough = clamp(0.18 + 0.58 * wor + 0.14 * (1.0 - fbm), 0.04, 0.98)
            cav = clamp(1.0 - 0.75 * abs(ht - 0.5), 0.15, 1.0)
            h = fnv_f32(h, ht)
            h = fnv_f32(h, nz)
            h = fnv_f32(h, rough)
            h = fnv_f32(h, cav)
        mat_parts.append(h)
    h_mat = reduce_hashes(mat_parts)

    sdf_parts: list[int] = []
    total = 64 * 1024
    for cid in range(64):
        base = cid * 1024
        h = FNV_OFFSET
        for j in range(1024):
            idx = base + j
            t = (idx + 0.5) / total
            px = -1.2 + 2.4 * t
            py = -0.8 + 1.6 * ((idx * 73) % 1024) / 1024.0
            pz = -0.9 + 1.8 * ((idx * 197) % 1024) / 1024.0
            d = sdf_composite(px, py, pz)
            _, ny, _ = sdf_normal(px, py, pz)
            cos_i = clamp(0.05 + 0.95 * t, 0.05, 1.0)
            alpha = 0.08 + 0.72 * t
            gd = ggx_d(cos_i, alpha)
            g2 = smith_g1_ggx(cos_i, alpha) * smith_g1_ggx(max(0.1, abs(ny)), alpha)
            f_au = fresnel_conductor(cos_i, 0.166, 3.15)
            h = fnv_f32(h, d)
            h = fnv_f32(h, ny)
            h = fnv_f32(h, gd)
            h = fnv_f32(h, g2)
            h = fnv_f32(h, f_au)
        sdf_parts.append(h)
    h_sdf = reduce_hashes(sdf_parts)

    flo_parts: list[int] = []
    for cid in range(50):
        base = cid * 200
        h = FNV_OFFSET
        for j in range(200):
            idx = base + j
            s = (idx % 100) * 0.004
            r1, r2, r3 = 0.12 + s, 0.09 + 0.7 * s, 0.07 + 0.5 * s
            rp = (r1 ** 2.5 + r2 ** 2.5 + r3 ** 2.5) ** (1.0 / 2.5)
            rad = rp * 2.0
            inside_ok = 1
            for k in range(16):
                ang = k * 0.39269908169872414
                vx = 0.85 * rad * math.cos(ang)
                vy = 0.25 * rad * math.sin(ang * 2.0)
                vz = 0.40 * rad * math.sin(ang)
                if math.sqrt(vx * vx + vy * vy + vz * vz) > rad + 1e-9:
                    inside_ok = 0
            h = fnv_f32(h, rp)
            h = fnv_u8(h, inside_ok)
        flo_parts.append(h)
    h_flo = reduce_hashes(flo_parts)
    py_ns = time.perf_counter_ns() - t0
    return {
        "mat": f"{h_mat:x}",
        "sdf_ggx": f"{h_sdf:x}",
        "flora": f"{h_flo:x}",
        "python_ns": py_ns,
    }


def run_buildlang_workload(repo_root: Path, buildc: str) -> dict[str, object]:
    bld = repo_root / "universe" / "workstation" / "procgen_forge.bld"
    env = os.environ.copy()
    env["BUILDLANG_STDLIB"] = str(repo_root / "stdlib")
    proc = subprocess.run(
        [buildc, "run", "--release", str(bld)],
        cwd=str(repo_root),
        env=env,
        capture_output=True,
        text=True,
        check=True,
    )
    out = proc.stdout
    if "[FAIL]" in out or "[PASS] procgen_forge summary failures=0" not in out:
        raise RuntimeError(f"procgen_forge.bld self-test failed:\n{out}\n{proc.stderr}")
    m = re.search(
        r"\[PASS\] procgen_par_for_bit_exact mat=([0-9a-f]+) sdf_ggx=([0-9a-f]+) "
        r"flora=([0-9a-f]+) serial_ns=(\d+) par_ns=(\d+)",
        out,
    )
    if not m:
        raise RuntimeError(f"missing benchmark line in procgen_forge output:\n{out}")
    return {
        "mat": m.group(1),
        "sdf_ggx": m.group(2),
        "flora": m.group(3),
        "serial_ns": int(m.group(4)),
        "par_ns": int(m.group(5)),
    }


def main() -> int:
    repo_root = Path(__file__).resolve().parents[2]
    buildc = os.environ.get("BUILDC", "buildc")
    bl = run_buildlang_workload(repo_root, buildc)
    py = run_python_workload()

    parity = (
        bl["mat"] == py["mat"]
        and bl["sdf_ggx"] == py["sdf_ggx"]
        and bl["flora"] == py["flora"]
    )
    serial_x = py["python_ns"] / max(int(bl["serial_ns"]), 1)
    par_x = py["python_ns"] / max(int(bl["par_ns"]), 1)
    report = {
        "parity_bit_exact": parity,
        "buildlang": bl,
        "python": py,
        "speedup_serial_vs_python": round(serial_x, 2),
        "speedup_par_for_vs_python": round(par_x, 2),
        "speedup_par_vs_serial": round(int(bl["serial_ns"]) / max(int(bl["par_ns"]), 1), 2),
    }
    print(json.dumps(report, indent=2))
    if not parity:
        print("[FAIL] hash mismatch between BuildLang and Python reference", file=sys.stderr)
        return 1
    print(
        f"[PASS] procgen_forge bit-exact parity: serial={serial_x:.1f}x, "
        f"par_for={par_x:.1f}x faster than Python"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
