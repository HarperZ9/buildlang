//! Trace-ingestion tests: parse a normalized JSON capture and replay it, then
//! query ground truth. The JSON is what a RenderDoc/PIX/ETW adapter emits.

use photon_frametrace::trace::parse_trace;
use photon_frametrace::*;

const SSR_TRACE: &str = r#"[
  {"op":"register_view","view":1,"resource":21042,"kind":"srv"},
  {"op":"register_view","view":2,"resource":21042,"kind":"rtv"},
  {"op":"set_render_targets","rtvs":[2],"dsv":0},
  {"op":"set_shader_resources","stage":"ps","start":27,"views":[1]},
  {"op":"draw"},
  {"op":"set_shader_resources","stage":"ps","start":27,"views":[0]},
  {"op":"draw"}
]"#;

#[test]
fn replay_json_detects_then_clears_ssr_hazard() {
    let mut fs = FrameState::new();
    let n = fs.replay_json(SSR_TRACE).expect("trace should parse");
    assert_eq!(n, 7);
    assert_eq!(fs.hazard_log().len(), 2);
    let first = fs.hazards_at(1).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].kind, HazardKind::ReadWrite);
    assert_eq!(first[0].resource, ResourceId(21042));
    assert_eq!(fs.hazards_at(2).map(|h| h.len()), Some(0));
}

#[test]
fn parse_trace_yields_expected_event_count() {
    assert_eq!(parse_trace(SSR_TRACE).unwrap().len(), 7);
}

#[test]
fn malformed_or_unknown_op_is_an_error() {
    let mut fs = FrameState::new();
    assert!(fs.replay_json("not json").is_err());
    assert!(parse_trace(r#"[{"op":"bogus"}]"#).is_err());
}

#[test]
fn read_only_dsv_trace_is_clean() {
    let t = r#"[
      {"op":"register_view","view":1,"resource":7,"kind":"srv"},
      {"op":"register_view","view":2,"resource":7,"kind":"dsv_read_only"},
      {"op":"set_render_targets","rtvs":[],"dsv":2},
      {"op":"set_shader_resources","stage":"ps","start":0,"views":[1]},
      {"op":"draw"}
    ]"#;
    let mut fs = FrameState::new();
    fs.replay_json(t).unwrap();
    assert!(fs.hazards().is_empty());
}
