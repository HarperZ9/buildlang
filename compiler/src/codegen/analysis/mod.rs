//! Reusable MIR dataflow substrate: CFG queries, use-def scans, move tracking,
//! and (Task 2) backward liveness. Consumed by the C backend's drop insertion
//! and, later, by the MIR affine/linear checker. Everything here is a pure
//! function of MIR (`codegen::ir`); nothing mutates.

pub mod cap_effect_audit;
pub(crate) mod cfg;
pub mod ct_taint_audit;
pub mod det_repro_audit;
pub(crate) mod drops;
pub(crate) mod flags;
pub(crate) mod int_overflow;
pub(crate) mod linear;
pub(crate) mod liveness;
pub mod sovereign_audit;
pub(crate) mod structured_cfg;
pub mod wcet_stack_audit;
