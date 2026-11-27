// ===============================================================================
// BUILDLANG COMPILER - UNIFIED 4-PILLAR SOVEREIGN MIR AUDIT ORCHESTRATOR
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Unified 4-pillar compile-time MIR formal verification suite
//! (`buildc check --sovereign-audit --sovereign-manifest-json`).
//!
//! Runs all four zero-runtime-overhead MIR analysis engines in a single
//! deterministic pass over the lowered [`MirModule`]:
//! 1. **Capability & Effect Call-Graph Verifier** (`cap_effect_audit`),
//! 2. **Bounded-Stack & Static WCET Verifier** (`wcet_stack_audit`),
//! 3. **Constant-Time Cryptography & Side-Channel Taint Verifier** (`ct_taint_audit`),
//! 4. **Bit-Exact Determinism & Reproducibility Verifier** (`det_repro_audit`).
//!
//! Seals the four pillar SHA-256 digests under one composite cryptographic root:
//! `sovereign_root_sha256 = SHA256(effect_sha || wcet_sha || ct_sha || det_sha)`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

use crate::codegen::analysis::cap_effect_audit::{
    audit_module as audit_mir_effects, CapEffect, EffectManifest,
};
use crate::codegen::analysis::ct_taint_audit::{audit_mir_constant_time, CtManifest};
use crate::codegen::analysis::det_repro_audit::{audit_mir_determinism, DetManifest};
use crate::codegen::analysis::wcet_stack_audit::{audit_mir_wcet_stack, WcetManifest};
use crate::codegen::ir::MirModule;

/// Canonical schema identifier for `--sovereign-manifest-json`.
pub const SOVEREIGN_AUDIT_SCHEMA: &str = "buildlang.sovereign-audit/v1";

/// Default stack-frame call-chain budget enforced by `--sovereign-audit` when
/// `--max-stack-bytes` is omitted (one 4 KiB kernel stack page).
pub const DEFAULT_SOVEREIGN_STACK_BUDGET_BYTES: u64 = 4096;

/// Default strict zero-egress + zero-unsafe capability deny set for `--sovereign-audit`.
pub fn default_sovereign_denied_effects() -> BTreeSet<CapEffect> {
    BTreeSet::from([
        CapEffect::Fs,
        CapEffect::Net,
        CapEffect::Exec,
        CapEffect::Unsafe,
    ])
}

/// Compute the composite root digest:
/// `SHA256(effect_sha || wcet_sha || ct_sha || det_sha)` as lowercase hex.
pub fn compute_sovereign_root_sha256(
    effect_sha: &str,
    wcet_sha: &str,
    ct_sha: &str,
    det_sha: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(effect_sha.as_bytes());
    hasher.update(wcet_sha.as_bytes());
    hasher.update(ct_sha.as_bytes());
    hasher.update(det_sha.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

/// Configuration for [`run_sovereign_mir_audit`].
#[derive(Debug, Clone)]
pub struct SovereignAuditConfig {
    /// Denied capability effects for Pillar 1 (`cap_effect`).
    pub denied_effects: BTreeSet<CapEffect>,
    /// Maximum transitive call-stack budget in bytes for Pillar 2 (`wcet_stack`).
    pub max_stack_bytes: u64,
}

impl Default for SovereignAuditConfig {
    fn default() -> Self {
        Self {
            denied_effects: default_sovereign_denied_effects(),
            max_stack_bytes: DEFAULT_SOVEREIGN_STACK_BUDGET_BYTES,
        }
    }
}

/// Per-pillar pass/fail summary entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SovereignPillarStatus {
    /// Pillar identifier (`cap_effect`, `wcet_stack`, `constant_time`, `strict_determinism`).
    pub pillar: String,
    /// True iff this pillar recorded zero violations.
    pub passed: bool,
    /// Number of violations reported by this pillar.
    pub violation_count: usize,
    /// Canonical SHA-256 digest of this pillar's manifest.
    pub manifest_sha256: String,
}

/// Unified 4-pillar compile-time MIR audit manifest (`buildlang.sovereign-audit/v1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SovereignAuditManifest {
    /// Schema tag (`buildlang.sovereign-audit/v1`).
    pub schema: String,
    /// Audited module label or source path.
    pub module: String,
    /// True iff all four pillars passed with zero violations.
    pub passed: bool,
    /// Number of pillars that passed (`0..=4`).
    pub pillars_passed: u32,
    /// Total number of pillars evaluated (`4`).
    pub pillars_total: u32,
    /// Names of failing pillars in deterministic order (empty iff `passed == true`).
    pub failing_pillars: Vec<String>,
    /// Ordered per-pillar status records.
    pub pillar_statuses: Vec<SovereignPillarStatus>,
    /// Composite root digest `SHA256(effect_sha || wcet_sha || ct_sha || det_sha)`.
    pub sovereign_root_sha256: String,
    /// Pillar 1: Capability & effect call-graph manifest.
    pub effect: EffectManifest,
    /// Pillar 2: Bounded-stack & static WCET manifest.
    pub wcet: WcetManifest,
    /// Pillar 3: Constant-time cryptography & side-channel taint manifest.
    pub constant_time: CtManifest,
    /// Pillar 4: Bit-exact determinism & reproducibility manifest.
    pub determinism: DetManifest,
}

