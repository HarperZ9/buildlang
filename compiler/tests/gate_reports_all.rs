//! The C backend gate reports every function it refuses, one refusal each.
//!
//! Before, `buildc check` printed one BL0901 and stopped: lowering returned
//! at the first function it could not compile. A file with three such
//! functions took three runs to read. The census of `universe/` had to stub
//! one function per run for the same reason.
//!
//! The refused form used here is `for (i, x) in bag.enumerate()` over a
//! struct with no `next` method: the type checker accepts it and the backend
//! has no loop for it ("`for` cannot iterate over a `Bag` in the C backend").
//!
//! Expected, worked out before the runs. In `THREE` the refused expressions
//! are `bag` at line 7 column 20, `bag` at line 19 column 20 and `other` at
//! line 28 column 24. `fine` (line 13) and `main` compile and report nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-gateall-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("program folder");
    let main = dir.join("main.bld");
    std::fs::write(&main, text).expect("source file");
    main
}

fn buildc(args: &[&str], file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(args)
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

const THREE: &str = r#"struct Bag {
    n: i32,
}

fn first(bag: Bag) -> usize {
    let mut total: usize = 0;
    for (i, _x) in bag.enumerate() {
        total = total + i;
    }
    total
}

fn fine(a: i32) -> i32 {
    a + 1
}

fn second(bag: Bag) -> usize {
    let mut total: usize = 0;
    for (i, _x) in bag.enumerate() {
        total = total + i;
    }
    total
}

impl Bag {
    fn third(&self, other: Bag) -> usize {
        let mut total: usize = 0;
        for (i, _x) in other.enumerate() {
            total = total + i;
        }
        total
    }
}

fn main() ~ Console {
    println!("{}", fine(1));
}
"#;

const REFUSAL: &str = "`for` cannot iterate over a `Bag` in the C backend";

#[test]
fn check_reports_three_refused_functions() {
    let main = program("three", THREE);
    let check = buildc(&["check"], &main);
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert_eq!(
        log.matches("[BL0901]").count(),
        3,
        "three functions are refused, so three BL0901 lines\n{log}"
    );
    for at in ["7:20: [BL0901]", "19:20: [BL0901]", "28:24: [BL0901]"] {
        assert!(log.contains(at), "no refusal at {at}\n{log}");
    }
    assert_eq!(log.matches(REFUSAL).count(), 3, "{log}");
}

#[test]
fn refusals_come_in_source_order() {
    let main = program("order", THREE);
    let log = text(&buildc(&["check"], &main));
    let first = log.find("7:20: [BL0901]").expect("first refusal");
    let second = log.find("19:20: [BL0901]").expect("second refusal");
    let third = log.find("28:24: [BL0901]").expect("third refusal");
    assert!(first < second && second < third, "{log}");
}

#[test]
fn the_json_report_lists_three_refusals() {
    let main = program("json", THREE);
    let check = buildc(&["check", "--error-format", "json"], &main);
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert_eq!(log.matches("BL0901").count(), 3, "{log}");
}

#[test]
fn the_c_build_reports_three_refused_functions() {
    let main = program("build", THREE);
    let out = main.with_extension("c");
    let build = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg(&main)
        .args(["--target", "c", "-o"])
        .arg(&out)
        .output()
        .expect("buildc runs");
    let log = text(&build);
    assert!(!build.status.success(), "the build passed\n{log}");
    assert_eq!(log.matches(REFUSAL).count(), 3, "{log}");
    assert!(!out.exists(), "a refused program left a C file");
}

/// One function refused twice still reports once: a body stops at its first
/// refusal. The count is per function, not per refused expression.
#[test]
fn one_function_reports_one_refusal() {
    let main = program(
        "twice",
        r#"struct Bag {
    n: i32,
}

fn both(a: Bag, b: Bag) -> usize {
    let mut total: usize = 0;
    for (i, _x) in a.enumerate() {
        total = total + i;
    }
    for (j, _y) in b.enumerate() {
        total = total + j;
    }
    total
}

fn main() ~ Console {
    println!("{}", 1);
}
"#,
    );
    let check = buildc(&["check"], &main);
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert_eq!(log.matches("[BL0901]").count(), 1, "{log}");
    assert!(log.contains("7:20: [BL0901]"), "{log}");
}

/// The control. A refused function must not disturb the ones after it: after
/// the refused function is removed, the same later functions compile and the
/// program prints what it should. `tally(4)` is 0+1+2+3 = 6 and `Bag { n: 5 }
/// .twice()` is 10, so the line is "6 10".
#[test]
fn functions_after_a_refused_one_still_compile_when_it_is_gone() {
    let after = r#"fn tally(n: usize) -> usize {
    let mut total: usize = 0;
    let v = vec![7, 8, 9, 10];
    for (i, _x) in v.iter().enumerate() {
        if i < n {
            total = total + i;
        }
    }
    total
}

impl Bag {
    fn twice(&self) -> i32 {
        self.n * 2
    }
}

fn main() ~ Console {
    let bag = Bag { n: 5 };
    println!("{} {}", tally(4), bag.twice());
}
"#;
    let refused = r#"fn first(bag: Bag) -> usize {
    let mut total: usize = 0;
    for (i, _x) in bag.enumerate() {
        total = total + i;
    }
    total
}

"#;
    let head = "struct Bag {\n    n: i32,\n}\n\n";

    let with = program("with", &format!("{head}{refused}{after}"));
    let check = buildc(&["check"], &with);
    let log = text(&check);
    assert!(!check.status.success(), "the check passed\n{log}");
    assert_eq!(
        log.matches("[BL0901]").count(),
        1,
        "only `first` is refused; the functions after it are sound\n{log}"
    );
    assert!(log.contains("7:20: [BL0901]"), "{log}");

    let without = program("without", &format!("{head}{after}"));
    let check = buildc(&["check"], &without);
    assert!(check.status.success(), "{}", text(&check));
    let run = buildc(&["run"], &without);
    assert!(run.status.success(), "{}", text(&run));
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "6 10");
}
