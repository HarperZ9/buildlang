// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Native compiler diagnostics, toolchain probe, and live self-test (`buildc doctor [--json]`).
//!
//! Reports compiler version, host triple, supported CPU/WASM/GPU/hardware targets,
//! live external tool availability with exact versions (or `absent`), and executes
//! a fast in-process end-to-end self-test (`lex -> parse -> typecheck -> MIR ->
//! superopt -> verify-cert` plus a planted tampered-certificate rejection check).

use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use buildlang::codegen::{CodeGenerator, Target};
use buildlang::lexer::{Lexer, SourceFile};
use buildlang::parser::Parser;
use buildlang::types::{TypeChecker, TypeContext};
use serde::Serialize;

/// Canonical schema identifier for `buildc doctor --json` output.
pub(crate) const DOCTOR_SCHEMA: &str = "buildlang.doctor/v1";

/// Built-in deterministic self-test program guaranteed to trigger algebraic MIR superopt rewrites.
const DEFAULT_SELF_TEST_SOURCE: &str = r#"fn doctor_probe(x: i64) -> i64 {
    let a = (x * 1) + 0;
    let b = a * 2;
    b + 0
}

fn main() -> i64 {
    doctor_probe(21)
}
"#;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct TargetsSummary {
    pub cpu: Vec<&'static str>,
    pub wasm: Vec<&'static str>,
    pub gpu: Vec<&'static str>,
    pub hardware: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct BackendMaturityRow {
    pub flag: &'static str,
    pub rank: &'static str,
    pub output: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ToolProbe {
    pub name: &'static str,
    pub status: &'static str,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SelfTestStages {
    pub lex: bool,
    pub parse: bool,
    pub typecheck: bool,
    pub mir_superopt: bool,
    pub verify_cert: bool,
    pub planted_invalid_cert_rejected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SelfTestReport {
    pub ok: bool,
    pub stages: SelfTestStages,
    pub rewrites_applied: usize,
    pub cost_before: u64,
    pub cost_after: u64,
    pub cert_sha256: Option<String>,
    pub elapsed_ms: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct DoctorReport {
    pub schema: &'static str,
    pub ok: bool,
    pub compiler_version: &'static str,
    pub host_triple: String,
    pub os: &'static str,
    pub arch: &'static str,
    pub c_backend_ready: bool,
    pub targets: TargetsSummary,
    pub backend_maturity: Vec<BackendMaturityRow>,
    pub tools: Vec<ToolProbe>,
    pub self_test: SelfTestReport,
}

/// Return a canonical host triple without invoking external tools or leaking local paths.
pub(crate) fn host_triple() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    let vendor = if cfg!(target_os = "windows") {
        "pc"
    } else if cfg!(target_os = "macos") {
        "apple"
    } else {
        "unknown"
    };
    let env = if cfg!(target_env = "msvc") {
        Some("msvc")
    } else if cfg!(target_env = "gnu") {
        Some("gnu")
    } else if cfg!(target_env = "musl") {
        Some("musl")
    } else {
        None
    };
    match env {
        Some(e) => format!("{arch}-{vendor}-{os}-{e}"),
        None => format!("{arch}-{vendor}-{os}"),
    }
}

/// Redact any accidental user-home path segments or OS username tokens from diagnostic text.
pub(crate) fn sanitize_no_user_paths(raw: &str) -> String {
    let mut out = raw.to_string();
    let win_upper = ["C:", "\\", "Users", "\\"].concat();
    let win_lower = ["c:", "\\", "Users", "\\"].concat();
    let mac_home = ["/", "Users", "/"].concat();
    let linux_home = ["/", "home", "/"].concat();
    for marker in [
        win_upper.as_str(),
        win_lower.as_str(),
        mac_home.as_str(),
        linux_home.as_str(),
    ] {
        while let Some(idx) = out.find(marker) {
            let after = idx + marker.len();
            let rest = &out[after..];
            let sep_offset = rest
                .find(['\\', '/', ' ', '\t', '"', '\'', ')'])
                .unwrap_or(rest.len());
            let mut rebuilt = String::with_capacity(out.len());
            rebuilt.push_str(&out[..idx]);
            rebuilt.push_str("<user-dir>");
            rebuilt.push_str(&rest[sep_offset..]);
            out = rebuilt;
        }
    }
    for key in ["USERNAME", "USER"] {
        if let Ok(user) = std::env::var(key) {
            let trimmed = user.trim();
            if trimmed.len() >= 3 {
                let lower_out = out.to_ascii_lowercase();
                let lower_user = trimmed.to_ascii_lowercase();
                if lower_out.contains(&lower_user) {
                    let mut result = String::with_capacity(out.len());
                    let mut cursor = 0;
                    while let Some(pos) = lower_out[cursor..].find(&lower_user) {
                        let start = cursor + pos;
                        let end = start + trimmed.len();
                        let prev_ok =
                            start == 0 || !out.as_bytes()[start - 1].is_ascii_alphanumeric();
                        let next_ok =
                            end >= out.len() || !out.as_bytes()[end].is_ascii_alphanumeric();
                        result.push_str(&out[cursor..start]);
                        if prev_ok && next_ok {
                            result.push_str("<user>");
                        } else {
                            result.push_str(&out[start..end]);
                        }
                        cursor = end;
                    }
                    result.push_str(&out[cursor..]);
                    out = result;
                }
            }
        }
    }
    out
}

fn first_non_empty_line(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let text = if !stdout.is_empty() {
        String::from_utf8_lossy(stdout)
    } else {
        String::from_utf8_lossy(stderr)
    };
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(sanitize_no_user_paths)
}

fn probe_tool(name: &'static str, bin: &str, args: &[&str], accept_nonzero: bool) -> ToolProbe {
    match Command::new(bin).args(args).output() {
        Ok(out) if out.status.success() || accept_nonzero => {
            let version = first_non_empty_line(&out.stdout, &out.stderr)
                .unwrap_or_else(|| "detected".to_string());
            ToolProbe {
                name,
                status: "present",
                version: Some(version),
            }
        }
        _ => ToolProbe {
            name,
            status: "absent",
            version: None,
        },
    }
}

fn probe_cl_tool(c_compiler: Option<&str>) -> ToolProbe {
    let direct = probe_tool("cl", "cl", &[], true);
    if direct.status == "present" {
        return direct;
    }
    if let Some(cc) = c_compiler {
        let lower = cc.to_ascii_lowercase();
        if lower.ends_with("cl.exe") || lower.ends_with("\\cl") || lower == "cl" {
            return probe_tool("cl", cc, &[], true);
        }
    }
    ToolProbe {
        name: "cl",
        status: "absent",
        version: None,
    }
}

fn probe_all_tools(c_compiler: Option<&str>) -> Vec<ToolProbe> {
    vec![
        probe_tool("cc", "cc", &["--version"], false),
        probe_tool("clang", "clang", &["--version"], false),
        probe_cl_tool(c_compiler),
        probe_tool("gcc", "gcc", &["--version"], false),
        probe_tool("dxc", "dxc", &["--version"], false),
        probe_tool(
            "glslangValidator",
            "glslangValidator",
            &["--version"],
            false,
        ),
        probe_tool("spirv-val", "spirv-val", &["--version"], false),
        probe_tool("naga", "naga", &["--version"], false),
    ]
}

fn tamper_hex_digest(digest: &str) -> String {
    let mut chars: Vec<char> = digest.chars().collect();
    let idx = if digest.starts_with("sha256:") && chars.len() > 7 {
        7
    } else {
        0
    };
    if let Some(ch) = chars.get_mut(idx) {
        *ch = if *ch == '0' { '1' } else { '0' };
    } else {
        chars.push('0');
    }
    chars.into_iter().collect()
}

fn round_us(ms: f64) -> f64 {
    (ms * 1000.0).round() / 1000.0
}

/// Run the fast in-process end-to-end self-test:
/// `lex -> parse -> typecheck -> MIR -> superopt -> verify-cert` + planted invalid-cert rejection.
pub(crate) fn run_self_test_with_source(source: &str) -> SelfTestReport {
    let t0 = Instant::now();
    let mut stages = SelfTestStages {
        lex: false,
        parse: false,
        typecheck: false,
        mir_superopt: false,
        verify_cert: false,
        planted_invalid_cert_rejected: false,
    };

    let fail = |stages: SelfTestStages, msg: String| SelfTestReport {
        ok: false,
        stages,
        rewrites_applied: 0,
        cost_before: 0,
        cost_after: 0,
        cert_sha256: None,
        elapsed_ms: round_us(t0.elapsed().as_secs_f64() * 1000.0),
        error: Some(sanitize_no_user_paths(&msg)),
    };

    let source_file = SourceFile::new("<doctor-self-test>", source.to_string());

    // 1. Lex
    let mut lexer = Lexer::new(&source_file);
    let tokens = match lexer.tokenize() {
        Ok(toks) => {
            stages.lex = true;
            toks
        }
        Err(e) => return fail(stages, format!("lex failed: {e}")),
    };

    // 2. Parse
    let mut parser = Parser::new(&source_file, tokens);
    let ast = match parser.parse() {
        Ok(module) if parser.errors().is_empty() => {
            stages.parse = true;
            module
        }
        Ok(_) => {
            let first = parser
                .errors()
                .first()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "parse error".to_string());
            return fail(stages, format!("parse failed: {first}"));
        }
        Err(e) => return fail(stages, format!("parse failed: {e}")),
    };

    // 3. Typecheck
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.check_module(&ast);
    if checker.has_errors() {
        let first = checker
            .errors()
            .first()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "type error".to_string());
        return fail(stages, format!("typecheck failed: {first}"));
    }
    stages.typecheck = true;

    // 4. MIR lowering + certified e-graph superoptimizer
    let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source_file.source()));
    codegen.superopt = true;
    if let Err(e) = codegen.generate(&ast) {
        return fail(stages, format!("mir/superopt codegen failed: {e}"));
    }
    if !codegen.linear_errors().is_empty() {
        return fail(stages, "linear type errors in self-test".to_string());
    }
    let Some(cert) = codegen.superopt_cert().cloned() else {
        return fail(
            stages,
            "superoptimizer did not emit a certificate".to_string(),
        );
    };
    stages.mir_superopt = true;

    // 5. Verify genuine certificate
    if let Err(e) = buildlang::codegen::superopt::verify_certificate(&cert) {
        return fail(stages, format!("verify-cert failed on genuine cert: {e}"));
    }
    stages.verify_cert = true;

    // 6. Planted fail-closed control: tampered cert_sha256 must be rejected
    let mut tampered = cert.clone();
    tampered.cert_sha256 = tamper_hex_digest(&cert.cert_sha256);
    if buildlang::codegen::superopt::verify_certificate(&tampered).is_ok() {
        return fail(
            stages,
            "planted tampered superopt certificate was unexpectedly accepted".to_string(),
        );
    }
    stages.planted_invalid_cert_rejected = true;

    SelfTestReport {
        ok: true,
        stages,
        rewrites_applied: cert.rewrites_applied,
        cost_before: cert.total_cost_before as u64,
        cost_after: cert.total_cost_after as u64,
        cert_sha256: Some(cert.cert_sha256),
        elapsed_ms: round_us(t0.elapsed().as_secs_f64() * 1000.0),
        error: None,
    }
}

