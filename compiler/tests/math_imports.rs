//! `use std::math::{sqrt, min, PI};` imports the built-in of that name.
//!
//! The maths functions (`sqrt`, `min`, `pow`, `clamp`, ...) and the constants
//! `PI`, `E` and `TAU` are built into the compiler and need no import. A
//! program that did import one from `std::math` or `core::math` was refused:
//!
//! ```text
//! [BL0611] unresolved import `std::math::sqrt`: the standard library module
//! `math` (.../stdlib/math.bld) has no item `sqrt`
//! ```
//!
//! 73 files of `universe/` import at least one. The import now resolves to
//! the built-in and binds nothing, so there is one definition of each name.
//!
//! `lerp(a, b, t)` on numbers passed the check and failed in the C compiler
//! ("incompatible type for argument 1 of 'build_lerp3'"): only the vector
//! form had a C function. It now computes `a + (b - a) * t`.
//!
//! Expected outputs, worked out before the runs:
//!
//! * min(2, 9) = 2, max = 9, pow(2, 3) = 8, abs(0 - 2) = 2
//! * exp(0) = 1, sqrt(9) = 3, sin(0) = 0, cos(0) = 1, log(1) = 0
//! * clamp(9, 0, 1) = 1, floor(2.7) = 2, atan2(0, 1) = 0, ceil(2.2) = 3
//! * tan(0) = 0, tanh(0) = 0, smoothstep(0, 1, 0.25) = 0.25^2 * (3 - 0.5) = 0.15625
//! * PI prints 3.141592653589793; as `f32`, PI * 2 * 2 prints 12.566371
//! * sqrt of the `f32` 16 is 4, and min(16, 4) is 4
//! * lerp(2, 9, 0.5) = 2 + 7 * 0.5 = 5.5; lerp(10, 20, 0.25) = 12.5;
//!   the vector lerp of (0, 0, 0) and (2, 4, 6) at 0.5 is (1, 2, 3)

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-mathimp-{}-{name}", std::process::id()));
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

/// Check, then run, and return what the program printed.
fn runs(name: &str, source: &str) -> String {
    let main = program(name, source);
    let check = buildc("check", &main);
    assert!(check.status.success(), "{name}: check\n{}", text(&check));
    let run = buildc("run", &main);
    assert!(run.status.success(), "{name}: run\n{}", text(&run));
    String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n")
}

const BODY: &str = r#"
fn area(r: f32) -> f32 {
    PI * r * r
}

fn main() ~ Console {
    let two: f64 = 2.0;
    let nine: f64 = 9.0;
    println!("{} {} {} {}", min(two, nine), max(two, nine), pow(two, 3.0), abs(0.0 - two));
    println!("{} {} {} {} {}", exp(0.0), sqrt(nine), sin(0.0), cos(0.0), log(1.0));
    println!("{} {} {} {}", clamp(nine, 0.0, 1.0), floor(2.7), atan2(0.0, 1.0), ceil(2.2));
    println!("{} {} {}", tan(0.0), tanh(0.0), smoothstep(0.0, 1.0, 0.25));
    println!("{}", PI);
    let r: f32 = 2.0;
    println!("{}", area(r));
    let small: f32 = 16.0;
    let root: f32 = sqrt(small);
    println!("{} {}", root, min(small, root));
}
"#;

const EXPECTED: &str =
    "2 9 8 2\n1 3 0 1 0\n1 2 0 3\n0 0 0.15625\n3.141592653589793\n12.566371\n4 4\n";

#[test]
fn the_names_imported_from_std_math_are_the_built_ins() {
    let source = format!(
        "use std::math::{{min, max, pow, abs, exp, sqrt, sin, cos, log, clamp}};\n\
         use std::math::{{floor, atan2, ceil, tan, tanh, smoothstep, PI}};\n{BODY}"
    );
    assert_eq!(runs("std", &source), EXPECTED);
}

#[test]
fn the_names_imported_from_core_math_are_the_built_ins() {
    let source = format!(
        "use core::math::{{min, max, pow, abs, exp, sqrt, sin, cos, log, clamp}};\n\
         use core::math::{{floor, atan2, ceil, tan, tanh, smoothstep, PI}};\n{BODY}"
    );
    assert_eq!(runs("core", &source), EXPECTED);
}

/// The control for the two tests above: the same program with no import
/// prints the same lines, so the import adds no second definition.
#[test]
fn the_same_program_with_no_import_prints_the_same() {
    assert_eq!(runs("bare", BODY), EXPECTED);
}

#[test]
fn single_imports_and_a_type_beside_a_function() {
    let out = runs(
        "mixed",
        r#"use std::math::sqrt;
use core::math::PI;
use std::math::{Vec3, max, E, TAU};

fn main() ~ Console {
    let v = Vec3::new(1.0, 2.0, 2.0);
    let len: f32 = v.length();
    println!("{} {} {}", sqrt(49.0), max(len, 1.0), PI);
    println!("{} {}", E, TAU);
}
"#,
    );
    assert_eq!(
        out,
        "7 3 3.141592653589793\n2.718281828459045 6.283185307179586\n"
    );
}

