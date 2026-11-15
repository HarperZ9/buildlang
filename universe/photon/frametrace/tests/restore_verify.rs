//! Restore-verify tests (Tier-1, the frametrace<->RAW seam): snapshot the bind
//! state, run an effect, restore, diff. A leaked slot is host corruption -- the
//! game runs with state it never set. Each test is a ground-truth oracle.

use photon_frametrace::*;

// Game binds PS t27 (res 100), OM rtv0 (res 200), dsv (res 300).
fn game_state() -> (FrameState, ViewId, ViewId, ViewId) {
    let mut fs = FrameState::new();
    let g_srv = ViewId(10);
    let rt = ViewId(20);
    let dv = ViewId(30);
    fs.apply_all([
        Event::RegisterView { view: g_srv, resource: ResourceId(100), kind: ViewKind::Srv },
        Event::RegisterView { view: rt, resource: ResourceId(200), kind: ViewKind::Rtv },
        Event::RegisterView { view: dv, resource: ResourceId(300), kind: ViewKind::Dsv },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(g_srv)] },
        Event::SetRenderTargets { rtvs: vec![Some(rt)], dsv: Some(dv) },
    ]);
    (fs, g_srv, rt, dv)
}

#[test]
fn incomplete_restore_leaks_the_slot() {
    let (mut fs, _g, rt, dv) = game_state();
    let saved = fs.snapshot();
    let e_srv = ViewId(99);
    fs.apply_all([
        Event::RegisterView { view: e_srv, resource: ResourceId(999), kind: ViewKind::Srv },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(e_srv)] },
        Event::Draw,
        // restore RT + DSV but FORGET PS t27 (the bug)
        Event::SetRenderTargets { rtvs: vec![Some(rt)], dsv: Some(dv) },
    ]);
    let restored = fs.snapshot();
    let leaks = saved.diff_restore(&restored);
    assert_eq!(leaks.len(), 1, "{:?}", leaks);
    assert_eq!(leaks[0].at, BindPoint::Srv { stage: Stage::Ps, slot: 27 });
    assert_eq!(leaks[0].saved_resource, Some(ResourceId(100)));
    assert_eq!(leaks[0].restored_resource, Some(ResourceId(999)));
}

#[test]
fn restore_to_null_leaks() {
    let (mut fs, _g, rt, dv) = game_state();
    let saved = fs.snapshot();
    fs.apply_all([
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![None] },
        Event::Draw,
        Event::SetRenderTargets { rtvs: vec![Some(rt)], dsv: Some(dv) },
    ]);
    let restored = fs.snapshot();
    let leaks = saved.diff_restore(&restored);
    assert_eq!(leaks.len(), 1);
    assert_eq!(leaks[0].saved_resource, Some(ResourceId(100)));
    assert_eq!(leaks[0].restored, None);
}

#[test]
fn transparent_restore_is_clean() {
    let (mut fs, g_srv, rt, dv) = game_state();
    let saved = fs.snapshot();
    let e_srv = ViewId(99);
    fs.apply_all([
        Event::RegisterView { view: e_srv, resource: ResourceId(999), kind: ViewKind::Srv },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(e_srv)] },
        Event::Draw,
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(g_srv)] },
        Event::SetRenderTargets { rtvs: vec![Some(rt)], dsv: Some(dv) },
    ]);
    let restored = fs.snapshot();
    assert!(saved.diff_restore(&restored).is_empty());
}

#[test]
fn dispatch_left_dsv_unbound_is_a_restore_leak() {
    let (mut fs, _g, rt, _dv) = game_state();
    let saved = fs.snapshot();
    fs.apply_all([
        Event::SetRenderTargets { rtvs: vec![Some(rt)], dsv: None },
        Event::Dispatch,
    ]);
    let restored = fs.snapshot();
    let leaks = saved.diff_restore(&restored);
    assert_eq!(leaks.len(), 1);
    assert_eq!(leaks[0].at, BindPoint::Dsv);
    assert_eq!(leaks[0].saved_resource, Some(ResourceId(300)));
    assert_eq!(leaks[0].restored, None);
}

#[test]
fn witness_string_names_slot_and_resources() {
    let (mut fs, _g, rt, dv) = game_state();
    let saved = fs.snapshot();
    fs.apply(Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![None] });
    fs.apply(Event::SetRenderTargets { rtvs: vec![Some(rt)], dsv: Some(dv) });
    let restored = fs.snapshot();
    let w = format!("{}", saved.diff_restore(&restored)[0]);
    assert!(w.contains("t27"), "{}", w);
    assert!(w.contains("res#100"), "{}", w);
    assert!(w.contains("NULL"), "{}", w);
}
