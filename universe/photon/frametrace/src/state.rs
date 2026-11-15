use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{
    BindPoint, Claim, Event, Hazard, HazardKind, ReadSlot, ResourceId, RestoreLeak, Stage,
    TemporalFault, TemporalViolation, Verdict, ViewId, ViewKind, WriteSlot,
};

/// Shader-resource slots per stage (D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT).
pub const SRV_SLOTS: u32 = 128;
/// Simultaneous render targets (D3D11_SIMULTANEOUS_RENDER_TARGET_COUNT).
pub const RTV_SLOTS: u32 = 8;
/// UAV slots (D3D11_1_UAV_SLOT_COUNT).
pub const UAV_SLOTS: u32 = 64;

const STAGES: [Stage; 6] = [Stage::Vs, Stage::Ps, Stage::Cs, Stage::Gs, Stage::Hs, Stage::Ds];

/// The hazards observed at one draw/dispatch checkpoint.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DrawHazards {
    /// The checkpoint index (1-based, monotonic).
    pub checkpoint: u64,
    /// Hazards present in the binding state at that checkpoint.
    pub hazards: Vec<Hazard>,
}

/// The live binding state of a D3D11 immediate context: a symbol table for the
/// GPU frame. Apply events in order, then query bindings or detect hazards.
#[derive(Default)]
pub struct FrameState {
    views: HashMap<ViewId, (ResourceId, ViewKind)>,
    srv: HashMap<Stage, BTreeMap<u32, ViewId>>,
    uav: BTreeMap<u32, ViewId>,
    rtv: BTreeMap<u32, ViewId>,
    dsv: Option<ViewId>,
    timeline: Vec<Event>,
    checkpoint: u64,
    log: Vec<DrawHazards>,
    history_pairs: Vec<(ResourceId, ResourceId)>,
    frame: u64,
    frame_read: BTreeSet<ResourceId>,
    frame_written: BTreeSet<ResourceId>,
    frame_cleared: BTreeSet<ResourceId>,
    prev_read: BTreeSet<ResourceId>,
    warmed: BTreeSet<ResourceId>,
    temporal_violations: Vec<TemporalViolation>,
}

