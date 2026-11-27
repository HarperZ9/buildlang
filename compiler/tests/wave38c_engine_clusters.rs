//! End-to-end compile, run, and planted fail-closed regression tests for the
//! Wave 38C compiler, typechecker, and C-backend upgrades.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-wave38c-{}-{name}", std::process::id()));
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
fn string_split_once_strip_prefix_suffix_and_ascii_case() {
    let main = program(
        "str-ops",
        r#"fn main() ~ Console {
    let raw = "v1.2.3+build9";
    let trimmed = raw.strip_prefix('v').unwrap_or(raw);
    let (ver, meta) = match trimmed.split_once('+') {
        Some((a, b)) => (a, b),
        None => (trimmed, "none"),
    };
    let no_suf = ver.strip_suffix(".3").unwrap_or("miss");
    let eq1 = "FoO".eq_ignore_ascii_case("foo");
    let eq2 = "FoO".eq_ignore_ascii_case("bar");
    let rep = "ab".repeat(3);
    let hits = "ababab".matches("ab").len();
    let idx_sum: usize = "abc".char_indices().map(|(i, _)| i).sum();
    println!("{}|{}|{}|{}|{}|{}|{}|{}", ver, meta, no_suf, eq1, eq2, rep, hits, idx_sum);
}
"#,
    );
    checks_and_prints(&main, "1.2.3|build9|1.2|true|false|ababab|3|3");
}

#[test]
fn option_combinators_tuple_option_match_and_transpose() {
    let main = program(
        "opt-comb",
        r#"fn fmt_pair(a: Option<i32>, b: Option<i32>) -> String {
    match (a, b) {
        (Some(x), Some(y)) => format!("{}.{}", x, y),
        (Some(x), None) => format!("{}.*", x),
        _ => "*".to_string(),
    }
}

fn parse_opt(v: &Vec<String>, idx: usize) -> Result<i32, String> {
    let parsed: i32 = v.get(idx)
        .filter(|s| !s.eq_ignore_ascii_case("x"))
        .map(|s| s.parse::<i32>())
        .transpose()
        .map_err(|_| "bad".to_string())?
        .unwrap_or(99);
    Ok(parsed)
}

fn main() ~ Console {
    let p1 = fmt_pair(Some(2), Some(5));
    let p2 = fmt_pair(Some(3), None);
    let p3 = fmt_pair(None, None);
    let items = vec!["42".to_string(), "X".to_string()];
    let r0 = parse_opt(&items, 0).unwrap();
    let r1 = parse_opt(&items, 1).unwrap();
    let r2 = parse_opt(&items, 9).unwrap();
    let d: i32 = None.unwrap_or_default();
    println!("{}|{}|{}|{}|{}|{}|{}", p1, p2, p3, r0, r1, r2, d);
}
"#,
    );
    checks_and_prints(&main, "2.5|3.*|*|42|99|99|0");
}

#[test]
fn iter_chains_windows_chunks_and_hashmap_collect() {
    let main = program(
        "iter-chains",
        r#"use std::collections::HashMap;

fn main() ~ Console {
    let words = vec!["10".to_string(), "nope".to_string(), "25".to_string(), "5".to_string()];
    let nums: Vec<i32> = words.iter().filter_map(|s| s.parse::<i32>().ok()).collect();
    let second = nums.iter().nth(1).unwrap();
    let pos = nums.iter().position(|x| x == 25).unwrap();
    let sum_red = nums.iter().reduce(|a, b| a + b).unwrap();
    let max_v = nums.iter().max_by(|a, b| a.cmp(&b)).unwrap();
    let min_v = nums.iter().min_by(|a, b| a.cmp(&b)).unwrap();

    let slice: &[i32] = nums.as_slice();
    let diffs: Vec<i32> = slice.windows(2).map(|w| w[1] - w[0]).collect();
    let mut chunk_sums = 0;
    for ch in slice.chunks_exact(2) {
        chunk_sums += ch[0] + ch[1];
    }

    let mut m: HashMap<String, f64> = HashMap::new();
    m.insert("a".to_string(), 2.0);
    m.insert("b".to_string(), 3.0);
    let doubled: HashMap<String, f64> = m.iter().map(|(k, v)| (k.clone(), v * 2.0)).collect();
    let val_sum: f64 = doubled.values().iter().sum();

    println!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{:.1}",
        nums.len(),
        second,
        pos,
        sum_red,
        max_v,
        min_v,
        diffs[0],
        chunk_sums,
        val_sum
    );
}
"#,
    );
    checks_and_prints(&main, "3|25|1|40|25|5|15|35|10.0");
}

