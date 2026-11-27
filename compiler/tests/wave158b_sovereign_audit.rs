//! Wave 158B integration tests for the unified 4-pillar MIR sovereign audit CLI
//! (`buildc check --sovereign-audit [--max-stack-bytes <N>] [--deny-effect <list>] --sovereign-manifest-json`).

use buildlang::codegen::{
    compute_sovereign_root_sha256, SovereignAuditManifest, SOVEREIGN_AUDIT_SCHEMA,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler/ parent is repo root")
        .to_path_buf()
}

fn temp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "buildlang_wave158b_sov_{tag}_{}_{}",
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp test dir");
    dir
}

fn write_bld(dir: &Path, name: &str, src: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, src).expect("write .bld fixture");
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
fn clean_sovereign_kernel_passes_all_four_pillars_in_one_command() {
    let dir = temp_dir("clean_4pillar");
    let src = r#"
fn ct_barrett_mod3329(a: i64) -> i64 {
    let v: i64 = 20159;
    let t: i64 = (v * a + 33554432) >> 26;
    a - t * 3329
}

fn ct_select(mask: u64, x: u64, y: u64) -> u64 {
    y ^ ((x ^ y) & mask)
}

fn main() -> i32 {
    let r: i64 = ct_barrett_mod3329(4096);
    let s: u64 = ct_select(0xffffffffffffffff, 11, 22);
    let _u1: i64 = r;
    let _u2: u64 = s;
    0
}
"#;
    let p = write_bld(&dir, "clean_sovereign.bld", src);
    let out = buildc()
        .args([
            "check",
            p.to_str().unwrap(),
            "--sovereign-audit",
            "--max-stack-bytes",
            "2048",
            "--sovereign-manifest-json",
        ])
        .output()
        .expect("run buildc check --sovereign-audit");
    assert!(
        out.status.success(),
        "clean sovereign kernel MUST pass all 4 pillars:\n{}",
        diag(&out)
    );
    let manifest: SovereignAuditManifest =
        serde_json::from_slice(&out.stdout).expect("parse sovereign manifest JSON");
    assert_eq!(manifest.schema, SOVEREIGN_AUDIT_SCHEMA);
    assert!(manifest.passed);
    assert_eq!(manifest.pillars_passed, 4);
    assert_eq!(manifest.pillars_total, 4);
    assert!(manifest.failing_pillars.is_empty());
    assert_eq!(
        compute_sovereign_root_sha256(
            &manifest.effect.manifest_sha256,
            &manifest.wcet.manifest_sha256,
            &manifest.constant_time.manifest_sha256,
            &manifest.determinism.manifest_sha256,
        ),
        manifest.sovereign_root_sha256
    );
}

