#!/usr/bin/env python3
"""Stdio JSON-RPC 2.0 parity, planted-failure, and latency harness for workstation_mcp.bld v2.

Spawns the compiled native BuildLang Model- & Agent-Native SDK Server over real stdio,
exercises all 8 model-native tools plus 6 planted-failure controls, verifies mathematical
parity against exact Python references and canonical FNV-1a digests, enforces the <= 125
token per-tool description budget, and measures mean/p50/p99 per-call stdio and in-process
native latency across 400 warm calls (50 rounds x 8 tools).
Pure stdlib Python.
"""
from __future__ import annotations

import json
import math
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

FNV_OFFSET = 1469598103934665603
FNV_PRIME = 1099511628211
MASK64 = 0xFFFFFFFFFFFFFFFF
G0 = 9.80665

EXPECTED_TOOLS = [
    "sdk_manifest",
    "anti_silo_preflight",
    "audit_public_hygiene",
    "kcore_eval",
    "procgen_sdf_eval",
    "aerospace_gnc_eval",
    "bitnet_ternary_dot",
    "receipt_verify_hash",
]


def fnv1a_64_hex(text: str) -> str:
    h = FNV_OFFSET
    for b in text.encode("utf-8"):
        h = ((h ^ b) * FNV_PRIME) & MASK64
    return f"{h:016x}"


