"""Convert a RenderDoc capture (.rdc) to a frametrace JSON trace.

Run INSIDE RenderDoc Python environment (the qrenderdoc shell, or standalone
with the renderdoc module on PYTHONPATH). It walks every draw/dispatch action,
reads the bound D3D11 views from the pipeline state, and emits the op-tagged
event array that frametrace FrameState::replay_json consumes.

UNVERIFIED HERE: targets the documented RenderDoc Python API but has not been run
against a live capture in this environment. The high-level flow (OpenCaptureFile
-> OpenCapture -> GetRootActions -> SetFrameEvent -> GetD3D11PipelineState) is
stable; confirm the pipeline-state field names against your RenderDoc version
(Window > Python Shell: help(rd.D3D11State)). The getattr fallbacks below absorb
the common naming differences across versions.

    python renderdoc_to_frametrace.py capture.rdc > trace.json
"""
import json
import sys

try:
    import renderdoc as rd
except ImportError:
    sys.exit("run inside the RenderDoc Python environment (renderdoc not found)")

# (pipeline-state attribute, frametrace stage code)
STAGES = [
    ("vertexShader", "vs"),
    ("pixelShader", "ps"),
    ("computeShader", "cs"),
    ("geometryShader", "gs"),
    ("hullShader", "hs"),
    ("domainShader", "ds"),
]


def res_id(view):
    # The underlying resource ResourceId, across naming variants.
    for name in ("resourceResourceId", "resourceId", "resource"):
        v = getattr(view, name, None)
        if v is not None:
            return int(v)
    return 0


def open_capture(path):
    cap = rd.OpenCaptureFile()
    if cap.OpenFile(path, "", None) != rd.ResultCode.Succeeded:
        sys.exit("could not open capture: " + path)
    result, controller = cap.OpenCapture(rd.ReplayOptions(), None)
    if result != rd.ResultCode.Succeeded:
        sys.exit("could not replay capture")
    return cap, controller


def walk(actions):
    for a in actions:
        yield a
        for c in walk(a.children):
            yield c


def main(path):
    cap, controller = open_capture(path)
    events = []
    view_ids = {}
    counter = [0]

    def reg(resource_id, kind):
        # A view id must be unique per (resource, kind): the same resource used
        # as both SRV and RTV must keep two distinct registrations, or the
        # read/write hazard on it would be lost.
        rid = int(resource_id)
        if rid == 0:
            return 0
        key = (rid, kind)
        if key not in view_ids:
            counter[0] += 1
            view_ids[key] = counter[0]
            events.append({"op": "register_view", "view": counter[0],
                           "resource": rid, "kind": kind})
        return view_ids[key]

    for action in walk(controller.GetRootActions()):
        flags = action.flags
        is_dispatch = bool(flags & rd.ActionFlags.Dispatch)
        is_draw = bool(flags & rd.ActionFlags.Drawcall)
        if not (is_draw or is_dispatch):
            continue
        controller.SetFrameEvent(action.eventId, False)
        state = controller.GetD3D11PipelineState()

        for attr, code in STAGES:
            stage = getattr(state, attr, None)
            if stage is None:
                continue
            views = [reg(res_id(v), "srv") for v in getattr(stage, "srvs", [])]
            if any(views):
                events.append({"op": "set_shader_resources", "stage": code,
                               "start": 0, "views": views})

        om = getattr(state, "outputMerger", None)
        if om is not None:
            rtvs = [reg(res_id(v), "rtv") for v in getattr(om, "renderTargets", [])]
            dsv = 0
            depth = getattr(om, "depthTarget", None)
            if depth is not None and res_id(depth):
                ro = bool(getattr(om, "depthReadOnly", False))
                dsv = reg(res_id(depth), "dsv_read_only" if ro else "dsv")
            events.append({"op": "set_render_targets", "rtvs": rtvs, "dsv": dsv})
            uavs = [reg(res_id(v), "uav") for v in getattr(om, "uavs", [])]
            if any(uavs):
                events.append({"op": "set_uav", "start": 0, "views": uavs})

        events.append({"op": "dispatch" if is_dispatch else "draw"})

    controller.Shutdown()
    cap.Shutdown()
    json.dump(events, sys.stdout, indent=2)
    sys.stdout.write(chr(10))


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: renderdoc_to_frametrace.py capture.rdc > trace.json")
    main(sys.argv[1])
