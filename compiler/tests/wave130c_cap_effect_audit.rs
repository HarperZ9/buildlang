// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! Wave 130C Interprocedural MIR Capability & Effect Call-Graph Verifier tests:
//! - Pure and zero-egress kernels passing `--deny-effect net,fs,exec`
//! - Indirect 3-hop hidden effect violations failing closed with exact call-chain trace
//! - Bit-exact deterministic `--effect-manifest-json` output across repeated runs
//! - Live zero-egress sweep over every file in `universe/buildos-core/`

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use buildlang::codegen::{compute_manifest_sha256, EffectManifest, EFFECT_MANIFEST_SCHEMA};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate lives under repo root")
        .to_path_buf()
}

fn temp_dir(tag: &str) -> PathBuf {
    let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "buildlang_wave130c_{}_{}_{}",
        std::process::id(),
        n,
        tag
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_bld(dir: &Path, name: &str, src: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, src).expect("write source");
    p
}

fn diag(out: &Output) -> String {
    format!(
        "status: {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn pure_and_zero_egress_kernels_pass_deny_net_fs_exec() {
    let dir = temp_dir("pure_zero_egress");

    // 1. Strictly pure arithmetic kernel passes denying ALL 6 non-pure effects.
    let pure_src = r#"
fn mix_u32(a: u32, b: u32) -> u32 {
    let x: u32 = a.wrapping_add(b);
    let y: u32 = (x << 5u32) ^ (x >> 3u32);
    y
}

fn main() -> i32 {
    let r: u32 = mix_u32(17u32, 42u32);
    if r == 0u32 { 1 } else { 0 }
}
"#;
    let pure_p = write_bld(&dir, "pure_kernel.bld", pure_src);
    let out = buildc()
        .args([
            "check",
            pure_p.to_str().unwrap(),
            "--deny-effect",
            "console,clock,fs,net,exec,unsafe",
            "--effect-manifest-json",
        ])
        .output()
        .expect("run buildc check");
    assert!(out.status.success(), "pure kernel failed:\n{}", diag(&out));
    let manifest: EffectManifest =
        serde_json::from_slice(&out.stdout).expect("stdout must be valid EffectManifest JSON");
    assert_eq!(manifest.schema, EFFECT_MANIFEST_SCHEMA);
    assert!(manifest.passed);
    assert!(manifest.zero_egress);
    assert_eq!(manifest.module_effects, vec!["pure"]);
    assert!(manifest.violations.is_empty());
    assert_eq!(
        compute_manifest_sha256(&manifest).unwrap(),
        manifest.manifest_sha256
    );

    // 2. Console-only kernel passes `--deny-effect net,fs,exec,unsafe` (zero egress)
    //    but fails closed under `--deny-effect console`.
    let console_src = r#"
fn helper(v: i32) -> i32 {
    v * 2 + 1
}

fn main() -> i32 ~ Console {
    println!("val={}", helper(21));
    0
}
"#;
    let console_p = write_bld(&dir, "console_kernel.bld", console_src);
    let ok_out = buildc()
        .args([
            "check",
            console_p.to_str().unwrap(),
            "--deny-effect",
            "net,fs,exec,unsafe",
            "--effect-manifest-json",
        ])
        .output()
        .expect("run buildc check");
    assert!(
        ok_out.status.success(),
        "console-only kernel must pass zero-egress deny list:\n{}",
        diag(&ok_out)
    );
    let cm: EffectManifest = serde_json::from_slice(&ok_out.stdout).unwrap();
    assert!(cm.passed);
    assert!(cm.zero_egress);
    assert_eq!(cm.module_effects, vec!["console"]);

    let deny_console = buildc()
        .args([
            "check",
            console_p.to_str().unwrap(),
            "--deny-effect",
            "console",
        ])
        .output()
        .expect("run buildc check");
    assert!(
        !deny_console.status.success(),
        "console kernel must fail closed under --deny-effect console:\n{}",
        diag(&deny_console)
    );
}

#[test]
fn indirect_3_hop_hidden_effects_fail_closed_with_exact_call_chain() {
    let dir = temp_dir("three_hop_hidden");

    // 1. 3-hop hidden Filesystem violation: `main -> hop_a -> hop_b -> forbidden_fs`
    let fs_src = r#"
fn forbidden_fs() -> bool ~ FileSystem {
    file_exists("secret.key")
}

fn hop_b() -> i32 ~ FileSystem {
    if forbidden_fs() { 1 } else { 0 }
}

fn hop_a() -> i32 ~ FileSystem {
    hop_b() + 1
}

fn main() -> i32 ~ FileSystem {
    hop_a()
}
"#;
    let fs_p = write_bld(&dir, "hidden_fs_3hop.bld", fs_src);
    let res = buildc()
        .args([
            "check",
            fs_p.to_str().unwrap(),
            "--deny-effect",
            "net,fs,exec",
            "--effect-manifest-json",
        ])
        .output()
        .expect("run buildc check");
    assert!(
        !res.status.success(),
        "3-hop hidden Fs call MUST fail closed under --deny-effect net,fs,exec:\n{}",
        diag(&res)
    );
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(
        stderr.contains("main -> hop_a -> hop_b -> forbidden_fs -> call:build_file_exists"),
        "stderr must print exact 3-hop call chain:\n{stderr}"
    );
    let manifest: EffectManifest =
        serde_json::from_slice(&res.stdout).expect("manifest JSON must still be emitted");
    assert!(!manifest.passed);
    assert!(!manifest.zero_egress);
    let main_v = manifest
        .violations
        .iter()
        .find(|v| v.function == "main" && v.effect == "fs")
        .expect("main must have an fs violation");
    assert_eq!(
        main_v.call_chain,
        vec![
            "main",
            "hop_a",
            "hop_b",
            "forbidden_fs",
            "call:build_file_exists"
        ]
    );

    // 2. 3-hop hidden Network egress violation via `extern "C"` socket / tcp symbol:
    let net_src = r#"
extern "C" {
    fn build_tcp_connect(host: &str, port: i32) -> i32;
}

fn exfil() -> i32 ~ Foreign + Network {
    build_tcp_connect("127.0.0.1", 443)
}

fn layer_b() -> i32 ~ Foreign + Network {
    exfil()
}

fn layer_a() -> i32 ~ Foreign + Network {
    layer_b()
}

fn main() -> i32 ~ Foreign + Network {
    layer_a()
}
"#;
    let net_p = write_bld(&dir, "hidden_net_3hop.bld", net_src);
    let net_res = buildc()
        .args([
            "check",
            net_p.to_str().unwrap(),
            "--deny-effect",
            "net",
            "--effect-manifest-json",
        ])
        .output()
        .expect("run buildc check");
    assert!(
        !net_res.status.success(),
        "3-hop hidden Net call MUST fail closed under --deny-effect net:\n{}",
        diag(&net_res)
    );
    let net_manifest: EffectManifest =
        serde_json::from_slice(&net_res.stdout).unwrap_or_else(|e| {
            panic!(
                "expected manifest JSON on net violation ({e}):\n{}",
                diag(&net_res)
            )
        });
    assert!(!net_manifest.passed);
    assert!(!net_manifest.zero_egress);
    let la_v = net_manifest
        .violations
        .iter()
        .find(|v| v.function == "main" && v.effect == "net")
        .expect("main must have a net violation");
    assert_eq!(
        la_v.call_chain,
        vec![
            "main",
            "layer_a",
            "layer_b",
            "exfil",
            "call:build_tcp_connect"
        ]
    );

    // 3. Invalid --deny-effect names fail closed immediately.
    for bad in ["bogus", "pure", ""] {
        let bad_res = buildc()
            .args(["check", fs_p.to_str().unwrap(), "--deny-effect", bad])
            .output()
            .expect("run buildc check");
        assert!(
            !bad_res.status.success(),
            "--deny-effect `{bad}` must fail closed"
        );
    }
}

#[test]
fn effect_manifest_json_is_deterministic_across_repeated_runs() {
    let dir = temp_dir("determinism");
    let src = r#"
fn step_b(x: i32) -> i32 { x + 7 }
fn step_a(x: i32) -> i32 { step_b(x * 3) }
fn main() -> i32 ~ Console {
    println!("{}", step_a(11));
    0
}
"#;
    let p = write_bld(&dir, "det.bld", src);
    let run_once = || {
        let out = buildc()
            .args([
                "check",
                p.to_str().unwrap(),
                "--deny-effect",
                "net,fs,exec,unsafe",
                "--effect-manifest-json",
            ])
            .output()
            .expect("run buildc check");
        assert!(out.status.success(), "{}", diag(&out));
        out.stdout
    };

    let b1 = run_once();
    let b2 = run_once();
    let b3 = run_once();
    assert_eq!(b1, b2, "run 1 and run 2 JSON must be bit-for-bit identical");
    assert_eq!(b2, b3, "run 2 and run 3 JSON must be bit-for-bit identical");

    let m: EffectManifest = serde_json::from_slice(&b1).unwrap();
    assert_eq!(compute_manifest_sha256(&m).unwrap(), m.manifest_sha256);
    assert!(m.zero_egress);
}

#[test]
fn live_sweep_over_universe_buildos_core_verifies_zero_egress() {
    let buildos_dir = repo_root().join("universe").join("buildos-core");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&buildos_dir)
        .expect("read universe/buildos-core")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bld"))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "universe/buildos-core must contain .bld modules"
    );

    let mut total_fns = 0usize;
    let mut pure_fns = 0usize;
    for file in &files {
        let out = buildc()
            .args([
                "check",
                file.to_str().unwrap(),
                "--deny-effect",
                "net,fs,exec,unsafe",
                "--effect-manifest-json",
            ])
            .output()
            .expect("run buildc check on buildos-core module");
        assert!(
            out.status.success(),
            "buildos-core module `{}` failed zero-egress audit:\n{}",
            file.display(),
            diag(&out)
        );
        let manifest: EffectManifest = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "invalid manifest JSON for `{}`: {e}\n{}",
                file.display(),
                diag(&out)
            )
        });
        assert!(
            manifest.passed && manifest.zero_egress && manifest.violations.is_empty(),
            "buildos-core module `{}` is not zero-egress: {manifest:?}",
            file.display()
        );
        assert_eq!(
            compute_manifest_sha256(&manifest).unwrap(),
            manifest.manifest_sha256
        );
        total_fns += manifest.functions.len();
        pure_fns += manifest.functions.iter().filter(|f| f.is_pure).count();
        println!(
            "BUILDOS_CORE_AUDIT: file={} funcs={} pure={} effects={:?} zero_egress={} sha256={}",
            file.file_name().unwrap().to_string_lossy(),
            manifest.functions.len(),
            manifest.functions.iter().filter(|f| f.is_pure).count(),
            manifest.module_effects,
            manifest.zero_egress,
            &manifest.manifest_sha256[..16]
        );
    }
    println!(
        "BUILDOS_CORE_SWEEP_TOTAL: files={} total_fns={} pure_fns={}",
        files.len(),
        total_fns,
        pure_fns
    );
}
