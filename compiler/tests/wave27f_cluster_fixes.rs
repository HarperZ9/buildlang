//! End-to-end compile, run, and planted-failure regression tests for the
//! Wave 27F compiler, C-backend, and stdlib upgrades.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-wave27f-{}-{name}", std::process::id()));
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

fn checks_and_prints(main: &Path, wanted: &str) {
    let check = buildc("check", main);
    assert!(check.status.success(), "check failed\n{}", text(&check));

    let run = buildc("run", main);
    let log = text(&run);
    if log.contains("no C compiler") {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI has no C compiler\n{log}"
        );
        return;
    }
    assert_eq!(run.status.code(), Some(0), "run failed\n{log}");
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert_eq!(stdout.trim_end(), wanted, "stdout mismatch\n{log}");
}

#[test]
fn raw_format_center_and_dynamic_width() {
    let main = program(
        "raw-fmt",
        r##"fn main() ~ Console {
    let x = 7;
    let s = format!(r#"a\nb={}"#, x);
    println!("{}", s);
    println!("[{:^5}] [{:^w$}]", x, "hi", w = 6);
}
"##,
    );
    checks_and_prints(&main, "a\\nb=7\n[  7  ] [  hi  ]");
}

#[test]
fn impl_display_and_debug_with_write_macro() {
    let main = program(
        "display-debug",
        r#"use std::fmt;

struct Pair {
    a: i32,
    b: i32,
}

impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.a, self.b)
    }
}

impl fmt::Debug for Pair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Pair[{}:{}]", self.a, self.b)
    }
}

fn main() ~ Console {
    let p = Pair { a: 3, b: 9 };
    println!("{} | {:?}", p, p);
}
"#,
    );
    checks_and_prints(&main, "(3, 9) | Pair[3:9]");
}

#[test]
fn first_class_impl_fn_and_next_power_of_two() {
    let main = program(
        "impl-fn-pow2",
        r#"fn apply(x: i32, f: impl Fn(i32) -> i32) -> i32 {
    f(x)
}

fn triple(v: i32) -> i32 {
    v * 3
}

fn main() ~ Console {
    let a = apply(7, triple);
    let b: u32 = 13u32.next_power_of_two();
    let c: u64 = 17u64.next_power_of_two();
    println!("{} {} {}", a, b, c);
}
"#,
    );
    checks_and_prints(&main, "21 16 32");
}

#[test]
fn iter_zip_slice_first_last_fold_max_and_range_map() {
    let main = program(
        "iter-zip-slice",
        r#"fn first_last(xs: &[f32]) -> f32 {
    let a = xs.first().copied().unwrap_or(0.0);
    let b = xs.last().copied().unwrap_or(0.0);
    a + b
}

fn main() ~ Console {
    let a: Vec<f32> = vec![1.0, 2.0, 3.0];
    let b: Vec<f32> = vec![10.0, 20.0, 30.0];
    let prod: Vec<f32> = a.iter().zip(b.iter()).map(|(&x, &y)| x * y).collect();
    let peak = prod.iter().cloned().fold(0.0f32, f32::max);
    let evens: Vec<i32> = (0..4).map(|i| i * 2).collect();
    let mut rev_sum = 0;
    for x in evens.iter().rev() {
        rev_sum = rev_sum * 10 + x;
    }
    let mut take_sum = 0;
    for x in evens.iter().take(2) {
        take_sum = take_sum + x;
    }
    let arr: [f32; 3] = [2.0, 5.0, 9.0];
    let fl = first_last(&arr);
    let mut vals: Vec<f32> = vec![3.0, 1.0, 4.0, 2.0];
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let hi = vals.iter().max().copied().unwrap_or(0.0);
    let lo = vals.iter().min().copied().unwrap_or(0.0);
    let opt_box: Option<i32> = Some(42);
    let mut seen = 0;
    if let Some(v) = &opt_box {
        seen = v;
    }
    println!("{} {} {} {} {} {} {} {}", peak as i32, fl as i32, rev_sum, take_sum, vals[0] as i32, vals[3] as i32, (hi + lo) as i32, seen);
}
"#,
    );
    checks_and_prints(&main, "90 11 6420 2 1 4 5 42");
}

