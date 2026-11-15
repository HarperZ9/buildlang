//! Ingest a normalized capture trace (JSON) and replay it into a FrameState.
//! A RenderDoc/PIX/ETW adapter emits this format; the tracer then answers
//! ground truth from the replayed bindings instead of anyone reasoning about
//! the frame in their head.
//!
//! A trace is a JSON array of event objects tagged by "op". Example:
//!   [{"op":"register_view","view":1,"resource":42,"kind":"srv"},
//!    {"op":"set_render_targets","rtvs":[2],"dsv":0},
//!    {"op":"set_shader_resources","stage":"ps","start":27,"views":[1]},
//!    {"op":"draw"}]
//! Ids are u64; id 0 in a view slot means null / unbound.

use serde::Deserialize;

use crate::{Event, FrameState, ResourceId, Stage, ViewId, ViewKind};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum KindStr {
    Srv,
    Rtv,
    Dsv,
    DsvReadOnly,
    Uav,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum StageStr {
    Vs,
    Ps,
    Cs,
    Gs,
    Hs,
    Ds,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum RawEvent {
    RegisterView { view: u64, resource: u64, kind: KindStr },
    SetShaderResources { stage: StageStr, start: u32, views: Vec<u64> },
    SetUav { start: u32, views: Vec<u64> },
    SetRenderTargets {
        rtvs: Vec<u64>,
        #[serde(default)]
        dsv: u64,
    },
    ClearRtv { rtv: u64 },
    ClearDsv { dsv: u64 },
    Draw,
    Dispatch,
    Present,
}

/// An error parsing a trace.
#[derive(Debug)]
pub struct TraceError(String);

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "trace parse error: {}", self.0)
    }
}
impl std::error::Error for TraceError {}

fn opt(id: u64) -> Option<ViewId> {
    if id == 0 {
        None
    } else {
        Some(ViewId(id))
    }
}

impl From<KindStr> for ViewKind {
    fn from(k: KindStr) -> Self {
        match k {
            KindStr::Srv => ViewKind::Srv,
            KindStr::Rtv => ViewKind::Rtv,
            KindStr::Dsv => ViewKind::Dsv,
            KindStr::DsvReadOnly => ViewKind::DsvReadOnly,
            KindStr::Uav => ViewKind::Uav,
        }
    }
}

impl From<StageStr> for Stage {
    fn from(s: StageStr) -> Self {
        match s {
            StageStr::Vs => Stage::Vs,
            StageStr::Ps => Stage::Ps,
            StageStr::Cs => Stage::Cs,
            StageStr::Gs => Stage::Gs,
            StageStr::Hs => Stage::Hs,
            StageStr::Ds => Stage::Ds,
        }
    }
}

impl From<RawEvent> for Event {
    fn from(r: RawEvent) -> Self {
        match r {
            RawEvent::RegisterView { view, resource, kind } => Event::RegisterView {
                view: ViewId(view),
                resource: ResourceId(resource),
                kind: kind.into(),
            },
            RawEvent::SetShaderResources { stage, start, views } => Event::SetShaderResources {
                stage: stage.into(),
                start_slot: start,
                views: views.into_iter().map(opt).collect(),
            },
            RawEvent::SetUav { start, views } => Event::SetUnorderedAccessViews {
                start_slot: start,
                views: views.into_iter().map(opt).collect(),
            },
            RawEvent::SetRenderTargets { rtvs, dsv } => Event::SetRenderTargets {
                rtvs: rtvs.into_iter().map(opt).collect(),
                dsv: opt(dsv),
            },
            RawEvent::ClearRtv { rtv } => Event::ClearRenderTargetView { rtv: ViewId(rtv) },
            RawEvent::ClearDsv { dsv } => Event::ClearDepthStencilView { dsv: ViewId(dsv) },
            RawEvent::Draw => Event::Draw,
            RawEvent::Dispatch => Event::Dispatch,
            RawEvent::Present => Event::Present,
        }
    }
}

/// Parse a JSON trace into events.
pub fn parse_trace(json: &str) -> Result<Vec<Event>, TraceError> {
    let raw: Vec<RawEvent> =
        serde_json::from_str(json).map_err(|e| TraceError(e.to_string()))?;
    Ok(raw.into_iter().map(Event::from).collect())
}

impl FrameState {
    /// Parse and replay a JSON trace, returning the number of events applied.
    pub fn replay_json(&mut self, json: &str) -> Result<usize, TraceError> {
        let events = parse_trace(json)?;
        let n = events.len();
        self.apply_all(events);
        Ok(n)
    }
}
