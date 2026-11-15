//! `for` over `enumerate()` and over a reversed range, and assertion macros
//! whose arguments hold a comma inside a call.
//!
//! All three were refused at `buildc check` by the C backend gate (BL0901):
//!
//! * `for (i, x) in v.iter().enumerate()`: "`for` cannot iterate over a `i32`"
//! * `for i in (0..n).rev()`: the same message
//! * `assert!(f(1, 2))`: "the arguments of `assert!` are not expressions: `f(1`"
//!
//! The census of `universe/` (universe/tools/census.py, old compiler) shows
//! the first two in 16 failing files and the third in 9.
//!
//! Expected outputs, worked out before the runs:
//!
//! * enumerate with `continue`: items 10 20 30 40, index 1 skipped. Sum of
//!   the indices seen 0+2+3 = 5; sum of the items seen 10+30+40 = 80. A count
//!   that stood still on `continue` would give 0+1+2 = 3.
//! * nested enumerate over 3 and 2 items (1 2 3 and 5 7): sum of x*y is
//!   6*12 = 72; sum of i*10+j is (0+10+20)*2 + (0+1)*3 = 63.
//! * struct items with x = 1 2 3 4: the odd indices hold 2 and 4, sum 6.
//! * a string: "abc" has 3 chars; indices 0+1+2 = 3.
//! * reversed ranges: (0..4) gives 3 2 1 0, read as digits 3210; (1..=3)
//!   gives 321; (0..n) with n: usize = 3 gives 2 1 0, plus one each 321;
//!   (0..5) with 2 skipped gives 4310; (3..3) runs 0 times.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-foradapt-{}-{name}", std::process::id()));
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

#[test]
fn enumerate_counts_every_pass_and_survives_continue() {
    let main = program(
        "enum-continue",
        r#"fn main() ~ Console {
    let v = vec![10, 20, 30, 40];
    let mut index_sum: usize = 0;
    let mut item_sum = 0;
    for (i, x) in v.iter().enumerate() {
        if i == 1 {
            continue;
        }
        index_sum = index_sum + i;
        item_sum = item_sum + x;
    }
    println!("{} {}", index_sum, item_sum);
}
"#,
    );
    checks_and_prints(&main, "5 80");
}

#[test]
fn an_enumerate_inside_an_enumerate_keeps_two_counts() {
    let main = program(
        "enum-nested",
        r#"fn main() ~ Console {
    let a = vec![1, 2, 3];
    let b = vec![5, 7];
    let mut products = 0;
    let mut places: usize = 0;
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            products = products + x * y;
            places = places + i * 10 + j;
        }
    }
    println!("{} {}", products, places);
}
"#,
    );
    checks_and_prints(&main, "72 63");
}

#[test]
fn enumerate_over_struct_items_and_over_a_string() {
    let main = program(
        "enum-struct",
        r#"struct Point {
    x: i32,
    y: i32,
}

fn odd_sum(points: Vec<Point>) -> i32 {
    let mut total = 0;
    for (i, p) in points.iter().enumerate() {
        if i % 2 == 1 {
            total = total + p.x;
        }
    }
    total
}

fn main() ~ Console {
    let mut points: Vec<Point> = Vec::new();
    points.push(Point { x: 1, y: 0 });
    points.push(Point { x: 2, y: 0 });
    points.push(Point { x: 3, y: 0 });
    points.push(Point { x: 4, y: 0 });
    println!("{}", odd_sum(points));

    let word = "abc";
    let mut chars: usize = 0;
    let mut places: usize = 0;
    for (i, _c) in word.chars().enumerate() {
        chars = chars + 1;
        places = places + i;
    }
    println!("{} {}", chars, places);
}
"#,
    );
    checks_and_prints(&main, "6\n3 3");
}

#[test]
fn a_reversed_range_counts_down_and_stops_at_its_start() {
    let main = program(
        "rev",
        r#"fn main() ~ Console {
    let mut a = 0;
    for i in (0..4).rev() {
        a = a * 10 + i;
    }
    println!("{}", a);

    let mut b = 0;
    for i in (1..=3).rev() {
        b = b * 10 + i;
    }
    println!("{}", b);

    let n: usize = 3;
    let mut c: usize = 0;
    for k in (0..n).rev() {
        c = c * 10 + k + 1;
    }
    println!("{}", c);

    let mut d = 0;
    for i in (0..5).rev() {
        if i == 2 {
            continue;
        }
        d = d * 10 + i;
    }
    println!("{}", d);

    let mut e = 0;
    for _i in (3..3).rev() {
        e = e + 1;
    }
    println!("{}", e);
}
"#,
    );
    checks_and_prints(&main, "3210\n321\n321\n4310\n0");
}

#[test]
fn an_assertion_takes_a_call_with_several_arguments() {
    let main = program(
        "assert-call",
        r#"fn below(a: i32, b: i32) -> bool {
    a < b
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() ~ Console {
    assert!(below(1, 2));
    assert_eq!(add(2, 3), 5);
    assert_eq!(add(add(1, 1), 3), add(2, 3));
    assert_ne!(add(2, 2), add(2, 3));
    let v = vec![4, 5, 6];
    assert!(below(v[0], v[2]), "items {} {}", v[0], v[2]);
    println!("reached");
}
"#,
    );
    checks_and_prints(&main, "reached");
}

#[test]
fn a_failing_assertion_on_a_call_stops_the_program_and_names_the_call() {
    let main = program(
        "assert-fails",
        r#"fn below(a: i32, b: i32) -> bool {
    a < b
}

fn main() ~ Console {
    assert!(below(2, 1));
    println!("reached");
}
"#,
    );
    let check = buildc("check", &main);
    assert!(check.status.success(), "the check failed\n{}", text(&check));
    let run = buildc("run", &main);
    let log = text(&run);
    if log.contains("no C compiler") {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI has no C compiler\n{log}"
        );
        return;
    }
    assert!(!run.status.success(), "the program exited 0\n{log}");
    assert!(
        !log.contains("reached"),
        "ran past the failing check\n{log}"
    );
    assert!(log.contains("below(2, 1)"), "the call is not named\n{log}");
}

/// The control: an `enumerate()` over something the backend has no counted
/// loop for stays refused. It must never compile to a loop with no index.
#[test]
fn enumerate_over_an_unknown_iterable_is_still_refused() {
    let main = program(
        "enum-refused",
        r#"struct Bag {
    n: i32,
}

fn main() ~ Console {
    let bag = Bag { n: 3 };
    let mut total: usize = 0;
    for (i, _x) in bag.enumerate() {
        total = total + i;
    }
    println!("{}", total);
}
"#,
    );
    let check = buildc("check", &main);
    assert!(
        !check.status.success(),
        "the check accepted an enumerate over a struct\n{}",
        text(&check)
    );
}
