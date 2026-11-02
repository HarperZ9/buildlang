// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

use super::export::to_qdb;
use super::*;

fn example(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// A program with one networked function, one file reader, a pure helper and
/// a caller that reaches the network through `fetch`.
const NET: &str = r#"
fn fetch(host: str) -> i64 ~ Network {
    let sock = tcp_connect(host, 80);
    tcp_close(sock);
    sock
}

fn load(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn double(x: i64) -> i64 {
    x * 2
}

fn main() ~ Console + Network + FileSystem {
    let s = fetch("example.com");
    let text = load("notes.txt");
    println!("{} {}", double(s), text);
}
"#;

fn q(store: &Store, sql: &str) -> QueryResult {
    run_query(store, sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

#[test]
fn functions_with_the_network_effect() {
    let store = Store::load_source("net.bld", NET).expect("net.bld loads");
    let r = q(
        &store,
        "SELECT DISTINCT function FROM effects WHERE effect = 'Network' ORDER BY function",
    );
    assert_eq!(r.column("function"), vec!["fetch", "main"]);
    // The pure helper and the file reader carry no Network fact.
    let none = q(&store, "SELECT * FROM effects WHERE function = 'double'");
    assert!(none.rows.is_empty(), "double is pure: {:?}", none.rows);
    let fs = q(
        &store,
        "SELECT DISTINCT function FROM effects WHERE effect = 'FileSystem' AND how = 'declared' ORDER BY function",
    );
    assert_eq!(fs.column("function"), vec!["load", "main"]);
}

#[test]
fn network_fact_names_the_call_that_made_it() {
    let store = Store::load_source("net.bld", NET).unwrap();
    let r = q(
        &store,
        "SELECT via FROM effects WHERE function = 'fetch' AND how = 'observed' AND effect = 'Network' ORDER BY via",
    );
    assert!(
        r.column("via").iter().any(|v| v.contains("tcp_connect")),
        "observed sources: {:?}",
        r.column("via")
    );
}

#[test]
fn call_graph_comes_from_mir() {
    let store = Store::load_source("net.bld", NET).unwrap();
    let r = q(
        &store,
        "SELECT callee FROM calls WHERE caller = 'main' ORDER BY callee",
    );
    let callees = r.column("callee");
    for want in ["double", "fetch", "load"] {
        assert!(
            callees.iter().any(|c| c == want),
            "main calls {want}: {callees:?}"
        );
    }
    let defined = q(
        &store,
        "SELECT name FROM functions WHERE kind = 'defined' AND span_start IS NOT NULL ORDER BY name",
    );
    assert_eq!(
        defined.column("name"),
        vec!["double", "fetch", "load", "main"]
    );
}

#[test]
fn quickstart_examples_load() {
    for rel in [
        "examples/quickstart/ledger.bld",
        "examples/quickstart/effects_greeting.bld",
        "examples/quickstart/hello.bld",
        "examples/demo/temperature.bld",
    ] {
        let store = Store::load_source(rel, &example(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let main = q(
            &store,
            "SELECT effect FROM effects WHERE function = 'main' AND how = 'declared'",
        );
        assert_eq!(main.column("effect"), vec!["Console"], "{rel}");
    }
    let greet = Store::load_source(
        "effects_greeting.bld",
        &example("examples/quickstart/effects_greeting.bld"),
    )
    .unwrap();
    let r = q(
        &greet,
        "SELECT effect FROM effects WHERE function = 'welcome' AND how = 'declared'",
    );
    assert_eq!(r.column("effect"), vec!["Greeting"]);
    let items = q(&greet, "SELECT name FROM items WHERE kind = 'effect'");
    assert_eq!(items.column("name"), vec!["Greeting"]);
}

#[test]
fn store_is_deterministic() {
    let a = Store::load_source("net.bld", NET).unwrap();
    let b = Store::load_source("net.bld", NET).unwrap();
    assert_eq!(a.digest(), b.digest());
    assert_eq!(a.schema, BDB_SCHEMA);
}

#[test]
fn qdb_export_has_every_table_and_row() {
    let store = Store::load_source("net.bld", NET).unwrap();
    let text = to_qdb(&store).unwrap();
    for t in export::TABLES {
        assert!(text.contains(&format!("TABLE {t}\n")), "{t}");
    }
    assert!(text.contains("SCHEMA function:TEXT effect:TEXT how:TEXT via:TEXT\n"));
    let rows = text.lines().filter(|l| l.starts_with("ROW ")).count();
    let expected =
        store.functions.len() + store.effects.len() + store.calls.len() + store.items.len();
    assert_eq!(rows, expected);
    assert!(
        text.contains("ROW main|Console|declared|\n"),
        "NULL via is an empty cell"
    );
}

#[test]
fn query_subset_refuses_what_it_cannot_run() {
    let store = Store::load_source("net.bld", NET).unwrap();
    for sql in [
        "SELECT effect, COUNT(*) FROM effects GROUP BY effect",
        "SELECT * FROM effects JOIN calls ON effects.function = calls.caller",
        "SELECT * FROM effects WHERE function LIKE 'f%'",
        "SELECT nosuch FROM effects",
        "SELECT * FROM nosuch",
        "SELECT * FROM effects WHERE function = 'main' garbage",
    ] {
        assert!(run_query(&store, sql).is_err(), "should refuse: {sql}");
    }
}

#[test]
fn rename_updates_definition_and_every_call() {
    let src = example("examples/quickstart/ledger.bld");
    let (out, receipt) = rename_function("ledger.bld", &src, "add_month", "accrue").unwrap();
    assert!(!out.contains("add_month"));
    assert_eq!(out.matches("accrue").count(), 2, "definition and one call");
    assert_eq!(receipt.edits.len(), 2);
    assert_eq!(receipt.check, "mir-equal-under-rename");
    assert_ne!(receipt.mir_sha256_before, receipt.mir_sha256_after);
    let store = Store::load_source("ledger.bld", &out).unwrap();
    let r = q(
        &store,
        "SELECT callee FROM calls WHERE caller = 'main' AND callee = 'accrue'",
    );
    assert_eq!(r.rows.len(), 1);
    assert!(store.function("add_month").is_none());
}

#[test]
fn rename_keeps_effect_facts() {
    let (out, _) = rename_function("net.bld", NET, "fetch", "download").unwrap();
    let store = Store::load_source("net.bld", &out).unwrap();
    let r = q(
        &store,
        "SELECT DISTINCT function FROM effects WHERE effect = 'Network' ORDER BY function",
    );
    assert_eq!(r.column("function"), vec!["download", "main"]);
}

#[test]
fn rename_refuses_unsafe_cases() {
    // A local shares the function's name: MIR would change beyond the name.
    let shadow = "fn step(x: i32) -> i32 { x + 1 }\n\
                  fn main() ~ Console { let step: i32 = 2; println!(\"{}\", step); }\n";
    let err = rename_function("s.bld", shadow, "step", "advance").unwrap_err();
    assert!(matches!(err, BqlError::Refused(_)), "{err}");

    let src = example("examples/quickstart/ledger.bld");
    for (old, new, why) in [
        ("add_month", "balance", "capture of an existing name"),
        ("add_month", "fn", "keyword"),
        ("add_month", "two words", "not an identifier"),
        ("main", "start", "entry point"),
        ("nosuch", "x", "unknown function"),
    ] {
        let r = rename_function("ledger.bld", &src, old, new);
        assert!(matches!(r, Err(BqlError::Refused(_))), "{why}: {r:?}");
    }
}

#[test]
fn multi_file_programs_are_refused() {
    let src = "mod other;\nfn main() ~ Console { println!(\"x\"); }\n";
    let err = Store::load_source("m.bld", src).unwrap_err();
    assert!(matches!(err, BqlError::Unsupported(_)), "{err}");
}

/// Differential check against qdb itself. Set `QDB_BIN` to a qdb binary built
/// from `programs/db.bld` to run it; without it the test returns early.
#[test]
fn in_process_subset_matches_qdb() {
    let Ok(bin) = std::env::var("QDB_BIN") else {
        eprintln!("QDB_BIN not set; skipping the qdb differential check");
        return;
    };
    let store = Store::load_source("net.bld", NET).unwrap();
    let dir = std::env::temp_dir().join(format!("bql-qdb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("net.qdb");
    std::fs::write(&db, to_qdb(&store).unwrap()).unwrap();
    for sql in [
        "SELECT DISTINCT function FROM effects WHERE effect = 'Network' ORDER BY function",
        "SELECT caller, callee FROM calls WHERE caller = 'main' ORDER BY callee",
        "SELECT name, kind FROM functions WHERE kind = 'defined' ORDER BY name",
        "SELECT function, effect FROM effects WHERE how = 'declared' ORDER BY function, effect",
    ] {
        let ours = q(&store, sql);
        assert!(
            !ours.rows.is_empty(),
            "the check needs rows to compare: {sql}"
        );
        let out = std::process::Command::new(&bin)
            .arg(&db)
            .arg(sql)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).replace('\r', "");
        let theirs: Vec<Vec<String>> = text
            .lines()
            .skip(2)
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split('|').map(|c| c.trim().to_string()).collect())
            .collect();
        let ours_rows: Vec<Vec<String>> = ours
            .rows
            .iter()
            .map(|r| r.iter().map(|c| c.clone().unwrap_or_default()).collect())
            .collect();
        assert_eq!(ours_rows, theirs, "{sql}\nqdb printed:\n{text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