#[test]
fn binary_bytes_rotate_bytestr_slices_and_euclid_math() {
    let main = program(
        "bin-math",
        r#"fn main() ~ Console {
    let le16 = u16::from_le_bytes([0x34u8, 0x12u8]);
    let be32 = u32::from_be_bytes([0x01u8, 0x02u8, 0x03u8, 0x04u8]);
    let le32 = u32::from_le_bytes([0x04u8, 0x03u8, 0x02u8, 0x01u8]);
    let back_be = be32.to_be_bytes();
    let rot = (0x80000001u32).rotate_left(1);

    let raw = b"ABCD";
    let sw = raw.starts_with(b"AB");
    let rep = raw.repeat(2);
    let g1 = raw.get(1).copied().unwrap_or(0u8);
    let has_c = raw.contains(&67u8);

    let q = (-7i64).div_euclid(4i64);
    let r = (-7i64).rem_euclid(4i64);
    let sg = (-3.5f64).signum();

    println!(
        "{:x}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{:.1}",
        le16,
        be32 == le32,
        back_be[0],
        rot,
        sw,
        rep.len(),
        g1,
        has_c,
        q,
        r,
        sg
    );
}
"#,
    );
    checks_and_prints(&main, "1234|true|1|3|true|8|66|true|-2|1|-1.0");
}

#[test]
fn vec_dedup_extend_user_iter_collect_and_enum_static_method() {
    let main = program(
        "vec-enum-iter",
        r#"struct Counter {
    cur: i32,
    end: i32,
}

impl Counter {
    fn next(&mut self) -> Option<i32> {
        if self.cur < self.end {
            let v = self.cur;
            self.cur += 1;
            Some(v)
        } else {
            None
        }
    }
}

struct SemVer {
    major: u32,
    minor: u32,
}

impl SemVer {
    fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('.').collect();
        let major = parts[0].parse().map_err(|_| "err".to_string())?;
        let minor = parts[1].parse().map_err(|_| "err".to_string())?;
        Ok(Self { major, minor })
    }
}

enum Tag {
    Num(u32),
    Wild { major: Option<u32>, minor: Option<u32> },
}

impl Tag {
    fn parse_num(s: &str) -> Result<Self, String> {
        let major = s.parse::<u32>().map_err(|_| "err".to_string())?;
        Ok(Self::Num(major))
    }

    fn parse_wild(s: &str) -> Self {
        let parts: Vec<&str> = s.split('.').collect();
        let major = parts.get(0).filter(|p| *p != "*").and_then(|p| p.parse().ok());
        let minor = parts.get(1).filter(|p| *p != "*").and_then(|p| p.parse().ok());
        Self::Wild { major, minor }
    }
}

fn slice_first(v: &Vec<i32>) -> &[i32] {
    v
}

fn main() ~ Console {
    let mut xs = vec![3, 1, 3, 2, 2];
    xs.swap(0, 1);
    xs.sort();
    xs.dedup();
    xs.extend(vec![9, 10]);
    let s = slice_first(&xs);

    let c = Counter { cur: 1, end: 4 };
    let collected: Vec<i32> = c.collect();

    let mut target = 10;
    let prev = std::mem::replace(&mut target, 20);

    let sv = SemVer::parse("7.3").unwrap();
    let t1 = Tag::parse_num("77").unwrap();
    let n1 = match t1 {
        Tag::Num(n) => n,
        Tag::Wild { .. } => 0,
    };
    let t2 = Tag::parse_wild("4.*");
    let (m1, m2) = match t2 {
        Tag::Wild { major, minor } => (major.unwrap_or(0), minor.unwrap_or(99)),
        Tag::Num(_) => (0, 0),
    };

    println!("{}|{}|{}|{}|{}|{}|{}|{}|{}|{}", s.len(), s[0], collected.len(), prev, target, sv.major, sv.minor, n1, m1, m2);
}
"#,
    );
    checks_and_prints(&main, "5|1|3|10|20|7|3|77|4|99");
}

#[test]
fn planted_fail_closed_vec_of_fixed_array_refused() {
    let main = program(
        "fail-vec-tup-arr",
        r#"fn main() ~ Console {
    let mut v: Vec<([i64; 2], i64)> = Vec::new();
    v.push(([1, 2], 3));
    println!("{}", v.len());
}
"#,
    );
    let check = buildc("check", &main);
    assert!(
        !check.status.success(),
        "Vec<([i64; 2], i64)> must fail closed\n{}",
        text(&check)
    );
}

#[test]
fn planted_fail_closed_undeclared_filesystem_effect_refused() {
    let main = program(
        "fail-fs-effect",
        r#"fn main() {
    let _ = read_file("nonexistent.txt");
}
"#,
    );
    let check = buildc("check", &main);
    assert!(
        !check.status.success(),
        "undeclared FileSystem effect must fail closed\n{}",
        text(&check)
    );
}

#[test]
fn planted_fail_closed_invalid_format_spec_refused() {
    let main = program(
        "fail-fmt-spec",
        r#"fn main() ~ Console {
    println!("{:q}", 42);
}
"#,
    );
    let check = buildc("check", &main);
    assert!(
        !check.status.success(),
        "invalid format specifier must fail closed\n{}",
        text(&check)
    );
}
