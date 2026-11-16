//! What the formula kernels need from the compiler and the standard library.
//!
//! The kernels in superframe's `packages/kernels/formulas/` are BuildLang
//! files that C code calls through `extern "C"` exports. Writing them hit:
//!
//! * no free `ln`, `cbrt`, `erf` or `erfc` (the colour kernel carries 64
//!   Newton steps for a cube root; `x.ln()` and `x.cbrt()` existed as
//!   methods only);
//! * an export inside a module was named after the module (`kern_fk_twice`),
//!   so C code could not link `fk_twice`, and kernels could not share a
//!   helper module;
//! * no seeded random stream, no complex numbers, no normal quantile.
//!
//! Known answers, with their sources:
//!
//! * `xmur3-mulberry32/1`: superframe `packages/superstack/vectors/seed.json`
//!   (the Python, JavaScript and C++ superstack implementations agree on it).
//!   Seeds `""`, `"folded-light"`, `"\u{e9}t\u{e9}"`, `"\u{1F600}"` and
//!   `"a\u{1F600}b\u{FFFF}"` cover ASCII, a two-byte character and a surrogate
//!   pair.
//! * `ln`, `cbrt`, `erf`, `erfc`: CPython 3.12 `math`.
//! * `normal_quantile`: CPython 3.12 `statistics.NormalDist().inv_cdf`, which
//!   implements the same algorithm (Wichura 1988, AS 241); `normal_cdf`:
//!   SciPy 1.17 `scipy.stats.norm.cdf`.
//! * Complex arithmetic: CPython 3.12 `complex` and `cmath`.
//!
//! Floats are compared within a relative 1e-15 (a few units in the last
//! place): `cbrt(2)` is 1.259921049894873 with glibc and 1.2599210498948732
//! with the mingw C library.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-kneeds-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