#[test]
fn lerp_on_numbers_compiles_and_interpolates() {
    let out = runs(
        "lerp",
        r#"use std::math::lerp;

fn blend(a: f32, b: f32, t: f32) -> f32 {
    lerp(a, b, t)
}

fn main() ~ Console {
    let x: f64 = 2.0;
    let y: f64 = 9.0;
    println!("{}", lerp(x, y, 0.5));
    println!("{}", blend(10.0, 20.0, 0.25));
    let v = lerp(vec3(0.0, 0.0, 0.0), vec3(2.0, 4.0, 6.0), 0.5);
    println!("{} {} {}", v.x, v.y, v.z);
}
"#,
    );
    assert_eq!(out, "5.5\n12.5\n1 2 3\n");
}

/// A program's own function still takes the name, with the import or
/// without it: 1 + 100 = 101, never the built-in clamp.
#[test]
fn a_function_the_program_declares_keeps_the_name() {
    let own = "fn clamp(x: i32) -> i32 {\n    x + 100\n}\n\nfn main() ~ Console {\n    println!(\"{}\", clamp(1));\n}\n";
    assert_eq!(runs("own", own), "101\n");
    let imported = format!("use std::math::clamp;\n{own}");
    assert_eq!(runs("own-imported", &imported), "101\n");
}

/// The same for `lerp`, which is not a C library name and so takes another
/// path in the compiler. The program's function adds 100, so the answer is
/// 1 + (3 - 1) * 0.5 + 100 = 102; the built-in would give 2. The first fix
/// run sent this call to the built-in, and `tests/tv_float.rs` caught it.
#[test]
fn a_lerp_the_program_declares_keeps_the_name() {
    let own = r#"fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t + 100.0
}

fn main() ~ Console {
    println!("{}", lerp(1.0, 3.0, 0.5));
}
"#;
    assert_eq!(runs("own-lerp", own), "102\n");
    let imported = format!("use std::math::lerp;\n{own}");
    assert_eq!(runs("own-lerp-imported", &imported), "102\n");
}

fn refused(name: &str, source: &str, needles: &[&str]) {
    let main = program(name, source);
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(!check.status.success(), "{name}: the check passed\n{log}");
    for needle in needles {
        assert!(log.contains(needle), "{name}: no `{needle}` in\n{log}");
    }
}

/// The controls: a name the compiler does not provide is still an
/// unresolved import, and a built-in is importable from `math` only.
#[test]
fn a_name_that_is_not_a_built_in_is_still_refused() {
    refused(
        "lgamma",
        "use std::math::lgamma;\n\nfn main() ~ Console {\n    println!(\"{}\", 1);\n}\n",
        &["[BL0611]", "`std::math::lgamma`", "has no item `lgamma`"],
    );
    refused(
        "nonsense",
        "use core::math::{sqrt, wobble};\n\nfn main() ~ Console {\n    println!(\"{}\", sqrt(4.0));\n}\n",
        &["[BL0611]", "`core::math::wobble`", "has no item `wobble`"],
    );
    let main = program(
        "one-report",
        "use core::math::{sqrt, wobble};\n\nfn main() ~ Console {\n    println!(\"{}\", sqrt(4.0));\n}\n",
    );
    let log = text(&buildc("check", &main));
    assert_eq!(log.matches("[BL0611]").count(), 1, "{log}");
}

#[test]
fn a_built_in_is_not_an_item_of_another_library_module() {
    refused(
        "strings",
        "use std::strings::sqrt;\n\nfn main() ~ Console {\n    println!(\"{}\", sqrt(4.0));\n}\n",
        &["[BL0611]", "`std::strings::sqrt`", "has no item `sqrt`"],
    );
}

/// One definition of each name: `stdlib/math.bld` (and its copy in the
/// compiler crate) must not declare a function or constant named like a
/// built-in. A declared `fn sqrt` would replace the built-in for every
/// program that imports from the module.
#[test]
fn the_math_module_declares_no_function_named_like_a_built_in() {
    let built_ins = [
        "sqrt",
        "sin",
        "cos",
        "tan",
        "pow",
        "abs",
        "sinh",
        "cosh",
        "tanh",
        "asin",
        "acos",
        "atan",
        "log",
        "log2",
        "log10",
        "exp",
        "exp2",
        "atan2",
        "floor",
        "ceil",
        "round",
        "min",
        "max",
        "clamp",
        "smoothstep",
        "mix",
        "fract",
        "step",
        "dot",
        "cross",
        "normalize",
        "length",
        "reflect",
        "lerp",
        "PI",
        "E",
        "TAU",
    ];
    for file in ["stdlib/math.bld", "compiler/stdlib/math.bld"] {
        let text = std::fs::read_to_string(repo_root().join(file)).expect(file);
        for line in text.lines() {
            // Top-level items only: a method inside an `impl` is indented.
            let rest = line.strip_prefix("pub ").unwrap_or(line);
            let name = rest
                .strip_prefix("fn ")
                .or_else(|| rest.strip_prefix("const "))
                .or_else(|| rest.strip_prefix("static "));
            let Some(name) = name else { continue };
            let name: String = name
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            assert!(
                !built_ins.contains(&name.as_str()),
                "{file} declares `{name}`, which is a built-in"
            );
        }
    }
}
