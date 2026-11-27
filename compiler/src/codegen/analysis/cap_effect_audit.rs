// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 130C: Interprocedural MIR Capability & Effect Call-Graph Verifier.
//!
//! Computes exact transitive capability effects (`Pure`, `Console`, `Clock`,
//! `Fs`, `Net`, `Exec`, `Unsafe`) across the full MIR call graph at compile
//! time with zero runtime overhead, emitting deterministic SHA-256-sealed
//! `buildlang.effect-manifest/v1` manifests and fail-closed call-chain
//! diagnostics when any function transitively reaches a `--deny-effect`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::codegen::ir::{
    CastKind, MirFunction, MirModule, MirRValue, MirStmtKind, MirTerminator, MirType, MirValue,
};

/// Canonical schema identifier for serialized effect manifests.
pub const EFFECT_MANIFEST_SCHEMA: &str = "buildlang.effect-manifest/v1";

/// Concrete non-pure capability effects tracked across the MIR call graph.
///
/// A function or module with an empty set of `CapEffect`s is `Pure`
/// (serialized as `["pure"]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapEffect {
    Console,
    Clock,
    Fs,
    Net,
    Exec,
    Unsafe,
}

impl CapEffect {
    /// Lowercase canonical identifier used on the CLI and in JSON manifests.
    pub const fn as_str(self) -> &'static str {
        match self {
            CapEffect::Console => "console",
            CapEffect::Clock => "clock",
            CapEffect::Fs => "fs",
            CapEffect::Net => "net",
            CapEffect::Exec => "exec",
            CapEffect::Unsafe => "unsafe",
        }
    }

    /// Parse a single effect token (case-insensitive).
    pub fn parse_one(token: &str) -> Result<Self, String> {
        match token.trim().to_ascii_lowercase().as_str() {
            "console" | "stdio" => Ok(CapEffect::Console),
            "clock" | "time" => Ok(CapEffect::Clock),
            "fs" | "file" | "filesystem" => Ok(CapEffect::Fs),
            "net" | "network" => Ok(CapEffect::Net),
            "exec" | "process" | "env" => Ok(CapEffect::Exec),
            "unsafe" | "ffi" => Ok(CapEffect::Unsafe),
            "pure" => Err(
                "`pure` is the absence of effects and cannot be denied; valid effects: console, clock, fs, net, exec, unsafe"
                    .to_string(),
            ),
            other => Err(format!(
                "unknown effect `{other}` in --deny-effect; valid effects: console, clock, fs, net, exec, unsafe"
            )),
        }
    }

    /// Parse a comma-separated `--deny-effect` specification fail-closed.
    pub fn parse_deny_list(spec: &str) -> Result<BTreeSet<Self>, String> {
        let mut out = BTreeSet::new();
        for raw in spec.split(',') {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            out.insert(Self::parse_one(trimmed)?);
        }
        if out.is_empty() {
            return Err(
                "--deny-effect requires at least one effect name (console, clock, fs, net, exec, unsafe)"
                    .to_string(),
            );
        }
        Ok(out)
    }
}

impl fmt::Display for CapEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-function deterministic capability and effect summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionEffectRecord {
    pub name: String,
    pub is_pure: bool,
    pub direct_effects: Vec<String>,
    pub transitive_effects: Vec<String>,
    pub callees: Vec<String>,
    /// Shortest deterministic call chain from this function to each transitive effect's trigger.
    pub witnesses: BTreeMap<String, Vec<String>>,
}

/// Fail-closed violation when a function transitively reaches a denied effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectViolation {
    pub function: String,
    pub effect: String,
    pub call_chain: Vec<String>,
    pub trigger: String,
    pub message: String,
}

/// Canonical SHA-256-sealed interprocedural effect manifest for a MIR module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectManifest {
    pub schema: String,
    pub module: String,
    pub zero_egress: bool,
    pub module_effects: Vec<String>,
    pub denied_effects: Vec<String>,
    pub passed: bool,
    pub functions: Vec<FunctionEffectRecord>,
    pub violations: Vec<EffectViolation>,
    pub manifest_sha256: String,
}

