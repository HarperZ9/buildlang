//! `DVec3`, `DMat3` and `DQuat`: the f64 forms of `Vec3`, `Mat3` and
//! `Quaternion` that the formula kernels need.
//!
//! The kernels in superframe's `packages/kernels/formulas/` must match a slow
//! f64 reference byte for byte. The known answers below come from such a
//! reference: Python 3.12 floats evaluating the formulas stated in the doc
//! comments of `stdlib/math.bld`, one correctly rounded operation at a time.
//! Two independent checks back that reference (they are not used as answers):
//! NumPy 2.4 `linalg.det`, `linalg.inv` and `@` agree within a relative 6e-15,
//! and exact `fractions.Fraction` arithmetic on the same inputs puts the
//! reference determinant within a relative 4.6e-15 of the true value and the
//! quaternion product within 2.4e-16.
//!
//! Every row is compared bit for bit, except the arc branch of `slerp`, which
//! calls the C library's `acos` and `sin` and is compared within a relative
//! 1e-15. The f32 row is the same determinant through `Mat3`, by NumPy
//! `float32` with one rounding per operation: it differs from the f64 answer
//! in the seventh digit.

use std::path::PathBuf;
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
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
    let dir = std::env::temp_dir().join(format!("bl-kf64-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    let main = dir.join("main.bld");
    std::fs::write(&main, source).expect("source file");
    for step in ["check", "run"] {
        let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
            .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
            .arg(step)
            .arg(&main)
            .output()
            .expect("buildc runs");
        assert!(out.status.success(), "{name}: {step}\n{}", text(&out));
        if step == "run" {
            return String::from_utf8_lossy(&out.stdout)
                .replace("\r\n", "\n")
                .lines()
                .map(str::to_string)
                .collect();
        }
    }
    unreachable!()
}

/// The program. `PRODUCT`, `DET` and `QPROD` are replaced to plant faults.
const PROGRAM: &str = r#"use std::math::{Vec3, Mat3, Quaternion, DVec3, DMat3, DQuat};

fn show(v: DVec3) ~ Console {
    println!("{} {} {}", v.x, v.y, v.z);
}

fn show4(q: DQuat) ~ Console {
    println!("{} {} {} {}", q.x, q.y, q.z, q.w);
}

fn main() ~ Console {
    let a = DMat3::new(0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 1.0);
    let b = DMat3::new(1.5, -2.25, 0.3, 0.7, 1.1, -0.9, 2.0, 0.4, 0.6);
    println!("{}", DET);
    let ai = a.inverse();
    show(ai.x_axis);
    show(ai.y_axis);
    show(ai.z_axis);
    let p = PRODUCT;
    show(p.x_axis);
    show(p.y_axis);
    show(p.z_axis);
    show(a.mul_vec3(DVec3::new(1.0, -2.0, 3.0)));
    let q = DQuat::new(0.1, 0.2, 0.3, 0.9).normalize();
    let r = DQuat::new(-0.4, 0.5, 0.2, 0.7).normalize();
    show4(QPROD);
    let m = q.to_mat3();
    show(m.x_axis);
    show(m.y_axis);
    show(m.z_axis);
    show(q.mul_vec3(DVec3::new(3.0, -1.0, 2.0)));
    show4(q.slerp(r, 0.3));
    show4(q.slerp(DQuat::new(0.1001, 0.2, 0.3, 0.9).normalize(), 0.25));
    let f = Mat3::new(0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 1.0);
    let fd = f.determinant() as f64;
    println!("{}", fd);
    show(DVec3::new(0.1, 0.2, 0.3).as_vec3().as_dvec3());
    let back = a.as_mat3().as_dmat3();
    show(back.z_axis);
    let iq = Quaternion::identity().as_dquat() * DQuat::IDENTITY;
    show4(iq);
    println!("{}", (Vec3::new(1.0, 2.0, 3.0).as_dvec3() + DVec3::X).length_squared());
}
"#;

fn program(det: &str, product: &str, qprod: &str) -> String {
    PROGRAM
        .replace("DET", det)
        .replace("PRODUCT", product)
        .replace("QPROD", qprod)
}

const ARC: usize = 13;

/// The slow f64 reference, row by row.
const WANT: [&[f64]; 20] = [
    &[-0.0030000000000000096],
    &[-6.666666666666652, -13.333333333333284, 9.999999999999968],
    &[-6.666666666666633, 36.66666666666655, -19.999999999999936],
    &[9.999999999999941, -19.99999999999993, 9.999999999999973],
    &[-0.54, -0.585, -0.5999999999999999],
    &[-0.12, -0.030000000000000027, -0.030000000000000027],
    &[0.78, 1.08, 1.44],
    &[1.3999999999999995, 1.6000000000000003, 2.1],
    &[
        -0.42328634821688516,
        0.47619714174399586,
        0.5502722526819506,
        0.5396900939765283,
    ],
    &[0.7263157894736842, 0.6105263157894737, -0.31578947368421056],
    &[-0.5263157894736842, 0.7894736842105263, 0.3157894736842105],
    &[0.4421052631578947, -0.06315789473684214, 0.8947368421052632],
    &[3.5894736842105264, 0.9157894736842105, 0.5263157894736841],
    // ARC: the arc branch of slerp (acos and sin from the C library).
    &[
        -0.057665563621386,
        0.3143268755582608,
        0.28971192753695413,
        0.9021863982109419,
    ],
    // The nlerp branch: only + - * / and sqrt.
    &[
        0.1026232143723642,
        0.20519513036732415,
        0.30779269555098615,
        0.9233780866529585,
    ],
    // The same determinant through the f32 Mat3.
    &[-0.0030000004917383194],
    // 0.1, 0.2 and 0.3 rounded to f32, then widened exactly.
    &[
        0.10000000149011612,
        0.20000000298023224,
        0.30000001192092896,
    ],
    // The third column of `a` through f32 and back.
    &[0.699999988079071, 0.800000011920929, 1.0],
    &[0.0, 0.0, 0.0, 1.0],
    &[17.0],
];

/// The index of every row that differs from the reference.
fn mismatches(lines: &[String]) -> Vec<usize> {
    assert_eq!(lines.len(), WANT.len(), "lines {lines:?}");
    let mut bad = Vec::new();
    for (i, (line, want)) in lines.iter().zip(WANT).enumerate() {
        let got: Vec<f64> = line
            .split_whitespace()
            .map(|w| w.parse().unwrap_or_else(|_| panic!("not a number: {w:?}")))
            .collect();
        let same = got.len() == want.len()
            && got.iter().zip(want.iter()).all(|(g, w)| {
                if i == ARC {
                    ((g - w) / w).abs() <= 1e-15
                } else {
                    g.to_bits() == w.to_bits()
                }
            });
        if !same {
            eprintln!("row {i}: got {line:?}, want {want:?}");
            bad.push(i);
        }
    }
    bad
}

const GOOD: (&str, &str, &str) = ("a.determinant()", "a * b", "q * r");

#[test]
fn dmat3_and_dquat_match_the_slow_f64_reference_bit_for_bit() {
    let lines = run_lines("good", &program(GOOD.0, GOOD.1, GOOD.2));
    assert_eq!(mismatches(&lines), Vec::<usize>::new(), "{lines:?}");
    // f32 and f64 differ on the same matrix.
    let f64_det: f64 = lines[0].parse().unwrap();
    let f32_det: f64 = lines[15].parse().unwrap();
    assert!((f64_det - f32_det).abs() > 1e-10, "{f64_det} {f32_det}");
}

/// The control: a transposed product, a determinant with a dropped term and
/// a quaternion product in the wrong order must each fail the comparison,
/// and only on their own rows.
#[test]
fn planted_faults_fail_the_comparison() {
    let dropped = "a.z_axis.x * (a.x_axis.y * a.y_axis.z - a.x_axis.z * a.y_axis.y) \
                   + a.z_axis.y * (a.x_axis.z * a.y_axis.x - a.x_axis.x * a.y_axis.z)";
    let lines = run_lines("planted", &program(dropped, "a.transpose() * b", "r * q"));
    assert_eq!(mismatches(&lines), vec![0, 4, 5, 6, 8], "{lines:?}");
}
