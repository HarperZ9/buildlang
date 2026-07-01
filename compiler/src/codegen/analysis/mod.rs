//! Reusable MIR dataflow substrate: CFG queries, use-def scans, move tracking,
//! and (Task 2) backward liveness. Consumed by the C backend's drop insertion
//! and, later, by the MIR affine/linear checker. Everything here is a pure
//! function of MIR (`codegen::ir`); nothing mutates.

pub(crate) mod cfg;
// pub(crate) mod liveness; // Task 2
// pub(crate) mod drops;    // Task 4
