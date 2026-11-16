//! The f64 cube root is the runtime's own, correctly rounded for every double.
//!
//! `tests/detmath/cbrt.bld` prints the bits of `cbrt(x)` (and a line if
//! `x.cbrt()` ever differs) for 9624 inputs: special values, every power of
//! two, subnormals, perfect cubes, doubles next to the cube of a rounding
//! midpoint, and 4000 random bit patterns. `cbrt.expected` is the double
//! nearest the true cube root of each, from exact integer arithmetic in
//! `tools/detmath/cbrt_vectors.py` (checked there against mpmath at 300 bits).
//!
//! Controls, each on the same generated C: the host C library's `cbrt`, a
//! cube root one unit too large at the input nearest a midpoint, one that
//! drops the sign of a negative input, one by `pow(x, 1/3)`, and the runtime's
//! own guess without its exact last-bit fix. Every one must differ from the
//! table; the controls need gcc and say so when it is missing.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").trim_end().to_string()
}

fn expected() -> String {
    normalize(&std::fs::read_to_string(repo_root().join("tests/detmath/cbrt.expected")).unwrap())
}

/// Lines of `got` that differ from the table, as (got, want) pairs; a
/// different line count counts as a difference too.
fn differences(got: &str, want: &str) -> Vec<(String, String)> {
    let mut d: Vec<(String, String)> = got
        .lines()
        .zip(want.lines())
        .filter(|(g, w)| g != w)
        .map(|(g, w)| (g.to_string(), w.to_string()))
        .collect();
    if got.lines().count() != want.lines().count() {
        d.push((
            format!("{} lines", got.lines().count()),
            format!("{} lines", want.lines().count()),
        ));
    }
    d
}

#[test]
fn cbrt_is_correctly_rounded_on_every_vector() {
    let file = repo_root().join("tests/detmath/cbrt.bld");
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got = normalize(&String::from_utf8_lossy(&out.stdout));
    let want = expected();
    assert_eq!(want.lines().count(), 9624, "the table lost rows");
    let d = differences(&got, &want);
    assert!(
        d.is_empty(),
        "{} of 9624 rows differ from the correctly rounded table, first: {:?}",
        d.len(),
        &d[..d.len().min(5)]
    );
}

struct Emitted {
    dir: PathBuf,
    runtime: String,
    program: String,
}

/// Build `tests/detmath/cbrt.bld` to C and split it at the runtime's end marker.
fn emit(tag: &str) -> Emitted {
    let dir = std::env::temp_dir().join(format!("buildlang-cbrt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        repo_root().join("tests/detmath/cbrt.bld"),
        dir.join("main.bld"),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["build", "--emit", "c"])
        .arg(&dir)
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let c_text = std::fs::read_to_string(dir.join("target/debug/main.c")).expect("main.c written");
    let split = c_text
        .find("End BuildLang Runtime")
        .expect("the runtime end marker");
    let (runtime, program) = c_text.split_at(split);
    Emitted {
        dir,
        runtime: runtime.to_string(),
        program: program.to_string(),
    }
}

#[test]
fn generated_c_calls_the_native_cbrt_and_never_the_c_library() {
    let e = emit("calls");
    assert!(e.runtime.contains("static double build_det_cbrt(double x)"));
    let calls = e.program.matches("build_det_cbrt(").count();
    assert!(calls >= 2, "the program calls build_det_cbrt {calls} times");
    assert_eq!(
        e.program.matches("cbrt(").count(),
        calls,
        "the program calls a cbrt other than build_det_cbrt"
    );
    // the runtime's cube root uses no C library function at all
    let body_start = e
        .runtime
        .find("static double build_det_cbrt(double x)")
        .unwrap();
    let body = &e.runtime[body_start..];
    let body = &body[..body.find("\n}\n").unwrap()];
    for libm in [
        "cbrt(", "pow(", "fma(", "frexp(", "ldexp(", "scalbn(", "sqrt(",
    ] {
        let bare = body
            .match_indices(libm)
            .any(|(i, _)| !body[..i].ends_with("build_det_") && !body[..i].ends_with('_'));
        assert!(!bare, "build_det_cbrt calls {libm}");
    }
    let _ = std::fs::remove_dir_all(&e.dir);
}

fn gcc() -> Option<&'static str> {
    ["gcc", "cc"].into_iter().find(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).contains("clang"))
            .unwrap_or(false)
    })
}

fn compile_and_run(cc: &str, dir: &Path, name: &str, c_text: &str) -> String {
    let file = dir.join(format!("{name}.c"));
    std::fs::write(&file, c_text).unwrap();
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let exe = dir.join(format!("{name}{ext}"));
    let mut cmd = Command::new(cc);
    cmd.args(["-w", "-O0", "-std=gnu99"])
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .arg("-lm");
    if cfg!(windows) {
        cmd.arg("-lws2_32");
    }
    let built = cmd.output().expect("C compiler runs");
    assert!(
        built.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&exe).output().expect("control runs");
    normalize(&String::from_utf8_lossy(&run.stdout))
}