#[derive(Serialize)]
struct ManifestDigestPayload<'a> {
    schema: &'a str,
    module: &'a str,
    zero_egress: bool,
    module_effects: &'a [String],
    denied_effects: &'a [String],
    passed: bool,
    functions: &'a [FunctionEffectRecord],
    violations: &'a [EffectViolation],
}

/// Compute the deterministic SHA-256 digest of an [`EffectManifest`].
pub fn compute_manifest_sha256(manifest: &EffectManifest) -> Result<String, String> {
    let payload = ManifestDigestPayload {
        schema: &manifest.schema,
        module: &manifest.module,
        zero_egress: manifest.zero_egress,
        module_effects: &manifest.module_effects,
        denied_effects: &manifest.denied_effects,
        passed: manifest.passed,
        functions: &manifest.functions,
        violations: &manifest.violations,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|e| format!("effect manifest serialize error: {e}"))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Classify a known runtime or C library symbol into its exact capability effect.
/// Returns `Some(None)` for known pure math/container/runtime helpers,
/// `Some(Some(eff))` for known effectful runtime/libc symbols, and `None`
/// for unknown external/FFI symbols (which fail closed as `Unsafe`).
fn classify_known_symbol(name: &str) -> Option<Option<CapEffect>> {
    // 1. Console / stdio / command-line args
    if matches!(
        name,
        "printf"
            | "fprintf"
            | "puts"
            | "putchar"
            | "getchar"
            | "fgets"
            | "scanf"
            | "fflush"
            | "print"
            | "println"
            | "eprint"
            | "eprintln"
            | "build_read_line"
            | "build_read_all"
            | "build_stdin_is_pipe"
            | "build_args_count"
            | "build_args_get"
            | "build_args_init"
            | "__build_init_io"
    ) || name.starts_with("build_print_")
        || name.starts_with("build_println_")
        || name.starts_with("build_eprint")
    {
        return Some(Some(CapEffect::Console));
    }

    // 2. Clock / wall-clock / sleep
    if matches!(
        name,
        "time"
            | "clock"
            | "gettimeofday"
            | "clock_gettime"
            | "nanosleep"
            | "usleep"
            | "sleep"
            | "build_clock_ms"
            | "build_clock_ns"
            | "build_time_unix"
    ) || name.starts_with("build_clock_")
        || name.starts_with("build_time_")
        || name.starts_with("build_sleep_")
    {
        return Some(Some(CapEffect::Clock));
    }

    // 3. Filesystem I/O
    if matches!(
        name,
        "fopen"
            | "fclose"
            | "fread"
            | "fwrite"
            | "fseek"
            | "ftell"
            | "remove"
            | "rename"
            | "unlink"
            | "mkdir"
            | "rmdir"
            | "stat"
            | "opendir"
            | "readdir"
            | "open"
            | "close"
            | "read"
            | "write"
            | "fsync"
            | "read_file"
            | "write_file"
            | "append_file"
            | "file_exists"
            | "read_bytes"
            | "write_bytes"
            | "read_file_bytes"
            | "write_file_bytes"
            | "append_file_bytes"
            | "read_file_at"
            | "write_file_at"
            | "remove_file"
            | "rename_file"
            | "fsync_file"
            | "list_dir"
            | "is_dir"
            | "file_size"
            | "build_read_file"
            | "build_write_file"
            | "build_append_file"
            | "build_file_exists"
            | "build_read_bytes"
            | "build_write_bytes"
            | "build_read_file_bytes"
            | "build_write_file_bytes"
            | "build_append_file_bytes"
            | "build_read_file_at"
            | "build_write_file_at"
            | "build_remove_file"
            | "build_rename_file"
            | "build_fsync_file"
            | "build_list_dir"
            | "build_is_dir"
            | "build_file_size"
            | "build_vk_load_shader_file"
    ) {
        return Some(Some(CapEffect::Fs));
    }

    // 4. Network / socket egress
    if matches!(
        name,
        "socket"
            | "connect"
            | "bind"
            | "listen"
            | "accept"
            | "send"
            | "sendto"
            | "recv"
            | "recvfrom"
            | "getaddrinfo"
            | "tcp_connect"
            | "tcp_send"
            | "tcp_recv"
            | "tcp_close"
    ) || name.starts_with("build_tcp_")
        || name.starts_with("build_udp_")
        || name.starts_with("build_http_")
        || name.starts_with("build_net_")
        || name.starts_with("curl_")
    {
        return Some(Some(CapEffect::Net));
    }

    // 5. Process execution, exit, and host environment
    if matches!(
        name,
        "system"
            | "popen"
            | "execv"
            | "execve"
            | "execvp"
            | "fork"
            | "spawn"
            | "posix_spawn"
            | "exit"
            | "_exit"
            | "abort"
            | "getenv"
            | "setenv"
            | "process_exit"
            | "build_exit"
            | "build_process_exit"
            | "build_getenv"
    ) || name.starts_with("build_exec_")
        || name.starts_with("build_spawn_")
        || name.starts_with("build_cmd_")
    {
        return Some(Some(CapEffect::Exec));
    }

    // 6. Explicit unsafe memory / MMIO helpers
    if matches!(
        name,
        "ptr_read"
            | "ptr_write"
            | "volatile_read"
            | "volatile_write"
            | "build_ptr_read"
            | "build_ptr_write"
    ) || name.starts_with("mmio_")
        || name.starts_with("build_mmio_")
    {
        return Some(Some(CapEffect::Unsafe));
    }

    // 7. Known pure math, checked-arith, string, container, and vector helpers
    if matches!(
        name,
        "abs"
            | "labs"
            | "llabs"
            | "fabs"
            | "fabsf"
            | "sqrt"
            | "sqrtf"
            | "cbrt"
            | "sin"
            | "sinf"
            | "cos"
            | "cosf"
            | "tan"
            | "tanf"
            | "asin"
            | "asinf"
            | "acos"
            | "acosf"
            | "atan"
            | "atanf"
            | "atan2"
            | "atan2f"
            | "sinh"
            | "cosh"
            | "tanh"
            | "exp"
            | "expf"
            | "exp2"
            | "exp2f"
            | "log"
            | "logf"
            | "log2"
            | "log2f"
            | "log10"
            | "pow"
            | "powf"
            | "floor"
            | "floorf"
            | "ceil"
            | "ceilf"
            | "round"
            | "roundf"
            | "trunc"
            | "truncf"
            | "fmod"
            | "fmodf"
            | "fmin"
            | "fminf"
            | "fmax"
            | "fmaxf"
            | "fma"
            | "fmaf"
            | "hypot"
            | "copysign"
            | "erf"
            | "erfc"
            | "min"
            | "max"
            | "clamp"
            | "strlen"
            | "strcmp"
            | "strncmp"
            | "memcpy"
            | "memmove"
            | "memset"
            | "memcmp"
            | "malloc"
            | "calloc"
            | "realloc"
            | "free"
            | "atoi"
            | "atof"
            | "atol"
            | "strtol"
            | "strtod"
            | "isalpha"
            | "isdigit"
            | "isalnum"
            | "isspace"
            | "toupper"
            | "tolower"
            | "snprintf"
            | "sprintf"
    ) || name.starts_with("bl_")
        || name.starts_with("build_min_")
        || name.starts_with("build_max_")
        || name.starts_with("build_clamp")
        || name.starts_with("build_dot")
        || name.starts_with("build_cross")
        || name.starts_with("build_normalize")
        || name.starts_with("build_length")
        || name.starts_with("build_reflect")
        || name.starts_with("build_mix")
        || name.starts_with("build_lerp")
        || name.starts_with("build_smoothstep")
        || name.starts_with("build_fract")
        || name.starts_with("build_step")
        || name.starts_with("build_saturate")
        || name.starts_with("build_vec")
        || name.starts_with("build_mat")
        || name.starts_with("build_hvec_")
        || name.starts_with("build_map_")
        || name.starts_with("build_hmap_")
        || name.starts_with("build_str_")
        || name.starts_with("build_string_")
        || name.starts_with("build_i32_to_string")
        || name.starts_with("build_i64_to_string")
        || name.starts_with("build_f32_to_string")
        || name.starts_with("build_f64_to_string")
        || name.starts_with("build_f32_")
        || name.starts_with("build_f64_")
        || name.starts_with("build_det_")
        || name.starts_with("build_rem_euclid_")
        || name.starts_with("build_panic")
        || name.starts_with("Vec_")
        || name.starts_with("String_")
        || name.starts_with("HashMap_")
        || name.starts_with("HashSet_")
        || name.starts_with("Map_")
        || name.starts_with("Set_")
        || name.starts_with("Option_")
        || name.starts_with("Result_")
        || name.starts_with("Box_")
        || name.starts_with("build_assert")
    {
        return Some(None);
    }

    None
}

struct FunctionScan {
    direct_effects: BTreeMap<CapEffect, String>,
    callees: BTreeSet<String>,
}

fn scan_rvalue_effects(
    rv: &MirRValue,
    func: &MirFunction,
    direct: &mut BTreeMap<CapEffect, String>,
) {
    if let MirRValue::Cast { kind, value, ty } = rv {
        match kind {
            CastKind::IntToPtr => {
                direct
                    .entry(CapEffect::Unsafe)
                    .or_insert_with(|| "op:int_to_ptr".to_string());
            }
            CastKind::PtrToInt => {
                direct
                    .entry(CapEffect::Unsafe)
                    .or_insert_with(|| "op:ptr_to_int".to_string());
            }
            CastKind::Transmute => {
                let src_ty = match value {
                    MirValue::Local(id) => func.locals.iter().find(|l| l.id == *id).map(|l| &l.ty),
                    _ => None,
                };
                // Safe bool<->int or scalar numeric transmutes are pure; pointer transmutes are Unsafe.
                let involves_ptr =
                    matches!(ty, MirType::Ptr(_)) || matches!(src_ty, Some(MirType::Ptr(_)));
                if involves_ptr {
                    direct
                        .entry(CapEffect::Unsafe)
                        .or_insert_with(|| "op:ptr_transmute".to_string());
                }
            }
            _ => {}
        }
    }
}

fn scan_function(
    func: &MirFunction,
    defined_fns: &BTreeSet<String>,
    extern_fns: &BTreeSet<String>,
) -> FunctionScan {
    let mut direct_effects = BTreeMap::new();
    let mut callees = BTreeSet::new();

    if func.blocks.is_none() {
        // Extern/declaration-only function
        if let Some(known) = classify_known_symbol(func.name.as_ref()) {
            if let Some(eff) = known {
                direct_effects.insert(eff, format!("extern:{}", func.name));
            }
        } else {
            direct_effects.insert(CapEffect::Unsafe, format!("extern:{}", func.name));
        }
        return FunctionScan {
            direct_effects,
            callees,
        };
    }

    if let Some(blocks) = &func.blocks {
        for block in blocks {
            for stmt in &block.stmts {
                match &stmt.kind {
                    MirStmtKind::Assign { value, .. }
                    | MirStmtKind::DerefAssign { value, .. }
                    | MirStmtKind::FieldDerefAssign { value, .. }
                    | MirStmtKind::FieldAssign { value, .. }
                    | MirStmtKind::IndexStore { value, .. }
                    | MirStmtKind::GlobalStore { value, .. } => {
                        scan_rvalue_effects(value, func, &mut direct_effects);
                    }
                    MirStmtKind::WorkgroupBarrier
                    | MirStmtKind::StorageLive(_)
                    | MirStmtKind::StorageDead(_)
                    | MirStmtKind::Nop => {}
                }
            }

            if let Some(MirTerminator::Call { func: callee, .. }) = &block.terminator {
                match callee {
                    MirValue::Function(name) | MirValue::Global(name) => {
                        let cname = name.as_ref();
                        if defined_fns.contains(cname) {
                            callees.insert(cname.to_string());
                        } else if let Some(known) = classify_known_symbol(cname) {
                            if let Some(eff) = known {
                                direct_effects
                                    .entry(eff)
                                    .or_insert_with(|| format!("call:{cname}"));
                            }
                        } else if extern_fns.contains(cname) {
                            callees.insert(cname.to_string());
                        } else {
                            // Unrecognized external call fails closed as Unsafe.
                            direct_effects
                                .entry(CapEffect::Unsafe)
                                .or_insert_with(|| format!("extern_call:{cname}"));
                        }
                    }
                    MirValue::Local(_) | MirValue::Const(_) => {
                        // Indirect function-pointer call cannot be statically bounded:
                        // fail closed as Unsafe.
                        direct_effects
                            .entry(CapEffect::Unsafe)
                            .or_insert_with(|| "op:indirect_call".to_string());
                    }
                }
            }
        }
    }

    FunctionScan {
        direct_effects,
        callees,
    }
}

fn effects_to_strings(effects: &BTreeSet<CapEffect>) -> Vec<String> {
    if effects.is_empty() {
        vec!["pure".to_string()]
    } else {
        effects.iter().map(|e| e.as_str().to_string()).collect()
    }
}

/// Perform exact interprocedural capability and effect analysis over a MIR module,
/// computing transitive effects, shortest deterministic call-chain witnesses,
/// zero-egress status, fail-closed `--deny-effect` violations, and SHA-256 seal.
pub fn audit_module(
    module: &MirModule,
    module_label: &str,
    denied_effects: &BTreeSet<CapEffect>,
) -> Result<EffectManifest, String> {
    let mut defined_fns = BTreeSet::new();
    let mut extern_fns = BTreeSet::new();
    for f in &module.functions {
        if f.blocks.is_some() {
            defined_fns.insert(f.name.to_string());
        } else {
            extern_fns.insert(f.name.to_string());
        }
    }

    let mut scans: BTreeMap<String, FunctionScan> = BTreeMap::new();
    for f in &module.functions {
        let name = f.name.to_string();
        let scan = scan_function(f, &defined_fns, &extern_fns);
        // Merge if duplicate declaration + definition exist; definition wins.
        if let Some(existing) = scans.get_mut(&name) {
            if f.blocks.is_some() {
                *existing = scan;
            }
        } else {
            scans.insert(name, scan);
        }
    }

    // Build reverse call graph for deterministic BFS witness computation.
    let mut callers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (fname, scan) in &scans {
        for callee in &scan.callees {
            callers
                .entry(callee.clone())
                .or_default()
                .insert(fname.clone());
        }
    }

    // For each effect, run deterministic multi-source BFS backwards along the call graph
    // to find EVERY function that transitively reaches `effect` AND its shortest call chain!
    let all_effects = [
        CapEffect::Console,
        CapEffect::Clock,
        CapEffect::Fs,
        CapEffect::Net,
        CapEffect::Exec,
        CapEffect::Unsafe,
    ];
    let mut trans_witnesses: BTreeMap<String, BTreeMap<CapEffect, Vec<String>>> = BTreeMap::new();
    for fname in scans.keys() {
        trans_witnesses.insert(fname.clone(), BTreeMap::new());
    }

    for eff in all_effects {
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut best_chain: BTreeMap<String, Vec<String>> = BTreeMap::new();

        // Seed with all functions that directly trigger `eff` (sorted order via BTreeMap).
        for (fname, scan) in &scans {
            if let Some(trigger) = scan.direct_effects.get(&eff) {
                best_chain.insert(fname.clone(), vec![fname.clone(), trigger.clone()]);
                queue.push_back(fname.clone());
            }
        }

        while let Some(callee) = queue.pop_front() {
            let callee_chain = best_chain[&callee].clone();
            if let Some(caller_set) = callers.get(&callee) {
                for caller in caller_set {
                    if !best_chain.contains_key(caller) {
                        let mut chain = Vec::with_capacity(callee_chain.len() + 1);
                        chain.push(caller.clone());
                        chain.extend(callee_chain.iter().cloned());
                        best_chain.insert(caller.clone(), chain);
                        queue.push_back(caller.clone());
                    }
                }
            }
        }

        for (fname, chain) in best_chain {
            if let Some(slot) = trans_witnesses.get_mut(&fname) {
                slot.insert(eff, chain);
            }
        }
    }

    let mut module_effect_set = BTreeSet::new();
    let mut functions = Vec::with_capacity(scans.len());
    let mut violations = Vec::new();

    for (fname, scan) in &scans {
        // Only include defined functions (and externs explicitly referenced) in manifest.
        if !defined_fns.contains(fname) && callers.get(fname).is_none_or(BTreeSet::is_empty) {
            continue;
        }
        let direct_set: BTreeSet<CapEffect> = scan.direct_effects.keys().copied().collect();
        let witness_map = trans_witnesses.get(fname).cloned().unwrap_or_default();
        let trans_set: BTreeSet<CapEffect> = witness_map.keys().copied().collect();
        module_effect_set.extend(trans_set.iter().copied());

        let mut str_witnesses = BTreeMap::new();
        for (eff, chain) in &witness_map {
            str_witnesses.insert(eff.as_str().to_string(), chain.clone());
            if denied_effects.contains(eff) {
                let trigger = chain
                    .last()
                    .cloned()
                    .unwrap_or_else(|| eff.as_str().to_string());
                let chain_str = chain.join(" -> ");
                violations.push(EffectViolation {
                    function: fname.clone(),
                    effect: eff.as_str().to_string(),
                    call_chain: chain.clone(),
                    trigger: trigger.clone(),
                    message: format!(
                        "denied effect `{}` reachable from `{}` via {}",
                        eff.as_str(),
                        fname,
                        chain_str
                    ),
                });
            }
        }

        functions.push(FunctionEffectRecord {
            name: fname.clone(),
            is_pure: trans_set.is_empty(),
            direct_effects: effects_to_strings(&direct_set),
            transitive_effects: effects_to_strings(&trans_set),
            callees: scan.callees.iter().cloned().collect(),
            witnesses: str_witnesses,
        });
    }

    let zero_egress = !module_effect_set.contains(&CapEffect::Net)
        && !module_effect_set.contains(&CapEffect::Fs)
        && !module_effect_set.contains(&CapEffect::Exec);

    let mut manifest = EffectManifest {
        schema: EFFECT_MANIFEST_SCHEMA.to_string(),
        module: module_label.to_string(),
        zero_egress,
        module_effects: effects_to_strings(&module_effect_set),
        denied_effects: denied_effects
            .iter()
            .map(|e| e.as_str().to_string())
            .collect(),
        passed: violations.is_empty(),
        functions,
        violations,
        manifest_sha256: String::new(),
    };
    manifest.manifest_sha256 = compute_manifest_sha256(&manifest)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::{BlockId, MirBlock, MirFnSig};
    use std::sync::Arc;

    fn make_caller_fn(name: &str, callee: &str) -> MirFunction {
        let mut f = MirFunction::new(name, MirFnSig::new(vec![], MirType::Void));
        let mut bb = MirBlock::new(BlockId::ENTRY);
        bb.set_terminator(MirTerminator::Call {
            func: MirValue::Function(Arc::from(callee)),
            args: vec![],
            dest: None,
            target: None,
            unwind: None,
        });
        f.add_block(bb);
        f
    }

    #[test]
    fn three_hop_transitive_net_effect_reports_exact_call_chain() {
        let mut module = MirModule::new("three_hop");
        module.add_function(make_caller_fn("main", "hop_a"));
        module.add_function(make_caller_fn("hop_a", "hop_b"));
        module.add_function(make_caller_fn("hop_b", "build_tcp_connect"));

        let denied = CapEffect::parse_deny_list("net,fs,exec").unwrap();
        let manifest = audit_module(&module, "three_hop.bld", &denied).unwrap();

        assert!(!manifest.passed);
        assert!(!manifest.zero_egress);
        let main_v = manifest
            .violations
            .iter()
            .find(|v| v.function == "main" && v.effect == "net")
            .expect("main must report transitive net violation");
        assert_eq!(
            main_v.call_chain,
            vec!["main", "hop_a", "hop_b", "call:build_tcp_connect"]
        );
        assert_eq!(
            compute_manifest_sha256(&manifest).unwrap(),
            manifest.manifest_sha256
        );
    }

    #[test]
    fn pure_module_is_zero_egress_and_passes_strict_deny_list() {
        let mut module = MirModule::new("pure_math");
        module.add_function(make_caller_fn("main", "sqrt"));

        let denied = CapEffect::parse_deny_list("console,clock,fs,net,exec,unsafe").unwrap();
        let manifest = audit_module(&module, "pure_math.bld", &denied).unwrap();

        assert!(manifest.passed);
        assert!(manifest.zero_egress);
        assert_eq!(manifest.module_effects, vec!["pure"]);
    }

    #[test]
    fn parse_deny_list_rejects_unknown_and_pure_fail_closed() {
        assert!(CapEffect::parse_deny_list("bogus").is_err());
        assert!(CapEffect::parse_deny_list("pure").is_err());
        assert!(CapEffect::parse_deny_list("").is_err());
    }
}