#[test]
fn vecdeque_entry_drain_and_vec_of_option() {
    let main = program(
        "vecdeque-entry-opt",
        r#"use std::collections::{HashMap, VecDeque};

fn main() ~ Console {
    let mut q = VecDeque::new();
    q.push_back(10);
    q.push_back(20);
    q.push_front(5);
    let a = q.pop_front().unwrap_or(-1);
    let b = q.pop_back().unwrap_or(-1);
    let c = q.front().copied().unwrap_or(-1);

    let mut groups: HashMap<String, Vec<i32>> = HashMap::new();
    let g = groups.entry("k".to_string()).or_insert_with(Vec::new);
    g.push(42);
    let got = groups.get("k").len();

    let mut counts: HashMap<String, i32> = HashMap::new();
    *counts.entry(String::from("a")).or_insert(0) += 1;
    *counts.entry(String::from("a")).or_insert(0) += 2;
    let cnt_a = counts.get("a");

    let mut items: Vec<i32> = vec![1, 2, 3];
    let mut drained = 0;
    for x in items.drain(..) {
        drained = drained + x;
    }

    let mut opts: Vec<Option<i32>> = Vec::new();
    opts.push(Some(7));
    opts.push(None);
    let o0 = opts[0].unwrap_or(0);
    let o1 = opts[1].unwrap_or(9);

    println!("{} {} {} {} {} {} {} {} {}", a, b, c, got, cnt_a, drained, items.len(), o0, o1);
}
"#,
    );
    checks_and_prints(&main, "5 20 10 1 3 6 0 7 9");
}

#[test]
fn char_iter_peekable_rfind_split_once_option_take_and_mem_swap() {
    let main = program(
        "char-rfind-swap",
        r#"fn main() ~ Console {
    let s = "a-b-c";
    let rpos = s.rfind('-').unwrap_or(99);
    let (lhs, rhs) = match "1.2+build".split_once('+') {
        Some(pair) => pair,
        None => ("", ""),
    };

    let mut it = "ab".chars().peekable();
    let p0 = it.peek().unwrap_or('?');
    let c0 = it.next().unwrap_or('?');
    let c1 = it.next().unwrap_or('?');
    let done = it.next().is_none();

    let mut opt: Option<i32> = Some(55);
    let taken = opt.take().unwrap_or(0);
    let after = opt.is_none();

    let mut x = 1;
    let mut y = 2;
    std::mem::swap(&mut x, &mut y);

    println!("{} {} {} {} {} {} {} {} {} {} {}", rpos, lhs, rhs, p0, c0, c1, done, taken, after, x, y);
}
"#,
    );
    checks_and_prints(&main, "3 1.2 build a a b true 55 true 2 1");
}

#[test]
fn stdlib_math_geometry_and_buffer_slice() {
    let main = program(
        "stdlib-math-geom",
        r#"use std::math::{Vec3, AABB, Plane, Mat4};
use std::memory::Buffer;

fn main() ~ Console {
    let a = Vec3::new(1.0, 5.0, 2.0);
    let b = Vec3::new(3.0, 2.0, 4.0);
    let mn = a.min(b);
    let mx = a.max(b);
    let box1 = AABB::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0));
    let box2 = AABB::new(Vec3::new(1.0, 1.0, 1.0), Vec3::new(3.0, 3.0, 3.0));
    let hit = box1.intersects(&box2);
    let plane = Plane::new(Vec3::new(0.0, 1.0, 0.0), -2.0);
    let dist = plane.distance_to_point(Vec3::new(0.0, 5.0, 0.0));
    let r0 = Mat4::identity().row(0);
    let mut buf: Buffer<i32> = Buffer::new(4);
    buf.push(11);
    buf.push(22);
    let sl = buf.as_slice();
    println!("{} {} {} {} {} {}", mn.y as i32, mx.x as i32, hit, dist as i32, r0.x as i32, sl.len());
}
"#,
    );
    checks_and_prints(&main, "2 3 true 3 1 2");
}

#[test]
fn planted_negative_controls_still_fail_closed() {
    for (label, src) in [
        (
            "vec_of_tuple_of_array",
            "fn main() ~ Console {\n    let mut v: Vec<([i64; 2], i64)> = Vec::new();\n    v.push(([1, 2], 3));\n    println!(\"{}\", v.len());\n}\n",
        ),
        (
            "binary_format_spec",
            "fn main() ~ Console {\n    println!(\"{:b}\", 5);\n}\n",
        ),
        (
            "non_literal_format",
            "fn main() ~ Console {\n    let n = 5;\n    println!(n);\n}\n",
        ),
    ] {
        let main = program(&format!("neg-{label}"), src);
        let check = buildc("check", &main);
        assert!(
            !check.status.success(),
            "{label}: planted negative control must fail closed\n{}",
            text(&check)
        );
    }
}