/// Run the emitted program with its `build_det_cbrt(` calls pointed at
/// `planted` (C defined between the runtime and the program), or with the
/// runtime edited by `runtime_edit`; return how many rows differ.
fn control(
    tag: &str,
    planted: Option<(&str, &str)>,
    runtime_edit: Option<(&str, &str)>,
) -> Option<usize> {
    let Some(cc) = gcc() else {
        eprintln!("skipping {tag}: no gcc on PATH");
        return None;
    };
    let e = emit(tag);
    let mut runtime = e.runtime.clone();
    let mut program = e.program.clone();
    if let Some((from, to)) = runtime_edit {
        assert!(runtime.contains(from), "{tag}: the runtime has no {from:?}");
        runtime = runtime.replace(from, to);
    }
    let mut extra = String::new();
    if let Some((name, def)) = planted {
        assert!(program.contains("build_det_cbrt("));
        program = program.replace("build_det_cbrt(", &format!("{name}("));
        extra = format!("\n{def}\n");
    }
    // the end marker sits inside a comment line; put new definitions after it
    let (marker_line, rest) = program.split_at(program.find('\n').unwrap() + 1);
    let got = compile_and_run(
        cc,
        &e.dir,
        tag,
        &format!("{runtime}{marker_line}{extra}{rest}"),
    );
    let n = differences(&got, &expected()).len();
    eprintln!("control {tag}: {n} of 9624 rows differ from the table");
    let _ = std::fs::remove_dir_all(&e.dir);
    Some(n)
}

#[test]
fn control_host_c_library_cbrt_differs_from_the_table() {
    let Some(n) = control(
        "libm",
        Some((
            "bl_libm_cbrt",
            "static double bl_libm_cbrt(double x) { return cbrt(x); }",
        )),
        None,
    ) else {
        return;
    };
    assert!(n > 0, "the host C library cbrt matched every row");
}

#[test]
fn control_one_ulp_high_at_the_hardest_halfway_case_fails() {
    // Row 3231 is the input whose cube root lies nearest a rounding midpoint
    // in the table (6e-5 units in the last place away).
    let planted = "static double bl_off_cbrt(double x) {\n\
        double y = build_det_cbrt(x);\n\
        uint64_t b; memcpy(&b, &x, 8);\n\
        if (b == 0x94a3e3f98465f3d8ULL) { memcpy(&b, &y, 8); b += 1; memcpy(&y, &b, 8); }\n\
        return y;\n}";
    let Some(n) = control("one-ulp", Some(("bl_off_cbrt", planted)), None) else {
        return;
    };
    assert_eq!(
        n, 1,
        "one wrong last bit on one row must be exactly one difference"
    );
}

#[test]
fn control_sign_dropped_for_negatives_fails() {
    let planted =
        "static double bl_unsigned_cbrt(double x) { return build_det_cbrt(x < 0 ? -x : x); }";
    let Some(n) = control("sign", Some(("bl_unsigned_cbrt", planted)), None) else {
        return;
    };
    assert!(n > 1000, "dropping the sign changed only {n} rows");
}

#[test]
fn control_cbrt_by_pow_one_third_fails() {
    let planted = "static double bl_pow_cbrt(double x) {\n\
        return x < 0 ? -build_det_pow(-x, 1.0 / 3.0) : build_det_pow(x, 1.0 / 3.0);\n}";
    let Some(n) = control("pow", Some(("bl_pow_cbrt", planted)), None) else {
        return;
    };
    assert!(n > 1000, "x^(1/3) by pow differed on only {n} rows");
}

#[test]
fn control_guess_without_the_exact_fix_fails() {
    let fix = "    while (bl_dm_cube_cmp(2 * y + 1, n8) < 0) y++;\n    while (bl_dm_cube_cmp(2 * y - 1, n8) > 0) y--;\n";
    let Some(n) = control("no-fix", None, Some((fix, ""))) else {
        return;
    };
    assert!(n > 0, "the exact last-bit fix changed nothing");
}

#[test]
fn translation_validation_certifies_the_detmath_methods() {
    // `buildc tv` checks each C function against its source. On 12ddf3c1 the
    // checker named `x.exp()` `exp` while the backend emitted `build_det_exp`,
    // so `exp_method` (and ln, sin, cos, powf) came out not certified.
    let file = repo_root().join("tests/detmath/tv_methods.bld");
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["tv", "--json"])
        .arg(&file)
        .output()
        .expect("buildc runs");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {}", String::from_utf8_lossy(&out.stderr)));
    let functions = doc["functions"].as_array().expect("a function list");
    let names: Vec<&str> = functions
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    for want in [
        "cbrt_method",
        "cbrt_free",
        "exp_method",
        "ln_method",
        "sin_cos_method",
        "powf_method",
    ] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    for f in functions {
        assert_eq!(f["status"], "certified", "{f}");
    }
}