#[test]
fn four_planted_single_pillar_violations_each_fail_closed_with_exact_pillar_and_trace() {
    let dir = temp_dir("four_planted");

    // Pillar 1: hidden fs egress call.
    let fs_src = r#"
fn sneak_fs() -> bool ~ FileSystem {
    file_exists("secret.bin")
}

fn main() -> i32 ~ FileSystem {
    if sneak_fs() { 1 } else { 0 }
}
"#;
    let p1 = write_bld(&dir, "p1_fs.bld", fs_src);
    let o1 = buildc()
        .args([
            "check",
            p1.to_str().unwrap(),
            "--sovereign-audit",
            "--sovereign-manifest-json",
        ])
        .output()
        .expect("run sovereign audit on fs breach");
    assert!(
        !o1.status.success(),
        "fs breach must fail closed:\n{}",
        diag(&o1)
    );
    let m1: SovereignAuditManifest = serde_json::from_slice(&o1.stdout).unwrap();
    assert!(!m1.passed);
    assert_eq!(m1.failing_pillars, vec!["cap_effect"]);
    assert!(String::from_utf8_lossy(&o1.stderr).contains("cap_effect/fs"));

    // Pillar 2: oversized local array [i64; 320] = 2,560 B under --max-stack-bytes 2048.
    let stack_src = r#"
fn blow_frame(v: i64) -> i64 {
    let mut buf: [i64; 320] = [0; 320];
    buf[0] = v;
    buf[319] = v + 1;
    buf[0] + buf[319]
}

fn main() -> i32 {
    let r: i64 = blow_frame(5);
    if r > 0 { 0 } else { 1 }
}
"#;
    let p2 = write_bld(&dir, "p2_stack.bld", stack_src);
    let o2 = buildc()
        .args([
            "check",
            p2.to_str().unwrap(),
            "--sovereign-audit",
            "--max-stack-bytes",
            "2048",
            "--sovereign-manifest-json",
        ])
        .output()
        .expect("run sovereign audit on stack breach");
    assert!(
        !o2.status.success(),
        "stack breach must fail closed:\n{}",
        diag(&o2)
    );
    let m2: SovereignAuditManifest = serde_json::from_slice(&o2.stdout).unwrap();
    assert!(!m2.passed);
    assert_eq!(m2.failing_pillars, vec!["wcet_stack"]);
    assert!(String::from_utf8_lossy(&o2.stderr).contains("main -> blow_frame"));

    // Pillar 3: sbox[secret] cache-timing lookup.
    let ct_src = r#"
fn ct_leaky_sbox(secret_i: i64) -> i64 {
    let sbox: [i64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let idx: i64 = secret_i & 7;
    sbox[idx]
}

fn main() -> i32 {
    let _v: i64 = ct_leaky_sbox(2);
    0
}
"#;
    let p3 = write_bld(&dir, "p3_ct.bld", ct_src);
    let o3 = buildc()
        .args([
            "check",
            p3.to_str().unwrap(),
            "--sovereign-audit",
            "--sovereign-manifest-json",
        ])
        .output()
        .expect("run sovereign audit on ct leak");
    assert!(
        !o3.status.success(),
        "ct sbox leak must fail closed:\n{}",
        diag(&o3)
    );
    let m3: SovereignAuditManifest = serde_json::from_slice(&o3.stdout).unwrap();
    assert!(!m3.passed);
    assert_eq!(m3.failing_pillars, vec!["constant_time"]);
    assert!(String::from_utf8_lossy(&o3.stderr).contains("SecretDependentMemoryIndex"));

    // Pillar 4: direct f64 == f64 equality trap.
    let det_src = r#"
fn cmp_f64(a: f64, b: f64) -> bool {
    a == b
}

fn main() -> i32 {
    if cmp_f64(1.0, 1.0) { 0 } else { 1 }
}
"#;
    let p4 = write_bld(&dir, "p4_det.bld", det_src);
    let o4 = buildc()
        .args([
            "check",
            p4.to_str().unwrap(),
            "--sovereign-audit",
            "--sovereign-manifest-json",
        ])
        .output()
        .expect("run sovereign audit on float eq hazard");
    assert!(
        !o4.status.success(),
        "direct f64 == f64 must fail closed:\n{}",
        diag(&o4)
    );
    let m4: SovereignAuditManifest = serde_json::from_slice(&o4.stdout).unwrap();
    assert!(!m4.passed);
    assert_eq!(m4.failing_pillars, vec!["strict_determinism"]);
    assert!(String::from_utf8_lossy(&o4.stderr).contains("op:float_eq(f64)"));
}

#[test]
fn sovereign_manifest_json_and_composite_root_are_deterministic_across_three_runs() {
    let dir = temp_dir("det_3x");
    let src = r#"
fn ct_mix(a: u64, b: u64) -> u64 {
    (a ^ b) + ((a & b) << 1)
}

fn main() -> i32 {
    let _r: u64 = ct_mix(7, 13);
    0
}
"#;
    let p = write_bld(&dir, "det_3x.bld", src);
    let run_once = || {
        let out = buildc()
            .args([
                "check",
                p.to_str().unwrap(),
                "--sovereign-audit",
                "--sovereign-manifest-json",
            ])
            .output()
            .expect("run sovereign audit");
        assert!(out.status.success(), "{}", diag(&out));
        out.stdout
    };

    let b1 = run_once();
    let b2 = run_once();
    let b3 = run_once();
    assert_eq!(
        b1, b2,
        "run 1 and run 2 sovereign JSON must match byte-for-byte"
    );
    assert_eq!(
        b2, b3,
        "run 2 and run 3 sovereign JSON must match byte-for-byte"
    );

    let m: SovereignAuditManifest = serde_json::from_slice(&b1).unwrap();
    assert!(m.passed && m.pillars_passed == 4);
    assert_eq!(
        compute_sovereign_root_sha256(
            &m.effect.manifest_sha256,
            &m.wcet.manifest_sha256,
            &m.constant_time.manifest_sha256,
            &m.determinism.manifest_sha256,
        ),
        m.sovereign_root_sha256
    );
}

#[test]
fn live_four_pillar_sweep_over_universe_buildos_core_passes_all_five_modules() {
    let buildos_dir = repo_root().join("universe").join("buildos-core");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&buildos_dir)
        .expect("read universe/buildos-core")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bld"))
        .collect();
    files.sort();
    assert!(files.len() >= 5, "expected >= 5 buildos-core modules");

    let mut total_fns = 0usize;
    for file in &files {
        let out = buildc()
            .args([
                "check",
                file.to_str().unwrap(),
                "--sovereign-audit",
                "--sovereign-manifest-json",
            ])
            .output()
            .expect("run 4-pillar sovereign audit on buildos-core module");
        assert!(
            out.status.success(),
            "buildos-core module `{}` failed 4-pillar sovereign audit:\n{}",
            file.display(),
            diag(&out)
        );
        let m: SovereignAuditManifest = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "invalid sovereign manifest JSON for `{}`: {e}\n{}",
                file.display(),
                diag(&out)
            )
        });
        assert!(m.passed && m.pillars_passed == 4 && m.failing_pillars.is_empty());
        assert_eq!(
            compute_sovereign_root_sha256(
                &m.effect.manifest_sha256,
                &m.wcet.manifest_sha256,
                &m.constant_time.manifest_sha256,
                &m.determinism.manifest_sha256,
            ),
            m.sovereign_root_sha256
        );
        total_fns += m.effect.functions.len();
        println!(
            "BUILDOS_SOVEREIGN_AUDIT: file={} funcs={} pillars={}/{} max_stack={}B root={}",
            file.file_name().unwrap().to_string_lossy(),
            m.effect.functions.len(),
            m.pillars_passed,
            m.pillars_total,
            m.wcet.module_max_stack_bytes,
            &m.sovereign_root_sha256[..16]
        );
    }
    println!(
        "BUILDOS_SOVEREIGN_SWEEP_TOTAL: files={} total_fns={} pillars_all=20/20",
        files.len(),
        total_fns
    );
    assert!(total_fns >= 60);
}
