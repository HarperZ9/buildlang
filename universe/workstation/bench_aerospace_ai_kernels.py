#!/usr/bin/env python3
"""Head-to-head Python reference parity and speedup benchmark for aerospace_ai_kernels.bld.

Runs the exact same 3-stage workload as universe/workstation/aerospace_ai_kernels.bld:
  1. 64 orbital Velocity-Verlet+J2 + 6-DOF ZEM/ZEV powered-descent GNC missions
  2. BitNet b1.58 packed ternary GEMV (512 rows x 1024 cols x 4 layers, zero FP mults)
  3. Flash online-softmax + Factorized Context Attention (FCA, 128 queries x 256 keys x d=32)

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

FNV_OFFSET = 1469598103934665603
FNV_PRIME = 1099511628211
MASK64 = 0xFFFFFFFFFFFFFFFF
G0 = 9.80665
MU_EARTH = 398600441800000.0
RE_EARTH = 6378137.0
J2_EARTH = 0.00108262668


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


def accel_j2(px: float, py: float, pz: float) -> tuple[float, float, float]:
    r2 = px * px + py * py + pz * pz
    r = math.sqrt(r2)
    inv_r3 = 1.0 / (r2 * r)
    z2_r2 = (pz * pz) / r2
    fac = -1.5 * J2_EARTH * MU_EARTH * (RE_EARTH * RE_EARTH) / (r2 * r2 * r)
    return (
        -MU_EARTH * px * inv_r3 + fac * px * (1.0 - 5.0 * z2_r2),
        -MU_EARTH * py * inv_r3 + fac * py * (1.0 - 5.0 * z2_r2),
        -MU_EARTH * pz * inv_r3 + fac * pz * (3.0 - 5.0 * z2_r2),
    )


def propagate_verlet_j2(steps: int, dt: float) -> tuple[float, float]:
    r0 = RE_EARTH + 400000.0
    v_circ = math.sqrt(MU_EARTH / r0)
    px, py, pz = r0, 0.0, 0.0
    vx, vy, vz = 0.0, v_circ * 0.62114777, v_circ * 0.78369347
    ax, ay, az = accel_j2(px, py, pz)
    for _ in range(steps):
        px = px + vx * dt + 0.5 * ax * dt * dt
        py = py + vy * dt + 0.5 * ay * dt * dt
        pz = pz + vz * dt + 0.5 * az * dt * dt
        nax, nay, naz = accel_j2(px, py, pz)
        vx = vx + 0.5 * (ax + nax) * dt
        vy = vy + 0.5 * (ay + nay) * dt
        vz = vz + 0.5 * (az + naz) * dt
        ax, ay, az = nax, nay, naz
    return px, pz


def clamp_thrust(ax: float, ay: float, az: float) -> tuple[float, float, float]:
    az = max(az, 0.5)
    horiz = math.sqrt(ax * ax + ay * ay)
    max_h = az * 0.466307658
    if horiz > max_h and horiz > 1e-12:
        s = max_h / horiz
        ax *= s
        ay *= s
    mag = math.sqrt(ax * ax + ay * ay + az * az)
    if mag > 32.0 and mag > 1e-12:
        s = 32.0 / mag
        return ax * s, ay * s, az * s
    if mag < 2.0 and mag > 1e-12:
        s = 2.0 / mag
        return ax * s, ay * s, az * s
    return ax, ay, az


def simulate_descent(p0: tuple[float, float, float], steps: int) -> tuple[float, float, float, float]:
    t_total = 22.0
    dt = t_total / steps
    gz = -G0
    v_ex = 282.0 * G0
    px, py, pz = p0
    vx, vy, vz = -10.0, 7.0, -52.0
    mass = 26000.0
    m_dry = 20000.0
    peak_tan = 0.0
    for i in range(steps):
        t_go = max(t_total - i * dt, dt)
        zem_x = -(px + vx * t_go)
        zem_y = -(py + vy * t_go)
        zem_z = -(pz + vz * t_go + 0.5 * gz * t_go * t_go)
        zev_x, zev_y, zev_z = -vx, -vy, -(vz + gz * t_go)
        inv_t = 1.0 / t_go
        inv_t2 = inv_t * inv_t
        tx, ty, tz = clamp_thrust(
            6.0 * zem_x * inv_t2 - 2.0 * zev_x * inv_t,
            6.0 * zem_y * inv_t2 - 2.0 * zev_y * inv_t,
            6.0 * zem_z * inv_t2 - 2.0 * zev_z * inv_t,
        )
        if mass <= m_dry:
            tx = ty = tz = 0.0
        if tz > 1e-9:
            peak_tan = max(peak_tan, math.sqrt(tx * tx + ty * ty) / tz)
        az = tz + gz
        px = px + vx * dt + 0.5 * tx * dt * dt
        py = py + vy * dt + 0.5 * ty * dt * dt
        pz = pz + vz * dt + 0.5 * az * dt * dt
        vx = vx + tx * dt
        vy = vy + ty * dt
        vz = vz + az * dt
        tmag = math.sqrt(tx * tx + ty * ty + tz * tz)
        mass = max(mass - (mass * tmag / v_ex) * dt, m_dry)
    pos_err = math.sqrt(px * px + py * py + pz * pz)
    vel_err = math.sqrt(vx * vx + vy * vy + vz * vz)
    return pos_err, vel_err, (mass - m_dry) / 6000.0, peak_tan


def ternary_code(row: int, col: int, layer: int) -> int:
    h = fnv_u64(FNV_OFFSET, layer * 131 + row)
    h = fnv_u64(h, col)
    return h % 3


def synth_feat(row: int, dim: int, salt: int) -> float:
    return (((row * 37 + dim * 17 + salt * 53) % 199) - 99) * 0.012


def flash_attn_row(q_idx: int, n_keys: int, head_dim: int, tile_sz: int) -> list[float]:
    inv_sd = 1.0 / math.sqrt(head_dim)
    q = [synth_feat(q_idx, d, 1) for d in range(head_dim)]
    out = [0.0] * head_dim
    m_run = -1e30
    l_run = 0.0
    for t_start in range(0, n_keys, tile_sz):
        t_end = min(t_start + tile_sz, n_keys)
        scores = []
        m_tile = -1e30
        for k in range(t_start, t_end):
            dot = sum(q[j] * synth_feat(k, j, 2) for j in range(head_dim))
            sc = dot * inv_sd
            scores.append(sc)
            if sc > m_tile:
                m_tile = sc
        m_new = max(m_run, m_tile)
        alpha = math.exp(m_run - m_new)
        l_run *= alpha
        for j in range(head_dim):
            out[j] *= alpha
        for lk, k in enumerate(range(t_start, t_end)):
            w = math.exp(scores[lk] - m_new)
            l_run += w
            for j in range(head_dim):
                out[j] += w * synth_feat(k, j, 3)
        m_run = m_new
    inv_l = 1.0 / l_run
    return [v * inv_l for v in out]


def fca_attn_row(q_idx: int, n_keys: int, head_dim: int, block_sz: int) -> float:
    inv_sd = 1.0 / math.sqrt(head_dim)
    q = [synth_feat(q_idx, d, 1) for d in range(head_dim)]
    m_run = -1e30
    l_run = 0.0
    o0 = 0.0
    for b in range(n_keys // block_sz):
        base = b * block_sz
        dot = 0.0
        for j in range(head_dim):
            km = sum(synth_feat(base + t, j, 2) for t in range(block_sz)) / block_sz
            dot += q[j] * km
        v0m = sum(synth_feat(base + t, 0, 3) for t in range(block_sz)) / block_sz
        sc = dot * inv_sd
        m_new = max(m_run, sc)
        alpha = math.exp(m_run - m_new)
        w = math.exp(sc - m_new)
        l_run = l_run * alpha + w
        o0 = o0 * alpha + w * v0m
        m_run = m_new
    return o0 / l_run


def run_python_workload() -> dict[str, object]:
    t0 = time.perf_counter_ns()
    gnc_parts: list[int] = []
    for idx in range(64):
        px, pz = propagate_verlet_j2(2000, 1.0 + (idx % 4) * 0.25)
        off = (idx % 16) * 4.0
        pe, ve, fm, ptan = simulate_descent((100.0 + off, -70.0 + 0.5 * off, 480.0 + off), 400)
        touch_ok = 1 if (pe < 0.05 and ve < 0.1 and fm > 0.0) else 0
        h = FNV_OFFSET
        for val in (px, pz, fm, ptan):
            h = fnv_f32(h, val)
        h = fnv_u8(h, touch_ok)
        gnc_parts.append(h)
    h_gnc = reduce_hashes(gnc_parts)

    x_vec = [(((c * 29 + 13) % 101) - 50.0) * 0.02 for c in range(1024)]
    bn_parts: list[int] = []
    for row in range(512):
        h = FNV_OFFSET
        for layer in range(4):
            acc = 0.0
            for j in range(0, 1024, 4):
                bv = (
                    (ternary_code(row, j, layer) & 3)
                    | ((ternary_code(row, j + 1, layer) & 3) << 2)
                    | ((ternary_code(row, j + 2, layer) & 3) << 4)
                    | ((ternary_code(row, j + 3, layer) & 3) << 6)
                )
                for lane in range(4):
                    code = (bv >> (2 * lane)) & 3
                    if code == 1:
                        acc += x_vec[j + lane]
                    elif code == 2:
                        acc -= x_vec[j + lane]
            h = fnv_f32(h, acc)
        bn_parts.append(h)
    h_bn = reduce_hashes(bn_parts)

    at_parts: list[int] = []
    for q_idx in range(128):
        out = flash_attn_row(q_idx, 256, 32, 16)
        fca_v = fca_attn_row(q_idx, 256, 32, 16)
        h = FNV_OFFSET
        for v in out:
            h = fnv_f32(h, v)
        h = fnv_f32(h, fca_v)
        at_parts.append(h)
    h_at = reduce_hashes(at_parts)
    py_ns = time.perf_counter_ns() - t0
    return {"gnc": f"{h_gnc:x}", "bitnet": f"{h_bn:x}", "attn": f"{h_at:x}", "python_ns": py_ns}


def run_buildlang_workload(repo_root: Path, buildc: str) -> dict[str, object]:
    bld = repo_root / "universe" / "workstation" / "aerospace_ai_kernels.bld"
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
    if "[FAIL]" in out or "[PASS] aerospace_ai_kernels summary failures=0" not in out:
        raise RuntimeError(f"aerospace_ai_kernels.bld self-test failed:\n{out}\n{proc.stderr}")
    m = re.search(
        r"\[PASS\] aero_ai_par_for_bit_exact gnc=([0-9a-f]+) bitnet=([0-9a-f]+) "
        r"attn=([0-9a-f]+) serial_ns=(\d+) par_ns=(\d+)",
        out,
    )
    if not m:
        raise RuntimeError(f"missing benchmark line in aerospace_ai_kernels output:\n{out}")
    return {
        "gnc": m.group(1),
        "bitnet": m.group(2),
        "attn": m.group(3),
        "serial_ns": int(m.group(4)),
        "par_ns": int(m.group(5)),
    }


def main() -> int:
    repo_root = Path(__file__).resolve().parents[2]
    buildc = os.environ.get("BUILDC", "buildc")
    bl = run_buildlang_workload(repo_root, buildc)
    py = run_python_workload()

    parity = (
        bl["gnc"] == py["gnc"]
        and bl["bitnet"] == py["bitnet"]
        and bl["attn"] == py["attn"]
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
        f"[PASS] aerospace_ai_kernels bit-exact parity: serial={serial_x:.1f}x, "
        f"par_for={par_x:.1f}x faster than Python"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
