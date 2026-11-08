//! The package flow end to end: publish, resolve, lock, verify and build
//! offline, against copies of the in-repo registry.
//!
//! Checks written before the runs:
//! 1. Two clean copies of the registry and a project build offline from the
//!    same Build.lock and give the same artifact hash.
//! 2. A tampered package fails its hash with BL0702.
//! 3. A version range nothing meets fails with BL0701.
//! Plus: a stale lock fails with BL0703, a locked version missing from the
//! registry with BL0704, and republishing a version with other files with
//! BL0707.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("buildlang-pkg-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn copy_dir(src: &Path, dest: &Path) {
    std::fs::create_dir_all(dest).expect("create dir");
    for entry in std::fs::read_dir(src).expect("read dir") {
        let path = entry.expect("entry").path();
        let target = dest.join(path.file_name().expect("name"));
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy file");
        }
    }
}

fn buildc(args: &[&str], registry: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .env_remove("BUILDLANG_REGISTRY")
        .args(args)
        .arg("--registry")
        .arg(registry)
        .output()
        .expect("buildc runs")
}

fn log(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A clean clone: a registry copy plus a project that depends on `merkle`.
fn clone(name: &str, merkle_req: &str) -> (PathBuf, PathBuf) {
    let root = scratch(name);
    let registry = root.join("registry");
    copy_dir(&repo_root().join("registry"), &registry);
    let project = root.join("app");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(
        project.join("Build.toml"),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n\n[dependencies]\nmerkle = \"{}\"\n",
            merkle_req
        ),
    )
    .unwrap();
    std::fs::write(
        project.join("src/main.bld"),
        "use merkle;\n\nfn main() ~ Console {\n    let mut leaves: Vec<Vec<u8>> = Vec::new();\n    leaves.push(str_bytes(\"a\"));\n    leaves.push(str_bytes(\"b\"));\n    println!(\"{}\", merkle_root_hex(leaves));\n}\n",
    )
    .unwrap();
    (registry, project)
}

fn artifact_sha(project: &Path) -> String {
    let text = std::fs::read_to_string(project.join("target/pkg/build-receipt.json"))
        .expect("build receipt");
    let v: serde_json::Value = serde_json::from_str(&text).expect("receipt json");
    v["artifact"]["sha256"]
        .as_str()
        .expect("artifact sha")
        .to_string()
}

fn resolve_and_build(registry: &Path, project: &Path) -> String {
    let p = project.to_str().unwrap();
    let out = buildc(&["pkg", "resolve", p], registry);
    assert!(out.status.success(), "resolve failed:\n{}", log(&out));
    let out = buildc(&["pkg", "build", p, "--emit", "c"], registry);
    assert!(out.status.success(), "build failed:\n{}", log(&out));
    artifact_sha(project)
}

#[test]
fn two_clean_copies_build_the_same_artifact_from_the_lock() {
    let (reg_a, app_a) = clone("repro-a", "^1.0");
    let (reg_b, app_b) = clone("repro-b", "^1.0");
    let a = resolve_and_build(&reg_a, &app_a);
    let b = resolve_and_build(&reg_b, &app_b);
    assert_eq!(a, b, "two clean builds gave different artifacts");
    let lock_a = std::fs::read(app_a.join("Build.lock")).unwrap();
    let lock_b = std::fs::read(app_b.join("Build.lock")).unwrap();
    assert_eq!(lock_a, lock_b, "the same resolution wrote different locks");
    let lock = String::from_utf8(lock_a).unwrap();
    assert!(lock.contains("name = \"merkle\"") && lock.contains("name = \"sha256\""));
    assert!(lock.contains("checksum = \"sha256:"));
    // A second build of the same copy repeats the hash.
    let out = buildc(
        &["pkg", "build", app_a.to_str().unwrap(), "--emit", "c"],
        &reg_a,
    );
    assert!(out.status.success(), "{}", log(&out));
    assert_eq!(artifact_sha(&app_a), a);
}

