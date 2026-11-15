//! Runnable demonstration of the canonical hazard: the SSR output left bound as
//! a shader resource (t27) while it is also the active render target, so the
//! compositor would sample the very surface it is writing.
//!
//! cargo run --example ssr_hazard

use photon_frametrace::*;

fn main() {
    let mut fs = FrameState::new();
    let ssr = ResourceId(0x5232);
    let srv = ViewId(1);
    let rtv = ViewId(2);

    fs.apply_all([
        Event::RegisterView { view: srv, resource: ssr, kind: ViewKind::Srv },
        Event::RegisterView { view: rtv, resource: ssr, kind: ViewKind::Rtv },
        Event::SetRenderTargets { rtvs: vec![Some(rtv)], dsv: None },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(srv)] },
        Event::Draw,
    ]);

    println!("PS t27 bound to view: {:?}", fs.srv_at(Stage::Ps, 27));
    println!("render targets:       {:?}", fs.render_targets());

    let hz = fs.hazards();
    if hz.is_empty() {
        println!("no hazards");
    } else {
        for h in &hz {
            println!("HAZARD: {}", h);
        }
    }
}