impl FrameState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one event, updating the binding tables and recording it. At each
    /// draw/dispatch the current hazards are snapshotted into the hazard log.
    pub fn apply(&mut self, event: Event) {
        match &event {
            Event::RegisterView { view, resource, kind } => {
                self.views.insert(*view, (*resource, *kind));
            }
            Event::SetShaderResources { stage, start_slot, views } => {
                let table = self.srv.entry(*stage).or_default();
                for (i, v) in views.iter().enumerate() {
                    let slot = start_slot + i as u32;
                    match v {
                        Some(view) => { table.insert(slot, *view); }
                        None => { table.remove(&slot); }
                    }
                }
            }
            Event::SetUnorderedAccessViews { start_slot, views } => {
                for (i, v) in views.iter().enumerate() {
                    let slot = start_slot + i as u32;
                    match v {
                        Some(view) => { self.uav.insert(slot, *view); }
                        None => { self.uav.remove(&slot); }
                    }
                }
            }
            Event::SetRenderTargets { rtvs, dsv } => {
                self.rtv.clear();
                for (i, v) in rtvs.iter().enumerate() {
                    if let Some(view) = v {
                        self.rtv.insert(i as u32, *view);
                    }
                }
                self.dsv = *dsv;
            }
            Event::ClearRenderTargetView { .. } | Event::ClearDepthStencilView { .. } => {}
            Event::Draw | Event::Dispatch => {
                self.checkpoint += 1;
            }
            Event::Present => {
                self.evaluate_temporal();
            }
        }
        if !self.history_pairs.is_empty() {
            self.track_temporal(&event);
        }
        let is_checkpoint = matches!(event, Event::Draw | Event::Dispatch);
        self.timeline.push(event);
        if is_checkpoint {
            let hazards = self.hazards();
            let checkpoint = self.checkpoint;
            self.log.push(DrawHazards { checkpoint, hazards });
        }
    }

    /// Apply a sequence of events in order.
    pub fn apply_all<I: IntoIterator<Item = Event>>(&mut self, events: I) {
        for e in events {
            self.apply(e);
        }
    }

    /// The view bound to a shader-resource slot of a stage, if any.
    pub fn srv_at(&self, stage: Stage, slot: u32) -> Option<ViewId> {
        self.srv.get(&stage).and_then(|t| t.get(&slot)).copied()
    }

    /// The currently bound render targets as (slot, view) pairs.
    pub fn render_targets(&self) -> Vec<(u32, ViewId)> {
        self.rtv.iter().map(|(s, v)| (*s, *v)).collect()
    }

    /// The currently bound depth-stencil view, if any.
    pub fn depth_stencil(&self) -> Option<ViewId> {
        self.dsv
    }

    /// The currently bound unordered-access views as (slot, view) pairs.
    pub fn unordered_access(&self) -> Vec<(u32, ViewId)> {
        self.uav.iter().map(|(s, v)| (*s, *v)).collect()
    }

    /// Resolve a view to its (resource, kind), if registered.
    pub fn resolve(&self, view: ViewId) -> Option<(ResourceId, ViewKind)> {
        self.views.get(&view).copied()
    }

    /// The recorded event timeline.
    pub fn timeline(&self) -> &[Event] {
        &self.timeline
    }

    /// The number of draw/dispatch checkpoints applied so far.
    pub fn checkpoint(&self) -> u64 {
        self.checkpoint
    }

    /// The per-draw hazard log across the whole frame so far.
    pub fn hazard_log(&self) -> &[DrawHazards] {
        &self.log
    }

    /// Hazards recorded at a specific checkpoint, if that checkpoint exists.
    pub fn hazards_at(&self, checkpoint: u64) -> Option<&[Hazard]> {
        self.log.iter().find(|d| d.checkpoint == checkpoint).map(|d| d.hazards.as_slice())
    }

    /// Detect hazards in the CURRENT binding state. A resource read via an SRV
    /// and also written (RTV/DSV/UAV) is ReadWrite; a resource written through
    /// two or more distinct write views with no reader is WriteWrite. A
    /// read-only DSV is neither read nor write; a lone UAV is not a hazard.
    pub fn hazards(&self) -> Vec<Hazard> {
        let mut reads: BTreeMap<u64, Vec<ReadSlot>> = BTreeMap::new();
        let mut writes: BTreeMap<u64, Vec<WriteSlot>> = BTreeMap::new();

        for stage in STAGES {
            if let Some(table) = self.srv.get(&stage) {
                for (slot, view) in table {
                    if let Some((res, kind)) = self.resolve(*view) {
                        if kind.reads() {
                            reads.entry(res.0).or_default().push(ReadSlot { stage, slot: *slot });
                        }
                    }
                }
            }
        }
        for (slot, view) in &self.rtv {
            if let Some((res, _)) = self.resolve(*view) {
                writes.entry(res.0).or_default().push(WriteSlot::Rtv(*slot));
            }
        }
        if let Some(view) = self.dsv {
            if let Some((res, kind)) = self.resolve(view) {
                if kind == ViewKind::Dsv {
                    writes.entry(res.0).or_default().push(WriteSlot::Dsv);
                }
            }
        }
        for (slot, view) in &self.uav {
            if let Some((res, _)) = self.resolve(*view) {
                writes.entry(res.0).or_default().push(WriteSlot::Uav(*slot));
            }
        }

        let mut hazards = Vec::new();
        let resources: BTreeSet<u64> = writes.keys().copied().collect();
        for res in resources {
            let w = writes.get(&res).cloned().unwrap_or_default();
            if let Some(r) = reads.get(&res) {
                hazards.push(Hazard {
                    kind: HazardKind::ReadWrite,
                    resource: ResourceId(res),
                    reads: r.clone(),
                    writes: w,
                    at_checkpoint: self.checkpoint,
                });
            } else if w.len() >= 2 {
                hazards.push(Hazard {
                    kind: HazardKind::WriteWrite,
                    resource: ResourceId(res),
                    reads: Vec::new(),
                    writes: w,
                    at_checkpoint: self.checkpoint,
                });
            }
        }
        hazards
    }
}

impl FrameState {
    fn bound(&self, view: ViewId) -> Bound {
        Bound { view, resource: self.resolve(view).map(|(r, _)| r) }
    }

