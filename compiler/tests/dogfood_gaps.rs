//! End-to-end regression tests for dogfooded compiler/runtime gaps GAP-BL-11..14.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!(
            "buildlang_dogfood_gaps_{tag}_{}_{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("create tempdir");
        Self(p)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn buildc() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", repo_root().join("stdlib"));
    cmd
}

fn check_and_run(name: &str, src: &str, extra_args: &[&str]) -> String {
    let dir = TempDir::new(name);
    let file = dir.path().join(format!("{name}.bld"));
    std::fs::write(&file, src).expect("write source");

    let check = buildc()
        .arg("check")
        .arg(&file)
        .output()
        .expect("buildc check runs");
    assert!(
        check.status.success(),
        "{name}: buildc check failed:\n{}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let mut run_cmd = buildc();
    run_cmd.arg("run").arg(&file);
    if !extra_args.is_empty() {
        run_cmd.arg("--").args(extra_args);
    }
    let run = run_cmd.output().expect("buildc run runs");
    assert!(
        run.status.success(),
        "{name}: buildc run failed:\n{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n")
}

#[test]
fn gap_bl_11_argc_argv_locals_in_main_do_not_collide() {
    let src = r#"
fn main() ~ Console + Environment {
    let argc: i64 = args_count();
    let argv: i32 = 42;
    println!("argc={} argv={}", argc, argv);
}
"#;
    let out = check_and_run("gap_bl_11", src, &["one", "two"]);
    assert_eq!(out.trim(), "argc=3 argv=42");
}

#[test]
fn gap_bl_12_inline_vec_new_inside_enum_constructor() {
    let src = r#"
enum Json {
    Num(i32),
    Obj(Vec<String>, Vec<i32>),
}

fn empty_obj() -> Json {
    Json::Obj(Vec::new(), Vec::new())
}

fn main() ~ Console {
    let j: Json = empty_obj();
    match j {
        Json::Obj(ks, vs) => {
            println!("{} {}", ks.len(), vs.len());
        }
        Json::Num(n) => {
            println!("num {}", n);
        }
    }
}
"#;
    let out = check_and_run("gap_bl_12", src, &[]);
    assert_eq!(out.trim(), "0 0");
}

#[test]
fn gap_bl_13_unit_match_early_return_inside_else_if() {
    let src = r#"
fn handle(flag: bool, code: i32) -> i32 {
    if flag {
        return 10;
    } else if code > 0 {
        match code {
            1 => { return 100; }
            2 => { return 200; }
            _ => {}
        }
    }
    0
}

fn main() ~ Console {
    println!("{} {} {} {}", handle(true, 1), handle(false, 1), handle(false, 3), handle(false, 0));
}
"#;
    let out = check_and_run("gap_bl_13", src, &[]);
    assert_eq!(out.trim(), "10 100 0 0");
}

#[test]
fn gap_bl_14_file_exists_handles_files_dirs_and_missing_paths() {
    let dir = TempDir::new("gap_bl_14_fs");
    let existing_file = dir.path().join("present.txt");
    std::fs::write(&existing_file, "ok").expect("write file");
    let existing_dir = dir.path().join("subdir");
    std::fs::create_dir(&existing_dir).expect("mkdir");
    let missing = dir.path().join("does_not_exist");

    let norm = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let src = format!(
        r#"
fn main() ~ Console + FileSystem {{
    println!("file={{}} dir={{}} missing={{}}", file_exists("{f}"), file_exists("{d}"), file_exists("{m}"));
}}
"#,
        f = norm(&existing_file),
        d = norm(&existing_dir),
        m = norm(&missing),
    );
    let out = check_and_run("gap_bl_14", &src, &[]);
    assert_eq!(out.trim(), "file=true dir=true missing=false");
}

#[test]
fn matches_macro_variants_and_guards() {
    let src = r#"
enum Mode {
    Fast,
    Balanced,
    Quality,
}

fn main() ~ Console {
    let m: Mode = Mode::Balanced;
    assert!(matches!(m, Mode::Balanced));
    let is_fast: bool = matches!(m, Mode::Fast);
    let is_ok: bool = matches!(m, Mode::Balanced | Mode::Quality);
    let n: i32 = 12;
    let big: bool = matches!(n, v if v > 10);
    let small: bool = matches!(n, v if v < 5);
    println!("{} {} {} {}", is_fast, is_ok, big, small);
}
"#;
    let out = check_and_run("matches_macro", src, &[]);
    assert_eq!(out.trim(), "false true true false");
}

#[test]
fn integer_min_max_clamp_abs_and_slice_to_vec() {
    let src = r#"
fn sum_slice(xs: &[i32]) -> i32 {
    let mut s: i32 = 0;
    for x in xs {
        s = s + x;
    }
    s
}

fn copy_from_slice(xs: &[i32]) -> Vec<i32> {
    xs.to_vec()
}

fn main() ~ Console {
    let a: i32 = -7;
    let b: i32 = a.abs();
    let c: i32 = 15.min(9).max(3);
    let d: i32 = 25.clamp(0, 10);
    let e: i32 = (-5).clamp(0, 10);
    let base: [i32; 3] = [10, 20, 30];
    let copied: Vec<i32> = copy_from_slice(&base);
    let total: i32 = sum_slice(&copied);
    println!("{} {} {} {} {} {}", b, c, d, e, copied.len(), total);
}
"#;
    let out = check_and_run("int_methods_and_to_vec", src, &[]);
    assert_eq!(out.trim(), "7 9 10 0 3 60");
}

#[test]
fn iter_map_method_and_field_sum() {
    let src = r#"
struct Sample {
    weight: f64,
}

impl Sample {
    fn doubled(&self) -> f64 {
        self.weight * 2.0
    }
}

fn main() ~ Console {
    let mut xs: Vec<Sample> = Vec::new();
    xs.push(Sample { weight: 1.5 });
    xs.push(Sample { weight: 2.5 });
    let s1: f64 = xs.iter().map(|s| s.weight).sum();
    let s2: f64 = xs.iter().map(|s| s.doubled()).sum();
    println!("{} {}", s1 as i32, s2 as i32);
}
"#;
    let out = check_and_run("iter_map_sum", src, &[]);
    assert_eq!(out.trim(), "4 8");
}

#[test]
fn mem_size_of_align_of_and_tuple_struct_ref() {
    let src = r#"
const MAGIC: [u8; 2] = [b'B', b'L'];
const LIMIT: f64 = f64::INFINITY;

struct Flags(u32);

fn read_flags(f: &Flags) -> u32 {
    f.0
}

fn main() ~ Console {
    let sz: usize = std::mem::size_of::<i64>();
    let al: usize = core::mem::align_of::<i32>();
    let f: Flags = Flags(77);
    let inf_ok: bool = LIMIT > 1000.0;
    println!("{} {} {} {} {}", sz, al, read_flags(&f), MAGIC[0], inf_ok);
}
"#;
    let out = check_and_run("mem_and_consts", src, &[]);
    assert_eq!(out.trim(), "8 4 77 66 true");
}

#[test]
fn option_as_ref_vec_slice_and_dyn_fn_typecheck() {
    let src = r#"
struct Item {
    id: i32,
}

struct Holder {
    item: Option<Item>,
}

impl Holder {
    fn get_ref(&self) -> Option<&Item> {
        self.item.as_ref()
    }

    fn get_mut(&mut self) -> Option<&mut Item> {
        self.item.as_mut()
    }
}

fn take_slice(xs: &[f64]) -> usize {
    xs.len()
}

fn pass_vec(v: &Vec<f64>) -> usize {
    take_slice(v)
}

fn run_callback(cb: &Box<dyn Fn(i32) -> bool>, x: i32) -> bool {
    cb(x)
}

fn main() {
    let mut h: Holder = Holder { item: Some(Item { id: 1 }) };
    let _r: Option<&Item> = h.get_ref();
    let _m: Option<&mut Item> = h.get_mut();
    let v: Vec<f64> = vec![1.0, 2.0];
    let _n: usize = pass_vec(&v);
}
"#;
    let tokens = buildlang::lexer::tokenize(src).expect("tokenize");
    let source_file = buildlang::lexer::SourceFile::anonymous(src);
    let mut parser = buildlang::parser::Parser::new(&source_file, tokens);
    let module = parser.parse().expect("parse module");
    assert!(
        parser.errors().is_empty(),
        "unexpected parse errors: {:?}",
        parser.errors()
    );
    let mut ctx = buildlang::types::TypeContext::new();
    let mut checker = buildlang::types::TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.check_module(&module);
    assert!(
        !checker.has_errors(),
        "unexpected type errors: {:?}",
        checker.errors()
    );
}
