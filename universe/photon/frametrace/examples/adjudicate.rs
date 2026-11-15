//! The agent-facing membrane gate: replay a capture, then adjudicate claims
//! against what the membrane OBSERVED. A confident-but-wrong claim is
//! CONTRADICTED with a witness; uninstrumented state is UNRESOLVABLE.
//!
//! The default demo replays the SSR trace -- which binds res 0x5232 to PS t27
//! then UNBINDS it. A model "knowing" t27 is SSR (from a slot-map comment) would
//! confidently claim it is still bound; the live state blocks that claim.
//!
//! cargo run --example adjudicate -- <trace.json> "srv ps 27 21042" "blend foo"

use photon_frametrace::*;
use std::fs;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "tests/data/ssr_trace.json".to_string());
    let mut claims: Vec<String> = args.collect();
    if claims.is_empty() {
        claims = vec![
            "srv ps 27 21042".to_string(),
            "srv ps 27 none".to_string(),
            "blend additive".to_string(),
        ];
    }

    let json = fs::read_to_string(&path).expect("read trace");
    let mut state = FrameState::new();
    state.replay_json(&json).expect("replay");

    println!("replayed {} -- adjudicating {} claim(s) against observed state:", path, claims.len());
    for c in &claims {
        let verdict = state.adjudicate(&Claim::parse(c));
        let tag = if verdict.may_assert() { "ok   " } else { "BLOCK" };
        println!("  [{}] {:<20} -> {}", tag, c, verdict);
    }
}
