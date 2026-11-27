//! End-to-end compile, run, and planted fail-closed regression tests for the
//! Wave 66B compiler, typechecker, MIR, stdlib, and C-runtime upgrades.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-wave66b-{}-{name}", std::process::id()));
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
fn arc_and_rc_refcount_clone_and_read() {
    let main = program(
        "arc-rc",
        r#"use std::sync::Arc;
use std::rc::Rc;

struct Counter {
    val: i64,
}

fn main() ~ Console {
    let a = Arc::new(Counter { val: 42 });
    let a2 = Arc::clone(&a);
    let ac = Arc::strong_count(&a);
    let r = Rc::new(Counter { val: 7 });
    let r2 = Rc::clone(&r);
    let rc = Rc::strong_count(&r);
    println!("{}|{}|{}|{}", a2.val, ac, r2.val, rc);
}
"#,
    );
    checks_and_prints(&main, "42|2|7|2");
}

#[test]
fn iter_chain_step_by_find_for_each_nested_zip_and_enumerate_rev() {
    let main = program(
        "iter-wave66b",
        r#"fn main() ~ Console {
    let a = vec![1i64, 2, 3];
    let b = vec![4i64, 5];
    let mut chain_sum = 0i64;
    for x in a.iter().chain(b.iter()) {
        chain_sum += *x;
    }

    let stepped: Vec<i64> = vec![10i64, 20, 30, 40, 50].iter().step_by(2).copied().collect();
    let found = vec![3i64, 8, 12, 5].iter().find(|&&x| x > 9).copied().unwrap_or(0);

    let mut fe_sum = 0i64;
    vec![2i64, 4, 6].iter().for_each(|&x| {
        fe_sum += x;
    });

    let xs = vec![1.0, 2.0, 3.0];
    let ys = vec![10.0, 20.0, 30.0];
    let zs = vec![100.0, 200.0, 300.0];
    let triple_sum: f64 = xs.iter().zip(ys.iter()).zip(zs.iter())
        .map(|((&x, &y), &z)| x + y + z)
        .sum();

    let mut rev_weighted = 0i64;
    let items = vec![5i64, 7, 9];
    for (idx, val) in items.iter().enumerate().rev() {
        rev_weighted = rev_weighted * 100 + (idx as i64) * 10 + *val;
    }

    println!("{}|{}|{}|{}|{}|{}", chain_sum, stepped.len(), found, fe_sum, triple_sum as i64, rev_weighted);
}
"#,
    );
    checks_and_prints(&main, "15|3|12|12|666|291705");
}

#[test]
fn collections_vec_resize_split_at_mut_array_fill_and_nested_vec_repeat() {
    let main = program(
        "collections-wave66b",
        r#"fn main() ~ Console {
    let mut xs = vec![1i64, 2];
    xs.resize(5, 9);
    let (left, right) = xs.split_at_mut(2);
    let left_len = left.len();
    let right_first = right[0];

    let mut arr = [0i32; 4];
    arr.fill(7);
    let arr_sum = arr[0] + arr[1] + arr[2] + arr[3];

    let mut buckets: Vec<Vec<i64>> = vec![Vec::new(); 3];
    buckets[0].push(11);
    buckets[2].push(22);

    println!("{}|{}|{}|{}|{}|{}", xs.len(), left_len, right_first, arr_sum, buckets[0].len(), buckets[1].len());
}
"#,
    );
    checks_and_prints(&main, "5|2|9|28|1|0");
}

#[test]
fn strings_splitn_parse_bool_and_char_is_ascii_hexdigit() {
    let main = program(
        "strings-wave66b",
        r#"fn main() ~ Console {
    let parts: Vec<String> = "a:b:c:d".splitn(3, ":").collect();
    let t = "true".parse::<bool>().unwrap_or(false);
    let f = "false".parse::<bool>().unwrap_or(true);
    let bad = "maybe".parse::<bool>().is_err();
    let h1 = 'a'.is_ascii_hexdigit();
    let h2 = 'F'.is_ascii_hexdigit();
    let h3 = 'g'.is_ascii_hexdigit();
    println!("{}|{}|{}|{}|{}|{}|{}|{}", parts.len(), parts[2], t, f, bad, h1, h2, h3);
}
"#,
    );
    checks_and_prints(&main, "3|c:d|true|false|true|true|true|false");
}

