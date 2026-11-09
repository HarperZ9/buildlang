//! `buildc build-hash`: the emitted C is the same bytes on every compiler run.
//!
//! Trait programs used to emit their vtables in hash-map order, so two runs of
//! the compiler could print different C for the same source. Every program
//! named here must hash the same over repeated fresh compiler processes.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn build_hash(program: &str, runs: &str) -> (Option<i32>, serde_json::Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["build-hash", "--json", "--runs", runs])
        .arg(repo_root().join("tests/programs").join(program))
        .output()
        .expect("buildc runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let doc = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    (out.status.code(), doc)
}

#[test]
fn trait_programs_hash_the_same_on_every_run() {
    // Each of these differed between runs before vtable emission was sorted.
    for program in [
        "20_traits.bld",
        "49_dynamic_dispatch.bld",
        "50_generic_option.bld",
        "51_dyn_dispatch.bld",
        "86_trait_method_on_params.bld",
        "88_self_return_chain.bld",
    ] {
        let (code, doc) = build_hash(program, "8");
        assert_eq!(code, Some(0), "{program}: {doc}");
        assert_eq!(doc["deterministic"], true, "{program}");
        assert_eq!(doc["runs"], 8);
        assert_eq!(doc["c_sha256"].as_str().map(str::len), Some(64));
        assert!(doc["first_difference"].is_null());
    }
}

#[test]
fn two_invocations_agree_and_two_sources_differ() {
    let (_, a) = build_hash("01_hello.bld", "2");
    let (_, b) = build_hash("01_hello.bld", "2");
    let (_, c) = build_hash("20_traits.bld", "2");
    assert_eq!(a["schema"], "buildlang-build-hash/v1");
    assert_eq!(a["c_sha256"], b["c_sha256"]);
    assert_eq!(a["source_sha256"], b["source_sha256"]);
    assert_ne!(a["c_sha256"], c["c_sha256"]);
    assert_ne!(a["source_sha256"], c["source_sha256"]);
}

#[test]
fn one_run_is_refused() {
    let (code, _) = build_hash("01_hello.bld", "1");
    assert_eq!(code, Some(2));
}