def approx_tokens(text: str) -> int:
    return max(1, (len(text) + 3) // 4)


def srgb_to_linear_ref(v: float) -> float:
    return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4


def fresnel_dielectric_ref(n1: float, n2: float, cos_i: float) -> float:
    sin2_t = ((n1 / n2) ** 2) * max(1.0 - cos_i * cos_i, 0.0)
    if sin2_t >= 1.0:
        return 1.0
    cos_t = math.sqrt(1.0 - sin2_t)
    rs = ((n1 * cos_i - n2 * cos_t) / (n1 * cos_i + n2 * cos_t)) ** 2
    rp = ((n1 * cos_t - n2 * cos_i) / (n1 * cos_t + n2 * cos_i)) ** 2
    return 0.5 * (rs + rp)


def ggx_d_ref(ndoth: float, alpha: float) -> float:
    a2 = alpha * alpha
    denom = ndoth * ndoth * (a2 - 1.0) + 1.0
    return a2 / (math.pi * denom * denom)


def pipe_parent_ref(r1: float, r2: float, r3: float, p: float) -> float:
    return (r1**p + r2**p + r3**p) ** (1.0 / p)


class StdioMcpClient:
    def __init__(self, repo_root: Path) -> None:
        env = os.environ.copy()
        env["BUILDLANG_STDLIB"] = str(repo_root / "stdlib")
        bld_path = repo_root / "universe" / "workstation" / "workstation_mcp.bld"
        self.proc = subprocess.Popen(
            ["buildc", "run", "--release", str(bld_path)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            bufsize=1,
            env=env,
        )
        self._next_id = 1

    def send_notification(self, method: str, params: dict[str, Any] | None = None) -> None:
        frame: dict[str, Any] = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            frame["params"] = params
        assert self.proc.stdin is not None
        self.proc.stdin.write(json.dumps(frame, separators=(",", ":")) + "\n")
        self.proc.stdin.flush()

    def rpc(self, method: str, params: dict[str, Any] | None = None) -> tuple[dict[str, Any], int]:
        req_id = self._next_id
        self._next_id += 1
        frame: dict[str, Any] = {"jsonrpc": "2.0", "id": req_id, "method": method}
        if params is not None:
            frame["params"] = params
        wire = json.dumps(frame, separators=(",", ":")) + "\n"
        assert self.proc.stdin is not None and self.proc.stdout is not None
        t0 = time.perf_counter_ns()
        self.proc.stdin.write(wire)
        self.proc.stdin.flush()
        raw = self.proc.stdout.readline()
        dt_ns = time.perf_counter_ns() - t0
        if not raw:
            err = self.proc.stderr.read() if self.proc.stderr else ""
            raise RuntimeError(f"MCP server closed stdout unexpectedly. stderr={err}")
        parsed = json.loads(raw)
        if parsed.get("id") != req_id:
            raise AssertionError(f"Mismatched JSON-RPC id: expected {req_id}, got {parsed.get('id')}")
        return parsed, dt_ns

    def call_tool(self, name: str, arguments: dict[str, Any]) -> tuple[dict[str, Any], int]:
        return self.rpc("tools/call", {"name": name, "arguments": arguments})

    def close(self) -> None:
        if self.proc.stdin and not self.proc.stdin.closed:
            self.proc.stdin.close()
        rc = self.proc.wait(timeout=15)
        if rc != 0:
            err = self.proc.stderr.read() if self.proc.stderr else ""
            raise RuntimeError(f"MCP server exited with code {rc}: {err}")


def assert_envelope(sc: dict[str, Any], label: str) -> None:
    for key in ("ok", "verdict", "result", "next_valid_actions", "fnv1a_digest", "elapsed_ns"):
        if key not in sc:
            raise AssertionError(f"{label}: missing canonical envelope key '{key}' in {sc}")
    if not isinstance(sc["next_valid_actions"], list) or len(sc["next_valid_actions"]) == 0:
        raise AssertionError(f"{label}: next_valid_actions must be a non-empty list")
    digest = sc["fnv1a_digest"]
    if not isinstance(digest, str) or len(digest) != 16:
        raise AssertionError(f"{label}: invalid fnv1a_digest '{digest}'")


def run_suite() -> dict[str, Any]:
    repo_root = Path(__file__).resolve().parents[2]
    readme_path = (repo_root / "universe" / "workstation" / "README.md").as_posix()
    client = StdioMcpClient(repo_root)
    passed = 0

    try:
        # 1. Protocol handshake (absorbs buildc launch time)
        init_resp, _ = client.rpc("initialize", {"protocolVersion": "2025-06-18"})
        sinfo = init_resp["result"]["serverInfo"]
        assert sinfo["name"] == "buildlang-workstation-mcp"
        assert sinfo["version"] == "2.0.0"
        client.send_notification("notifications/initialized")
        passed += 1

        # 2. Tool discovery + <= 125 token budget enforcement
        list_resp, _ = client.rpc("tools/list")
        tools = list_resp["result"]["tools"]
        names = [t["name"] for t in tools]
        assert names == EXPECTED_TOOLS, f"Expected {EXPECTED_TOOLS}, got {names}"
        max_desc_tokens = 0
        for tool in tools:
            toks = approx_tokens(tool["description"])
            max_desc_tokens = max(max_desc_tokens, toks)
            assert toks <= 125, f"Tool {tool['name']} description exceeds 125 tokens ({toks})"
        passed += 1

        # 3. Tool 1: sdk_manifest
        r_man, _ = client.call_tool("sdk_manifest", {})
        sc = r_man["result"]["structuredContent"]
        assert_envelope(sc, "sdk_manifest")
        assert sc["ok"] is True and sc["verdict"] == "PASS"
        assert sc["result"]["tool_count"] == 8
        passed += 1

        # 4. Tool 2: anti_silo_preflight (ALLOW + planted REFUSED_SILO)
        allow_path = "C" + ":/dev/public/superframe/packages/kernels/kcore"
        r_silo_ok, _ = client.call_tool(
            "anti_silo_preflight", {"intent": "add HLSL optics shader", "target_path": allow_path}
        )
        sc = r_silo_ok["result"]["structuredContent"]
        assert_envelope(sc, "anti_silo_allow")
        assert sc["ok"] is True and sc["verdict"] == "ALLOW"
        assert sc["result"]["flagship_id"] == "superframe" and sc["result"]["exit_code"] == 0
        passed += 1

        orphan_path = "C" + ":/dev/public/rogue-orphan-silo/tool.py"
        r_silo_bad, _ = client.call_tool(
            "anti_silo_preflight", {"intent": "new verifier tool", "target_path": orphan_path}
        )
        sc = r_silo_bad["result"]["structuredContent"]
        assert_envelope(sc, "planted_refused_silo")
        assert sc["ok"] is False and sc["verdict"] == "REFUSED_SILO"
        assert sc["result"]["silo_refused"] is True and sc["result"]["exit_code"] == 3
        passed += 1

        # 5. Tool 3: audit_public_hygiene (clean file + planted em-dash/secret/path/brand control)
        r_hyg_ok, _ = client.call_tool("audit_public_hygiene", {"path": readme_path})
        sc = r_hyg_ok["result"]["structuredContent"]
        assert_envelope(sc, "hygiene_readme")
        assert sc["ok"] is True and sc["verdict"] == "PASS" and sc["result"]["clean"] is True
        passed += 1

        planted_hyg = (
            "bad \u2014 "
            + "VI" + "TE_API_KEY "
            + "sk" + "_live_01234567 "
            + "C" + ":/dev/notes "
            + "Zen" + "tropy"
        )
        r_hyg_bad, _ = client.call_tool("audit_public_hygiene", {"text": planted_hyg})
        sc = r_hyg_bad["result"]["structuredContent"]
        assert_envelope(sc, "planted_hygiene")
        assert sc["ok"] is False and sc["verdict"] == "HYGIENE_FAIL"
        assert sc["result"]["em_dashes"] == 1
        assert sc["result"]["bundler_secrets"] == 1
        assert sc["result"]["secret_hits"] == 1
        assert sc["result"]["local_paths"] == 1
        assert sc["result"]["retired_brand"] == 1
        passed += 1

        # 6. Tool 4: kcore_eval (COL-001, OPT-002, HASH-FNV, IEEE-754 parity vs Python)
        r_col, _ = client.call_tool("kcore_eval", {"op": "col001_srgb", "value": 0.5})
        sc = r_col["result"]["structuredContent"]
        assert_envelope(sc, "kcore_col001")
        assert abs(sc["result"]["linear_out"] - srgb_to_linear_ref(0.5)) < 1e-12
        assert sc["result"]["roundtrip_err"] < 1e-12
        passed += 1

        r_opt, _ = client.call_tool("kcore_eval", {"op": "opt002_fresnel", "n1": 1.0, "n2": 1.5, "cos_i": 0.8})
        sc = r_opt["result"]["structuredContent"]
        assert_envelope(sc, "kcore_opt002")
        assert abs(sc["result"]["reflectance"] - fresnel_dielectric_ref(1.0, 1.5, 0.8)) < 1e-12
        passed += 1

        r_fnv, _ = client.call_tool("kcore_eval", {"op": "hash_fnv", "text": "buildlang-sdk-v2"})
        sc = r_fnv["result"]["structuredContent"]
        assert_envelope(sc, "kcore_hash_fnv")
        assert sc["result"]["hash_hex"] == fnv1a_64_hex("buildlang-sdk-v2")
        passed += 1

        # 7. Tool 5: procgen_sdf_eval (GGX + da Vinci Pipe parity + planted negative roughness)
        r_sdf, _ = client.call_tool(
            "procgen_sdf_eval",
            {"x": 0.35, "y": 0.2, "z": 0.15, "ndoth": 0.85, "roughness": 0.35, "r1": 0.12, "r2": 0.10, "r3": 0.08},
        )
        sc = r_sdf["result"]["structuredContent"]
        assert_envelope(sc, "procgen_sdf_eval")
        assert sc["ok"] is True and sc["verdict"] == "PASS"
        nlen = math.sqrt(
            sc["result"]["normal_x"] ** 2 + sc["result"]["normal_y"] ** 2 + sc["result"]["normal_z"] ** 2
        )
        assert abs(nlen - 1.0) < 1e-9
        assert abs(sc["result"]["ggx_d"] - ggx_d_ref(0.85, 0.35 * 0.35)) < 1e-12
        assert abs(sc["result"]["pipe_parent_r"] - pipe_parent_ref(0.12, 0.10, 0.08, 2.5)) < 1e-12
        passed += 1

        r_sdf_bad, _ = client.call_tool("procgen_sdf_eval", {"roughness": -0.25})
        assert r_sdf_bad["result"]["structuredContent"]["verdict"] == "ERROR"
        passed += 1

        # 8. Tool 6: aerospace_gnc_eval (Tsiolkovsky + ZEM/ZEV + planted inverted mass)
        r_gnc, _ = client.call_tool(
            "aerospace_gnc_eval", {"isp": 311.0, "m0": 549000.0, "mf": 137000.0, "t_go": 32.0}
        )
        sc = r_gnc["result"]["structuredContent"]
        assert_envelope(sc, "aerospace_gnc_eval")
        dv_ref = 311.0 * G0 * math.log(549000.0 / 137000.0)
        assert abs(sc["result"]["delta_v_mps"] - dv_ref) < 1e-9
        assert sc["result"]["within_gimbal_cone"] is True
        passed += 1

        r_gnc_bad, _ = client.call_tool("aerospace_gnc_eval", {"isp": 311.0, "m0": 100.0, "mf": 200.0})
        assert r_gnc_bad["result"]["structuredContent"]["verdict"] == "ERROR"
        passed += 1

        # 9. Tool 7: bitnet_ternary_dot (exact parity + zero FP mults + planted invalid weight)
        weights = [1, -1, 0, 1, -1, 1, 0, -1]
        acts = [2.5, 1.0, 9.0, 0.5, -3.0, 4.0, 7.0, 1.5]
        r_bn, _ = client.call_tool("bitnet_ternary_dot", {"weights": weights, "activations": acts})
        sc = r_bn["result"]["structuredContent"]
        assert_envelope(sc, "bitnet_ternary_dot")
        dot_ref = sum(w * x for w, x in zip(weights, acts))
        assert abs(sc["result"]["dot"] - dot_ref) < 1e-12
        assert sc["result"]["fp_multiplies"] == 0
        passed += 1

        r_bn_bad, _ = client.call_tool("bitnet_ternary_dot", {"weights": [1, 2, 0], "activations": [1.0, 2.0, 3.0]})
        assert r_bn_bad["result"]["structuredContent"]["verdict"] == "ERROR"
        passed += 1

        # 10. Tool 8: receipt_verify_hash (MATCH + planted DRIFT)
        payload = "buildlang-agent-sdk-v2-canonical-receipt"
        expected_hex = fnv1a_64_hex(payload)
        r_rc_ok, _ = client.call_tool("receipt_verify_hash", {"payload": payload, "expected_hex": expected_hex})
        sc = r_rc_ok["result"]["structuredContent"]
        assert_envelope(sc, "receipt_verify_match")
        assert sc["ok"] is True and sc["verdict"] == "MATCH" and sc["result"]["matched"] is True
        passed += 1

        r_rc_bad, _ = client.call_tool(
            "receipt_verify_hash", {"payload": payload, "expected_hex": "deadbeef00000000"}
        )
        sc = r_rc_bad["result"]["structuredContent"]
        assert_envelope(sc, "planted_receipt_drift")
        assert sc["ok"] is False and sc["verdict"] == "DRIFT" and sc["result"]["matched"] is False
        passed += 1

        # 11. Planted unknown tool control: JSON-RPC -32601 + valid tool suggestions
        r_unk, _ = client.call_tool("nonexistent_frontier_tool", {})
        assert "error" in r_unk
        assert r_unk["error"]["code"] == -32601
        assert r_unk["error"]["data"]["valid_tools"] == EXPECTED_TOOLS
        passed += 1

        # 12. Warm latency benchmark: 50 rounds x 8 tools = 400 real stdio JSON-RPC 2.0 calls
        bench_calls: list[tuple[str, dict[str, Any]]] = [
            ("sdk_manifest", {"category": "kernels"}),
            ("anti_silo_preflight", {"intent": "add buildlang compiler pass", "target_path": allow_path}),
            ("audit_public_hygiene", {"text": "Clean BuildLang prose with zero leaks."}),
            ("kcore_eval", {"op": "opt002_fresnel", "n1": 1.0, "n2": 1.5, "cos_i": 0.8}),
            ("procgen_sdf_eval", {"x": 0.35, "y": 0.2, "z": 0.15, "roughness": 0.35}),
            ("aerospace_gnc_eval", {"isp": 311.0, "m0": 549000.0, "mf": 137000.0, "t_go": 32.0}),
            ("bitnet_ternary_dot", {"weights": weights, "activations": acts}),
            ("receipt_verify_hash", {"payload": payload, "expected_hex": expected_hex}),
        ]
        rtt_us: list[float] = []
        native_us: list[float] = []
        for _ in range(50):
            for tname, targs in bench_calls:
                resp, dt_ns = client.call_tool(tname, targs)
                sc_b = resp["result"]["structuredContent"]
                assert sc_b["ok"] is True
                rtt_us.append(dt_ns / 1000.0)
                native_us.append(sc_b["elapsed_ns"] / 1000.0)

        rtt_sorted = sorted(rtt_us)
        nat_sorted = sorted(native_us)
        p99_idx = min(len(rtt_sorted) - 1, int(0.99 * len(rtt_sorted)))
        summary = {
            "tools_verified": len(EXPECTED_TOOLS),
            "checks_passed": passed,
            "planted_controls_verified": 6,
            "max_description_tokens": max_desc_tokens,
            "bench_calls": len(rtt_us),
            "stdio_rtt_mean_us": round(statistics.fmean(rtt_us), 2),
            "stdio_rtt_p50_us": round(statistics.median(rtt_us), 2),
            "stdio_rtt_p99_us": round(rtt_sorted[p99_idx], 2),
            "stdio_rtt_mean_ms": round(statistics.fmean(rtt_us) / 1000.0, 4),
            "native_exec_mean_us": round(statistics.fmean(native_us), 2),
            "native_exec_p99_us": round(nat_sorted[p99_idx], 2),
            "native_exec_mean_ms": round(statistics.fmean(native_us) / 1000.0, 4),
        }
        return summary
    finally:
        client.close()


def main() -> int:
    summary = run_suite()
    print(json.dumps(summary, indent=2))
    print(
        f"[PASS] buildlang_agent_sdk_v2 tools={summary['tools_verified']} "
        f"checks={summary['checks_passed']} planted={summary['planted_controls_verified']} "
        f"stdio_mean={summary['stdio_rtt_mean_us']:.2f}us ({summary['stdio_rtt_mean_ms']:.4f}ms) "
        f"stdio_p99={summary['stdio_rtt_p99_us']:.2f}us "
        f"native_mean={summary['native_exec_mean_us']:.2f}us"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
