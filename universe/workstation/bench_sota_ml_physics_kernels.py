#!/usr/bin/env python3
"""Head-to-head Python reference parity and speedup benchmark for sota_ml_physics_kernels.bld.

Runs the exact same 3-stage SOTA ML & 6-DOF physics workload as
universe/workstation/sota_ml_physics_kernels.bld:
  1. 512 tokens x d=64 RMSNorm + RoPE + SwiGLU
  2. 64 chunks of tiled Flash online-softmax (128 keys x d=32, tile=16) + 256x256 BitNet b1.58 scaled ternary GEMV
  3. 128 independent 6-DOF Velocity-Verlet + Rodrigues unit-quaternion + Tsiolkovsky powered-descent trajectories

Asserts bit-exact FNV-1a hash parity across all 3 stages between Python and the
compiled BuildLang binary, and reports exact speedup ratios.
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


def synth_feat(row: int, dim: int, salt: int) -> float:
    return (((row * 37 + dim * 17 + salt * 53) % 199) - 99) * 0.012


def eval_stage1_token(tok_id: int, dim: int = 64) -> int:
    x = [synth_feat(tok_id, d, 5) for d in range(dim)]
    w = [1.0 + 0.05 * synth_feat(d, 0, 7) for d in range(dim)]
    u = [synth_feat(tok_id, d, 9) for d in range(dim)]
    sq_sum = 0.0
    for xi in x:
        sq_sum += xi * xi
    inv_rms = 1.0 / math.sqrt(sq_sum / dim + 1e-6)
    xn = [x[i] * inv_rms * w[i] for i in range(dim)]
    h = FNV_OFFSET
    for p in range(dim // 2):
        i0, i1 = 2 * p, 2 * p + 1
        theta = tok_id * math.pow(10000.0, (-2.0 * p) / dim)
        c, s = math.cos(theta), math.sin(theta)
        ra = xn[i0] * c - xn[i1] * s
        rb = xn[i0] * s + xn[i1] * c
        y0 = (ra / (1.0 + math.exp(-ra))) * u[i0]
        y1 = (rb / (1.0 + math.exp(-rb))) * u[i1]
        h = fnv_f32(h, y0)
        h = fnv_f32(h, y1)
    return h


def ternary_code_at(row: int, col: int, layer: int) -> int:
    h = fnv_u64(FNV_OFFSET, layer * 131 + row)
    h = fnv_u64(h, col)
    return h % 3


def eval_stage2_chunk(
    chunk_id: int, n_keys: int = 128, head_dim: int = 32, tile_size: int = 16, bn_cols: int = 256
) -> int:
    inv_sqrt_d = 1.0 / math.sqrt(head_dim)
    q = [synth_feat(chunk_id, d, 1) for d in range(head_dim)]
    out = [0.0] * head_dim
    m_run = -1e30
    l_run = 0.0
    t_start = 0
    while t_start < n_keys:
        t_end = min(t_start + tile_size, n_keys)
        scores = []
        m_tile = -1e30
        for k in range(t_start, t_end):
            dot = 0.0
            for j in range(head_dim):
                dot += q[j] * synth_feat(k, j, 2)
            s = dot * inv_sqrt_d + 12.0
            scores.append(s)
            if s > m_tile:
                m_tile = s
        m_new = m_run if m_run > m_tile else m_tile
        alpha = math.exp(m_run - m_new)
        for j in range(head_dim):
            out[j] *= alpha
        l_tile = 0.0
        for loc, k_id in enumerate(range(t_start, t_end)):
            w = math.exp(scores[loc] - m_new)
            l_tile += w
            for j in range(head_dim):
                out[j] += w * synth_feat(k_id, j, 3)
        l_run = l_run * alpha + l_tile
        m_run = m_new
        t_start = t_end

    inv_l = 1.0 / l_run
    h = FNV_OFFSET
    for j in range(head_dim):
        out[j] *= inv_l
        h = fnv_f32(h, out[j])

    x = [synth_feat(chunk_id, c, 11) for c in range(bn_cols)]
    for r_off in range(4):
        row = chunk_id * 4 + r_off
        acc = 0.0
        for col in range(0, bn_cols, 4):
            c0 = ternary_code_at(row, col, 0)
            c1 = ternary_code_at(row, col + 1, 0)
            c2 = ternary_code_at(row, col + 2, 0)
            c3 = ternary_code_at(row, col + 3, 0)
            for lane, code in enumerate((c0, c1, c2, c3)):
                if code == 1:
                    acc += x[col + lane]
                elif code == 2:
                    acc -= x[col + lane]
        scale = 0.75 + 0.01 * (row % 17)
        h = fnv_f32(h, acc * scale)
    return h


def quat_normalize(w: float, x: float, y: float, z: float) -> tuple[float, float, float, float]:
    n = math.sqrt(w * w + x * x + y * y + z * z)
    if n <= 1e-15:
        return (1.0, 0.0, 0.0, 0.0)
    inv = 1.0 / n
    return (w * inv, x * inv, y * inv, z * inv)


def quat_mul(
    a: tuple[float, float, float, float], b: tuple[float, float, float, float]
) -> tuple[float, float, float, float]:
    return (
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    )


def quat_rotate(q: tuple[float, float, float, float], v: tuple[float, float, float]) -> tuple[float, float, float]:
    tx = 2.0 * (q[2] * v[2] - q[3] * v[1])
    ty = 2.0 * (q[3] * v[0] - q[1] * v[2])
    tz = 2.0 * (q[1] * v[1] - q[2] * v[0])
    return (
        v[0] + q[0] * tx + (q[2] * tz - q[3] * ty),
        v[1] + q[0] * ty + (q[3] * tx - q[1] * tz),
        v[2] + q[0] * tz + (q[1] * ty - q[2] * tx),
    )


def eval_stage3_mission(traj_id: int, steps: int = 200, dt: float = 0.05) -> int:
    tid = float(traj_id)
    px, py, pz = 40.0 + 0.5 * tid, -25.0 + 0.25 * tid, 1200.0 + 2.0 * tid
    vx, vy, vz = -4.0, 2.0, -65.0
    q = quat_normalize(1.0, 0.02, -0.015, 0.005)
    ox, oy, oz = 0.04, -0.03, 0.01
    mass = 50000.0
    m_dry = 22000.0
    v_ex = 311.0 * G0
    inv_i = 1.0 / 180000.0
    max_tan = 0.466307658
    gz = -G0
    max_q_err = 0.0
    total_dv = 0.0

    for _ in range(steps):
        tx = -12000.0 * q[1] - 8000.0 * ox
        ty = -12000.0 * q[2] - 8000.0 * oy
        tz = -12000.0 * q[3] - 8000.0 * oz
        gx = min(max(-0.0015 * px - 0.02 * vx, -0.35), 0.35)
        gy = min(max(-0.0015 * py - 0.02 * vy, -0.35), 0.35)
        gh = math.sqrt(gx * gx + gy * gy)
        if gh > max_tan and gh > 1e-12:
            sc = max_tan / gh
            gx *= sc
            gy *= sc
        dir_inv = 1.0 / math.sqrt(1.0 + gx * gx + gy * gy)
        t_mag = max(mass * (G0 + 4.5), 0.0) if mass > m_dry else 0.0
        fw0 = quat_rotate(q, (t_mag * gx * dir_inv, t_mag * gy * dir_inv, t_mag * dir_inv))
        inv_m0 = 1.0 / mass
        a0x, a0y, a0z = fw0[0] * inv_m0, fw0[1] * inv_m0, fw0[2] * inv_m0 + gz
        px = px + vx * dt + 0.5 * a0x * dt * dt
        py = py + vy * dt + 0.5 * a0y * dt * dt
        pz = pz + vz * dt + 0.5 * a0z * dt * dt
        ohx = ox + 0.5 * tx * inv_i * dt
        ohy = oy + 0.5 * ty * inv_i * dt
        ohz = oz + 0.5 * tz * inv_i * dt
        wmag = math.sqrt(ohx * ohx + ohy * ohy + ohz * ohz)
        if wmag > 1e-12:
            ha = 0.5 * wmag * dt
            s = math.sin(ha) / wmag
            dq = (math.cos(ha), ohx * s, ohy * s, ohz * s)
        else:
            dq = (1.0, 0.0, 0.0, 0.0)
        q = quat_normalize(*quat_mul(q, dq))
        ox = ohx + 0.5 * tx * inv_i * dt
        oy = ohy + 0.5 * ty * inv_i * dt
        oz = ohz + 0.5 * tz * inv_i * dt
        dm = (t_mag / v_ex) * dt
        mass_next = max(mass - dm, m_dry)
        if mass_next < mass:
            total_dv += v_ex * math.log(mass / mass_next)
        t_mag1 = t_mag if mass_next > m_dry else 0.0
        fw1 = quat_rotate(q, (t_mag1 * gx * dir_inv, t_mag1 * gy * dir_inv, t_mag1 * dir_inv))
        inv_m1 = 1.0 / mass_next
        a1x, a1y, a1z = fw1[0] * inv_m1, fw1[1] * inv_m1, fw1[2] * inv_m1 + gz
        vx = vx + 0.5 * (a0x + a1x) * dt
        vy = vy + 0.5 * (a0y + a1y) * dt
        vz = vz + 0.5 * (a0z + a1z) * dt
        mass = mass_next
        q_err = abs(math.sqrt(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]) - 1.0)
        if q_err > max_q_err:
            max_q_err = q_err

    h = FNV_OFFSET
    h = fnv_f32(h, px)
    h = fnv_f32(h, pz)
    h = fnv_f32(h, vz)
    h = fnv_f32(h, q[0])
    h = fnv_f32(h, mass)
    h = fnv_f32(h, total_dv)
    return fnv_u8(h, 1 if (max_q_err < 1e-12 and mass > m_dry) else 0)


def run_buildlang(repo_root: Path) -> dict[str, object]:
    env = os.environ.copy()
    env["BUILDLANG_STDLIB"] = str(repo_root / "stdlib")
    buildc_bin = env.get("BUILDC_BIN", "buildc")
    bld = repo_root / "universe" / "workstation" / "sota_ml_physics_kernels.bld"
    proc = subprocess.run(
        [buildc_bin, "run", "--release", str(bld)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=env,
        check=True,
    )
    out = proc.stdout
    if "[PASS] sota_ml_physics_kernels summary checks=6 planted=6 failures=0" not in out:
        raise RuntimeError(f"BuildLang self-test failed:\n{out}\n{proc.stderr}")
    m = re.search(
        r"\[PASS\] par_for_parity ml=([0-9a-f]{16}) flash_bn=([0-9a-f]{16}) "
        r"phys6dof=([0-9a-f]{16}) serial_ns=(\d+) par_ns=(\d+)",
        out,
    )
    if not m:
        raise RuntimeError(f"Missing par_for_parity line:\n{out}")
    return {
        "ml": m.group(1),
        "flash_bn": m.group(2),
        "phys6dof": m.group(3),
        "serial_ns": int(m.group(4)),
        "par_ns": int(m.group(5)),
    }


def main() -> int:
    repo_root = Path(__file__).resolve().parents[2]
    bl = run_buildlang(repo_root)

    t0 = time.perf_counter_ns()
    h1 = reduce_hashes([eval_stage1_token(i) for i in range(512)])
    h2 = reduce_hashes([eval_stage2_chunk(i) for i in range(64)])
    h3 = reduce_hashes([eval_stage3_mission(i) for i in range(128)])
    py_ns = time.perf_counter_ns() - t0

    py = {
        "ml": f"{h1:016x}",
        "flash_bn": f"{h2:016x}",
        "phys6dof": f"{h3:016x}",
        "python_ns": py_ns,
    }
    parity = bl["ml"] == py["ml"] and bl["flash_bn"] == py["flash_bn"] and bl["phys6dof"] == py["phys6dof"]
    speedup_serial = py_ns / max(1, int(bl["serial_ns"]))
    speedup_par = py_ns / max(1, int(bl["par_ns"]))
    speedup_par_vs_ser = int(bl["serial_ns"]) / max(1, int(bl["par_ns"]))

    report = {
        "parity_bit_exact": parity,
        "buildlang": bl,
        "python": py,
        "speedup_serial_vs_python": round(speedup_serial, 2),
        "speedup_par_for_vs_python": round(speedup_par, 2),
        "speedup_par_vs_serial": round(speedup_par_vs_ser, 2),
    }
    print(json.dumps(report, indent=2))
    if not parity:
        print("[FAIL] sota_ml_physics_kernels parity mismatch")
        return 1
    print(
        f"[PASS] sota_ml_physics_kernels bit-exact parity: "
        f"serial={speedup_serial:.1f}x, par_for={speedup_par:.1f}x faster than Python"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