#[test]
fn hashmap_get_option_binding_and_tuple_if_let() {
    let main = program(
        "hashmap-option-wave66b",
        r#"use std::collections::HashMap;

struct Inv {
    quantity: i64,
}

fn main() ~ Console {
    let mut inv_map: HashMap<String, Inv> = HashMap::new();
    inv_map.insert("AAPL".to_string(), Inv { quantity: 25 });
    let hit = inv_map.get("AAPL");
    let miss = inv_map.get("MSFT");
    let q_hit = hit.map(|i| i.quantity).unwrap_or(-1);
    let q_miss = miss.map(|i| i.quantity).unwrap_or(-1);

    let mut series: HashMap<String, Vec<f64>> = HashMap::new();
    series.insert("X".to_string(), vec![1.0, 2.0, 3.0]);
    series.insert("Y".to_string(), vec![4.0, 5.0]);
    let r1 = series.get("X");
    let r2 = series.get("Y");
    let pair_len = if let (Some(a), Some(b)) = (r1, r2) {
        a.len() + b.len()
    } else {
        0
    };

    println!("{}|{}|{}", q_hit, q_miss, pair_len);
}
"#,
    );
    checks_and_prints(&main, "25|-1|5");
}

#[test]
fn math_mat4_m_mat3_literal_vec_clamp_atomic_and_f64_consts() {
    let main = program(
        "math-atomic-wave66b",
        r#"use core::math::{Vec3, Mat3, Mat4};
use core::atomic::{AtomicU32, Ordering};

fn main() ~ Console {
    let m4 = Mat4::identity();
    let diag = m4.m[0][0] + m4.m[1][1] + m4.m[2][2] + m4.m[3][3];
    let m3 = Mat3 { m: [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 4.0]] };
    let v = Vec3::new(5.0, -2.0, 0.5).clamp(0.0, 1.0);
    let ctr = AtomicU32::new(10);
    ctr.fetch_add(5, Ordering::SeqCst);
    let cval = ctr.load(Ordering::SeqCst);
    let pi_int = (std::f64::consts::PI * 100.0) as i64;
    println!("{}|{}|{}|{}|{}", diag as i64, m3.m[1][1] as i64, (v.x * 10.0) as i64, cval, pi_int);
}
"#,
    );
    checks_and_prints(&main, "4|3|10|15|314");
}

#[test]
fn planted_control_undeclared_effect_still_rejected_bl0402() {
    let main = program(
        "neg-bl0402",
        r#"fn pure_fn() {
    println!("should fail without Console effect");
}

fn main() ~ Console {
    pure_fn();
}
"#,
    );
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(
        !check.status.success() && log.contains("BL0402"),
        "expected BL0402 undeclared effect failure, got:\n{log}"
    );
}

#[test]
fn planted_control_duplicate_definition_still_rejected_bl0380() {
    let main = program(
        "neg-bl0380",
        r#"struct Dup { x: i32 }
struct Dup { y: i32 }
fn main() {}
"#,
    );
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(
        !check.status.success() && log.contains("BL0380"),
        "expected BL0380 duplicate definition failure, got:\n{log}"
    );
}

#[test]
fn planted_control_type_mismatch_still_rejected_bl0301() {
    let main = program(
        "neg-bl0301",
        r#"fn takes_i64(x: i64) -> i64 { x }
fn main() {
    let _ = takes_i64("not-an-int");
}
"#,
    );
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(
        !check.status.success() && log.contains("BL0301"),
        "expected BL0301 type mismatch failure, got:\n{log}"
    );
}

#[test]
fn planted_control_unresolved_import_still_rejected_bl0611() {
    let main = program(
        "neg-bl0611",
        r#"use nonexistent_crate_wave66b::MissingType;
fn main() {}
"#,
    );
    let check = buildc("check", &main);
    let log = text(&check);
    assert!(
        !check.status.success() && log.contains("BL0611"),
        "expected BL0611 unresolved import failure, got:\n{log}"
    );
}
