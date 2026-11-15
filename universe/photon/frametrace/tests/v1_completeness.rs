//! v1 hazard-completeness tests: read-only DSV, write/write conflicts, hazard
//! classification, and the per-draw hazard log.

use photon_frametrace::*;

// A depth buffer bound as SRV and as a READ-ONLY DSV is legal, not a hazard.
#[test]
fn read_only_dsv_with_srv_is_not_a_hazard() {
    let mut fs = FrameState::new();
    let depth = ResourceId(2);
    let srv = ViewId(20);
    let ro_dsv = ViewId(21);
    fs.apply_all([
        Event::RegisterView { view: srv, resource: depth, kind: ViewKind::Srv },
        Event::RegisterView { view: ro_dsv, resource: depth, kind: ViewKind::DsvReadOnly },
        Event::SetRenderTargets { rtvs: vec![], dsv: Some(ro_dsv) },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 0, views: vec![Some(srv)] },
        Event::Draw,
    ]);
    assert!(fs.hazards().is_empty(), "read-only DSV must not hazard with an SRV");
}

// A writable DSV with the same SRV is still a hazard, classified ReadWrite.
#[test]
fn writable_dsv_with_srv_is_read_write() {
    let mut fs = FrameState::new();
    let depth = ResourceId(2);
    let srv = ViewId(20);
    let dsv = ViewId(22);
    fs.apply_all([
        Event::RegisterView { view: srv, resource: depth, kind: ViewKind::Srv },
        Event::RegisterView { view: dsv, resource: depth, kind: ViewKind::Dsv },
        Event::SetRenderTargets { rtvs: vec![], dsv: Some(dsv) },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 0, views: vec![Some(srv)] },
        Event::Draw,
    ]);
    let hz = fs.hazards();
    assert_eq!(hz.len(), 1);
    assert_eq!(hz[0].kind, HazardKind::ReadWrite);
}

// One resource bound as RTV and UAV with no reader is a WriteWrite conflict.
#[test]
fn rtv_and_uav_on_same_resource_is_write_write() {
    let mut fs = FrameState::new();
    let res = ResourceId(7);
    let rtv = ViewId(70);
    let uav = ViewId(71);
    fs.apply_all([
        Event::RegisterView { view: rtv, resource: res, kind: ViewKind::Rtv },
        Event::RegisterView { view: uav, resource: res, kind: ViewKind::Uav },
        Event::SetRenderTargets { rtvs: vec![Some(rtv)], dsv: None },
        Event::SetUnorderedAccessViews { start_slot: 3, views: vec![Some(uav)] },
        Event::Draw,
    ]);
    let hz = fs.hazards();
    assert_eq!(hz.len(), 1);
    assert_eq!(hz[0].kind, HazardKind::WriteWrite);
    assert_eq!(hz[0].reads, vec![]);
    assert_eq!(hz[0].writes, vec![WriteSlot::Rtv(0), WriteSlot::Uav(3)]);
}

// The SSR read/write case is classified ReadWrite.
#[test]
fn srv_rtv_case_is_classified_read_write() {
    let mut fs = FrameState::new();
    let res = ResourceId(1);
    let srv = ViewId(10);
    let rtv = ViewId(11);
    fs.apply_all([
        Event::RegisterView { view: srv, resource: res, kind: ViewKind::Srv },
        Event::RegisterView { view: rtv, resource: res, kind: ViewKind::Rtv },
        Event::SetRenderTargets { rtvs: vec![Some(rtv)], dsv: None },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(srv)] },
        Event::Draw,
    ]);
    assert_eq!(fs.hazards()[0].kind, HazardKind::ReadWrite);
}

// The per-draw hazard log captures which draw was dirty and which was clean.
#[test]
fn hazard_log_records_each_draw() {
    let mut fs = FrameState::new();
    let res = ResourceId(1);
    let srv = ViewId(10);
    let rtv = ViewId(11);
    fs.apply_all([
        Event::RegisterView { view: srv, resource: res, kind: ViewKind::Srv },
        Event::RegisterView { view: rtv, resource: res, kind: ViewKind::Rtv },
        Event::SetRenderTargets { rtvs: vec![Some(rtv)], dsv: None },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(srv)] },
        Event::Draw,
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![None] },
        Event::Draw,
    ]);
    assert_eq!(fs.hazard_log().len(), 2);
    assert_eq!(fs.hazards_at(1).map(|h| h.len()), Some(1));
    assert_eq!(fs.hazards_at(2).map(|h| h.len()), Some(0));
    assert_eq!(fs.hazards_at(99), None);
}
