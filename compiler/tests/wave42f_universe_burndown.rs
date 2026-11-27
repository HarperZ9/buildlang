//! End-to-end compile, run, and planted fail-closed regression tests for the
//! Wave 42F compiler, parser, typechecker, MIR, and C-runtime upgrades.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-wave42f-{}-{name}", std::process::id()));
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
fn vec_reserve_truncate_swap_remove_and_local_struct() {
    let main = program(
        "vec-local-struct",
        r#"struct BoxVal {
    v: i64,
}

impl BoxVal {
    fn doubled(&self) -> i64 {
        self.v * 2
    }
}

fn run_local_bin() -> i64 {
    struct LocalBin {
        item: BoxVal,
        count: usize,
    }
    let b = LocalBin { item: BoxVal { v: 21 }, count: 3 };
    b.item.doubled() + b.count as i64
}

fn main() ~ Console {
    let mut xs = Vec::<i64>::new();
    xs.reserve(16);
    xs.push(10);
    xs.push(20);
    xs.push(30);
    xs.push(40);
    let removed = xs.swap_remove(1);
    xs.truncate(2);
    let local_res = run_local_bin();
    println!("{}|{}|{}|{}|{}", removed, xs.len(), xs[0], xs[1], local_res);
}
"#,
    );
    checks_and_prints(&main, "20|2|10|40|45");
}

#[test]
fn iter_combinators_chunks_windows_dedup_intersperse_running_fold() {
    let main = program(
        "iter-comb",
        r#"fn main() ~ Console {
    let nums = vec![1i64, 1, 2, 2, 2, 3, 1];
    let deduped: Vec<i64> = nums.clone().into_iter().dedup().collect();
    let uniq: Vec<i64> = nums.clone().into_iter().unique().collect();
    let base = vec![10i64, 20, 30];
    let spaced: Vec<i64> = base.clone().into_iter().intersperse(0).collect();
    let folded: Vec<i64> = base.into_iter().running_fold(0i64, |acc, x| acc + x).collect();
    let win_count = vec![1i64, 2, 3, 4, 5].into_iter().windows(3).count();
    let chunk_count = vec![1i64, 2, 3, 4, 5].into_iter().chunks(2).count();
    println!(
        "{}|{}|{}|{}|{}|{}",
        deduped.len(),
        uniq.len(),
        spaced.len(),
        folded[2],
        win_count,
        chunk_count
    );
}
"#,
    );
    checks_and_prints(&main, "4|3|5|60|3|3");
}

#[test]
fn inline_module_qualified_calls_and_generic_freshening() {
    let main = program(
        "inline-mod-calls",
        r#"pub mod sampling {
    pub fn pair_sum(a: i64, b: i64) -> i64 {
        a + b
    }

    pub fn apply<F>(f: F, x: i64) -> i64
    where
        F: Fn(i64) -> i64,
    {
        f(x)
    }
}

fn main() ~ Console {
    let s = sampling::pair_sum(12, 30);
    let sq = sampling::apply(|v| v * v, 7);
    println!("{}|{}", s, sq);
}
"#,
    );
    checks_and_prints(&main, "42|49");
}

#[test]
fn cross_function_result_option_no_local_id_leak() {
    let main = program(
        "opt-res-isolation",
        r#"fn first_ok(flag: bool) -> Result<i64, String> {
    if flag {
        Ok(100)
    } else {
        Err("bad".to_string())
    }
}

fn second_opt(x: i64) -> Option<i64> {
    if x > 0 {
        Some(x + 5)
    } else {
        None
    }
}

fn combine(flag: bool, x: i64) -> Result<i64, String> {
    let a = first_ok(flag)?;
    let b = second_opt(x).unwrap_or(0);
    Ok(a + b)
}

fn main() ~ Console {
    let r1 = combine(true, 10).unwrap_or(-1);
    let r2 = combine(false, 10).unwrap_or(-1);
    let r3 = combine(true, -5).unwrap_or(-1);
    println!("{}|{}|{}", r1, r2, r3);
}
"#,
    );
    checks_and_prints(&main, "115|-1|100");
}

