//! Replay a JSON capture trace and print the per-draw hazard log. This is
//! offline capture analysis: feed it a trace a RenderDoc/PIX/ETW adapter wrote.
//!
//! cargo run --example replay_trace -- path/to/trace.json

use photon_frametrace::FrameState;
use std::fs;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "tests/data/ssr_trace.json".to_string());
    let json = fs::read_to_string(&path).expect("read trace file");
    let mut fs = FrameState::new();
    let n = fs.replay_json(&json).expect("parse trace");
    println!("replayed {} events from {}", n, path);
    for d in fs.hazard_log() {
        if d.hazards.is_empty() {
            println!("draw {}: clean", d.checkpoint);
        } else {
            for h in &d.hazards {
                println!("draw {}: {}", d.checkpoint, h);
            }
        }
    }
}