fn buildc(args: &[&str], path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(args)
        .arg(path)
        .output()
        .expect("buildc runs")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Check, then run, one file; return its lines.
fn run_lines(name: &str, source: &str) -> Vec<String> {
    let main = scratch(name).join("main.bld");
    std::fs::write(&main, source).expect("source file");
    let check = buildc(&["check"], &main);
    assert!(check.status.success(), "{name}: check\n{}", text(&check));
    let run = buildc(&["run"], &main);
    assert!(run.status.success(), "{name}: run\n{}", text(&run));
    String::from_utf8_lossy(&run.stdout)
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

fn numbers(line: &str) -> Vec<f64> {
    line.split_whitespace()
        .map(|w| {
            w.parse::<f64>()
                .unwrap_or_else(|_| panic!("not a number: {w:?} in {line:?}"))
        })
        .collect()
}

fn close(got: f64, want: f64) -> bool {
    close_within(got, want, 1e-15)
}

fn close_within(got: f64, want: f64, tol: f64) -> bool {
    if want == 0.0 {
        return got.abs() < 1e-300;
    }
    if want.is_infinite() || want.is_nan() {
        return got == want || (got.is_nan() && want.is_nan());
    }
    ((got - want) / want).abs() <= tol
}

fn assert_lines(name: &str, lines: &[String], want: &[&[f64]]) {
    assert_eq!(lines.len(), want.len(), "{name}: lines {lines:?}");
    for (line, w) in lines.iter().zip(want) {
        let got = numbers(line);
        assert_eq!(got.len(), w.len(), "{name}: {line:?}");
        for (g, x) in got.iter().zip(w.iter()) {
            assert!(close(*g, *x), "{name}: got {g:e}, want {x:e} in {line:?}");
        }
    }
}

#[test]
fn ln_cbrt_erf_erfc_are_free_built_ins() {
    let lines = run_lines(
        "scalars",
        "use std::math::{ln, cbrt, erf, erfc};\n\nfn main() ~ Console {\n    \
         let two: f64 = 2.0;\n    \
         println!(\"{} {}\", ln(two), ln(10.0));\n    \
         println!(\"{} {} {}\", cbrt(27.0), cbrt(two), cbrt(-8.0));\n    \
         println!(\"{} {}\", erf(0.5), erf(-1.0));\n    \
         println!(\"{} {}\", erfc(2.0), erfc(5.0));\n}\n",
    );
    assert_lines(
        "scalars",
        &lines,
        &[
            &[0.6931471805599453, 2.302585092994046],
            &[3.0, 1.259921049894873, -2.0],
            &[0.5204998778130465, -0.8427007929497149],
            &[0.004677734981047265, 1.5374597944280351e-12],
        ],
    );
}

#[test]
fn the_free_forms_need_no_import_and_agree_with_the_methods() {
    let lines = run_lines(
        "methods",
        "fn main() ~ Console {\n    let x: f64 = 3.5;\n    \
         println!(\"{} {}\", ln(x) - x.ln(), cbrt(x) - x.cbrt());\n}\n",
    );
    assert_lines("methods", &lines, &[&[0.0, 0.0]]);
}

#[test]
fn erf_of_a_unit_carrying_value_is_refused() {
    let main = scratch("erf-unit").join("main.bld");
    std::fs::write(
        &main,
        "fn main() ~ Console {\n    let d: f64<m> = 2.0;\n    println!(\"{}\", erf(d));\n}\n",
    )
    .unwrap();
    let check = buildc(&["check"], &main);
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert!(log.contains("erf()"), "{log}");
}

const SHARED: &str = "pub fn sq(x: f64) -> f64 {\n    x * x\n}\n";

const KERN_A: &str =
    "use super::shared::sq;\n\nextern \"C\" fn fk_twice(x: f64) -> f64 {\n    sq(x) * 2.0\n}\n";

const KERN_B: &str =
    "use super::shared::sq;\n\nextern \"C\" fn fk_thrice(x: f64) -> f64 {\n    sq(x) * 3.0\n}\n";

const DRIVER: &str = "#include <stdio.h>\n#include \"main.h\"\n\nint main(void) {\n    \
    printf(\"%.1f %.1f\\n\", fk_twice(2.0), fk_thrice(2.0));\n    return 0;\n}\n";

fn c_compiler() -> Option<&'static str> {
    let found = ["cc", "gcc", "clang"].into_iter().find(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    });
    if found.is_none() && std::env::var_os("CI").is_some() {
        panic!("no C compiler on PATH; CI must have one");
    }
    found
}