#[test]
fn selective_import_no_unimported_stdlib_collision_and_scalar_vec_mul() {
    let main = program(
        "selective-math-aabb",
        r#"use std::math::{Vec2, Vec3};

pub struct AABB {
    pub min: Vec3,
    pub max: Vec3,
}

impl AABB {
    pub fn volume(&self) -> f32 {
        let d = self.max - self.min;
        d.x * d.y * d.z
    }
}

fn main() ~ Console {
    let v = Vec2::new(3.0, 4.0);
    let scaled = 2.0 * v - Vec2::ONE;
    let b = AABB {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(2.0, 3.0, 4.0),
    };
    println!("{}|{}|{}", scaled.x as i64, scaled.y as i64, b.volume() as i64);
}
"#,
    );
    checks_and_prints(&main, "5|7|24");
}

#[test]
fn float_from_bytes_and_utf16_lossy() {
    let main = program(
        "float-bytes-utf16",
        r#"fn main() ~ Console {
    let f = f32::from_le_bytes([0x00u8, 0x00, 0x80, 0x3F]);
    let d = f64::from_be_bytes([0x40u8, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    let units: Vec<u16> = vec![72u16, 105, 33];
    let s = String::from_utf16_lossy(&units);
    println!("{}|{}|{}", f as i64, d as i64, s);
}
"#,
    );
    checks_and_prints(&main, "1|3|Hi!");
}

#[test]
fn if_let_tuple_of_ref_options_and_nested_enum_in_some() {
    let main = program(
        "if-let-nested",
        r#"enum Payload {
    Num(i64),
    Empty,
}

fn main() ~ Console {
    let oa: Option<i64> = Some(11);
    let ob: Option<i64> = Some(31);
    let sum = if let (Some(ref a), Some(ref b)) = (&oa, &ob) {
        *a + *b
    } else {
        0
    };
    let wrapped: Option<Payload> = Some(Payload::Num(99));
    let nested = if let Some(Payload::Num(v)) = wrapped {
        v
    } else {
        -1
    };
    println!("{}|{}", sum, nested);
}
"#,
    );
    checks_and_prints(&main, "42|99");
}

#[test]
fn byte_string_repeat_and_vec_fill_and_contains() {
    let main = program(
        "byte-repeat-vec-fill",
        r#"fn main() ~ Console {
    let bytes: Vec<u8> = b"abc".repeat(3);
    let mut xs = vec![1i64, 2, 3, 4];
    xs.fill(7);
    let has_seven = xs.contains(&7);
    let has_one = xs.contains(&1);
    println!("{}|{}|{}|{}", bytes.len(), xs[2], has_seven, has_one);
}
"#,
    );
    checks_and_prints(&main, "9|7|true|false");
}

#[test]
fn btreemap_vec_reverse_sort_by_key_and_char_predicates() {
    let main = program(
        "btreemap-vec-char",
        r#"use std::collections::BTreeMap;

fn main() ~ Console {
    let mut m: BTreeMap<String, i64> = BTreeMap::new();
    m.insert("two".to_string(), 20);
    m.insert("one".to_string(), 10);
    let v = m.get("two").unwrap_or(0);
    let mut xs = vec![30i64, 10, 20];
    xs.sort_by_key(|x| *x);
    xs.reverse();
    let ok_alpha = 'Z'.is_alphabetic();
    let ok_digit = '7'.is_numeric();
    println!("{}|{}|{}|{}|{}|{}", m.len(), v, xs[0], xs[2], ok_alpha, ok_digit);
}
"#,
    );
    checks_and_prints(&main, "2|20|30|10|true|true");
}

#[test]
fn planted_control_vec_of_fixed_array_refused() {
    let main = program(
        "planted-vec-tuple-array",
        r#"fn main() {
    let mut v: Vec<([i64; 2], i64)> = Vec::new();
    v.push(([1, 2], 3));
}
"#,
    );
    let out = buildc("check", &main);
    let log = text(&out);
    assert!(
        !out.status.success(),
        "planted control Vec<([i64; 2], i64)> must be refused, got pass:\n{log}"
    );
}

#[test]
fn planted_control_undefined_function_refused() {
    let main = program(
        "planted-undef-fn",
        r#"fn main() {
    let _x = nonexistent_wave42f_helper(42);
}
"#,
    );
    let out = buildc("check", &main);
    let log = text(&out);
    assert!(
        !out.status.success(),
        "planted control undefined function must fail check, got pass:\n{log}"
    );
}

#[test]
fn planted_control_undeclared_console_effect_refused() {
    let main = program(
        "planted-undeclared-effect",
        r#"fn main() {
    println!("missing console effect");
}
"#,
    );
    let out = buildc("check", &main);
    let log = text(&out);
    assert!(
        !out.status.success() && log.contains("BL0402"),
        "planted control undeclared effect must fail with BL0402, got:\n{log}"
    );
}
