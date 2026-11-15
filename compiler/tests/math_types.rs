//! Vectors, a matrix and operators on a user struct.
//!
//! The standard library's `math` module had scalar helpers only, while 42
//! files of `universe/` import `Vec2`, `Vec3`, `Vec4` or `Mat4` from
//! `core::math` or `std::math`. The module now declares the four types.
//!
//! Two compiler faults stood in the way and are fixed with them. `v * 2.0`
//! with `impl Mul<f32> for Vec3` was refused ("expected `Vec3`, found
//! `{float}`"): both operands of an operator had to be of one type. `-v`
//! with `impl Neg for Vec3` passed the check and the C compiler then
//! stopped on "wrong type argument to unary minus".
//!
//! Checks written before the runs. (3,0,4) + (1,1,1) = (4,1,5); its double
//! is (8,2,10); a quarter of that is (2,0.5,2.5); the length of (3,0,4) is
//! 5; -(3,0,4) has x = -3. A translation by (1,2,3) after a scale by 2
//! takes (1,1,1) to (3,4,5).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-mathtypes-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("program folder");
    let main = dir.join("main.bld");
    std::fs::write(&main, text).expect("source file");
    main
}

fn buildc(command: &str, file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg(command)
        .arg(file)
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

/// Check, build and run the program; its stdout must be `wanted`.
fn checks_and_prints(main: &Path, wanted: &str) {
    let check = buildc("check", main);
    assert!(check.status.success(), "the check failed\n{}", text(&check));

    let run = buildc("run", main);
    let log = text(&run);
    // A machine with no C compiler cannot answer. CI has one, and there a
    // missing compiler is a failure, not a pass.
    if log.contains("no C compiler") {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI has no C compiler\n{log}"
        );
        return;
    }
    assert_eq!(run.status.code(), Some(0), "{log}");
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert_eq!(stdout.trim_end(), wanted, "{log}");
}

const MONEY: &str = "struct Money {\n    cents: i64,\n}\n\nimpl Mul<i64> for Money {\n    type Output = Money;\n    fn mul(self, n: i64) -> Money {\n        Money { cents: self.cents * n }\n    }\n}\n\nimpl Neg for Money {\n    type Output = Money;\n    fn neg(self) -> Money {\n        Money { cents: 0 - self.cents }\n    }\n}\n";

#[test]
fn an_operator_takes_a_right_operand_of_another_type() {
    let main = program(
        "money",
        &format!(
            "{MONEY}\nfn main() ~ Console {{\n    let m = Money {{ cents: 250 }};\n    let t = m * 3;\n    println!(\"{{}}\", t.cents);\n}}\n"
        ),
    );
    checks_and_prints(&main, "750");
}

#[test]
fn unary_minus_calls_the_neg_of_a_struct() {
    let main = program(
        "neg",
        &format!(
            "{MONEY}\nfn main() ~ Console {{\n    let m = Money {{ cents: 250 }};\n    let d = -m;\n    println!(\"{{}}\", d.cents);\n}}\n"
        ),
    );
    checks_and_prints(&main, "-250");
}

#[test]
fn the_right_operand_must_have_the_type_the_operator_takes() {
    let main = program(
        "wrong-operand",
        &format!(
            "{MONEY}\nfn main() ~ Console {{\n    let m = Money {{ cents: 250 }};\n    let n = Money {{ cents: 2 }};\n    let t = m * n;\n    println!(\"{{}}\", t.cents);\n}}\n"
        ),
    );
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert!(
        log.contains("expected `i64`"),
        "no `expected `i64`` in\n{log}"
    );
}

#[test]
fn vectors_come_from_the_standard_library() {
    let main = program(
        "vec3",
        "use std::math::Vec3;\n\nfn main() ~ Console {\n    let a = Vec3::new(3.0, 0.0, 4.0);\n    let c = a + Vec3::ONE;\n    let d = c * 2.0;\n    let e = d / 4.0;\n    let n = -a;\n    println!(\"{} {} {}\", c.x, c.y, c.z);\n    println!(\"{} {} {}\", d.x, d.y, d.z);\n    println!(\"{} {} {}\", e.x, e.y, e.z);\n    println!(\"{} {}\", a.length(), n.x);\n}\n",
    );
    checks_and_prints(&main, "4 1 5\n8 2 10\n2 0.5 2.5\n5 -3");
}

#[test]
fn core_math_names_the_same_module() {
    let main = program(
        "core",
        "use core::math::{Vec2, Vec3, Vec4, Mat4};\n\nfn main() ~ Console {\n    let m = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)) * Mat4::from_scale(Vec3::splat(2.0));\n    let p = m.transform_point(Vec3::ONE);\n    let q = Vec2::new(3.0, 4.0);\n    let w = Vec4::from_point(p);\n    println!(\"{} {} {}\", p.x, p.y, p.z);\n    println!(\"{} {}\", q.length(), w.w);\n}\n",
    );
    checks_and_prints(&main, "3 4 5\n5 1");
}

#[test]
fn the_math_module_checks_on_its_own() {
    let check = buildc("check", &repo_root().join("stdlib/math.bld"));
    assert!(check.status.success(), "{}", text(&check));
}