/// Two kernel modules share one helper module, and C code links their
/// exports by their own names.
#[test]
fn an_export_in_a_module_keeps_its_c_name() {
    let Some(cc) = c_compiler() else {
        eprintln!("no C compiler on PATH: not run");
        return;
    };
    let dir = scratch("exports");
    std::fs::write(
        dir.join("main.bld"),
        "mod shared;\nmod kern_a;\nmod kern_b;\n",
    )
    .unwrap();
    std::fs::write(dir.join("shared.bld"), SHARED).unwrap();
    std::fs::write(dir.join("kern_a.bld"), KERN_A).unwrap();
    std::fs::write(dir.join("kern_b.bld"), KERN_B).unwrap();
    std::fs::write(dir.join("driver.c"), DRIVER).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .env("BUILDLANG_CC", cc)
        .args(["build", "--emit", "staticlib"])
        .arg(&dir)
        .output()
        .expect("buildc runs");
    assert!(out.status.success(), "staticlib build\n{}", text(&out));

    let lib_dir = dir.join("target/debug");
    let header = std::fs::read_to_string(lib_dir.join("main.h")).expect("main.h written");
    assert!(header.contains("fk_twice("), "header:\n{header}");
    assert!(header.contains("fk_thrice("), "header:\n{header}");
    assert!(!header.contains("kern_a_fk_twice"), "header:\n{header}");

    let flags = std::fs::read_to_string(lib_dir.join("main.link")).expect("main.link written");
    let exe = lib_dir.join(if cfg!(windows) {
        "driver.exe"
    } else {
        "driver"
    });
    let link = Command::new(cc)
        .arg(dir.join("driver.c"))
        .arg("-I")
        .arg(&lib_dir)
        .arg(lib_dir.join("libmain.a"))
        .args(flags.lines().filter(|l| !l.trim().is_empty()))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("C compiler runs");
    assert!(
        link.status.success(),
        "C host failed to link\n{}",
        text(&link)
    );
    let run = Command::new(&exe).output().expect("driver runs");
    assert!(run.status.success(), "driver\n{}", text(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n"),
        "8.0 12.0\n"
    );
}

/// The control: BuildLang code still calls an export of a module by its
/// path, in a statement and inside `println!`.
#[test]
fn a_path_call_reaches_an_export_of_a_module() {
    let dir = scratch("path-call");
    let main = dir.join("main.bld");
    std::fs::write(
        &main,
        "mod shared;\nmod kern_a;\n\nfn main() ~ Console {\n    \
         let y = kern_a::fk_twice(3.0);\n    println!(\"{} {}\", y, kern_a::fk_twice(1.5));\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("shared.bld"), SHARED).unwrap();
    std::fs::write(dir.join("kern_a.bld"), KERN_A).unwrap();
    let run = buildc(&["run"], &main);
    assert!(run.status.success(), "run\n{}", text(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n"),
        "18 4.5\n"
    );
}

#[test]
fn the_seeded_stream_matches_the_superstack_vectors() {
    let lines = run_lines(
        "random",
        "use std::random::{xmur3, Mulberry32};\n\n\
         fn show(seed: &str) ~ Console {\n    \
         let mut r = Mulberry32::from_seed(seed);\n    \
         println!(\"{} {} {} {}\", xmur3(seed), r.next_u32(), r.next_u32(), r.next_u32());\n}\n\n\
         fn main() ~ Console {\n    \
         show(\"\");\n    show(\"folded-light\");\n    show(\"\\u{e9}t\\u{e9}\");\n    \
         show(\"\\u{1F600}\");\n    show(\"a\\u{1F600}b\\u{FFFF}\");\n    \
         let mut z = Mulberry32::from_seed(\"\");\n    println!(\"{}\", z.next_f64());\n    \
         let mut s = Mulberry32::new(0);\n    println!(\"{}\", s.next_u32());\n}\n",
    );
    let want = [
        "167010153 4190637403 2672292534 2825484934",
        "2941645112 4254931054 1378730632 1556259348",
        "1533222292 3840223836 2359373613 460873549",
        "3276203938 2145498879 2730387803 2860082445",
        "1049297182 3558185124 489330452 2339182711",
    ];
    assert_eq!(&lines[..5], &want[..], "{lines:?}");
    assert_lines("random float", &lines[5..6], &[&[0.9757088038604707]]);
    // mulberry32 from state 0, by the superstack Python reference.
    assert_eq!(lines[6], "1144304738", "{lines:?}");
}

#[test]
fn complex_arithmetic_matches_cpython() {
    let lines = run_lines(
        "complex",
        "use std::complex::Complex;\n\n\
         fn show(z: Complex) ~ Console {\n    println!(\"{} {}\", z.re, z.im);\n}\n\n\
         fn main() ~ Console {\n    \
         let a = Complex::new(1.0, 2.0);\n    let b = Complex::new(3.0, 4.0);\n    \
         show(a + b);\n    show(a - b);\n    show(a * b);\n    show(a / b);\n    show(-a);\n    \
         show(a.conj());\n    println!(\"{} {}\", b.abs(), a.arg());\n    \
         show(Complex::new(0.0, 3.141592653589793).exp());\n    \
         show(Complex::new(-4.0, 0.0).sqrt());\n    show(a.ln());\n    \
         show(a.powi(3));\n    show(a.powi(-2));\n    show(Complex::from_polar(2.0, 0.5));\n    \
         show(Complex::new(1.0e300, 1.0e300) / Complex::new(2.0e300, 2.0e300));\n}\n",
    );
    assert_lines(
        "complex",
        &lines,
        &[
            &[4.0, 6.0],
            &[-2.0, -2.0],
            &[-5.0, 10.0],
            &[0.44, 0.08],
            &[-1.0, -2.0],
            &[1.0, -2.0],
            &[5.0, 1.1071487177940904],
            &[-1.0, 1.2246467991473532e-16],
            &[0.0, 2.0],
            &[0.8047189562170503, 1.1071487177940904],
            &[-11.0, -2.0],
            &[-0.12, -0.16],
            &[1.7551651237807455, 0.958851077208406],
            // Smith's method: the textbook formula overflows to NaN here.
            &[0.5, 0.0],
        ],
    );
}

#[test]
fn normal_quantile_and_cdf_match_their_references() {
    let lines = run_lines(
        "stats",
        "use std::stats::{normal_quantile, normal_cdf, normal_sf};\n\n\
         fn main() ~ Console {\n    \
         println!(\"{} {} {}\", normal_quantile(0.975), normal_quantile(0.5), normal_quantile(0.025));\n    \
         println!(\"{} {} {}\", normal_quantile(0.9), normal_quantile(0.001), normal_quantile(1e-10));\n    \
         println!(\"{} {}\", normal_quantile(1e-300), normal_quantile(0.999));\n    \
         println!(\"{} {}\", normal_quantile(0.0), normal_quantile(1.0));\n    \
         println!(\"{}\", normal_quantile(1.5));\n    \
         println!(\"{} {} {}\", normal_cdf(0.0), normal_cdf(-3.0), normal_sf(3.0));\n}\n",
    );
    assert_eq!(lines.len(), 6, "{lines:?}");
    assert_lines(
        "stats",
        &lines[..5],
        &[
            &[1.9599639845400536, 0.0, -1.9599639845400538],
            &[1.2815515655446008, -3.090232306167813, -6.361340902404056],
            &[-37.0470962993612, 3.090232306167813],
            &[f64::NEG_INFINITY, f64::INFINITY],
            &[f64::NAN],
        ],
    );
    // The C library's erfc decides the last digits: within 1e-13 of SciPy.
    let cdf = numbers(&lines[5]);
    let want = [0.5, 0.001349898031630093, 0.001349898031630093];
    assert_eq!(cdf.len(), 3, "{lines:?}");
    for (g, w) in cdf.iter().zip(want) {
        assert!(close_within(*g, w, 1e-13), "cdf: got {g:e}, want {w:e}");
    }
}

#[test]
fn student_t_matches_scipy() {
    let lines = run_lines(
        "student-t",
        "use std::stats::{student_t_quantile, student_t_cdf, student_t_pdf};\n\n\
         fn row(nu: i64) ~ Console {\n    \
         println!(\"{} {} {} {} {}\", student_t_quantile(0.975, nu), student_t_quantile(0.995, nu), \
         student_t_quantile(0.9, nu), student_t_quantile(0.05, nu), student_t_quantile(0.001, nu));\n}\n\n\
         fn main() ~ Console {\n    \
         row(1);\n    row(2);\n    row(3);\n    row(5);\n    row(10);\n    row(30);\n    \
         println!(\"{} {} {}\", student_t_cdf(2.0, 3), student_t_cdf(-1.5, 10), student_t_pdf(0.5, 4));\n    \
         println!(\"{} {} {}\", student_t_quantile(0.5, 7), student_t_quantile(0.0, 7), student_t_quantile(1.0, 7));\n    \
         println!(\"{} {}\", student_t_quantile(0.975, 0), student_t_cdf(1.0, 0));\n}\n",
    );
    // SciPy 1.17 `scipy.stats.t.ppf`, `t.cdf` and `t.pdf`.
    let want: [&[f64]; 9] = [
        &[
            12.706204736174694,
            63.656741162871526,
            3.0776835371752544,
            -6.313751514675044,
            -318.30883898555044,
        ],
        &[
            4.302652729749462,
            9.924843200918287,
            1.8856180831641272,
            -2.9199855803537256,
            -22.327124770119873,
        ],
        &[
            3.1824463052837078,
            5.840909309733355,
            1.637744353696209,
            -2.353363434801824,
            -10.214531852407386,
        ],
        &[
            2.570581835636315,
            4.032142983555228,
            1.4758840488244815,
            -2.015048373333024,
            -5.893429531356009,
        ],
        &[
            2.2281388519862744,
            3.16927267261695,
            1.372183641110336,
            -1.8124611228116758,
            -4.143700494046589,
        ],
        &[
            2.0422724563012378,
            2.7499956535672254,
            1.3104150253913958,
            -1.6972608865939576,
            -3.385184866829305,
        ],
        &[0.9303370157205785, 0.08225366322272007, 0.3222618685603871],
        &[0.0, f64::NEG_INFINITY, f64::INFINITY],
        &[f64::NAN, f64::NAN],
    ];
    assert_eq!(lines.len(), want.len(), "{lines:?}");
    for (line, w) in lines.iter().zip(want) {
        let got = numbers(line);
        assert_eq!(got.len(), w.len(), "{line:?}");
        for (g, x) in got.iter().zip(w.iter()) {
            assert!(
                close_within(*g, *x, 1e-12),
                "student t: got {g:e}, want {x:e} in {line:?}"
            );
        }
    }
}

#[test]
fn mat3_and_quaternion_rotate_and_invert() {
    let lines = run_lines(
        "mat3-quat",
        "use std::math::{Vec3, Mat3, Quaternion};\n\n\
         fn show(v: Vec3) ~ Console {\n    println!(\"{} {} {}\", v.x, v.y, v.z);\n}\n\n\
         fn main() ~ Console {\n    \
         let q = Quaternion::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), 1.5707964);\n    \
         show(q.mul_vec3(Vec3::new(1.0, 0.0, 0.0)));\n    \
         show(q.to_mat3().mul_vec3(Vec3::new(1.0, 0.0, 0.0)));\n    \
         let m = Mat3::new(2.0, 0.0, 0.0, 1.0, 3.0, 0.0, 0.0, 0.0, 4.0);\n    \
         println!(\"{}\", m.determinant());\n    \
         let i = m * m.inverse();\n    show(i.x_axis);\n    show(i.y_axis);\n    show(i.z_axis);\n    \
         show(m.mul_vec3(Vec3::new(1.0, 1.0, 1.0)));\n    show(m.transpose().mul_vec3(Vec3::new(1.0, 1.0, 1.0)));\n    \
         let h = Quaternion::identity().slerp(q, 0.5);\n    println!(\"{} {}\", h.z, h.w);\n    \
         let r = q * q.conjugate();\n    println!(\"{} {}\", r.z, r.w);\n    \
         println!(\"{}\", Quaternion::IDENTITY.w);\n}\n",
    );
    // f32 arithmetic: compare within 1e-6 absolute.
    let want: [&[f64]; 11] = [
        &[0.0, 1.0, 0.0],
        &[0.0, 1.0, 0.0],
        &[24.0],
        &[1.0, 0.0, 0.0],
        &[0.0, 1.0, 0.0],
        &[0.0, 0.0, 1.0],
        &[3.0, 3.0, 4.0],
        &[2.0, 4.0, 4.0],
        // Half of a quarter turn: sin and cos of pi / 8.
        &[0.3826834323650898, 0.9238795325112867],
        &[0.0, 1.0],
        &[1.0],
    ];
    assert_eq!(lines.len(), want.len(), "{lines:?}");
    for (line, w) in lines.iter().zip(want) {
        let got = numbers(line);
        assert_eq!(got.len(), w.len(), "{line:?}");
        for (g, x) in got.iter().zip(w.iter()) {
            assert!(
                (g - x).abs() <= 1e-6,
                "mat3/quat: got {g}, want {x} in {line:?}"
            );
        }
    }
}
