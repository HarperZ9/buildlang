//! NegativeLedger (agent-facing gate) tests: a claim about runtime state is
//! adjudicated against what the membrane observed. Confirmed = safe to assert;
//! Contradicted = the confident-but-wrong case, blocked with a witness;
//! Unresolvable = uninstrumented, assert neither way.

use photon_frametrace::*;

fn bound_state() -> FrameState {
    let mut fs = FrameState::new();
    fs.apply_all([
        Event::RegisterView { view: ViewId(1), resource: ResourceId(100), kind: ViewKind::Srv },
        Event::RegisterView { view: ViewId(2), resource: ResourceId(200), kind: ViewKind::Rtv },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 27, views: vec![Some(ViewId(1))] },
        Event::SetRenderTargets { rtvs: vec![Some(ViewId(2))], dsv: None },
    ]);
    fs
}

#[test]
fn correct_binding_claim_is_confirmed() {
    assert_eq!(bound_state().adjudicate(&Claim::parse("srv ps 27 100")), Verdict::Confirmed);
}

#[test]
fn wrong_resource_is_contradicted() {
    let v = bound_state().adjudicate(&Claim::parse("srv ps 27 999"));
    assert!(matches!(v, Verdict::Contradicted { .. }), "{:?}", v);
    assert!(!v.may_assert());
}

// THE SHARPEST TEST: a confident claim about an UNOBSERVED slot is BLOCKED, even
// though plausible context (a slot-map comment saying t5 is SSR) might suggest it.
#[test]
fn confident_claim_about_unobserved_slot_is_blocked() {
    let v = bound_state().adjudicate(&Claim::parse("srv ps 5 100"));
    assert!(!v.may_assert(), "{:?}", v);
    match v {
        Verdict::Contradicted { observed } => assert!(observed.contains("unbound"), "{}", observed),
        other => panic!("expected Contradicted, got {:?}", other),
    }
}

#[test]
fn correct_unbound_claim_is_confirmed() {
    assert_eq!(bound_state().adjudicate(&Claim::parse("srv ps 5 none")), Verdict::Confirmed);
}

#[test]
fn render_target_claims() {
    let fs = bound_state();
    assert_eq!(fs.adjudicate(&Claim::parse("rt 200")), Verdict::Confirmed);
    assert!(matches!(fs.adjudicate(&Claim::parse("rt 999")), Verdict::Contradicted { .. }));
}

#[test]
fn uninstrumented_state_is_unresolvable() {
    let v = bound_state().adjudicate(&Claim::parse("blend additive"));
    assert_eq!(v, Verdict::Unresolvable);
    assert!(!v.may_assert()); // do NOT assert true OR false
}

#[test]
fn hazard_claim_confirmed_when_present_else_contradicted() {
    let mut fs = FrameState::new();
    fs.apply_all([
        Event::RegisterView { view: ViewId(1), resource: ResourceId(50), kind: ViewKind::Srv },
        Event::RegisterView { view: ViewId(2), resource: ResourceId(50), kind: ViewKind::Rtv },
        Event::SetRenderTargets { rtvs: vec![Some(ViewId(2))], dsv: None },
        Event::SetShaderResources { stage: Stage::Ps, start_slot: 0, views: vec![Some(ViewId(1))] },
        Event::Draw,
    ]);
    assert_eq!(fs.adjudicate(&Claim::parse("hazard 50")), Verdict::Confirmed);
    assert!(matches!(fs.adjudicate(&Claim::parse("hazard 999")), Verdict::Contradicted { .. }));
}
