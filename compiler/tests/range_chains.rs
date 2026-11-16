//! A range as the source of an iterator chain.
//!
//! `(0..n).map(|_| make()).collect()` passed the check and was refused by
//! the C backend ("`collect` cannot be compiled: the C backend has no
//! function or method by that name (also: map)"): the chain lowering knew
//! only `v.iter()` as a source. `universe/prism/shadows.bld` stopped there
//! and nowhere else. A range with both bounds now starts a chain: element
//! `i` is `start + i` in the bounds' integer type, the count is worked out
//! in `i64` (so `5..2` is empty and an unsigned range does not wrap), and an
//! unannotated `map` closure over a range takes its result type from the
//! vector the chain is bound to (`let v: Vec<Item> = ...`).
//!
//! Expected outputs, worked out by hand: squares of 0..5 are 0 1 4 9 16;
//! 10 k for k in 1..=3 is 10 20 30; `make()` twice is 7 7; the sum of 1..=10
//! is 55; `5..2` counts 0; 1 + i for i in 0..3 (u32) is 1 2 3; 0..4 reversed
//! is 3 2 1 0.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
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

fn runs(name: &str, source: &str) -> String {
    let dir = std::env::temp_dir().join(format!("bl-rangechain-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("program folder");
    let main = dir.join("main.bld");
    std::fs::write(&main, source).expect("source file");
    let check = buildc("check", &main);
    assert!(check.status.success(), "{name}: check\n{}", text(&check));
    let run = buildc("run", &main);
    assert!(run.status.success(), "{name}: run\n{}", text(&run));
    String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n")
}

#[test]
fn a_range_maps_into_a_vector_of_numbers() {
    let out = runs(
        "squares",
        "fn main() ~ Console {\n    let sq: Vec<i32> = (0..5).map(|i| i * i).collect();\n    \
         let mut i: usize = 0;\n    while i < sq.len() {\n        println!(\"{}\", sq[i]);\n        \
         i = i + 1;\n    }\n}\n",
    );
    assert_eq!(out, "0\n1\n4\n9\n16\n");
}

const ITEM: &str = "#[derive(Clone, Copy)]\nstruct Item {\n    v: i32,\n}\n\n\
    fn make() -> Item {\n    Item { v: 7 }\n}\n\n";

#[test]
fn a_range_maps_into_a_vector_of_structs() {
    let out = runs(
        "structs",
        &format!(
            "{ITEM}fn main() ~ Console {{\n    \
             let items: Vec<Item> = (1..=3).map(|k| Item {{ v: k * 10 }}).collect();\n    \
             let made: Vec<Item> = (0..2).map(|_| make()).collect();\n    \
             println!(\"{{}} {{}} {{}} {{}}\", items.len(), items[0].v, items[1].v, items[2].v);\n    \
             println!(\"{{}} {{}} {{}}\", made.len(), made[0].v, made[1].v);\n}}\n"
        ),
    );
    assert_eq!(out, "3 10 20 30\n2 7 7\n");
}

#[test]
fn sums_counts_unsigned_bounds_and_reversal() {
    let out = runs(
        "terminals",
        "fn main() ~ Console {\n    let s: i32 = (1..=10).sum();\n    let c = (5..2).count();\n    \
         let n: u32 = 3;\n    let u: Vec<u32> = (0..n).map(|i| i + 1).collect();\n    \
         let r: Vec<i32> = (0..4).rev().map(|i| i).collect();\n    \
         println!(\"{} {}\", s, c);\n    println!(\"{} {} {} {}\", u.len(), u[0], u[1], u[2]);\n    \
         println!(\"{} {} {} {}\", r[0], r[1], r[2], r[3]);\n}\n",
    );
    assert_eq!(out, "55 0\n3 1 2 3\n3 2 1 0\n");
}

/// The control: a chain over a vector is lowered as before.
#[test]
fn a_chain_over_a_vector_is_unchanged() {
    let out = runs(
        "vector",
        "fn main() ~ Console {\n    let v: Vec<i32> = vec![1, 2, 3];\n    \
         let d: Vec<i32> = v.iter().map(|x| x * 2).collect();\n    \
         let t: i32 = v.iter().sum();\n    println!(\"{} {} {} {}\", d[0], d[1], d[2], t);\n}\n",
    );
    assert_eq!(out, "2 4 6 6\n");
}
