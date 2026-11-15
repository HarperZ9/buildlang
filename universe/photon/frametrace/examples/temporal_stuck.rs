//! Detect a stuck temporal ping-pong -- a failure provably low-observable to a
//! single-frame capture (it needs frame N vs N-1).
//!
//! cargo run --example temporal_stuck

use photon_frametrace::*;

fn main() {
    let mut fs = FrameState::new();
    let (a, b) = (ResourceId(0xA), ResourceId(0xB));
    fs.apply_all([
        Event::RegisterView { view: ViewId(10), resource: a, kind: ViewKind::Srv },
        Event::RegisterView { view: ViewId(11), resource: a, kind: ViewKind::Rtv },
        Event::RegisterView { view: ViewId(20), resource: b, kind: ViewKind::Srv },
        Event::RegisterView { view: ViewId(21), resource: b, kind: ViewKind::Rtv },
    ]);
    fs.declare_history_pair(a, b);

    // frame 0: warmup clears
    fs.apply_all([
        Event::ClearRenderTargetView { rtv: ViewId(11) },
        Event::ClearRenderTargetView { rtv: ViewId(21) },
        Event::Present,
    ]);
    // frame 1: read A, write B (correct)
    fs.apply_all([
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 0, views: vec![Some(ViewId(10))] },
        Event::SetRenderTargets { rtvs: vec![Some(ViewId(21))], dsv: None },
        Event::Draw,
        Event::Present,
    ]);
    // frame 2: BUG -- read A again (the ping-pong got stuck)
    fs.apply_all([
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 0, views: vec![Some(ViewId(10))] },
        Event::SetRenderTargets { rtvs: vec![Some(ViewId(21))], dsv: None },
        Event::Draw,
        Event::Present,
    ]);

    if fs.temporal_violations().is_empty() {
        println!("no temporal violations");
    } else {
        for v in fs.temporal_violations() {
            println!("TEMPORAL {}", v);
        }
    }
}