#[test]
fn a_tampered_package_fails_its_hash() {
    let (registry, project) = clone("tamper", "^1.0");
    resolve_and_build(&registry, &project);
    let lib = registry.join("packages/sha256/1.0.0/src/lib.bld");
    let mut text = std::fs::read_to_string(&lib).unwrap();
    text.push_str("\n// one extra line\n");
    std::fs::write(&lib, text).unwrap();
    for cmd in ["verify", "build"] {
        let out = buildc(&["pkg", cmd, project.to_str().unwrap()], &registry);
        assert!(!out.status.success(), "{} accepted a tampered package", cmd);
        let text = log(&out);
        assert!(text.contains("error[BL0702]"), "{}:\n{}", cmd, text);
        assert!(text.contains("sha256 1.0.0"), "{}:\n{}", cmd, text);
    }
}

#[test]
fn an_unmeetable_version_range_fails_with_bl0701() {
    let (registry, project) = clone("range", "^9.0");
    let out = buildc(&["pkg", "resolve", project.to_str().unwrap()], &registry);
    assert!(!out.status.success());
    let text = log(&out);
    assert!(text.contains("error[BL0701]"), "{}", text);
    assert!(text.contains("merkle ^9.0.0"), "{}", text);
    assert!(
        !project.join("Build.lock").exists(),
        "a failed resolve wrote a lock"
    );
}

#[test]
fn a_stale_or_missing_lock_fails_with_bl0703() {
    let (registry, project) = clone("stale", "^1.0");
    let p = project.to_str().unwrap();
    let out = buildc(&["pkg", "build", p], &registry);
    assert!(log(&out).contains("error[BL0703]"), "{}", log(&out));
    resolve_and_build(&registry, &project);
    let manifest = project.join("Build.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("^1.0", "^1.0.0, <2.0.0")).unwrap();
    let out = buildc(&["pkg", "build", p], &registry);
    assert!(!out.status.success());
    assert!(log(&out).contains("error[BL0703]"), "{}", log(&out));
}

#[test]
fn a_locked_version_missing_from_the_registry_fails_with_bl0704() {
    let (registry, project) = clone("missing", "^1.0");
    resolve_and_build(&registry, &project);
    let index = registry.join("index.json");
    let text = std::fs::read_to_string(&index).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["packages"].as_object_mut().unwrap().remove("sha256");
    std::fs::write(&index, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let out = buildc(&["pkg", "build", project.to_str().unwrap()], &registry);
    assert!(!out.status.success());
    assert!(log(&out).contains("error[BL0704]"), "{}", log(&out));
}

#[test]
fn publish_is_immutable_per_version() {
    let (registry, _) = clone("publish", "^1.0");
    let pkg = registry.parent().unwrap().join("newpkg");
    std::fs::create_dir_all(pkg.join("src")).unwrap();
    std::fs::write(
        pkg.join("Build.toml"),
        "[package]\nname = \"newpkg\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n\n[dependencies]\nsha256 = \"^1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        pkg.join("src/lib.bld"),
        "fn newpkg_answer() -> i64 {\n    42\n}\n",
    )
    .unwrap();
    let p = pkg.to_str().unwrap();
    let first = buildc(&["pkg", "publish", p], &registry);
    assert!(first.status.success(), "{}", log(&first));
    assert!(registry.join("packages/newpkg/0.1.0/src/lib.bld").is_file());
    let again = buildc(&["pkg", "publish", p], &registry);
    assert!(log(&again).contains("unchanged"), "{}", log(&again));
    std::fs::write(
        pkg.join("src/lib.bld"),
        "fn newpkg_answer() -> i64 {\n    43\n}\n",
    )
    .unwrap();
    let changed = buildc(&["pkg", "publish", p], &registry);
    assert!(!changed.status.success());
    assert!(log(&changed).contains("error[BL0707]"), "{}", log(&changed));
    let check = buildc(&["pkg", "check-registry"], &registry);
    assert!(check.status.success(), "{}", log(&check));
}

#[test]
fn the_in_repo_registry_matches_its_index() {
    let out = buildc(&["pkg", "check-registry"], &repo_root().join("registry"));
    assert!(out.status.success(), "{}", log(&out));
}
