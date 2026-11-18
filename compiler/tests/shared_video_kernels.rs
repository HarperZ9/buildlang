//! Three checked kernels in BuildLang, each with a planted control that must
//! be caught. The programs live in `tests/kernels/` and do their own checking;
//! this file runs them through `buildc` and reads the verdict lines.
//!
//! - `arith_coder.bld`: adaptive order-2 arithmetic coder. Decode must equal
//!   the message byte for byte, the code length must sit within its bound of
//!   the model's -log2 p, and the code's digest is pinned so Linux and Windows
//!   must produce the same bits. Control: a decoder whose update sums its
//!   learning rate in another floating-point order must fail to decode.
//! - `voxel_traverse.bld`: Amanatides and Woo traversal against a reference
//!   that tests every cell. Control: a corner-skipping step must be caught.
//! - `voxel_morton_bench.bld`: row-major against Morton storage on a
//!   256-cubed grid, built with optimisation. Both layouts must give the same
//!   hits; the times are printed and not asserted.
//! - `gauss_centre.bld`: maximum-likelihood centre of a Gaussian blob with
//!   Poisson noise against the Cramer-Rao bound. Controls: a quarter-sigma
//!   coordinate shift must fail the bias check, and the centroid must fail
//!   the variance check.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn run_kernel(name: &str) -> String {
    let file = repo_root().join(format!("tests/kernels/{name}.bld"));
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    assert!(
        out.status.success(),
        "{name} failed: {}\n{stdout}",
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("{name}:\n{stdout}");
    stdout
}

fn line<'a>(out: &'a str, prefix: &str) -> &'a str {
    out.lines()
        .find(|l| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("no line starting {prefix:?} in:\n{out}"))
}

fn field(l: &str, key: &str) -> f64 {
    let mut it = l.split_whitespace();
    while let Some(w) = it.next() {
        if w == key {
            return it.next().unwrap().parse().unwrap();
        }
    }
    panic!("no {key} in {l:?}")
}

#[test]
fn arithmetic_coder_round_trips_within_its_bound() {
    let out = run_kernel("arith_coder");
    assert_eq!(line(&out, "roundtrip"), "roundtrip ok");
    assert_eq!(line(&out, "bound"), "bound ok");
    assert_eq!(line(&out, "repeat"), "repeat ok");
    // the same independently: bits minus the model's ideal length
    let b = line(&out, "bits ");
    let excess = field(b, "excess");
    assert!(
        (-2.0..=field(b, "slack")).contains(&excess),
        "excess {excess}"
    );
    // the model learns: well under the 8 bits of a raw byte
    assert!(field(line(&out, "bits_per_symbol"), "bits_per_symbol") < 2.0);
}

#[test]
fn arithmetic_coder_bits_are_the_same_on_every_machine() {
    // pinned from a Windows run (gcc 15, x86-64); a Linux runner must agree
    let out = run_kernel("arith_coder");
    assert_eq!(
        line(&out, "digest"),
        "digest message 7b25b97fc7d4efdc code 0bd600e85c129e28"
    );
}

#[test]
fn arithmetic_coder_control_reordered_update_breaks_decode() {
    let out = run_kernel("arith_coder");
    assert_eq!(line(&out, "control increments"), "control increments 25 24");
    assert!(
        line(&out, "control caught").starts_with("control caught: decode diverged"),
        "{out}"
    );
    assert!(!out.contains("control MISSED"), "{out}");
}

#[test]
fn voxel_traversal_matches_every_cell_reference() {
    let out = run_kernel("voxel_traverse");
    assert!(line(&out, "visits").starts_with("visits ok"));
    assert!(line(&out, "hits").starts_with("hits ok"));
    assert!(line(&out, "layout").starts_with("layout ok"));
    let r = line(&out, "rays");
    assert_eq!(field(r, "rays"), 2000.0);
    // the rays must exercise the check: most enter the grid and many hit
    assert!(field(r, "cells") > 20000.0 && field(r, "rays_with_hit") > 500.0);
}

#[test]
fn voxel_traversal_control_corner_skip_is_caught() {
    let out = run_kernel("voxel_traverse");
    assert!(line(&out, "control").starts_with("control caught"), "{out}");
}

#[test]
fn voxel_layouts_agree_at_256_cubed() {
    let dir = std::env::temp_dir().join(format!("buildlang-voxel-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        repo_root().join("tests/kernels/voxel_morton_bench.bld"),
        dir.join("main.bld"),
    )
    .unwrap();
    let built = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["build", "--release"])
        .arg(&dir)
        .output()
        .expect("buildc runs");
    assert!(
        built.status.success(),
        "{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let exe = dir
        .join("target/release")
        .join(if cfg!(windows) { "main.exe" } else { "main" });
    let run = Command::new(&exe).output().expect("bench runs");
    let out = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    eprintln!("voxel_morton_bench:\n{out}");
    assert!(run.status.success());
    assert_eq!(line(&out, "layouts agree"), "layouts agree ok");
    assert_eq!(out.lines().filter(|l| l.starts_with("view ")).count(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gaussian_centre_fit_meets_the_cramer_rao_bound() {
    let out = run_kernel("gauss_centre");
    assert!(line(&out, "mle variance").starts_with("mle variance ok"));
    assert!(line(&out, "mle bias").starts_with("mle bias ok"));
    assert_eq!(line(&out, "check"), "check ok");
}

#[test]
fn gaussian_centre_controls_are_caught() {
    let out = run_kernel("gauss_centre");
    assert!(line(&out, "control_shift bias").starts_with("control_shift bias FAIL"));
    assert!(line(&out, "control_centroid variance").starts_with("control_centroid variance FAIL"));
    assert_eq!(line(&out, "control shift"), "control shift caught");
    assert_eq!(line(&out, "control centroid"), "control centroid caught");
}
