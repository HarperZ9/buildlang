//! Temporal (history ping-pong) tests. The marquee case -- a stuck ping-pong --
//! is cross-frame and provably unreachable by a single-frame capture. Each test
//! is a ground-truth oracle over the frame stream.
//!
//! Resource A=1, B=2 form a history pair. A: srv view 10, rtv view 11.
//! B: srv view 20, rtv view 21.

use photon_frametrace::*;

const A: u64 = 1;
const B: u64 = 2;

fn setup() -> FrameState {
    let mut fs = FrameState::new();
    fs.apply_all([
        Event::RegisterView { view: ViewId(10), resource: ResourceId(A), kind: ViewKind::Srv },
        Event::RegisterView { view: ViewId(11), resource: ResourceId(A), kind: ViewKind::Rtv },
        Event::RegisterView { view: ViewId(20), resource: ResourceId(B), kind: ViewKind::Srv },
        Event::RegisterView { view: ViewId(21), resource: ResourceId(B), kind: ViewKind::Rtv },
    ]);
    fs.declare_history_pair(ResourceId(A), ResourceId(B));
    fs
}
fn read(fs: &mut FrameState, srv: u64) {
    fs.apply(Event::SetShaderResources { stage: Stage::Ps, start_slot: 0, views: vec![Some(ViewId(srv))] });
}
fn write(fs: &mut FrameState, rtv: u64) {
    fs.apply(Event::SetRenderTargets { rtvs: vec![Some(ViewId(rtv))], dsv: None });
}
fn clear(fs: &mut FrameState, rtv: u64) {
    fs.apply(Event::ClearRenderTargetView { rtv: ViewId(rtv) });
}
fn frame(fs: &mut FrameState) {
    fs.apply(Event::Draw);
    fs.apply(Event::Present);
}
fn faults(fs: &FrameState, k: TemporalFault) -> Vec<TemporalViolation> {
    fs.temporal_violations().iter().copied().filter(|t| t.fault == k).collect()
}

#[test]
fn correct_ping_pong_is_clean() {
    let mut fs = setup();
    clear(&mut fs, 11);
    clear(&mut fs, 21);
    fs.apply(Event::Present); // frame 0: warmup
    read(&mut fs, 10); write(&mut fs, 21); frame(&mut fs); // f1: read A, write B
    read(&mut fs, 20); write(&mut fs, 11); frame(&mut fs); // f2: read B, write A
    read(&mut fs, 10); write(&mut fs, 21); frame(&mut fs); // f3: read A, write B
    assert!(fs.temporal_violations().is_empty(), "{:?}", fs.temporal_violations());
}

#[test]
fn stuck_buffer_fires_on_frame_2() {
    let mut fs = setup();
    clear(&mut fs, 11); clear(&mut fs, 21); fs.apply(Event::Present); // f0 warmup
    read(&mut fs, 10); write(&mut fs, 21); frame(&mut fs); // f1: read A
    read(&mut fs, 10); write(&mut fs, 21); frame(&mut fs); // f2: read A AGAIN (stuck)
    let v = faults(&fs, TemporalFault::SwapDesync);
    assert_eq!(v.len(), 1, "{:?}", fs.temporal_violations());
    assert_eq!(v[0].frame, 2);
    assert_eq!(v[0].resource, ResourceId(A));
}

#[test]
fn within_frame_feedback_read_and_write_same_buffer() {
    let mut fs = setup();
    clear(&mut fs, 11); clear(&mut fs, 21); fs.apply(Event::Present); // f0
    read(&mut fs, 10); write(&mut fs, 11); frame(&mut fs); // f1: read A AND write A
    let v = faults(&fs, TemporalFault::WithinFrameFeedback);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].resource, ResourceId(A));
}

#[test]
fn reading_uninitialized_history_is_flagged() {
    let mut fs = setup();
    read(&mut fs, 10); frame(&mut fs); // f0: read A with no prior write/clear
    let v = faults(&fs, TemporalFault::UninitializedRead);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].resource, ResourceId(A));
}

#[test]
fn clear_warms_the_buffer() {
    let mut fs = setup();
    clear(&mut fs, 11); // clear A this frame
    read(&mut fs, 10); frame(&mut fs); // read A same frame after clear
    assert!(faults(&fs, TemporalFault::UninitializedRead).is_empty());
}

#[test]
fn no_pairs_declared_is_silent() {
    let mut fs = FrameState::new();
    fs.apply(Event::Present);
    fs.apply(Event::Present);
    assert!(fs.temporal_violations().is_empty());
    assert_eq!(fs.frame_index(), 2);
}
