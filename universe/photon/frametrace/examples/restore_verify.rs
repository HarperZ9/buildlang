//! Demonstrate the restore-verify organ: an effect that forgets to restore PS
//! t27 leaves the game running with the effect resource bound -- host
//! corruption a single-frame capture would not flag as a RESTORE fault.
//!
//! cargo run --example restore_verify

use photon_frametrace::*;

fn main() {
    let mut fs = FrameState::new();
    fs.apply_all([
        Event::RegisterView { view: ViewId(10), resource: ResourceId(0x5232), kind: ViewKind::Srv },
        Event::RegisterView { view: ViewId(20), resource: ResourceId(0x6000), kind: ViewKind::Rtv },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(ViewId(10))] },
        Event::SetRenderTargets { rtvs: vec![Some(ViewId(20))], dsv: None },
    ]);
    let saved = fs.snapshot();

    // Effect binds its own SRV at t27, draws, restores the RTV but forgets t27.
    fs.apply_all([
        Event::RegisterView { view: ViewId(99), resource: ResourceId(0xEFFEC7), kind: ViewKind::Srv },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(ViewId(99))] },
        Event::Draw,
        Event::SetRenderTargets { rtvs: vec![Some(ViewId(20))], dsv: None },
    ]);
    let restored = fs.snapshot();

    let leaks = saved.diff_restore(&restored);
    if leaks.is_empty() {
        println!("restore transparent: no leaks");
    } else {
        println!("{} restore leak(s) -- the effect was NOT transparent:", leaks.len());
        for l in &leaks {
            println!("  RESTORE-LEAK {}", l);
        }
    }
}