/// Entry point for `buildc doctor [--json]`.
pub(crate) fn cmd_doctor(json: bool) -> Result<(), i32> {
    let c_compiler = crate::find_c_compiler();
    let tools = probe_all_tools(c_compiler.as_deref());

    let self_test_src = std::env::var("BUILDC_DOCTOR_SELF_TEST_SRC").ok();
    let self_test = match self_test_src.as_deref() {
        Some(custom) => run_self_test_with_source(custom),
        None => run_self_test_with_source(DEFAULT_SELF_TEST_SOURCE),
    };

    let targets = TargetsSummary {
        cpu: vec![
            "c",
            "rust",
            "llvm",
            "x86-64",
            "x86_64-pe",
            "x86_64-elf",
            "arm64",
            "riscv64",
        ],
        wasm: vec!["wasm"],
        gpu: vec!["hlsl", "glsl", "spirv", "wgsl", "msl"],
        hardware: vec!["verilog"],
    };

    let backend_maturity: Vec<BackendMaturityRow> = crate::BACKEND_MATURITY
        .iter()
        .map(|(flag, rank, output)| BackendMaturityRow { flag, rank, output })
        .collect();

    let report = DoctorReport {
        schema: DOCTOR_SCHEMA,
        ok: self_test.ok,
        compiler_version: buildlang::VERSION,
        host_triple: host_triple(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        c_backend_ready: c_compiler.is_some(),
        targets,
        backend_maturity,
        tools,
        self_test,
    };

    if json {
        let rendered = serde_json::to_string_pretty(&report).map_err(|e| {
            eprintln!("Error serializing doctor JSON: {}", e);
            1
        })?;
        println!("{}", rendered);
        return if report.ok { Ok(()) } else { Err(1) };
    }

    println!("BuildLang Doctor");
    println!("=================");
    println!();
    println!("buildc: {} ({})", report.compiler_version, report.os);
    println!("host: {}", report.host_triple);

    match &c_compiler {
        Some(compiler) => println!("C backend: ready via {}", sanitize_no_user_paths(compiler)),
        None => println!("C backend: missing C compiler; install MSVC, gcc, clang, or cc"),
    }

    match crate::find_stdlib_path() {
        Some(path) => println!(
            "stdlib: {}",
            sanitize_no_user_paths(&path.display().to_string())
        ),
        None => {
            println!("stdlib: not found; set BUILDLANG_STDLIB or install stdlib/ beside buildc")
        }
    }

    let registry = crate::load_local_registry_index();
    if registry.is_empty() {
        println!("registry: no local packages found");
    } else {
        println!("registry: {} local package(s)", registry.len());
    }

    println!();
    println!("Optional tools:");
    for tool in &report.tools {
        match &tool.version {
            Some(ver) => println!("  {:<18} present  {}", tool.name, ver),
            None => println!("  {:<18} absent", tool.name),
        }
    }

    println!();
    println!("Backend maturity:");
    for row in &report.backend_maturity {
        println!("  {:<9}{:<14}{}", row.flag, row.rank, row.output);
    }

    println!();
    crate::print_gpu_probe();

    let corpus_root = crate::find_semantic_corpus_root();
    crate::print_substrate_evidence(corpus_root.as_deref());

    println!();
    println!("Self-test:");
    if report.self_test.ok {
        println!(
            "  pipeline   ok       lex->parse->typecheck->MIR->superopt->verify-cert (rewrites={}, cost={}->{}, planted_invalid_cert_rejected={}, {:.3} ms)",
            report.self_test.rewrites_applied,
            report.self_test.cost_before,
            report.self_test.cost_after,
            report.self_test.stages.planted_invalid_cert_rejected,
            report.self_test.elapsed_ms
        );
    } else {
        println!(
            "  pipeline   FAILED   {}",
            report
                .self_test
                .error
                .as_deref()
                .unwrap_or("self-test failed")
        );
    }

    println!();
    if report.c_backend_ready {
        println!("Ready for practical C-backend examples: yes");
    } else {
        println!("Ready for practical C-backend examples: no");
    }

    if report.ok {
        Ok(())
    } else {
        Err(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test_default_program_passes_and_rejects_tampered_cert() {
        let res = run_self_test_with_source(DEFAULT_SELF_TEST_SOURCE);
        assert!(res.ok, "default self-test failed: {:?}", res.error);
        assert!(res.stages.lex);
        assert!(res.stages.parse);
        assert!(res.stages.typecheck);
        assert!(res.stages.mir_superopt);
        assert!(res.stages.verify_cert);
        assert!(res.stages.planted_invalid_cert_rejected);
        assert!(res.rewrites_applied >= 1);
        assert!(res.cost_after <= res.cost_before);
        assert!(res.cert_sha256.as_deref().is_some_and(|s| s.len() >= 64));
    }

    #[test]
    fn self_test_planted_type_error_fails_closed() {
        let res = run_self_test_with_source("fn bad() -> i64 { \"nope\" }");
        assert!(!res.ok);
        assert!(res.stages.lex);
        assert!(res.stages.parse);
        assert!(!res.stages.typecheck);
        assert!(res.error.is_some());
    }

    #[test]
    fn sanitize_redacts_windows_and_posix_user_home_paths() {
        let win_sample = ["C:", "\\", "Users", "\\", "Alice\\bin\\clang.exe"].concat();
        let linux_sample = ["/", "home", "/", "bob/.cargo/bin/naga"].concat();
        let mac_sample = ["/", "Users", "/", "carol/sdk/dxc"].concat();
        assert_eq!(
            sanitize_no_user_paths(&win_sample),
            "<user-dir>\\bin\\clang.exe"
        );
        assert_eq!(
            sanitize_no_user_paths(&linux_sample),
            "<user-dir>/.cargo/bin/naga"
        );
        assert_eq!(sanitize_no_user_paths(&mac_sample), "<user-dir>/sdk/dxc");
    }
}
