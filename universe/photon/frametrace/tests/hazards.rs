//! Hazard and binding-table tests: these ARE the ground-truth oracle. Each
//! asserts a concrete frame-state question that is hard to answer by reasoning
//! but trivial to answer by construction.

use photon_frametrace::*;

// Canonical case: resource R bound as SRV (t27, read) and RTV0 (write) at the
// same draw: is the SSR output still bound when the compositor samples it?
#[test]
fn srv_and_rtv_on_same_resource_is_a_hazard() {
    let mut fs = FrameState::new();
    let res = ResourceId(1);
    let srv_view = ViewId(10);
    let rtv_view = ViewId(11);
    fs.apply(Event::RegisterView { view: srv_view, resource: res, kind: ViewKind::Srv });
    fs.apply(Event::RegisterView { view: rtv_view, resource: res, kind: ViewKind::Rtv });
    fs.apply(Event::SetRenderTargets { rtvs: vec![Some(rtv_view)], dsv: None });
    fs.apply(Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(srv_view)] });
    fs.apply(Event::Draw);

    let hz = fs.hazards();
    assert_eq!(hz.len(), 1, "expected one hazard, got {:?}", hz);
    assert_eq!(hz[0].resource, res);
    assert_eq!(hz[0].reads, vec![ReadSlot { stage: Stage::Ps, slot: 27 }]);
    assert_eq!(hz[0].writes, vec![WriteSlot::Rtv(0)]);
    assert_eq!(hz[0].at_checkpoint, 1);
}

#[test]
fn unbinding_the_srv_clears_the_hazard() {
    let mut fs = FrameState::new();
    let res = ResourceId(1);
    let srv_view = ViewId(10);
    let rtv_view = ViewId(11);
    fs.apply_all([
        Event::RegisterView { view: srv_view, resource: res, kind: ViewKind::Srv },
        Event::RegisterView { view: rtv_view, resource: res, kind: ViewKind::Rtv },
        Event::SetRenderTargets { rtvs: vec![Some(rtv_view)], dsv: None },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(srv_view)] },
    ]);
    assert_eq!(fs.hazards().len(), 1);
    fs.apply(Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![None] });
    assert!(fs.hazards().is_empty(), "hazard should clear after unbind");
    assert_eq!(fs.srv_at(Stage::Ps, 27), None);
}

// Was the DSV unbound before this draw to avoid a read/write hazard?
#[test]
fn depth_bound_as_srv_and_dsv_then_unbound() {
    let mut fs = FrameState::new();
    let depth = ResourceId(2);
    let depth_srv = ViewId(20);
    let dsv = ViewId(21);
    fs.apply_all([
        Event::RegisterView { view: depth_srv, resource: depth, kind: ViewKind::Srv },
        Event::RegisterView { view: dsv, resource: depth, kind: ViewKind::Dsv },
        Event::SetRenderTargets { rtvs: vec![], dsv: Some(dsv) },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 0, views: vec![Some(depth_srv)] },
        Event::Draw,
    ]);
    let hz = fs.hazards();
    assert_eq!(hz.len(), 1);
    assert_eq!(hz[0].writes, vec![WriteSlot::Dsv]);
    fs.apply(Event::SetRenderTargets { rtvs: vec![], dsv: None });
    assert!(fs.hazards().is_empty());
    assert_eq!(fs.depth_stencil(), None);
}

#[test]
fn compute_srv_and_uav_on_same_resource_is_a_hazard() {
    let mut fs = FrameState::new();
    let res = ResourceId(3);
    let srv_view = ViewId(30);
    let uav_view = ViewId(31);
    fs.apply_all([
        Event::RegisterView { view: srv_view, resource: res, kind: ViewKind::Srv },
        Event::RegisterView { view: uav_view, resource: res, kind: ViewKind::Uav },
        Event::SetShaderResources { stage: Stage::Cs, start_slot: 0, views: vec![Some(srv_view)] },
        Event::SetUnorderedAccessViews { start_slot: 0, views: vec![Some(uav_view)] },
        Event::Dispatch,
    ]);
    let hz = fs.hazards();
    assert_eq!(hz.len(), 1);
    assert_eq!(hz[0].writes, vec![WriteSlot::Uav(0)]);
}

#[test]
fn lone_uav_is_not_a_hazard() {
    let mut fs = FrameState::new();
    let res = ResourceId(3);
    let uav_view = ViewId(31);
    fs.apply_all([
        Event::RegisterView { view: uav_view, resource: res, kind: ViewKind::Uav },
        Event::SetUnorderedAccessViews { start_slot: 0, views: vec![Some(uav_view)] },
        Event::Dispatch,
    ]);
    assert!(fs.hazards().is_empty());
    assert_eq!(fs.unordered_access(), vec![(0, uav_view)]);
}

#[test]
fn render_targets_remain_bound_after_dispatch() {
    let mut fs = FrameState::new();
    let rt = ResourceId(4);
    let rtv = ViewId(40);
    fs.apply_all([
        Event::RegisterView { view: rtv, resource: rt, kind: ViewKind::Rtv },
        Event::SetRenderTargets { rtvs: vec![Some(rtv)], dsv: None },
        Event::Dispatch,
    ]);
    assert_eq!(fs.render_targets(), vec![(0, rtv)]);
}

#[test]
fn distinct_resources_read_and_written_are_clean() {
    let mut fs = FrameState::new();
    let a = ResourceId(5);
    let b = ResourceId(6);
    let sa = ViewId(50);
    let rb = ViewId(51);
    fs.apply_all([
        Event::RegisterView { view: sa, resource: a, kind: ViewKind::Srv },
        Event::RegisterView { view: rb, resource: b, kind: ViewKind::Rtv },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 5, views: vec![Some(sa)] },
        Event::SetRenderTargets { rtvs: vec![Some(rb)], dsv: None },
        Event::Draw,
    ]);
    assert_eq!(fs.srv_at(Stage::Ps, 5), Some(sa));
    assert_eq!(fs.srv_at(Stage::Ps, 6), None);
    assert!(fs.hazards().is_empty());
}

#[test]
fn timeline_records_every_event() {
    let mut fs = FrameState::new();
    fs.apply(Event::Draw);
    fs.apply(Event::Dispatch);
    assert_eq!(fs.timeline().len(), 2);
    assert_eq!(fs.checkpoint(), 2);
}