    /// Capture the current resource-binding state for restore-verification.
    pub fn snapshot(&self) -> Snapshot {
        let mut srv = BTreeMap::new();
        for (stage, table) in &self.srv {
            for (slot, view) in table {
                srv.insert((stage.index(), *slot), self.bound(*view));
            }
        }
        Snapshot {
            srv,
            uav: self.uav.iter().map(|(s, v)| (*s, self.bound(*v))).collect(),
            rtv: self.rtv.iter().map(|(s, v)| (*s, self.bound(*v))).collect(),
            dsv: self.dsv.map(|v| self.bound(v)),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Bound {
    view: ViewId,
    resource: Option<ResourceId>,
}

/// An immutable capture of the resource-binding state at one instant. Diff a
/// pre-effect SAVE against the post-effect RESTORE: any differing slot is a
/// restore leak (the effect was not transparent to the host).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Snapshot {
    srv: BTreeMap<(u8, u32), Bound>,
    uav: BTreeMap<u32, Bound>,
    rtv: BTreeMap<u32, Bound>,
    dsv: Option<Bound>,
}

fn view_of(b: Option<&Bound>) -> Option<ViewId> {
    b.map(|x| x.view)
}

fn make_leak(at: BindPoint, s: Option<&Bound>, r: Option<&Bound>) -> RestoreLeak {
    RestoreLeak {
        at,
        saved: s.map(|b| b.view),
        restored: r.map(|b| b.view),
        saved_resource: s.and_then(|b| b.resource),
        restored_resource: r.and_then(|b| b.resource),
    }
}

impl Snapshot {
    /// Slots whose binding differs between this (SAVED) snapshot and the
    /// RESTORED one. Empty = the restore was transparent. Deterministic order.
    pub fn diff_restore(&self, restored: &Snapshot) -> Vec<RestoreLeak> {
        let mut leaks = Vec::new();

        let mut srv_keys: BTreeSet<(u8, u32)> = self.srv.keys().copied().collect();
        srv_keys.extend(restored.srv.keys().copied());
        for (si, slot) in srv_keys {
            let s = self.srv.get(&(si, slot));
            let r = restored.srv.get(&(si, slot));
            if view_of(s) != view_of(r) {
                leaks.push(make_leak(BindPoint::Srv { stage: Stage::from_index(si), slot }, s, r));
            }
        }
        let mut uav_keys: BTreeSet<u32> = self.uav.keys().copied().collect();
        uav_keys.extend(restored.uav.keys().copied());
        for slot in uav_keys {
            let (s, r) = (self.uav.get(&slot), restored.uav.get(&slot));
            if view_of(s) != view_of(r) {
                leaks.push(make_leak(BindPoint::Uav(slot), s, r));
            }
        }
        let mut rtv_keys: BTreeSet<u32> = self.rtv.keys().copied().collect();
        rtv_keys.extend(restored.rtv.keys().copied());
        for slot in rtv_keys {
            let (s, r) = (self.rtv.get(&slot), restored.rtv.get(&slot));
            if view_of(s) != view_of(r) {
                leaks.push(make_leak(BindPoint::Rtv(slot), s, r));
            }
        }
        if view_of(self.dsv.as_ref()) != view_of(restored.dsv.as_ref()) {
            leaks.push(make_leak(BindPoint::Dsv, self.dsv.as_ref(), restored.dsv.as_ref()));
        }
        leaks
    }
}

impl FrameState {
    /// Declare two resources as a temporal ping-pong (history) pair. Temporal
    /// invariants are then checked at each Present. No pairs = zero cost.
    pub fn declare_history_pair(&mut self, a: ResourceId, b: ResourceId) {
        self.history_pairs.push((a, b));
    }

    /// The current frame index (incremented at each Present).
    pub fn frame_index(&self) -> u64 {
        self.frame
    }

    /// All temporal violations witnessed so far.
    pub fn temporal_violations(&self) -> &[TemporalViolation] {
        &self.temporal_violations
    }

    fn member_resource(&self, view: ViewId) -> Option<ResourceId> {
        let r = self.resolve(view).map(|(res, _)| res)?;
        if self.history_pairs.iter().any(|&(a, b)| a == r || b == r) {
            Some(r)
        } else {
            None
        }
    }

    fn track_temporal(&mut self, event: &Event) {
        match event {
            Event::SetShaderResources { views, .. } => {
                for v in views.iter().flatten() {
                    if let Some(r) = self.member_resource(*v) {
                        self.frame_read.insert(r);
                    }
                }
            }
            Event::SetUnorderedAccessViews { views, .. } => {
                for v in views.iter().flatten() {
                    if let Some(r) = self.member_resource(*v) {
                        self.frame_written.insert(r);
                    }
                }
            }
            Event::SetRenderTargets { rtvs, dsv } => {
                for v in rtvs.iter().flatten() {
                    if let Some(r) = self.member_resource(*v) {
                        self.frame_written.insert(r);
                    }
                }
                if let Some(v) = dsv {
                    if let Some(r) = self.member_resource(*v) {
                        self.frame_written.insert(r);
                    }
                }
            }
            Event::ClearRenderTargetView { rtv } => {
                if let Some(r) = self.member_resource(*rtv) {
                    self.frame_cleared.insert(r);
                }
            }
            Event::ClearDepthStencilView { dsv } => {
                if let Some(r) = self.member_resource(*dsv) {
                    self.frame_cleared.insert(r);
                }
            }
            _ => {}
        }
    }

    /// Evaluate temporal invariants for the frame just completed, then roll
    /// per-frame state. Three faults: within-frame feedback (a member read AND
    /// written this frame), swap-desync (read did not alternate from last frame:
    /// stuck buffer), uninitialized-read (read before any write/clear: warmup).
    fn evaluate_temporal(&mut self) {
        let frame = self.frame;
        let fr = self.frame_read.clone();
        let fw = self.frame_written.clone();
        let pr_prev = self.prev_read.clone();
        let mut warmed_now = self.warmed.clone();
        warmed_now.extend(fw.iter().copied());
        warmed_now.extend(self.frame_cleared.iter().copied());

        for (a, b) in self.history_pairs.clone() {
            let members = [a, b];
            let inpair = |s: &BTreeSet<ResourceId>| -> BTreeSet<ResourceId> {
                members.iter().copied().filter(|r| s.contains(r)).collect()
            };
            let read = inpair(&fr);
            let written = inpair(&fw);
            let prev_read = inpair(&pr_prev);

            for r in read.intersection(&written) {
                self.temporal_violations.push(TemporalViolation {
                    frame,
                    fault: TemporalFault::WithinFrameFeedback,
                    pair: (a, b),
                    resource: *r,
                });
            }
            for r in &read {
                if !warmed_now.contains(r) {
                    self.temporal_violations.push(TemporalViolation {
                        frame,
                        fault: TemporalFault::UninitializedRead,
                        pair: (a, b),
                        resource: *r,
                    });
                }
            }
            if !read.is_empty() && read == prev_read {
                for r in &read {
                    self.temporal_violations.push(TemporalViolation {
                        frame,
                        fault: TemporalFault::SwapDesync,
                        pair: (a, b),
                        resource: *r,
                    });
                }
            }
        }

        self.warmed = warmed_now;
        self.prev_read = fr;
        self.frame_read.clear();
        self.frame_written.clear();
        self.frame_cleared.clear();
        self.frame += 1;
    }
}

impl FrameState {
    /// Adjudicate a claim about current runtime state against what the membrane
    /// actually OBSERVED. The agent-facing gate: a confident claim about an
    /// unobserved or contradicted binding is blocked, not believed -- prose
    /// context (a slot-map comment) never overrides the live observation.
    pub fn adjudicate(&self, claim: &Claim) -> Verdict {
        match claim {
            Claim::SrvBinding { stage, slot, resource } => {
                let observed = self.srv_at(*stage, *slot).and_then(|v| self.resolve(v)).map(|(r, _)| r);
                if observed == *resource {
                    Verdict::Confirmed
                } else {
                    Verdict::Contradicted {
                        observed: match observed {
                            Some(r) => format!("{:?} t{} = res#{}", stage, slot, r.0),
                            None => format!("{:?} t{} unbound", stage, slot),
                        },
                    }
                }
            }
            Claim::IsRenderTarget { resource } => {
                let bound = self
                    .render_targets()
                    .iter()
                    .filter_map(|(_, v)| self.resolve(*v).map(|(r, _)| r))
                    .any(|r| r == *resource);
                if bound {
                    Verdict::Confirmed
                } else {
                    Verdict::Contradicted { observed: format!("res#{} is not a bound render target", resource.0) }
                }
            }
            Claim::IsDepthStencil { resource } => {
                let ds = self.depth_stencil().and_then(|v| self.resolve(v)).map(|(r, _)| r);
                if ds == Some(*resource) {
                    Verdict::Confirmed
                } else {
                    Verdict::Contradicted {
                        observed: match ds {
                            Some(r) => format!("depth-stencil is res#{}", r.0),
                            None => "no depth-stencil bound".to_string(),
                        },
                    }
                }
            }
            Claim::IsHazarded { resource } => {
                if self.hazards().iter().any(|h| h.resource == *resource) {
                    Verdict::Confirmed
                } else {
                    Verdict::Contradicted { observed: format!("res#{} is not currently hazarded", resource.0) }
                }
            }
            Claim::Uninstrumented { .. } => Verdict::Unresolvable,
        }
    }
}
