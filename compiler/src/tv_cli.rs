// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! `buildc tv FILE` and `buildc tv-check SOURCE C CERT`.
//!
//! `tv` compiles FILE through the C backend (the same `buildc FILE -o x.c`
//! path users run), writes a certificate for every function, then hands the
//! three texts, read back from disk, to the independent checker.
//! `tv-check` runs the checker alone on saved files.

use buildlang::lexer::{Lexer, SourceFile};
use buildlang::parser::Parser;
use buildlang::tv::{cert, check};
use std::path::{Path, PathBuf};
use std::process::Command;

fn read(p: &Path) -> Result<String, i32> {
    std::fs::read_to_string(p).map_err(|e| {
        eprintln!("Error reading '{}': {e}", p.display());
        2
    })
}

fn print_report(file: &str, results: &[check::FnResult], json: bool) {
    let doc = check::report(file, results);
    if json {
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
        return;
    }
    for r in results {
        match &r.status {
            check::Status::Certified { segments } => {
                println!("{}: certified ({segments} segment(s))", r.name)
            }
            check::Status::NotCertified { reason } => {
                println!("{}: NOT CERTIFIED: {reason}", r.name)
            }
            check::Status::OutsideSubset { reason } => {
                println!("{}: outside the subset: {reason}", r.name)
            }
        }
    }
    let c = &doc["coverage"];
    println!(
        "{} of {} compiled functions certified ({} not certified, {} outside the subset)",
        c["certified"], c["compiled"], c["not_certified"], c["outside_subset"]
    );
}

/// Exit 1 when any function in the subset is not certified.
fn exit_code(results: &[check::FnResult]) -> Result<(), i32> {
    if results
        .iter()
        .any(|r| matches!(r.status, check::Status::NotCertified { .. }))
    {
        return Err(1);
    }
    Ok(())
}

pub fn cmd_tv(file: &Path, out_dir: Option<&Path>, json: bool) -> Result<(), i32> {
    let dir: PathBuf = match out_dir {
        Some(d) => d.to_path_buf(),
        None => std::env::temp_dir().join(format!("buildc-tv-{}", std::process::id())),
    };
    std::fs::create_dir_all(&dir).map_err(|e| {
        eprintln!("Error creating '{}': {e}", dir.display());
        2
    })?;
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let c_path = dir.join(format!("{stem}.c"));
    let cert_path = dir.join(format!("{stem}.tv-cert.json"));
    let exe = std::env::current_exe().map_err(|_| 2)?;
    let status = Command::new(exe)
        .arg(file)
        .arg("-o")
        .arg(&c_path)
        .args(["--target", "c"])
        .output()
        .map_err(|_| 2)?;
    if !status.status.success() {
        eprint!("{}", String::from_utf8_lossy(&status.stderr));
        eprintln!("'{}' does not compile to C", file.display());
        return Err(2);
    }
    let source = read(file)?;
    let c = read(&c_path)?;
    let sf = SourceFile::new(file.to_string_lossy(), source.clone());
    let tokens = Lexer::new(&sf).tokenize().map_err(|_| 2)?;
    let module = Parser::new(&sf, tokens).parse().map_err(|_| 2)?;
    let certificate = cert::generate(&file.to_string_lossy(), &sf, &module, &c);
    let text = serde_json::to_string_pretty(&certificate).unwrap_or_default();
    std::fs::write(&cert_path, format!("{text}\n")).map_err(|_| 2)?;
    if out_dir.is_none() {
        // Read back before the scratch directory goes.
        let results = run_checker(file, &c_path, &cert_path)?;
        let _ = std::fs::remove_dir_all(&dir);
        print_report(&file.to_string_lossy(), &results, json);
        return exit_code(&results);
    }
    let results = run_checker(file, &c_path, &cert_path)?;
    print_report(&file.to_string_lossy(), &results, json);
    exit_code(&results)
}

fn run_checker(source: &Path, c: &Path, cert: &Path) -> Result<Vec<check::FnResult>, i32> {
    let s = read(source)?;
    let c = read(c)?;
    let cert: serde_json::Value = serde_json::from_str(&read(cert)?).map_err(|e| {
        eprintln!("certificate is not JSON: {e}");
        2
    })?;
    check::check(&s, &c, &cert).map_err(|e| {
        eprintln!("{e}");
        2
    })
}

pub fn cmd_tv_check(source: &Path, c: &Path, cert: &Path, json: bool) -> Result<(), i32> {
    let results = run_checker(source, c, cert)?;
    print_report(&source.to_string_lossy(), &results, json);
    exit_code(&results)
}