/// Execute all four compile-time MIR verifiers in one deterministic pass and
/// seal their digests under `sovereign_root_sha256`.
pub fn run_sovereign_mir_audit(
    module: &MirModule,
    module_label: &str,
    config: &SovereignAuditConfig,
) -> Result<SovereignAuditManifest, String> {
    let effect = audit_mir_effects(module, module_label, &config.denied_effects)?;
    let wcet = audit_mir_wcet_stack(module, module_label, Some(config.max_stack_bytes))?;
    let constant_time = audit_mir_constant_time(module, module_label)?;
    let determinism = audit_mir_determinism(module, module_label)?;

    let pillar_statuses = vec![
        SovereignPillarStatus {
            pillar: "cap_effect".to_string(),
            passed: effect.passed,
            violation_count: effect.violations.len(),
            manifest_sha256: effect.manifest_sha256.clone(),
        },
        SovereignPillarStatus {
            pillar: "wcet_stack".to_string(),
            passed: wcet.passed,
            violation_count: wcet.violations.len(),
            manifest_sha256: wcet.manifest_sha256.clone(),
        },
        SovereignPillarStatus {
            pillar: "constant_time".to_string(),
            passed: constant_time.passed,
            violation_count: constant_time.violations.len(),
            manifest_sha256: constant_time.manifest_sha256.clone(),
        },
        SovereignPillarStatus {
            pillar: "strict_determinism".to_string(),
            passed: determinism.passed,
            violation_count: determinism.violations.len(),
            manifest_sha256: determinism.manifest_sha256.clone(),
        },
    ];

    let failing_pillars: Vec<String> = pillar_statuses
        .iter()
        .filter(|p| !p.passed)
        .map(|p| p.pillar.clone())
        .collect();
    let pillars_total = pillar_statuses.len() as u32;
    let pillars_passed = pillars_total - (failing_pillars.len() as u32);
    let passed = failing_pillars.is_empty();

    let sovereign_root_sha256 = compute_sovereign_root_sha256(
        &effect.manifest_sha256,
        &wcet.manifest_sha256,
        &constant_time.manifest_sha256,
        &determinism.manifest_sha256,
    );

    Ok(SovereignAuditManifest {
        schema: SOVEREIGN_AUDIT_SCHEMA.to_string(),
        module: module_label.to_string(),
        passed,
        pillars_passed,
        pillars_total,
        failing_pillars,
        pillar_statuses,
        sovereign_root_sha256,
        effect,
        wcet,
        constant_time,
        determinism,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{
        BinOp, BlockId, LocalId, MirBlock, MirConst, MirFnSig, MirFunction, MirLocal, MirRValue,
        MirStmt, MirTerminator, MirType, MirValue,
    };

    #[test]
    fn clean_pure_ct_det_module_passes_all_four_pillars_with_reproducible_root() {
        let mut m = MirModule::new("sovereign_ok");
        let mut f = MirFunction::new(
            "ct_add",
            MirFnSig::new(vec![MirType::u64(), MirType::u64()], MirType::u64()),
        );
        let mut p0 = MirLocal::named(LocalId(0), "a", MirType::u64());
        p0.is_param = true;
        let mut p1 = MirLocal::named(LocalId(1), "b", MirType::u64());
        p1.is_param = true;
        f.add_local(p0);
        f.add_local(p1);
        f.add_local(MirLocal::new(LocalId(2), MirType::u64()));
        let mut bb = MirBlock::new(BlockId(0));
        bb.push_stmt(MirStmt::assign(
            LocalId(2),
            MirRValue::BinaryOp {
                op: BinOp::BitXor,
                left: MirValue::Local(LocalId(0)),
                right: MirValue::Local(LocalId(1)),
            },
        ));
        bb.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(2)))));
        f.add_block(bb);
        m.add_function(f);

        let cfg = SovereignAuditConfig::default();
        let r1 = run_sovereign_mir_audit(&m, "sovereign_ok.bld", &cfg).unwrap();
        let r2 = run_sovereign_mir_audit(&m, "sovereign_ok.bld", &cfg).unwrap();
        assert!(r1.passed);
        assert_eq!(r1.pillars_passed, 4);
        assert_eq!(r1.pillars_total, 4);
        assert!(r1.failing_pillars.is_empty());
        assert_eq!(r1.sovereign_root_sha256, r2.sovereign_root_sha256);
        assert_eq!(
            compute_sovereign_root_sha256(
                &r1.effect.manifest_sha256,
                &r1.wcet.manifest_sha256,
                &r1.constant_time.manifest_sha256,
                &r1.determinism.manifest_sha256,
            ),
            r1.sovereign_root_sha256
        );
    }

    #[test]
    fn single_pillar_float_eq_failure_isolates_strict_determinism_pillar() {
        let mut m = MirModule::new("det_only_fail");
        let mut f = MirFunction::new(
            "bad_eq",
            MirFnSig::new(vec![MirType::f64(), MirType::f64()], MirType::Bool),
        );
        let mut p0 = MirLocal::named(LocalId(0), "x", MirType::f64());
        p0.is_param = true;
        let mut p1 = MirLocal::named(LocalId(1), "y", MirType::f64());
        p1.is_param = true;
        f.add_local(p0);
        f.add_local(p1);
        f.add_local(MirLocal::new(LocalId(2), MirType::Bool));
        let mut bb = MirBlock::new(BlockId(0));
        bb.push_stmt(MirStmt::assign(
            LocalId(2),
            MirRValue::BinaryOp {
                op: BinOp::Eq,
                left: MirValue::Local(LocalId(0)),
                right: MirValue::Local(LocalId(1)),
            },
        ));
        bb.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(2)))));
        f.add_block(bb);
        m.add_function(f);

        let cfg = SovereignAuditConfig::default();
        let res = run_sovereign_mir_audit(&m, "det_only_fail.bld", &cfg).unwrap();
        assert!(!res.passed);
        assert_eq!(res.pillars_passed, 3);
        assert_eq!(res.failing_pillars, vec!["strict_determinism"]);
    }

    #[test]
    fn zero_stack_budget_rejected_fail_closed() {
        let m = MirModule::new("empty");
        let cfg = SovereignAuditConfig {
            denied_effects: default_sovereign_denied_effects(),
            max_stack_bytes: 0,
        };
        let _ = MirConst::Unit;
        assert!(run_sovereign_mir_audit(&m, "empty.bld", &cfg).is_err());
    }
}
