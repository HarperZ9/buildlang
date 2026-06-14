// ===============================================================================
// QUANTALANG COMPILER - MAIN ENTRY POINT
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! QuantaLang Compiler (`quantac`)
//!
//! This is the main entry point for the QuantaLang compiler command-line tool.

use clap::{Parser as ClapParser, Subcommand};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use quantalang::ast::{self, ItemKind, Module, Visibility};
use quantalang::codegen::{CodeGenerator, Target};
use quantalang::lexer::{Lexer, SourceFile, Span};
use quantalang::parser::Parser;
use quantalang::types::{
    FunctionEffectSummary, TypeChecker, TypeContext, TypeError, TypeErrorWithSpan,
};

fn parse_codegen_target(target: &str) -> Result<Target, String> {
    match target {
        "c" => Ok(Target::C),
        "llvm" | "llvm-ir" | "ll" => Ok(Target::LlvmIr),
        "x86-64" | "x86_64" | "x64" => Ok(Target::X86_64),
        "arm64" | "aarch64" => Ok(Target::Arm64),
        "wasm" | "wasm32" | "wat" => Ok(Target::Wasm),
        "spirv" | "spir-v" | "spv" => Ok(Target::SpirV),
        "hlsl" | "dx" | "directx" => Ok(Target::Hlsl),
        "glsl" | "opengl" | "gl" => Ok(Target::Glsl),
        "rust" | "rs" => Ok(Target::Rust),
        other => Err(format!(
            "Unknown target '{}'. Supported: c, llvm, wasm, spirv, hlsl, glsl, rust, x86-64, arm64",
            other
        )),
    }
}

fn target_from_extension(ext: &str) -> Option<Target> {
    match ext {
        "c" => Some(Target::C),
        "ll" => Some(Target::LlvmIr),
        "wasm" | "wat" => Some(Target::Wasm),
        "spv" => Some(Target::SpirV),
        "s" | "asm" => Some(Target::X86_64),
        "hlsl" | "fx" => Some(Target::Hlsl),
        "glsl" => Some(Target::Glsl),
        "rs" => Some(Target::Rust),
        _ => None,
    }
}

/// QuantaLang Compiler
#[derive(ClapParser)]
#[command(name = "quantac")]
#[command(author = "Zain Dana Harper")]
#[command(version)]
#[command(about = "The QuantaLang compiler - a multi-paradigm systems programming language")]
#[command(long_about = None)]
struct Cli {
    /// The command to run
    #[command(subcommand)]
    command: Option<Commands>,

    /// Input file to compile
    #[arg(value_name = "FILE")]
    input: Option<PathBuf>,

    /// Output file
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Emit debug information
    #[arg(short = 'g', long)]
    debug: bool,

    /// Optimization level (0-3)
    #[arg(short = 'O', long, default_value = "0")]
    opt_level: u8,

    /// Code generation target (c, llvm, wasm, spirv, rust, x86-64, arm64)
    #[arg(long)]
    target: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Tokenize a file and print the tokens
    Lex {
        /// Input file
        file: PathBuf,

        /// Print token details
        #[arg(short, long)]
        verbose: bool,
    },

    /// Parse a file and print the AST
    Parse {
        /// Input file
        file: PathBuf,

        /// Print AST in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Type-check a file
    Check {
        /// Input file
        file: PathBuf,

        /// Write a machine-readable check receipt to a path, or '-' for stdout
        #[arg(long, value_name = "PATH")]
        receipt: Option<PathBuf>,

        /// Evaluate a machine-readable check policy profile
        #[arg(long, value_name = "PATH", conflicts_with = "profile")]
        policy: Option<PathBuf>,

        /// Evaluate a built-in check policy profile
        #[arg(long, value_name = "NAME", conflicts_with = "policy")]
        profile: Option<String>,

        /// Require the selected built-in profile to match a SHA-256 digest
        #[arg(long, value_name = "HEX")]
        expect_profile_digest: Option<String>,
    },

    /// Build a project
    Build {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Build in release mode
        #[arg(long)]
        release: bool,

        /// Emit type: 'c' for C source only, 'exe' for executable (default)
        #[arg(long, default_value = "exe")]
        emit: String,

        /// Keep the intermediate .c file after compilation
        #[arg(long)]
        keep_c: bool,

        /// Code generation target: c, llvm, x86-64, arm64, wasm, spirv, hlsl, glsl, rust
        #[arg(long, default_value = "c")]
        target: String,
    },

    /// Run a file directly
    Run {
        /// Input file
        file: PathBuf,

        /// Arguments to pass to the program
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Start a REPL session
    Repl,

    /// Start the Language Server Protocol server
    Lsp,

    /// Watch shader files and recompile on change
    Watch {
        /// Directory or file to watch
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Target format: 'spirv' (default), 'c'
        #[arg(long, default_value = "spirv")]
        target: String,
    },

    /// Format QuantaLang source files
    Fmt {
        /// Input file to format
        file: PathBuf,

        /// Check formatting without modifying (exit 1 if changes needed)
        #[arg(long)]
        check: bool,

        /// Write formatted output back to the file
        #[arg(short, long)]
        write: bool,
    },

    /// Package manager
    Pkg {
        #[command(subcommand)]
        command: PkgCommands,
    },

    /// Semantic corpus verification and receipt checks
    Corpus {
        #[command(subcommand)]
        command: CorpusCommands,
    },

    /// Built-in check policy profiles for CI and release gates
    Policy {
        #[command(subcommand)]
        command: PolicyCommands,
    },

    /// Verify saved accountability receipts against current source inputs
    Receipt {
        #[command(subcommand)]
        command: ReceiptCommands,
    },

    /// Run tests - compile .quanta programs and verify output against .expected files
    Test {
        /// Directory containing test programs [default: tests/programs]
        #[arg(default_value = "tests/programs")]
        directory: PathBuf,

        /// Only run tests matching this substring
        #[arg(short, long)]
        filter: Option<String>,

        /// Show output for passing tests
        #[arg(long)]
        verbose: bool,

        /// Don't stop on first failure
        #[arg(long)]
        no_fail_fast: bool,
    },

    /// Lint QuantaLang source files
    Lint {
        /// Input file to lint
        file: PathBuf,
    },

    /// Diagnose local compiler, toolchain, backend, and package readiness
    Doctor,

    /// Print version information
    Version,
}

#[derive(Subcommand)]
enum PkgCommands {
    /// Initialize a new Quanta.toml manifest
    Init {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Add a dependency
    Add {
        /// Package name
        name: String,
        /// Version requirement (e.g., "^1.0")
        #[arg(long)]
        version: Option<String>,
    },
    /// Resolve dependencies and generate lockfile
    Resolve {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Search the package registry
    Search {
        /// Search query
        query: String,
    },
}

#[derive(Subcommand)]
enum CorpusCommands {
    /// Verify manifest, receipts, and C backend stdout against the semantic corpus
    Verify {
        /// Semantic corpus root directory
        #[arg(long, value_name = "DIR")]
        root: Option<PathBuf>,
        /// Rewrite the C execution receipt after C stdout verification passes
        #[arg(long)]
        write: bool,
    },
}

#[derive(Subcommand)]
enum PolicyCommands {
    /// List built-in check policy profiles
    List {
        /// Emit the built-in policy catalog as machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Print a built-in check policy profile as JSON
    Print {
        /// Built-in profile name
        name: String,
        /// Write the profile to a file instead of stdout
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ReceiptCommands {
    /// Verify a quantac check receipt against current source inputs
    Verify {
        /// Check receipt JSON written by `quantac check --receipt`
        receipt: PathBuf,
        /// Source file to verify instead of the source path embedded in the receipt
        #[arg(long, value_name = "PATH")]
        source: Option<PathBuf>,
        /// Emit a machine-readable verification report
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Lex { file, verbose }) => cmd_lex(&file, verbose),
        Some(Commands::Parse { file, json }) => cmd_parse(&file, json),
        Some(Commands::Check {
            file,
            receipt,
            policy,
            profile,
            expect_profile_digest,
        }) => cmd_check(
            &file,
            receipt.as_deref(),
            policy.as_deref(),
            profile.as_deref(),
            expect_profile_digest.as_deref(),
        ),
        Some(Commands::Build {
            path,
            release,
            emit,
            keep_c,
            target,
        }) => cmd_build(&path, release, &emit, keep_c, &target),
        Some(Commands::Run { file, args }) => cmd_run(&file, &args),
        Some(Commands::Repl) => cmd_repl(),
        Some(Commands::Lsp) => cmd_lsp(),
        Some(Commands::Watch { path, target }) => cmd_watch(&path, &target),
        Some(Commands::Fmt { file, check, write }) => cmd_fmt(&file, check, write),
        Some(Commands::Pkg { command }) => cmd_pkg(command),
        Some(Commands::Corpus { command }) => cmd_corpus(command),
        Some(Commands::Policy { command }) => cmd_policy(command),
        Some(Commands::Receipt { command }) => cmd_receipt(command),
        Some(Commands::Lint { file }) => cmd_lint(&file),
        Some(Commands::Doctor) => cmd_doctor(),
        Some(Commands::Test {
            directory,
            filter,
            verbose,
            no_fail_fast,
        }) => cmd_test(&directory, filter.as_deref(), verbose, no_fail_fast),
        Some(Commands::Version) => {
            print_version();
            Ok(())
        }
        None => {
            if let Some(input) = cli.input {
                cmd_compile(
                    &input,
                    cli.output.as_deref(),
                    cli.opt_level,
                    cli.debug,
                    cli.target.as_deref(),
                )
            } else {
                eprintln!("No input file specified. Use --help for usage information.");
                Err(1)
            }
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code as u8),
    }
}

fn print_version() {
    println!("QuantaLang Compiler (quantac) {}", quantalang::VERSION);
    println!(
        "Language version: {}.{}.{}",
        quantalang::LANGUAGE_VERSION.0,
        quantalang::LANGUAGE_VERSION.1,
        quantalang::LANGUAGE_VERSION.2
    );
    println!("{}", quantalang::COPYRIGHT);
}

fn command_version(command: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(command)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr)
    } else {
        String::from_utf8_lossy(&output.stdout)
    };
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn print_tool_probe(label: &str, command: &str, args: &[&str]) {
    match command_version(command, args) {
        Some(version) => println!("  {:<10} found    {}", label, version),
        None => println!("  {:<10} missing  install or add to PATH", label),
    }
}

fn cmd_doctor() -> Result<(), i32> {
    println!("QuantaLang Doctor");
    println!("=================");
    println!();
    println!(
        "quantac: {} ({})",
        quantalang::VERSION,
        std::env::consts::OS
    );

    let c_compiler = find_c_compiler();
    match &c_compiler {
        Some(compiler) => println!("C backend: ready via {}", compiler),
        None => println!("C backend: missing C compiler; install MSVC, gcc, clang, or cc"),
    }

    match find_stdlib_path() {
        Some(path) => println!("stdlib: {}", path.display()),
        None => {
            println!("stdlib: not found; set QUANTALANG_STDLIB or install stdlib/ beside quantac")
        }
    }

    let registry = load_local_registry_index();
    if registry.is_empty() {
        println!("registry: no local packages found");
    } else {
        println!("registry: {} local package(s)", registry.len());
    }

    println!();
    println!("Optional tools:");
    print_tool_probe("rustc", "rustc", &["--version"]);
    print_tool_probe("clang", "clang", &["--version"]);
    if cfg!(windows) {
        print_tool_probe("nasm", "nasm", &["--version"]);
    } else {
        print_tool_probe("as", "as", &["--version"]);
    }
    print_tool_probe("wasmtime", "wasmtime", &["--version"]);
    print_tool_probe("spirv-val", "spirv-val", &["--version"]);

    println!();
    println!("Backend maturity:");
    println!("  c        primary       executable C99 path used by quantac run");
    println!("  hlsl     supported     shader source output");
    println!("  glsl     supported     shader source output");
    println!("  rust     experimental  source output with semantic-corpus subset checks");
    println!("  llvm     experimental  LLVM IR; executable path depends on clang");
    println!("  wasm     experimental  WASM/WAT output; runtime depends on wasmtime");
    println!("  spirv    experimental  SPIR-V output; validate with spirv-val");
    println!("  x86-64   experimental  assembly/object output; linker integration is partial");
    println!("  arm64    experimental  assembly/object output; linker integration is partial");

    println!();
    if c_compiler.is_some() {
        println!("Ready for practical C-backend examples: yes");
    } else {
        println!("Ready for practical C-backend examples: no");
    }

    Ok(())
}

#[derive(serde::Deserialize)]
struct SemanticCorpusManifest {
    schema: String,
    programs: Vec<SemanticCorpusProgram>,
}

#[derive(Clone, Debug, serde::Serialize)]
struct CheckReceiptSourceDigest {
    algorithm: &'static str,
    hex: String,
}

#[derive(Clone, Debug, serde::Serialize)]
struct CheckReceiptInputDigest {
    role: String,
    source: String,
    digest: CheckReceiptSourceDigest,
}

#[derive(Default)]
struct InputDigestLedger {
    records: BTreeMap<String, CheckReceiptInputDigest>,
}

impl InputDigestLedger {
    fn record(&mut self, role: &str, path: &Path, bytes: &[u8]) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let source = canonical.to_string_lossy().to_string();
        self.records
            .entry(source.clone())
            .or_insert_with(|| CheckReceiptInputDigest {
                role: role.to_string(),
                source,
                digest: CheckReceiptSourceDigest {
                    algorithm: "sha256",
                    hex: source_digest_hex(bytes),
                },
            });
    }

    fn into_sorted_records(self) -> Vec<CheckReceiptInputDigest> {
        let mut records = self.records.into_values().collect::<Vec<_>>();
        records.sort_by(|left, right| {
            (left.role.as_str(), left.source.as_str())
                .cmp(&(right.role.as_str(), right.source.as_str()))
        });
        records
    }
}

#[derive(serde::Serialize)]
struct CheckReceipt {
    schema: &'static str,
    compiler: &'static str,
    compiler_version: &'static str,
    language_version: String,
    source: String,
    source_digest: CheckReceiptSourceDigest,
    input_graph_digest: CheckReceiptSourceDigest,
    input_digests: Vec<CheckReceiptInputDigest>,
    status: &'static str,
    items: usize,
    tokens: usize,
    declared_effects: BTreeMap<String, Vec<String>>,
    observed_capabilities: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    propagated_effects: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    diagnostics: Vec<CheckReceiptDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<CheckReceiptPolicy>,
}

#[derive(serde::Serialize)]
struct CheckReceiptDiagnostic {
    stage: &'static str,
    kind: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct CheckPolicyProfile {
    schema: String,
    #[serde(default)]
    allowed_effects: Vec<String>,
    #[serde(default)]
    denied_effects: Vec<String>,
    #[serde(default)]
    direct_effect_allowlist: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    propagated_effect_allowlist: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    require_source_digest: bool,
    #[serde(default)]
    require_input_graph_digest: bool,
}

#[derive(Clone, Debug)]
struct LoadedCheckPolicy {
    source: String,
    source_digest: CheckReceiptSourceDigest,
    builtin_profile: Option<String>,
    builtin_profile_digest: Option<CheckReceiptSourceDigest>,
    profile: CheckPolicyProfile,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct CheckPolicyEvidence {
    function: String,
    effect: String,
    surface: &'static str,
    source: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
struct CheckPolicyViolation {
    kind: &'static str,
    effect: String,
    function: String,
    surface: &'static str,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    source: String,
    message: String,
}

impl Ord for CheckPolicyViolation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.function.as_str(),
            self.effect.as_str(),
            self.surface,
            self.source.as_str(),
            self.kind,
            self.message.as_str(),
        )
            .cmp(&(
                other.function.as_str(),
                other.effect.as_str(),
                other.surface,
                other.source.as_str(),
                other.kind,
                other.message.as_str(),
            ))
    }
}

impl PartialOrd for CheckPolicyViolation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug)]
struct CheckPolicyDecision {
    schema: String,
    source: String,
    source_digest: CheckReceiptSourceDigest,
    builtin_profile: Option<String>,
    builtin_profile_digest: Option<CheckReceiptSourceDigest>,
    violations: Vec<CheckPolicyViolation>,
}

#[derive(serde::Serialize)]
struct CheckReceiptPolicy {
    schema: String,
    source: String,
    source_digest: CheckReceiptSourceDigest,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_digest: Option<CheckReceiptSourceDigest>,
    status: &'static str,
    violations: Vec<CheckPolicyViolation>,
}

#[derive(serde::Serialize)]
struct ReceiptVerificationReport {
    schema: &'static str,
    receipt: String,
    source: String,
    status: &'static str,
    checks: Vec<ReceiptVerificationCheck>,
}

#[derive(serde::Serialize)]
struct ReceiptVerificationCheck {
    name: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

struct CheckOutcome {
    source: String,
    compiler_version: &'static str,
    language_version: String,
    source_digest: CheckReceiptSourceDigest,
    input_graph_digest: CheckReceiptSourceDigest,
    input_digests: Vec<CheckReceiptInputDigest>,
    items: usize,
    tokens: usize,
    parse_errors: Vec<String>,
    type_errors: Vec<TypeErrorWithSpan>,
    function_summaries: Vec<FunctionEffectSummary>,
}

#[derive(serde::Deserialize)]
struct SemanticCorpusProgram {
    id: String,
    path: String,
    #[serde(default)]
    surfaces: Vec<String>,
    expected_stdout: String,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct CorpusExecutionReceipt {
    receipt_id: String,
    created_at: String,
    compiler: String,
    backend: String,
    evidence_class: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_mode: Option<String>,
    result: CorpusExecutionResult,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    declared_effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    observed_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capability_gate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capability_gate_test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_execution_test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    receipt_consistency_test: Option<String>,
    #[serde(default)]
    validator_chain: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<String>,
    programs: Vec<CorpusExecutionProgram>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct CorpusExecutionResult {
    passed: usize,
    failed: usize,
    ignored: usize,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct CorpusExecutionProgram {
    id: String,
    path: String,
    expected_stdout: String,
}

struct BuiltinPolicyTemplate {
    name: &'static str,
    summary: &'static str,
}

const BUILTIN_POLICY_TEMPLATES: &[BuiltinPolicyTemplate] = &[
    BuiltinPolicyTemplate {
        name: "pure",
        summary: "deny all built-in ambient capability effects",
    },
    BuiltinPolicyTemplate {
        name: "console-only",
        summary: "allow Console only; deny other ambient capability effects",
    },
    BuiltinPolicyTemplate {
        name: "offline",
        summary: "allow local file/env/clock/console work; deny network/process/FFI/GPU",
    },
    BuiltinPolicyTemplate {
        name: "ci-review",
        summary: "require digests and deny Network, Process, Foreign, and Gpu",
    },
];

fn builtin_policy_profile(name: &str) -> Option<serde_json::Value> {
    match name {
        "pure" => Some(serde_json::json!({
            "schema": "quantalang-check-policy/v1",
            "denied_effects": [
                "FileSystem",
                "Network",
                "Process",
                "Environment",
                "Clock",
                "Console",
                "Foreign",
                "Gpu"
            ],
            "require_source_digest": true,
            "require_input_graph_digest": true
        })),
        "console-only" => Some(serde_json::json!({
            "schema": "quantalang-check-policy/v1",
            "allowed_effects": ["Console"],
            "denied_effects": [
                "FileSystem",
                "Network",
                "Process",
                "Environment",
                "Clock",
                "Foreign",
                "Gpu"
            ],
            "require_source_digest": true,
            "require_input_graph_digest": true
        })),
        "offline" => Some(serde_json::json!({
            "schema": "quantalang-check-policy/v1",
            "allowed_effects": [
                "FileSystem",
                "Environment",
                "Clock",
                "Console"
            ],
            "denied_effects": [
                "Network",
                "Process",
                "Foreign",
                "Gpu"
            ],
            "require_source_digest": true,
            "require_input_graph_digest": true
        })),
        "ci-review" => Some(serde_json::json!({
            "schema": "quantalang-check-policy/v1",
            "denied_effects": [
                "Network",
                "Process",
                "Foreign",
                "Gpu"
            ],
            "require_source_digest": true,
            "require_input_graph_digest": true
        })),
        _ => None,
    }
}

fn builtin_policy_names() -> String {
    BUILTIN_POLICY_TEMPLATES
        .iter()
        .map(|template| template.name)
        .collect::<Vec<_>>()
        .join(", ")
}

fn builtin_policy_json(name: &str) -> Option<String> {
    let profile = builtin_policy_profile(name)?;
    let mut json = serde_json::to_string_pretty(&profile).expect("built-in policy profile is JSON");
    json.push('\n');
    Some(json)
}

fn builtin_policy_digest(name: &str) -> Option<CheckReceiptSourceDigest> {
    let json = builtin_policy_json(name)?;
    Some(CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(json.as_bytes()),
    })
}

fn normalize_profile_digest_pin(pin: &str) -> &str {
    pin.strip_prefix("sha256:")
        .or_else(|| pin.strip_prefix("SHA256:"))
        .unwrap_or(pin)
}

fn builtin_policy_catalog_json() -> String {
    let profiles = BUILTIN_POLICY_TEMPLATES
        .iter()
        .map(|template| {
            let digest =
                builtin_policy_digest(template.name).expect("built-in policy has a digest");
            serde_json::json!({
                "name": template.name,
                "summary": template.summary,
                "policy_schema": "quantalang-check-policy/v1",
                "digest": digest
            })
        })
        .collect::<Vec<_>>();
    let mut json = serde_json::to_string_pretty(&serde_json::json!({
        "schema": "quantalang-policy-catalog/v1",
        "profiles": profiles
    }))
    .expect("built-in policy catalog is JSON");
    json.push('\n');
    json
}

fn cmd_policy(command: PolicyCommands) -> Result<(), i32> {
    match command {
        PolicyCommands::List { json } => {
            if json {
                print!("{}", builtin_policy_catalog_json());
            } else {
                println!("Built-in check policy profiles:");
                for template in BUILTIN_POLICY_TEMPLATES {
                    println!("  {:<14} {}", template.name, template.summary);
                }
            }
            Ok(())
        }
        PolicyCommands::Print { name, output } => {
            let json = builtin_policy_json(&name).ok_or_else(|| {
                eprintln!(
                    "Unknown built-in policy profile '{}'. Available: {}",
                    name,
                    builtin_policy_names()
                );
                1
            })?;
            if let Some(path) = output {
                std::fs::write(&path, json).map_err(|err| {
                    eprintln!("Error writing policy profile '{}': {}", path.display(), err);
                    1
                })?;
            } else {
                print!("{json}");
            }
            Ok(())
        }
    }
}

fn receipt_field_str<'a>(
    receipt: &'a serde_json::Value,
    pointer: &str,
    label: &str,
) -> Result<&'a str, i32> {
    receipt
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            eprintln!("Error: receipt is missing string field `{}`", label);
            1
        })
}

fn receipt_digest_hex<'a>(
    receipt: &'a serde_json::Value,
    pointer: &str,
    label: &str,
) -> Result<&'a str, i32> {
    let algorithm = receipt_field_str(receipt, &format!("{pointer}/algorithm"), label)?;
    if algorithm != "sha256" {
        eprintln!(
            "Error: receipt field `{}` uses unsupported digest algorithm `{}`",
            label, algorithm
        );
        return Err(1);
    }
    let hex = receipt_field_str(receipt, &format!("{pointer}/hex"), label)?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        eprintln!(
            "Error: receipt field `{}` is not a sha256 hex digest",
            label
        );
        return Err(1);
    }
    Ok(hex)
}

fn verify_receipt_digest(
    receipt: &serde_json::Value,
    pointer: &str,
    label: &str,
    actual: &CheckReceiptSourceDigest,
) -> Result<(), i32> {
    let expected_hex = receipt_digest_hex(receipt, pointer, label)?;
    if actual.algorithm != "sha256" || !actual.hex.eq_ignore_ascii_case(expected_hex) {
        eprintln!(
            "Error: {} mismatch: expected sha256:{}, actual sha256:{}",
            label, expected_hex, actual.hex
        );
        return Err(1);
    }
    Ok(())
}

fn current_policy_source_digest(policy_source: &str) -> Result<CheckReceiptSourceDigest, i32> {
    if let Some(profile) = policy_source.strip_prefix("builtin:") {
        return builtin_policy_digest(profile).ok_or_else(|| {
            eprintln!("Error: unknown built-in policy profile `{}`", profile);
            1
        });
    }

    let path = Path::new(policy_source);
    let bytes = std::fs::read(path).map_err(|err| {
        eprintln!("Error reading policy '{}': {}", path.display(), err);
        1
    })?;
    Ok(CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(&bytes),
    })
}

fn cmd_receipt(command: ReceiptCommands) -> Result<(), i32> {
    match command {
        ReceiptCommands::Verify {
            receipt,
            source,
            json,
        } => cmd_receipt_verify(&receipt, source.as_deref(), json),
    }
}

fn digest_label(digest: &CheckReceiptSourceDigest) -> String {
    format!("{}:{}", digest.algorithm, digest.hex)
}

fn push_receipt_verification_check(
    checks: &mut Vec<ReceiptVerificationCheck>,
    name: &str,
    expected: Option<String>,
    actual: Option<String>,
    profile: Option<String>,
    message: Option<String>,
) {
    checks.push(ReceiptVerificationCheck {
        name: name.to_string(),
        status: if message.is_none() {
            "passed"
        } else {
            "failed"
        },
        expected,
        actual,
        profile,
        message,
    });
}

fn compact_receipt_value(value: Option<&serde_json::Value>) -> Option<String> {
    value.map(|value| {
        serde_json::to_string(value).unwrap_or_else(|err| format!("<unserializable: {err}>"))
    })
}

fn receipt_replay_fields(
    receipt: &serde_json::Value,
    current_receipt: &serde_json::Value,
) -> Vec<(&'static str, &'static str)> {
    let mut fields = vec![
        ("/status", "status"),
        ("/items", "items"),
        ("/tokens", "tokens"),
        ("/declared_effects", "declared_effects"),
        ("/observed_capabilities", "observed_capabilities"),
        ("/propagated_effects", "propagated_effects"),
        ("/diagnostics", "diagnostics"),
    ];
    if receipt.pointer("/policy").is_some() || current_receipt.pointer("/policy").is_some() {
        fields.push(("/policy/status", "policy_status"));
        fields.push(("/policy/violations", "policy_violations"));
    }
    fields
}

fn load_receipt_policy(receipt: &serde_json::Value) -> Result<Option<LoadedCheckPolicy>, i32> {
    if let Some(profile) = receipt
        .pointer("/policy/profile")
        .and_then(serde_json::Value::as_str)
    {
        return load_builtin_check_policy(profile).map(Some);
    }

    let Some(policy_source) = receipt
        .pointer("/policy/source")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(None);
    };

    if let Some(profile) = policy_source.strip_prefix("builtin:") {
        load_builtin_check_policy(profile).map(Some)
    } else {
        load_check_policy(Path::new(policy_source)).map(Some)
    }
}

fn current_replayed_receipt_value(
    receipt: &serde_json::Value,
    current: &CheckOutcome,
) -> Result<serde_json::Value, i32> {
    let loaded_policy = load_receipt_policy(receipt)?;
    let policy_decision = loaded_policy
        .as_ref()
        .map(|policy| evaluate_check_policy(policy, current));
    let current_receipt = build_check_receipt(current, policy_decision.as_ref());
    serde_json::to_value(current_receipt).map_err(|err| {
        eprintln!("Error rebuilding receipt for verification: {}", err);
        1
    })
}

fn verify_receipt_replay_fields(
    receipt: &serde_json::Value,
    current_receipt: &serde_json::Value,
) -> Result<(), i32> {
    for (pointer, name) in receipt_replay_fields(receipt, current_receipt) {
        if receipt.pointer(pointer) != current_receipt.pointer(pointer) {
            eprintln!("Error: receipt {} mismatch", name);
            return Err(1);
        }
    }
    Ok(())
}

fn push_receipt_replay_checks(
    checks: &mut Vec<ReceiptVerificationCheck>,
    receipt: &serde_json::Value,
    current_receipt: &serde_json::Value,
) {
    for (pointer, name) in receipt_replay_fields(receipt, current_receipt) {
        let expected = compact_receipt_value(receipt.pointer(pointer));
        let actual = compact_receipt_value(current_receipt.pointer(pointer));
        let mismatch = receipt.pointer(pointer) != current_receipt.pointer(pointer);
        push_receipt_verification_check(
            checks,
            name,
            expected,
            actual,
            None,
            mismatch.then(|| format!("receipt {} mismatch", name)),
        );
    }
}

fn cmd_receipt_verify(
    receipt_path: &Path,
    source_override: Option<&Path>,
    json: bool,
) -> Result<(), i32> {
    if json {
        return cmd_receipt_verify_json(receipt_path, source_override);
    }

    let receipt: serde_json::Value = read_json(receipt_path)?;
    let schema = receipt_field_str(&receipt, "/schema", "schema")?;
    if schema != "quantalang-check-receipt/v1" {
        eprintln!("Error: unsupported check receipt schema `{}`", schema);
        return Err(1);
    }
    let compiler = receipt_field_str(&receipt, "/compiler", "compiler")?;
    if compiler != "quantac" {
        eprintln!(
            "Error: receipt compiler mismatch: expected quantac, got {}",
            compiler
        );
        return Err(1);
    }
    let compiler_version = receipt_field_str(&receipt, "/compiler_version", "compiler_version")?;
    if compiler_version != env!("CARGO_PKG_VERSION") {
        eprintln!(
            "Error: compiler version mismatch: expected {}, actual {}",
            compiler_version,
            env!("CARGO_PKG_VERSION")
        );
        return Err(1);
    }
    let language_version = receipt_field_str(&receipt, "/language_version", "language_version")?;
    let current_language_version = language_version_string();
    if language_version != current_language_version {
        eprintln!(
            "Error: language version mismatch: expected {}, actual {}",
            language_version, current_language_version
        );
        return Err(1);
    }

    let source_path = if let Some(source_override) = source_override {
        source_override.to_path_buf()
    } else {
        PathBuf::from(receipt_field_str(&receipt, "/source", "source")?)
    };
    let current = run_check(&source_path)?;
    verify_receipt_digest(
        &receipt,
        "/source_digest",
        "source digest",
        &current.source_digest,
    )?;
    verify_receipt_digest(
        &receipt,
        "/input_graph_digest",
        "input graph digest",
        &current.input_graph_digest,
    )?;

    if let Some(policy_source) = receipt
        .pointer("/policy/source")
        .and_then(serde_json::Value::as_str)
    {
        let expected_hex =
            receipt_digest_hex(&receipt, "/policy/source_digest", "policy source digest")?;
        let actual = current_policy_source_digest(policy_source)?;
        if !actual.hex.eq_ignore_ascii_case(expected_hex) {
            eprintln!(
                "Error: policy source digest mismatch for '{}': expected sha256:{}, actual sha256:{}",
                policy_source, expected_hex, actual.hex
            );
            return Err(1);
        }
    }

    if let Some(profile) = receipt
        .pointer("/policy/profile")
        .and_then(serde_json::Value::as_str)
    {
        let expected_hex =
            receipt_digest_hex(&receipt, "/policy/profile_digest", "policy profile digest")?;
        let actual = builtin_policy_digest(profile).ok_or_else(|| {
            eprintln!("Error: unknown built-in policy profile `{}`", profile);
            1
        })?;
        if !actual.hex.eq_ignore_ascii_case(expected_hex) {
            eprintln!(
                "Error: built-in policy profile digest mismatch for '{}': expected sha256:{}, actual sha256:{}",
                profile, expected_hex, actual.hex
            );
            return Err(1);
        }
    }

    let current_receipt = current_replayed_receipt_value(&receipt, &current)?;
    verify_receipt_replay_fields(&receipt, &current_receipt)?;

    println!("Receipt verified: {}", receipt_path.display());
    Ok(())
}

fn cmd_receipt_verify_json(receipt_path: &Path, source_override: Option<&Path>) -> Result<(), i32> {
    let receipt: serde_json::Value = read_json(receipt_path)?;
    let mut checks = Vec::new();

    let schema = receipt_field_str(&receipt, "/schema", "schema")?;
    let expected_schema = "quantalang-check-receipt/v1".to_string();
    push_receipt_verification_check(
        &mut checks,
        "schema",
        Some(expected_schema.clone()),
        Some(schema.to_string()),
        None,
        (schema != expected_schema).then(|| "unsupported check receipt schema".to_string()),
    );

    let compiler = receipt_field_str(&receipt, "/compiler", "compiler")?;
    push_receipt_verification_check(
        &mut checks,
        "compiler",
        Some("quantac".to_string()),
        Some(compiler.to_string()),
        None,
        (compiler != "quantac").then(|| "receipt compiler mismatch".to_string()),
    );

    let compiler_version = receipt_field_str(&receipt, "/compiler_version", "compiler_version")?;
    let current_compiler_version = env!("CARGO_PKG_VERSION");
    push_receipt_verification_check(
        &mut checks,
        "compiler_version",
        Some(compiler_version.to_string()),
        Some(current_compiler_version.to_string()),
        None,
        (compiler_version != current_compiler_version)
            .then(|| "compiler version mismatch".to_string()),
    );

    let language_version = receipt_field_str(&receipt, "/language_version", "language_version")?;
    let current_language_version = language_version_string();
    push_receipt_verification_check(
        &mut checks,
        "language_version",
        Some(language_version.to_string()),
        Some(current_language_version.clone()),
        None,
        (language_version != current_language_version)
            .then(|| "language version mismatch".to_string()),
    );

    let source_path = if let Some(source_override) = source_override {
        source_override.to_path_buf()
    } else {
        PathBuf::from(receipt_field_str(&receipt, "/source", "source")?)
    };
    let current = run_check(&source_path)?;

    let expected_source_digest = receipt_digest_hex(&receipt, "/source_digest", "source digest")?;
    let actual_source_digest = digest_label(&current.source_digest);
    push_receipt_verification_check(
        &mut checks,
        "source_digest",
        Some(format!("sha256:{expected_source_digest}")),
        Some(actual_source_digest),
        None,
        (!current
            .source_digest
            .hex
            .eq_ignore_ascii_case(expected_source_digest))
        .then(|| "source digest mismatch".to_string()),
    );

    let expected_graph_digest =
        receipt_digest_hex(&receipt, "/input_graph_digest", "input graph digest")?;
    let actual_graph_digest = digest_label(&current.input_graph_digest);
    push_receipt_verification_check(
        &mut checks,
        "input_graph_digest",
        Some(format!("sha256:{expected_graph_digest}")),
        Some(actual_graph_digest),
        None,
        (!current
            .input_graph_digest
            .hex
            .eq_ignore_ascii_case(expected_graph_digest))
        .then(|| "input graph digest mismatch".to_string()),
    );

    if let Some(policy_source) = receipt
        .pointer("/policy/source")
        .and_then(serde_json::Value::as_str)
    {
        let expected_policy_digest =
            receipt_digest_hex(&receipt, "/policy/source_digest", "policy source digest")?;
        let actual_policy_digest = current_policy_source_digest(policy_source)?;
        push_receipt_verification_check(
            &mut checks,
            "policy_source_digest",
            Some(format!("sha256:{expected_policy_digest}")),
            Some(digest_label(&actual_policy_digest)),
            None,
            (!actual_policy_digest
                .hex
                .eq_ignore_ascii_case(expected_policy_digest))
            .then(|| "policy source digest mismatch".to_string()),
        );
    }

    if let Some(profile) = receipt
        .pointer("/policy/profile")
        .and_then(serde_json::Value::as_str)
    {
        let expected_profile_digest =
            receipt_digest_hex(&receipt, "/policy/profile_digest", "policy profile digest")?;
        let actual_profile_digest = builtin_policy_digest(profile).ok_or_else(|| {
            eprintln!("Error: unknown built-in policy profile `{}`", profile);
            1
        })?;
        push_receipt_verification_check(
            &mut checks,
            "policy_profile_digest",
            Some(format!("sha256:{expected_profile_digest}")),
            Some(digest_label(&actual_profile_digest)),
            Some(profile.to_string()),
            (!actual_profile_digest
                .hex
                .eq_ignore_ascii_case(expected_profile_digest))
            .then(|| "built-in policy profile digest mismatch".to_string()),
        );
    }

    let current_receipt = current_replayed_receipt_value(&receipt, &current)?;
    push_receipt_replay_checks(&mut checks, &receipt, &current_receipt);

    let passed = checks.iter().all(|check| check.status == "passed");
    let report = ReceiptVerificationReport {
        schema: "quantalang-receipt-verification/v1",
        receipt: receipt_path.to_string_lossy().to_string(),
        source: source_path.to_string_lossy().to_string(),
        status: if passed { "passed" } else { "failed" },
        checks,
    };
    let json = serde_json::to_string_pretty(&report).map_err(|err| {
        eprintln!(
            "Error serializing receipt verification report '{}': {}",
            receipt_path.display(),
            err
        );
        1
    })?;
    println!("{}", json);
    if passed {
        Ok(())
    } else {
        Err(1)
    }
}

fn cmd_corpus(command: CorpusCommands) -> Result<(), i32> {
    match command {
        CorpusCommands::Verify { root, write } => cmd_corpus_verify(root.as_deref(), write),
    }
}

fn cmd_corpus_verify(root: Option<&Path>, write: bool) -> Result<(), i32> {
    let corpus_root = match root {
        Some(path) => {
            if !path.join("manifest.json").is_file() {
                eprintln!(
                    "semantic corpus manifest not found at {}",
                    path.join("manifest.json").display()
                );
                return Err(1);
            }
            path.to_path_buf()
        }
        None => find_semantic_corpus_root().ok_or_else(|| {
            eprintln!(
                "semantic corpus not found; run from the repository or install semantic-corpus/"
            );
            1
        })?,
    };

    let manifest_path = corpus_root.join("manifest.json");
    let manifest: SemanticCorpusManifest = read_json(&manifest_path)?;
    if manifest.schema != "quantalang-semantic-corpus/v1" {
        eprintln!(
            "semantic corpus manifest has unsupported schema '{}'",
            manifest.schema
        );
        return Err(1);
    }

    let receipts_dir = corpus_root.join("receipts");
    let c_receipt_path = receipts_dir.join("c-execution-2026-06-13.json");
    let rust_receipt_path = receipts_dir.join("rust-execution-2026-06-13.json");
    let c_passed = if write {
        let rust_receipt: CorpusExecutionReceipt = read_json(&rust_receipt_path)?;
        verify_receipt(
            "rust",
            &rust_receipt,
            &manifest,
            manifest.programs.len() + 1,
        )?;

        let c_passed = verify_c_corpus_stdout(&corpus_root, &manifest)?;
        let c_receipt: CorpusExecutionReceipt = read_json(&c_receipt_path)?;
        let c_receipt = refresh_c_receipt_from_manifest(c_receipt, &manifest, c_passed);
        write_json(&c_receipt_path, &c_receipt)?;

        let c_receipt: CorpusExecutionReceipt = read_json(&c_receipt_path)?;
        verify_receipt("c", &c_receipt, &manifest, c_passed)?;
        c_passed
    } else {
        let c_receipt: CorpusExecutionReceipt = read_json(&c_receipt_path)?;
        let rust_receipt: CorpusExecutionReceipt = read_json(&rust_receipt_path)?;

        verify_receipt("c", &c_receipt, &manifest, manifest.programs.len())?;
        verify_receipt(
            "rust",
            &rust_receipt,
            &manifest,
            manifest.programs.len() + 1,
        )?;
        verify_c_corpus_stdout(&corpus_root, &manifest)?
    };

    println!("Semantic Corpus Verify");
    println!("manifest: {} program(s)", manifest.programs.len());
    println!("c receipt: ok");
    println!("rust receipt: ok");
    println!("c execution: {} passed", c_passed);
    if write {
        println!("c receipt: written");
    }
    Ok(())
}

fn find_semantic_corpus_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            candidates.push(ancestor.join("semantic-corpus"));
        }
    }

    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
            .join("semantic-corpus"),
    );

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            for ancestor in parent.ancestors() {
                candidates.push(ancestor.join("semantic-corpus"));
            }
        }
    }

    candidates
        .into_iter()
        .find(|path| path.join("manifest.json").is_file())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, i32> {
    let content = std::fs::read_to_string(path).map_err(|err| {
        eprintln!("failed to read {}: {}", path.display(), err);
        1
    })?;
    serde_json::from_str(&content).map_err(|err| {
        eprintln!("failed to parse {}: {}", path.display(), err);
        1
    })
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), i32> {
    let json = serde_json::to_string_pretty(value).map_err(|err| {
        eprintln!("failed to serialize {}: {}", path.display(), err);
        1
    })?;
    std::fs::write(path, format!("{}\n", json)).map_err(|err| {
        eprintln!("failed to write {}: {}", path.display(), err);
        1
    })
}

fn expected_receipt_capabilities(manifest: &SemanticCorpusManifest) -> Vec<String> {
    let mut capabilities = BTreeSet::new();
    for program in &manifest.programs {
        for surface in &program.surfaces {
            if surface == "stdout" {
                capabilities.insert("Console".to_string());
            }
        }
    }
    capabilities.into_iter().collect()
}

fn apply_capability_receipt_metadata(
    receipt: &mut CorpusExecutionReceipt,
    manifest: &SemanticCorpusManifest,
) {
    let capabilities = expected_receipt_capabilities(manifest);
    receipt.declared_effects = capabilities.clone();
    receipt.observed_capabilities = capabilities;
    receipt.capability_gate = Some("passed".to_string());
    receipt.capability_gate_test =
        Some("cargo test --manifest-path compiler/Cargo.toml capability --quiet".to_string());
}

fn refresh_c_receipt_from_manifest(
    mut receipt: CorpusExecutionReceipt,
    manifest: &SemanticCorpusManifest,
    passed: usize,
) -> CorpusExecutionReceipt {
    receipt.result.passed = passed;
    receipt.result.failed = 0;
    receipt.result.ignored = 0;
    receipt.programs = manifest
        .programs
        .iter()
        .map(|program| CorpusExecutionProgram {
            id: program.id.clone(),
            path: format!("../{}", program.path),
            expected_stdout: program.expected_stdout.clone(),
        })
        .collect();
    apply_capability_receipt_metadata(&mut receipt, manifest);
    receipt
}

fn verify_receipt(
    label: &str,
    receipt: &CorpusExecutionReceipt,
    manifest: &SemanticCorpusManifest,
    expected_passed: usize,
) -> Result<(), i32> {
    if receipt.backend != label {
        eprintln!(
            "{} receipt backend mismatch: expected '{}', found '{}'",
            label, label, receipt.backend
        );
        return Err(1);
    }
    if receipt.result.failed != 0 || receipt.result.ignored != 0 {
        eprintln!(
            "{} receipt is not clean: {} failed, {} ignored",
            label, receipt.result.failed, receipt.result.ignored
        );
        return Err(1);
    }
    if receipt.result.passed != expected_passed {
        eprintln!(
            "{} receipt pass count mismatch: expected {}, found {}",
            label, expected_passed, receipt.result.passed
        );
        return Err(1);
    }
    if receipt.programs.len() != manifest.programs.len() {
        eprintln!(
            "{} receipt program count mismatch: expected {}, found {}",
            label,
            manifest.programs.len(),
            receipt.programs.len()
        );
        return Err(1);
    }

    for (manifest_program, receipt_program) in manifest.programs.iter().zip(receipt.programs.iter())
    {
        let receipt_path = receipt_program.path.trim_start_matches("../");
        if receipt_program.id != manifest_program.id
            || receipt_path != manifest_program.path
            || receipt_program.expected_stdout != manifest_program.expected_stdout
        {
            eprintln!(
                "{} receipt drift for program '{}'",
                label, manifest_program.id
            );
            return Err(1);
        }
    }

    let expected_capabilities = expected_receipt_capabilities(manifest);
    if receipt.declared_effects != expected_capabilities
        || receipt.observed_capabilities != expected_capabilities
        || receipt.capability_gate.as_deref() != Some("passed")
        || receipt.capability_gate_test.as_deref()
            != Some("cargo test --manifest-path compiler/Cargo.toml capability --quiet")
    {
        eprintln!("{} receipt capability metadata drift", label);
        return Err(1);
    }

    Ok(())
}

fn verify_c_corpus_stdout(
    corpus_root: &Path,
    manifest: &SemanticCorpusManifest,
) -> Result<usize, i32> {
    let quantac = std::env::current_exe().map_err(|err| {
        eprintln!("failed to locate current quantac executable: {}", err);
        1
    })?;

    for program in &manifest.programs {
        let program_path = corpus_root.join(&program.path);
        let output = std::process::Command::new(&quantac)
            .arg("run")
            .arg(&program_path)
            .output()
            .map_err(|err| {
                eprintln!(
                    "failed to run semantic corpus program {}: {}",
                    program.id, err
                );
                1
            })?;

        if !output.status.success() {
            eprintln!(
                "semantic corpus program {} failed\nstdout:\n{}\nstderr:\n{}",
                program.id,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return Err(1);
        }

        let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
        if stdout != program.expected_stdout {
            eprintln!(
                "semantic corpus stdout drift for {}\nexpected:\n{:?}\nactual:\n{:?}",
                program.id, program.expected_stdout, stdout
            );
            return Err(1);
        }
    }

    Ok(manifest.programs.len())
}

fn cmd_lex(file: &PathBuf, verbose: bool) -> Result<(), i32> {
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    // Expand `include!("path")` directives
    let lex_base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, lex_base)?;

    let source_file = SourceFile::new(file.to_string_lossy(), source);
    let mut lexer = Lexer::new(&source_file);

    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    for token in &tokens {
        if verbose {
            let (start, end) = source_file.span_to_positions(token.span);
            let text = source_file.slice(token.span);
            println!(
                "{:4}:{:<3} - {:4}:{:<3}  {:20} {:?}",
                start.line,
                start.column,
                end.line,
                end.column,
                format!("{}", token.kind),
                text
            );
        } else {
            println!("{}", token.kind);
        }
    }

    println!("\nTotal: {} tokens", tokens.len());
    Ok(())
}

fn cmd_parse(file: &PathBuf, json: bool) -> Result<(), i32> {
    // Read source file
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    // Expand `include!("path")` directives
    let parse_base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, parse_base)?;

    let source_file = SourceFile::new(file.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        // Print any accumulated errors
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;

    // Display AST
    if json {
        // JSON output using serde if available
        println!("{}", format_ast_json(&ast));
    } else {
        // Pretty print AST
        println!("=== Abstract Syntax Tree ===");
        println!("File: {}", file.display());
        println!("Items: {}", ast.items.len());
        println!();

        for (i, item) in ast.items.iter().enumerate() {
            println!("Item {}: {}", i + 1, item_kind_name(&item.kind));
            print_item_summary(item, 1);
        }
    }

    Ok(())
}

fn item_kind_name(kind: &quantalang::ast::ItemKind) -> &'static str {
    match kind {
        quantalang::ast::ItemKind::Function(_) => "Function",
        quantalang::ast::ItemKind::Struct(_) => "Struct",
        quantalang::ast::ItemKind::Enum(_) => "Enum",
        quantalang::ast::ItemKind::Trait(_) => "Trait",
        quantalang::ast::ItemKind::Impl(_) => "Impl",
        quantalang::ast::ItemKind::TypeAlias(_) => "TypeAlias",
        quantalang::ast::ItemKind::Const(_) => "Const",
        quantalang::ast::ItemKind::Static(_) => "Static",
        quantalang::ast::ItemKind::Mod(_) => "Mod",
        quantalang::ast::ItemKind::Use(_) => "Use",
        quantalang::ast::ItemKind::ExternCrate(_) => "ExternCrate",
        quantalang::ast::ItemKind::ExternBlock(_) => "ExternBlock",
        quantalang::ast::ItemKind::Macro(_) => "Macro",
        quantalang::ast::ItemKind::MacroRules(_) => "MacroRules",
        quantalang::ast::ItemKind::Effect(_) => "Effect",
    }
}

fn format_ast_json(ast: &Module) -> String {
    // Simple JSON representation
    let mut output = String::new();
    output.push_str("{\n");
    output.push_str(&format!("  \"items\": {},\n", ast.items.len()));
    output.push_str("  \"item_kinds\": [\n");
    for (i, item) in ast.items.iter().enumerate() {
        let comma = if i < ast.items.len() - 1 { "," } else { "" };
        output.push_str(&format!(
            "    \"{}\"{}\n",
            item_kind_name(&item.kind),
            comma
        ));
    }
    output.push_str("  ]\n");
    output.push_str("}\n");
    output
}

fn struct_field_count(fields: &quantalang::ast::StructFields) -> usize {
    match fields {
        quantalang::ast::StructFields::Named(f) => f.len(),
        quantalang::ast::StructFields::Tuple(f) => f.len(),
        quantalang::ast::StructFields::Unit => 0,
    }
}

fn print_item_summary(item: &quantalang::ast::Item, indent: usize) {
    let prefix = "  ".repeat(indent);
    match &item.kind {
        quantalang::ast::ItemKind::Function(f) => {
            println!("{}fn {}()", prefix, f.name.name);
            if let Some(ret) = &f.sig.return_ty {
                println!("{}  -> {:?}", prefix, ret);
            }
        }
        quantalang::ast::ItemKind::Struct(s) => {
            println!(
                "{}struct {} ({} fields)",
                prefix,
                s.name.name,
                struct_field_count(&s.fields)
            );
        }
        quantalang::ast::ItemKind::Enum(e) => {
            println!(
                "{}enum {} ({} variants)",
                prefix,
                e.name.name,
                e.variants.len()
            );
        }
        quantalang::ast::ItemKind::Trait(t) => {
            println!("{}trait {} ({} items)", prefix, t.name.name, t.items.len());
        }
        quantalang::ast::ItemKind::Impl(i) => {
            println!("{}impl ({} items)", prefix, i.items.len());
        }
        quantalang::ast::ItemKind::TypeAlias(t) => {
            println!("{}type {}", prefix, t.name.name);
        }
        quantalang::ast::ItemKind::Const(c) => {
            println!("{}const {}", prefix, c.name.name);
        }
        quantalang::ast::ItemKind::Static(s) => {
            println!("{}static {}", prefix, s.name.name);
        }
        quantalang::ast::ItemKind::Mod(m) => {
            println!("{}mod {}", prefix, m.name.name);
        }
        quantalang::ast::ItemKind::Use(u) => {
            println!("{}use {:?}", prefix, u.tree);
        }
        quantalang::ast::ItemKind::ExternCrate(e) => {
            println!("{}extern crate {}", prefix, e.name.name);
        }
        quantalang::ast::ItemKind::ExternBlock(e) => {
            println!(
                "{}extern \"{}\" ({} items)",
                prefix,
                e.abi.as_deref().unwrap_or("C"),
                e.items.len()
            );
        }
        quantalang::ast::ItemKind::Macro(m) => {
            println!("{}macro {:?}!", prefix, m.name.as_ref().map(|n| &n.name));
        }
        quantalang::ast::ItemKind::MacroRules(m) => {
            println!("{}macro_rules! {}", prefix, m.name.name);
        }
        quantalang::ast::ItemKind::Effect(e) => {
            println!("{}effect {}", prefix, e.name.name);
        }
    }
}

// =============================================================================
// INCLUDE PREPROCESSING (textual `include!("path")` expansion)
// =============================================================================

/// Maximum recursion depth for nested includes to prevent infinite loops.
const MAX_INCLUDE_DEPTH: usize = 10;

/// Preprocess `include!("path")` directives in source code.
///
/// This is a textual inclusion mechanism (like C's `#include`): the referenced
/// file's contents replace the `include!()` line.  Paths are resolved relative
/// to `base_dir` (typically the directory containing the current source file).
///
/// Features:
/// - Nested includes up to `MAX_INCLUDE_DEPTH` levels
/// - Double-inclusion guard: each canonical path is included at most once
/// - Graceful error reporting on missing files or depth overflow
fn preprocess_includes(source: &str, base_dir: &Path) -> Result<String, i32> {
    let mut included: HashSet<PathBuf> = HashSet::new();
    let mut ledger = None;
    preprocess_includes_inner(source, base_dir, 0, &mut included, &mut ledger)
}

fn preprocess_includes_recording_inputs(
    source: &str,
    base_dir: &Path,
    ledger: &mut InputDigestLedger,
) -> Result<String, i32> {
    let mut included: HashSet<PathBuf> = HashSet::new();
    let mut ledger = Some(ledger);
    preprocess_includes_inner(source, base_dir, 0, &mut included, &mut ledger)
}

fn preprocess_includes_inner(
    source: &str,
    base_dir: &Path,
    depth: usize,
    included: &mut HashSet<PathBuf>,
    ledger: &mut Option<&mut InputDigestLedger>,
) -> Result<String, i32> {
    if depth > MAX_INCLUDE_DEPTH {
        eprintln!(
            "Error: include depth exceeds {} - possible circular inclusion",
            MAX_INCLUDE_DEPTH
        );
        return Err(1);
    }

    let mut result = String::with_capacity(source.len());

    for line in source.lines() {
        let trimmed = line.trim();

        // Match: include!("some/path.quanta");
        if let Some(path_str) = trimmed
            .strip_prefix("include!(\"")
            .and_then(|s| s.strip_suffix("\");"))
        {
            let full_path = base_dir.join(path_str);
            let canonical = full_path
                .canonicalize()
                .unwrap_or_else(|_| full_path.clone());

            // Double-inclusion guard
            if included.contains(&canonical) {
                // Already included - skip silently
                result.push_str("// [include already loaded: ");
                result.push_str(path_str);
                result.push_str("]\n");
                continue;
            }

            if full_path.exists() {
                let bytes = std::fs::read(&full_path).map_err(|e| {
                    eprintln!("Error reading include '{}': {}", full_path.display(), e);
                    1
                })?;
                if let Some(ledger) = ledger.as_deref_mut() {
                    ledger.record("include", &full_path, &bytes);
                }
                let contents = String::from_utf8(bytes).map_err(|e| {
                    eprintln!("Error reading include '{}': {}", full_path.display(), e);
                    1
                })?;

                included.insert(canonical);

                // Recursively expand includes in the included file
                let inc_dir = full_path.parent().unwrap_or(base_dir);
                let expanded =
                    preprocess_includes_inner(&contents, inc_dir, depth + 1, included, ledger)?;

                result.push_str("// === include: ");
                result.push_str(path_str);
                result.push_str(" ===\n");
                result.push_str(&expanded);
                if !expanded.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str("// === end include: ");
                result.push_str(path_str);
                result.push_str(" ===\n");
            } else {
                eprintln!(
                    "Error: include file not found: '{}' (resolved to '{}')",
                    path_str,
                    full_path.display()
                );
                return Err(1);
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    Ok(result)
}

// =============================================================================
// IMPORT RESOLUTION (simple `// import <pkg>` and `use <pkg>;` directives)
// =============================================================================

/// Scan `source` for lines matching `// import <name>` or `use <name>;`.
/// For each match, look for `registry/packages/<name>/src/lib.quanta` relative
/// to the repo root (derived from `input_file`).  If found, prepend its contents
/// to the source so the combined text can be parsed as a single compilation unit.
///
/// Name normalisation: underscores in the import name are converted to hyphens
/// when looking up the package directory (e.g. `use std_math;` maps to
/// `registry/packages/std-math/src/lib.quanta`).
fn resolve_imports(source: &str, input_file: &Path) -> Result<String, i32> {
    let mut ledger = None;
    resolve_imports_inner(source, input_file, &mut ledger)
}

fn resolve_imports_recording_inputs(
    source: &str,
    input_file: &Path,
    ledger: &mut InputDigestLedger,
) -> Result<String, i32> {
    let mut ledger = Some(ledger);
    resolve_imports_inner(source, input_file, &mut ledger)
}

fn resolve_imports_inner(
    source: &str,
    input_file: &Path,
    ledger: &mut Option<&mut InputDigestLedger>,
) -> Result<String, i32> {
    // Try to locate the registry directory.
    // Walk up from the input file looking for a directory that contains
    // `registry/packages`.
    let registry_dir = {
        let mut dir = input_file.parent();
        let mut found: Option<PathBuf> = None;
        while let Some(d) = dir {
            let candidate = d.join("registry").join("packages");
            if candidate.is_dir() {
                found = Some(candidate);
                break;
            }
            dir = d.parent();
        }
        found
    };

    let mut prepended = String::new();
    let mut found_any = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Match `// import <name>`
        let import_name = if let Some(rest) = trimmed.strip_prefix("// import ") {
            Some(rest.trim().to_string())
        }
        // Match `use <name>;`
        else if let Some(rest) = trimmed.strip_prefix("use ") {
            let rest = rest.trim();
            if let Some(name) = rest.strip_suffix(';') {
                let name = name.trim();
                // Skip complex use paths like `std::collections::HashMap` - we
                // only handle bare package names (no `::` separators).
                if !name.contains("::") && !name.contains('{') {
                    Some(name.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some(name) = import_name {
            if let Some(ref reg) = registry_dir {
                // Normalise: underscores -> hyphens for the directory name.
                let pkg_dir_name = name.replace('_', "-");
                let lib_path = reg.join(&pkg_dir_name).join("src").join("lib.quanta");
                if lib_path.exists() {
                    let bytes = std::fs::read(&lib_path).map_err(|e| {
                        eprintln!(
                            "Error reading import '{}' from '{}': {}",
                            name,
                            lib_path.display(),
                            e
                        );
                        1
                    })?;
                    if let Some(ledger) = ledger.as_deref_mut() {
                        ledger.record("import", &lib_path, &bytes);
                    }
                    let contents = String::from_utf8(bytes).map_err(|e| {
                        eprintln!(
                            "Error reading import '{}' from '{}': {}",
                            name,
                            lib_path.display(),
                            e
                        );
                        1
                    })?;
                    // Prepend with a separator comment for clarity.
                    prepended.push_str(&format!(
                        "// === imported from registry: {} ===\n{}\n// === end import: {} ===\n\n",
                        name, contents, name
                    ));
                    found_any = true;
                } else {
                    eprintln!(
                        "Warning: import '{}' not found at '{}'",
                        name,
                        lib_path.display()
                    );
                }
            } else {
                eprintln!(
                    "Warning: import '{}' requested but no registry directory found",
                    name
                );
            }
        }
    }

    if found_any {
        prepended.push_str(source);
        Ok(prepended)
    } else {
        Ok(source.to_string())
    }
}

fn type_error_kind(error: &TypeError) -> &'static str {
    match error {
        TypeError::TypeMismatch { .. } => "TypeMismatch",
        TypeError::InfiniteType { .. } => "InfiniteType",
        TypeError::MutabilityMismatch { .. } => "MutabilityMismatch",
        TypeError::UnknownEffect { .. } => "UnknownEffect",
        TypeError::UnhandledEffect { .. } => "UnhandledEffect",
        TypeError::UndeclaredEffect { .. } => "UndeclaredEffect",
        TypeError::UnknownEffectOperation { .. } => "UnknownEffectOperation",
        TypeError::MissingHandlerClause { .. } => "MissingHandlerClause",
        _ => "TypeError",
    }
}

fn language_version_string() -> String {
    format!(
        "{}.{}.{}",
        quantalang::LANGUAGE_VERSION.0,
        quantalang::LANGUAGE_VERSION.1,
        quantalang::LANGUAGE_VERSION.2
    )
}

fn source_digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write to string");
    }
    hex
}

fn input_graph_digest(records: &[CheckReceiptInputDigest]) -> CheckReceiptSourceDigest {
    let mut hasher = Sha256::new();
    for record in records {
        hasher.update(record.role.as_bytes());
        hasher.update([0]);
        hasher.update(record.digest.algorithm.as_bytes());
        hasher.update([0]);
        hasher.update(record.digest.hex.as_bytes());
        hasher.update([10]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write to string");
    }
    CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex,
    }
}

fn load_check_policy(path: &Path) -> Result<LoadedCheckPolicy, i32> {
    let bytes = std::fs::read(path).map_err(|err| {
        eprintln!("Error reading policy '{}': {}", path.display(), err);
        1
    })?;
    let source_digest = CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(&bytes),
    };
    let profile: CheckPolicyProfile = serde_json::from_slice(&bytes).map_err(|err| {
        eprintln!("Error parsing policy '{}': {}", path.display(), err);
        1
    })?;
    if profile.schema != "quantalang-check-policy/v1" {
        eprintln!("Unsupported check policy schema '{}'", profile.schema);
        return Err(1);
    }

    Ok(LoadedCheckPolicy {
        source: path.to_string_lossy().to_string(),
        source_digest,
        builtin_profile: None,
        builtin_profile_digest: None,
        profile,
    })
}

fn load_builtin_check_policy(name: &str) -> Result<LoadedCheckPolicy, i32> {
    let json = builtin_policy_json(name).ok_or_else(|| {
        eprintln!(
            "Unknown built-in policy profile '{}'. Available: {}",
            name,
            builtin_policy_names()
        );
        1
    })?;
    let source_digest = CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(json.as_bytes()),
    };
    let profile: CheckPolicyProfile = serde_json::from_str(&json).map_err(|err| {
        eprintln!("Error parsing built-in policy profile '{}': {}", name, err);
        1
    })?;
    if profile.schema != "quantalang-check-policy/v1" {
        eprintln!("Unsupported check policy schema '{}'", profile.schema);
        return Err(1);
    }

    Ok(LoadedCheckPolicy {
        source: format!("builtin:{name}"),
        source_digest: source_digest.clone(),
        builtin_profile: Some(name.to_string()),
        builtin_profile_digest: Some(source_digest),
        profile,
    })
}

fn check_policy_status(decision: &CheckPolicyDecision) -> &'static str {
    if decision.violations.is_empty() {
        "passed"
    } else {
        "failed"
    }
}

fn allowlist_allows(
    allowlist: &BTreeMap<String, Vec<String>>,
    effect: &str,
    function: &str,
) -> bool {
    allowlist
        .get(effect)
        .map(|functions| functions.iter().any(|allowed| allowed == function))
        .unwrap_or(true)
}

fn digest_is_sha256_hex(digest: &CheckReceiptSourceDigest) -> bool {
    digest.algorithm == "sha256"
        && digest.hex.len() == 64
        && digest.hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn collect_check_policy_evidence(outcome: &CheckOutcome) -> BTreeSet<CheckPolicyEvidence> {
    let mut evidence = BTreeSet::new();
    for summary in &outcome.function_summaries {
        for effect in &summary.declared_effects {
            evidence.insert(CheckPolicyEvidence {
                function: summary.function.clone(),
                effect: effect.clone(),
                surface: "declared_effects",
                source: String::new(),
            });
        }
        for (effect, sources) in &summary.observed_capabilities {
            for source in sources {
                evidence.insert(CheckPolicyEvidence {
                    function: summary.function.clone(),
                    effect: effect.clone(),
                    surface: "observed_capabilities",
                    source: source.clone(),
                });
            }
        }
        for (effect, sources) in &summary.propagated_effects {
            for source in sources {
                evidence.insert(CheckPolicyEvidence {
                    function: summary.function.clone(),
                    effect: effect.clone(),
                    surface: "propagated_effects",
                    source: source.clone(),
                });
            }
        }
    }
    evidence
}

fn evaluate_check_policy(
    policy: &LoadedCheckPolicy,
    outcome: &CheckOutcome,
) -> CheckPolicyDecision {
    let allowed: BTreeSet<&str> = policy
        .profile
        .allowed_effects
        .iter()
        .map(String::as_str)
        .collect();
    let denied: BTreeSet<&str> = policy
        .profile
        .denied_effects
        .iter()
        .map(String::as_str)
        .collect();
    let mut violations = BTreeSet::new();

    if policy.profile.require_source_digest && outcome.source_digest.algorithm != "sha256" {
        violations.insert(CheckPolicyViolation {
            kind: "MissingSourceDigest",
            effect: String::new(),
            function: String::new(),
            surface: "source_digest",
            source: String::new(),
            message: "policy requires sha256 source digest".to_string(),
        });
    }

    if policy.profile.require_input_graph_digest
        && !digest_is_sha256_hex(&outcome.input_graph_digest)
    {
        violations.insert(CheckPolicyViolation {
            kind: "MissingInputGraphDigest",
            effect: String::new(),
            function: outcome.source.clone(),
            surface: "input_graph_digest",
            source: String::new(),
            message: "policy requires a valid sha256 input graph digest".to_string(),
        });
    }

    for item in collect_check_policy_evidence(outcome) {
        if denied.contains(item.effect.as_str()) {
            violations.insert(CheckPolicyViolation {
                kind: "DeniedEffect",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!("policy denies effect `{}`", item.effect),
            });
        } else if !allowed.is_empty() && !allowed.contains(item.effect.as_str()) {
            violations.insert(CheckPolicyViolation {
                kind: "DisallowedEffect",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!("policy does not allow effect `{}`", item.effect),
            });
        } else if item.surface == "observed_capabilities"
            && !allowlist_allows(
                &policy.profile.direct_effect_allowlist,
                &item.effect,
                &item.function,
            )
        {
            violations.insert(CheckPolicyViolation {
                kind: "DirectEffectNotAllowed",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!(
                    "effect `{}` is directly used by `{}` via `{}` but policy does not allow that boundary",
                    item.effect, item.function, item.source
                ),
            });
        } else if item.surface == "propagated_effects"
            && !allowlist_allows(
                &policy.profile.propagated_effect_allowlist,
                &item.effect,
                &item.function,
            )
        {
            violations.insert(CheckPolicyViolation {
                kind: "PropagatedEffectNotAllowed",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                source: item.source.clone(),
                message: format!(
                    "effect `{}` is propagated into `{}` via `{}` but policy does not allow that caller",
                    item.effect, item.function, item.source
                ),
            });
        }
    }

    CheckPolicyDecision {
        schema: policy.profile.schema.clone(),
        source: policy.source.clone(),
        source_digest: policy.source_digest.clone(),
        builtin_profile: policy.builtin_profile.clone(),
        builtin_profile_digest: policy.builtin_profile_digest.clone(),
        violations: violations.into_iter().collect(),
    }
}

fn build_check_receipt(
    outcome: &CheckOutcome,
    policy: Option<&CheckPolicyDecision>,
) -> CheckReceipt {
    let mut declared_effects = BTreeMap::new();
    let mut observed_capabilities = BTreeMap::new();
    let mut propagated_effects = BTreeMap::new();

    for summary in &outcome.function_summaries {
        declared_effects.insert(summary.function.clone(), summary.declared_effects.clone());
        let mut capabilities = BTreeMap::new();
        for (effect, sources) in &summary.observed_capabilities {
            capabilities.insert(effect.clone(), sources.iter().cloned().collect::<Vec<_>>());
        }
        observed_capabilities.insert(summary.function.clone(), capabilities);

        let mut propagated = BTreeMap::new();
        for (effect, sources) in &summary.propagated_effects {
            propagated.insert(effect.clone(), sources.iter().cloned().collect::<Vec<_>>());
        }
        propagated_effects.insert(summary.function.clone(), propagated);
    }

    let mut diagnostics = Vec::new();
    diagnostics.extend(
        outcome
            .parse_errors
            .iter()
            .map(|message| CheckReceiptDiagnostic {
                stage: "parse",
                kind: "ParseError".to_string(),
                message: message.clone(),
                help: None,
                notes: Vec::new(),
            }),
    );
    diagnostics.extend(
        outcome
            .type_errors
            .iter()
            .map(|err| CheckReceiptDiagnostic {
                stage: "type",
                kind: type_error_kind(&err.error).to_string(),
                message: err.error.to_string(),
                help: err.help.clone(),
                notes: err.notes.clone(),
            }),
    );

    let policy_failed = policy
        .map(|decision| !decision.violations.is_empty())
        .unwrap_or(false);
    let receipt_policy = policy.map(|decision| CheckReceiptPolicy {
        schema: decision.schema.clone(),
        source: decision.source.clone(),
        source_digest: decision.source_digest.clone(),
        profile: decision.builtin_profile.clone(),
        profile_digest: decision.builtin_profile_digest.clone(),
        status: check_policy_status(decision),
        violations: decision.violations.clone(),
    });

    CheckReceipt {
        schema: "quantalang-check-receipt/v1",
        compiler: "quantac",
        compiler_version: outcome.compiler_version,
        language_version: outcome.language_version.clone(),
        source: outcome.source.clone(),
        source_digest: outcome.source_digest.clone(),
        input_graph_digest: outcome.input_graph_digest.clone(),
        input_digests: outcome.input_digests.clone(),
        status: if diagnostics.is_empty() && !policy_failed {
            "passed"
        } else {
            "failed"
        },
        items: outcome.items,
        tokens: outcome.tokens,
        declared_effects,
        observed_capabilities,
        propagated_effects,
        diagnostics,
        policy: receipt_policy,
    }
}

fn run_check(file: &Path) -> Result<CheckOutcome, i32> {
    let mut input_digest_ledger = InputDigestLedger::default();
    let source_bytes = std::fs::read(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;
    input_digest_ledger.record("entry", file, &source_bytes);
    let source_digest = CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: source_digest_hex(&source_bytes),
    };
    let source = String::from_utf8(source_bytes).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    let source = resolve_imports_recording_inputs(&source, file, &mut input_digest_ledger)?;
    let chk_base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes_recording_inputs(&source, chk_base, &mut input_digest_ledger)?;
    let source_file = SourceFile::new(file.to_string_lossy(), source);

    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;
    let token_count = tokens.len();

    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().unwrap();
    let parse_errors = parser
        .errors()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let item_count = ast.items.len();

    resolve_modules_recording_inputs(&mut ast, chk_base, &mut input_digest_ledger)?;

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_dir(chk_base.to_path_buf());
    checker.check_module(&ast);

    let input_digests = input_digest_ledger.into_sorted_records();
    let input_graph_digest = input_graph_digest(&input_digests);

    Ok(CheckOutcome {
        source: file.to_string_lossy().to_string(),
        compiler_version: quantalang::VERSION,
        language_version: language_version_string(),
        source_digest,
        input_graph_digest,
        input_digests,
        items: item_count,
        tokens: token_count,
        parse_errors,
        type_errors: checker.errors().to_vec(),
        function_summaries: checker.function_effect_summaries().to_vec(),
    })
}

fn render_check_line(receipt_to_stdout: bool, message: impl AsRef<str>) {
    if receipt_to_stdout {
        eprintln!("{}", message.as_ref());
    } else {
        println!("{}", message.as_ref());
    }
}

fn render_check_human_output(outcome: &CheckOutcome, receipt_to_stdout: bool) {
    render_check_line(
        receipt_to_stdout,
        format!("Lexing... OK ({} tokens)", outcome.tokens),
    );
    if outcome.parse_errors.is_empty() {
        render_check_line(
            receipt_to_stdout,
            format!("Parsing... OK ({} items)", outcome.items),
        );
    } else {
        render_check_line(
            receipt_to_stdout,
            format!(
                "Parsing... {} items ({} parse errors)",
                outcome.items,
                outcome.parse_errors.len()
            ),
        );
    }

    if !outcome.parse_errors.is_empty() {
        eprintln!("Parse errors:");
        for err in &outcome.parse_errors {
            eprintln!("  {}", err);
        }
    }
    if !outcome.type_errors.is_empty() {
        eprintln!("Type errors found:");
        for err in &outcome.type_errors {
            eprintln!("  {}", err);
        }
    }

    if outcome.parse_errors.is_empty() && outcome.type_errors.is_empty() {
        render_check_line(receipt_to_stdout, "Type checking... OK");
        render_check_line(receipt_to_stdout, "");
        render_check_line(
            receipt_to_stdout,
            format!("No errors found in '{}'", outcome.source),
        );
    }
}

fn write_check_receipt(path: &Path, receipt: &CheckReceipt) -> Result<(), i32> {
    let json = serde_json::to_string_pretty(receipt).map_err(|err| {
        eprintln!("Error serializing check receipt: {}", err);
        1
    })?;
    if path == Path::new("-") {
        println!("{}", json);
        Ok(())
    } else {
        std::fs::write(path, format!("{}\n", json)).map_err(|err| {
            eprintln!("Error writing check receipt '{}': {}", path.display(), err);
            1
        })
    }
}

fn render_check_policy_output(policy: Option<&CheckPolicyDecision>) {
    let Some(policy) = policy else {
        return;
    };
    for violation in &policy.violations {
        let target = if violation.function.is_empty() {
            violation.surface.to_string()
        } else {
            format!("{} in {}", violation.surface, violation.function)
        };
        eprintln!("Policy violation: {} ({})", violation.message, target);
    }
}

fn cmd_check(
    file: &Path,
    receipt: Option<&Path>,
    policy: Option<&Path>,
    profile: Option<&str>,
    expect_profile_digest: Option<&str>,
) -> Result<(), i32> {
    let receipt_to_stdout = receipt == Some(Path::new("-"));
    if policy.is_some() && profile.is_some() {
        eprintln!("Error: --policy and --profile cannot be used together");
        return Err(1);
    }
    if expect_profile_digest.is_some() && profile.is_none() {
        eprintln!("Error: --expect-profile-digest requires --profile");
        return Err(1);
    }
    let loaded_policy = if let Some(policy) = policy {
        Some(load_check_policy(policy)?)
    } else if let Some(profile) = profile {
        Some(load_builtin_check_policy(profile)?)
    } else {
        None
    };
    if let Some(expected_digest) = expect_profile_digest {
        let profile_name = profile.expect("profile is required for digest pinning");
        let actual_digest = loaded_policy
            .as_ref()
            .and_then(|policy| policy.builtin_profile_digest.as_ref())
            .expect("built-in profile digest is present");
        let expected_hex = normalize_profile_digest_pin(expected_digest);
        if !actual_digest.hex.eq_ignore_ascii_case(expected_hex) {
            eprintln!(
                "Error: Built-in policy profile digest mismatch for '{}': expected sha256:{}, actual sha256:{}",
                profile_name, expected_hex, actual_digest.hex
            );
            return Err(1);
        }
    }
    let outcome = run_check(file)?;
    let policy_decision = loaded_policy
        .as_ref()
        .map(|policy| evaluate_check_policy(policy, &outcome));
    let receipt_value = receipt.map(|_| build_check_receipt(&outcome, policy_decision.as_ref()));

    render_check_human_output(&outcome, receipt_to_stdout);
    render_check_policy_output(policy_decision.as_ref());
    if let Some(receipt_value) = receipt_value {
        write_check_receipt(receipt.expect("receipt path is present"), &receipt_value)?;
    }

    let policy_passed = policy_decision
        .as_ref()
        .map(|decision| decision.violations.is_empty())
        .unwrap_or(true);
    if outcome.parse_errors.is_empty() && outcome.type_errors.is_empty() && policy_passed {
        Ok(())
    } else {
        Err(1)
    }
}

// =============================================================================
// C COMPILER DISCOVERY AND INVOCATION
// =============================================================================

/// Try to locate a working C compiler on the system.
///
/// On Windows: tries `cl.exe` (MSVC), then `gcc`, then `clang`.
/// On Unix: tries `cc`, then `gcc`, then `clang`.
///
/// Returns the compiler command name if found.
fn find_c_compiler() -> Option<String> {
    // First: try compilers already in PATH
    let candidates: &[&str] = if cfg!(windows) {
        &["cl.exe", "cl", "gcc", "clang"]
    } else {
        &["cc", "gcc", "clang"]
    };

    for &compiler in candidates {
        let probe = std::process::Command::new(compiler)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        let ok = match probe {
            Ok(status) => status.success(),
            Err(_) if compiler.starts_with("cl") => std::process::Command::new(compiler)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|_| true)
                .unwrap_or(false),
            Err(_) => false,
        };

        if ok {
            return Some(compiler.to_string());
        }
    }

    // Second (Windows only): auto-discover MSVC from Visual Studio BuildTools
    #[cfg(windows)]
    {
        if let Some(cl_path) = find_msvc_cl() {
            return Some(cl_path);
        }
    }

    None
}

/// Find vcvarsall.bat from Visual Studio installation.
#[cfg(windows)]
#[allow(dead_code)]
fn find_vcvars_bat() -> Option<String> {
    let vs_roots = [
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools",
        r"C:\Program Files\Microsoft Visual Studio\2022\BuildTools",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Community",
        r"C:\Program Files\Microsoft Visual Studio\2022\Community",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise",
    ];

    for vs_root in &vs_roots {
        let vcvars = std::path::PathBuf::from(vs_root).join(r"VC\Auxiliary\Build\vcvarsall.bat");
        if vcvars.is_file() {
            return Some(vcvars.to_string_lossy().to_string());
        }
    }
    None
}

/// Auto-discover MSVC cl.exe from Visual Studio BuildTools installation.
/// Searches common install paths and sets INCLUDE/LIB/PATH environment
/// variables so cl.exe can find headers and libraries.
#[cfg(windows)]
fn find_msvc_cl() -> Option<String> {
    use std::path::PathBuf;

    let vs_roots = [
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools",
        r"C:\Program Files\Microsoft Visual Studio\2022\BuildTools",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Community",
        r"C:\Program Files\Microsoft Visual Studio\2022\Community",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional",
        r"C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise",
    ];

    for vs_root in &vs_roots {
        let vc_tools = PathBuf::from(vs_root).join(r"VC\Tools\MSVC");
        if !vc_tools.is_dir() {
            continue;
        }

        // Find the latest MSVC version directory
        let mut versions: Vec<_> = std::fs::read_dir(&vc_tools)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        versions.sort();

        let msvc_ver = versions.last()?;
        let msvc_dir = vc_tools.join(msvc_ver);
        let cl_exe = msvc_dir.join(r"bin\Hostx64\x64\cl.exe");

        if !cl_exe.is_file() {
            continue;
        }

        // Find Windows SDK
        let sdk_root = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10");
        let sdk_include = sdk_root.join("Include");
        let sdk_lib = sdk_root.join("Lib");

        // Find latest SDK version
        let sdk_ver = if sdk_include.is_dir() {
            let mut sdk_versions: Vec<_> = std::fs::read_dir(&sdk_include)
                .ok()
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            sdk_versions.sort();
            sdk_versions.last().cloned().unwrap_or_default()
        } else {
            String::new()
        };

        // Set INCLUDE
        let msvc_include = msvc_dir.join("include");
        let ucrt_include = sdk_include.join(&sdk_ver).join("ucrt");
        let um_include = sdk_include.join(&sdk_ver).join("um");
        let shared_include = sdk_include.join(&sdk_ver).join("shared");

        let include_path = format!(
            "{};{};{};{}",
            msvc_include.display(),
            ucrt_include.display(),
            um_include.display(),
            shared_include.display(),
        );

        // Set LIB
        let msvc_lib = msvc_dir.join(r"lib\x64");
        let ucrt_lib = sdk_lib.join(&sdk_ver).join(r"ucrt\x64");
        let um_lib = sdk_lib.join(&sdk_ver).join(r"um\x64");

        let lib_path = format!(
            "{};{};{}",
            msvc_lib.display(),
            ucrt_lib.display(),
            um_lib.display(),
        );

        // Set PATH to include the bin directory
        let bin_dir = msvc_dir.join(r"bin\Hostx64\x64");
        let current_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{};{}", bin_dir.display(), current_path);

        // Apply environment variables globally for this process.
        // This ensures cl.exe can find headers and libraries when invoked.
        std::env::set_var("INCLUDE", &include_path);
        std::env::set_var("LIB", &lib_path);
        std::env::set_var("PATH", &new_path);

        // Also store the paths for explicit use by invoke_c_compiler
        std::env::set_var("QUANTALANG_MSVC_INCLUDE", &include_path);
        std::env::set_var("QUANTALANG_MSVC_LIB", &lib_path);
        std::env::set_var("QUANTALANG_MSVC_BIN", bin_dir.to_string_lossy().as_ref());

        return Some(cl_exe.to_string_lossy().to_string());
    }

    None
}

/// Build the argument list for the chosen C compiler and invoke it.
///
/// `c_file`  - path to the generated `.c` source
/// `exe_file` - desired output executable path
/// `release` - if true, pass `-O2`; otherwise pass `-g`
/// `compiler` - the C compiler command (e.g. "gcc", "cl.exe")
///
/// Returns `Ok(())` on success, `Err(code)` on failure.
fn invoke_c_compiler(
    compiler: &str,
    c_file: &std::path::Path,
    exe_file: &std::path::Path,
    release: bool,
) -> Result<(), i32> {
    let is_msvc =
        compiler.starts_with("cl") || compiler.ends_with("cl.exe") || compiler.ends_with("cl");

    let mut cmd = std::process::Command::new(compiler);

    if is_msvc {
        // On Windows, write a temporary .bat file that sets the MSVC
        // environment and calls cl.exe. This avoids quoting issues
        // with PowerShell and cmd.exe invocations.
        let c_path = c_file.to_string_lossy().replace('/', "\\");
        let _exe_path = exe_file.to_string_lossy().replace('/', "\\");
        let opt_flag = if release { "/O2" } else { "/Zi" };

        if let (Ok(inc), Ok(lib), Ok(bin)) = (
            std::env::var("QUANTALANG_MSVC_INCLUDE"),
            std::env::var("QUANTALANG_MSVC_LIB"),
            std::env::var("QUANTALANG_MSVC_BIN"),
        ) {
            let bat_path = c_file.with_extension("bat");
            let exe_path = exe_file.to_string_lossy().replace('/', "\\");
            // Write bat file with MSVC env setup and compilation
            let bat_content = format!(
                "set \"INCLUDE={}\"\r\nset \"LIB={}\"\r\nset \"PATH={};%PATH%\"\r\ncl.exe /nologo /W0 /std:c11 {} \"{}\" /Fe\"{}\" {} 1>&2\r\n",
                inc,
                lib,
                bin,
                opt_flag,
                c_path,
                exe_path,
                host_c_link_libraries(true).join(" ")
            );
            std::fs::write(&bat_path, &bat_content).map_err(|e| {
                eprintln!("Failed to write build script: {}", e);
                1
            })?;

            cmd = std::process::Command::new("cmd.exe");
            cmd.args(&["/C", &bat_path.to_string_lossy().replace('/', "\\")]);
            if let Some(parent) = c_file.parent() {
                cmd.current_dir(parent);
            }
        } else {
            // Direct invocation fallback
            cmd.arg(c_file);
            cmd.arg(format!("/Fe:{}", exe_file.display()));
            cmd.arg("/std:c11");
            if release {
                cmd.arg("/O2");
            } else {
                cmd.arg("/Zi");
            }
            cmd.arg("/nologo");
            cmd.arg("/W0");
            cmd.args(host_c_link_libraries(true));
        }
    } else {
        // GCC / Clang / cc - POSIX-style flags
        cmd.arg(c_file);
        cmd.arg("-o");
        cmd.arg(exe_file);
        cmd.arg("-std=c99");
        if release {
            cmd.arg("-O2");
        } else {
            cmd.arg("-g");
        }
        cmd.args(host_c_link_libraries(false));
    }

    let output = cmd.output().map_err(|e| {
        eprintln!("Failed to invoke C compiler '{}': {}", compiler, e);
        1
    })?;

    if output.status.success() {
        if !exe_file.exists() {
            eprintln!(
                "Warning: C compiler succeeded but executable not found at {}",
                exe_file.display()
            );
        }
        Ok(())
    } else {
        eprintln!(
            "C compilation failed (exit code: {:?}):",
            output.status.code()
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            eprintln!("{}", stderr);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            eprintln!("{}", stdout);
        }
        Err(1)
    }
}

fn host_c_link_libraries(is_msvc: bool) -> &'static [&'static str] {
    c_link_libraries(std::env::consts::OS, is_msvc)
}

fn c_link_libraries(target_os: &str, is_msvc: bool) -> &'static [&'static str] {
    match (target_os, is_msvc) {
        ("windows", true) => &["ws2_32.lib"],
        ("windows", false) => &["-lws2_32"],
        (_, false) => &["-lm"],
        _ => &[],
    }
}

// =============================================================================
// BUILD COMMAND
// =============================================================================

fn cmd_build(
    path: &PathBuf,
    release: bool,
    emit: &str,
    keep_c: bool,
    target_str: &str,
) -> Result<(), i32> {
    // Look for Quanta.toml or main.quanta in the project directory
    let manifest_path = path.join("Quanta.toml");
    let main_path = if manifest_path.exists() {
        // Read manifest to find entry point
        path.join("src").join("main.quanta")
    } else {
        // Look for main.quanta directly
        let main_file = path.join("main.quanta");
        if main_file.exists() {
            main_file
        } else {
            path.join("src").join("main.quanta")
        }
    };

    if !main_path.exists() {
        eprintln!("Could not find entry point. Expected one of:");
        eprintln!("  - {}/main.quanta", path.display());
        eprintln!("  - {}/src/main.quanta", path.display());
        return Err(1);
    }

    let emit_c_only = emit == "c";

    // Resolve the code generation target.
    let target = parse_codegen_target(target_str).map_err(|err| {
        eprintln!("{}", err);
        1
    })?;
    let use_llvm = target == Target::LlvmIr;
    let use_spirv = target == Target::SpirV;
    let use_native = target == Target::X86_64 || target == Target::Arm64;
    let use_wasm = target == Target::Wasm;
    let use_shader = target == Target::Hlsl || target == Target::Glsl;
    let use_rust = target == Target::Rust;

    println!("Building project at '{}'", path.display());
    println!("Entry point: {}", main_path.display());
    println!("Mode: {}", if release { "release" } else { "debug" });
    println!("Target: {}", target);
    if emit_c_only && !use_llvm {
        println!("Emit: C source only");
    }
    println!();

    // Read source file
    let source = std::fs::read_to_string(&main_path).map_err(|e| {
        eprintln!("Error reading file '{}': {}", main_path.display(), e);
        1
    })?;

    // Resolve `// import <pkg>` and `use <pkg>;` directives
    let source = resolve_imports(&source, &main_path)?;

    // Expand `include!("path")` directives
    let inc_base = main_path.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, inc_base)?;

    let source_file = SourceFile::new(main_path.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    let total_steps =
        if emit_c_only || use_llvm || use_native || use_wasm || use_spirv || use_shader || use_rust
        {
            4
        } else {
            5
        };
    println!("[1/{}] Lexing... OK ({} tokens)", total_steps, tokens.len());

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;
    println!(
        "[2/{}] Parsing... OK ({} items)",
        total_steps,
        ast.items.len()
    );

    // Resolve `mod foo;` declarations - load and merge external module files
    let source_dir = main_path.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_dir(source_dir.to_path_buf());
    checker.check_module(&ast);

    if checker.has_errors() {
        eprintln!("Type errors found:");
        for err in checker.errors() {
            eprintln!("  {}", err);
        }
        return Err(1);
    }
    println!("[3/{}] Type checking... OK", total_steps);

    // Code generation - pass source for macro string extraction
    let mut codegen = CodeGenerator::with_source(&ctx, target, Arc::from(source_file.source()));
    let output = codegen.generate(&ast).map_err(|e| {
        eprintln!("Code generation error: {}", e);
        1
    })?;
    println!(
        "[4/{}] Code generation ({})... OK ({} bytes)",
        total_steps,
        target,
        output.data.len()
    );

    // Write output
    let output_dir = path
        .join("target")
        .join(if release { "release" } else { "debug" });
    std::fs::create_dir_all(&output_dir).map_err(|e| {
        eprintln!("Failed to create output directory: {}", e);
        1
    })?;

    if use_spirv {
        // SPIR-V target: write .spv binary
        let spv_output_file = output_dir.join("main.spv");
        std::fs::write(&spv_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write SPIR-V output: {}", e);
            1
        })?;
        println!("[5/5] SPIR-V written to {}", spv_output_file.display());
        println!();
        println!("Validate with: spirv-val {}", spv_output_file.display());
        return Ok(());
    } else if use_native {
        // x86-64 / ARM64 target: write assembly file
        let ext = if target == Target::X86_64 {
            "x86_64.s"
        } else {
            "aarch64.s"
        };
        let asm_output_file = output_dir.join(format!("main.{}", ext));
        std::fs::write(&asm_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write assembly output: {}", e);
            1
        })?;

        if !emit_c_only {
            // Try to assemble + link with system tools
            let assembler = if target == Target::X86_64 {
                if cfg!(windows) {
                    "ml64"
                } else {
                    "as"
                }
            } else {
                "aarch64-linux-gnu-as"
            };

            let asm_ok = std::process::Command::new(assembler)
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if asm_ok {
                println!("[5/5] Assembling {} -> executable...", ext);
                // For now, output the assembly; full linking requires platform-specific logic
                println!();
                println!("Build successful! (assembly output)");
                println!("Output: {}", asm_output_file.display());
                println!();
                if target == Target::X86_64 {
                    if cfg!(windows) {
                        println!("To link: ml64 /Fe:main.exe {}", asm_output_file.display());
                    } else {
                        println!("To assemble and link:");
                        println!(
                            "  as {} -o main.o && ld main.o -o main -lc",
                            asm_output_file.display()
                        );
                    }
                } else {
                    println!("To cross-compile:");
                    println!("  aarch64-linux-gnu-as {} -o main.o && aarch64-linux-gnu-ld main.o -o main -lc", asm_output_file.display());
                }
                return Ok(());
            }

            println!();
            println!("Build successful! (assembly only - no assembler found)");
            println!("Output: {}", asm_output_file.display());
            return Ok(());
        }

        println!();
        println!("Build successful!");
        println!("Output: {}", asm_output_file.display());
        return Ok(());
    } else if use_shader {
        // HLSL/GLSL target: write shader source file
        let (ext, label) = if target == Target::Hlsl {
            ("hlsl", "HLSL")
        } else {
            ("glsl", "GLSL")
        };
        let shader_output_file = output_dir.join(format!("main.{}", ext));
        std::fs::write(&shader_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write {} output: {}", label, e);
            1
        })?;
        println!();
        println!("Build successful!");
        println!("Output: {} ({})", shader_output_file.display(), label);
        return Ok(());
    } else if use_wasm {
        // WebAssembly target: write .wasm binary
        let wasm_output_file = output_dir.join("main.wasm");
        std::fs::write(&wasm_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write WebAssembly output: {}", e);
            1
        })?;

        // Try running with wasmtime if available
        if !emit_c_only {
            let wt_ok = std::process::Command::new("wasmtime")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if wt_ok {
                println!("[5/5] WebAssembly module ready (wasmtime available)");
                println!();
                println!("Build successful!");
                println!("Output: {}", wasm_output_file.display());
                println!();
                println!("Run with: wasmtime {}", wasm_output_file.display());
                return Ok(());
            }
        }

        println!();
        println!("Build successful!");
        println!("Output: {}", wasm_output_file.display());
        println!();
        println!("Run with: wasmtime {}", wasm_output_file.display());
        return Ok(());
    } else if use_llvm {
        // LLVM IR target: write .ll file
        let ll_output_file = output_dir.join("main.ll");
        std::fs::write(&ll_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write LLVM IR output: {}", e);
            1
        })?;

        // If --emit=exe (default), try to compile the .ll to an executable with clang
        if !emit_c_only {
            let exe_name = if cfg!(windows) { "main.exe" } else { "main" };
            let exe_output_file = output_dir.join(exe_name);

            // Check if clang is available
            let clang_ok = std::process::Command::new("clang")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if clang_ok {
                println!("[5/5] Compiling LLVM IR -> executable (using clang)...");

                let mut cmd = std::process::Command::new("clang");
                cmd.arg(&ll_output_file);
                cmd.arg("-o");
                cmd.arg(&exe_output_file);
                if release {
                    cmd.arg("-O2");
                } else {
                    cmd.arg("-g");
                }
                if !cfg!(windows) {
                    cmd.arg("-lm");
                }

                let clang_output = cmd.output().map_err(|e| {
                    eprintln!("Failed to invoke clang: {}", e);
                    1
                })?;

                if clang_output.status.success() {
                    println!("     Compilation... OK");
                    println!();
                    println!("Build successful!");
                    println!("Output: {}", exe_output_file.display());
                    return Ok(());
                } else {
                    eprintln!("clang compilation failed:");
                    let stderr = String::from_utf8_lossy(&clang_output.stderr);
                    if !stderr.is_empty() {
                        eprintln!("{}", stderr);
                    }
                    return Err(1);
                }
            } else {
                println!();
                println!("Build successful! (LLVM IR only)");
                println!("Output: {}", ll_output_file.display());
                println!();
                if cfg!(windows) {
                    println!("To compile to executable, install clang and run:");
                    println!(
                        "  clang {} -o {}",
                        ll_output_file.display(),
                        output_dir.join("main.exe").display()
                    );
                } else {
                    println!("To compile to executable, install clang and run:");
                    println!(
                        "  clang {} -o {} -lm",
                        ll_output_file.display(),
                        output_dir.join("main").display()
                    );
                }
                return Ok(());
            }
        }

        println!();
        println!("Build successful!");
        println!("Output: {}", ll_output_file.display());
        return Ok(());
    } else if use_rust {
        let rs_output_file = output_dir.join("main.rs");
        std::fs::write(&rs_output_file, &output.data).map_err(|e| {
            eprintln!("Failed to write Rust output: {}", e);
            1
        })?;
        println!();
        println!("Build successful!");
        println!("Output: {} (Rust source)", rs_output_file.display());
        println!(
            "Validate with: rustc --emit=metadata {}",
            rs_output_file.display()
        );
        return Ok(());
    }

    // C target path
    let c_output_file = output_dir.join("main.c");
    std::fs::write(&c_output_file, &output.data).map_err(|e| {
        eprintln!("Failed to write C output: {}", e);
        1
    })?;

    // If --emit=c, stop here
    if emit_c_only {
        println!();
        println!("Build successful!");
        println!("Output: {}", c_output_file.display());
        return Ok(());
    }

    // Otherwise compile the .c file to an executable
    let exe_name = if cfg!(windows) { "main.exe" } else { "main" };
    let exe_output_file = output_dir.join(exe_name);

    let compiler = find_c_compiler().ok_or_else(|| {
        eprintln!("Error: No C compiler found on the system.");
        eprintln!("QuantaLang needs a C compiler to produce executables.");
        eprintln!();
        if cfg!(windows) {
            eprintln!("Install one of the following:");
            eprintln!("  - Visual Studio Build Tools (cl.exe): https://visualstudio.microsoft.com/downloads/");
            eprintln!("  - MinGW-w64 (gcc): https://www.mingw-w64.org/");
            eprintln!("  - LLVM/Clang: https://releases.llvm.org/");
        } else {
            eprintln!("Install one of the following:");
            eprintln!("  - GCC: sudo apt install gcc  (Debian/Ubuntu)");
            eprintln!("  - Clang: sudo apt install clang");
        }
        eprintln!();
        eprintln!("Or use --emit=c to output only the C source file.");
        1
    })?;

    println!(
        "[5/{}] Compiling C -> executable (using {})...",
        total_steps, compiler
    );

    invoke_c_compiler(&compiler, &c_output_file, &exe_output_file, release)?;

    println!("     Compilation... OK");

    // Clean up .c file unless --keep-c
    if !keep_c {
        let _ = std::fs::remove_file(&c_output_file);
    }

    println!();
    println!("Build successful!");
    println!("Output: {}", exe_output_file.display());

    Ok(())
}

// =============================================================================
// RUN COMMAND
// =============================================================================

static RUN_TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn sanitize_temp_component(component: &str) -> String {
    let sanitized: String = component
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() {
        "program".to_string()
    } else {
        sanitized
    }
}

fn run_temp_build_dir(file: &Path) -> PathBuf {
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(sanitize_temp_component)
        .unwrap_or_else(|| "program".to_string());
    let counter = RUN_TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    std::env::temp_dir().join(format!(
        "quantalang_run_{}_{}_{}_{}",
        stem,
        std::process::id(),
        nanos,
        counter
    ))
}

fn cmd_run(file: &PathBuf, args: &[String]) -> Result<(), i32> {
    // Read source file
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    // Resolve `// import <pkg>` and `use <pkg>;` directives
    let source = resolve_imports(&source, file)?;

    // Expand `include!("path")` directives
    let run_base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, run_base)?;

    let source_file = SourceFile::new(file.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;

    // Resolve `mod foo;` declarations - load and merge external module files
    let source_dir = file.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_dir(source_dir.to_path_buf());
    checker.check_module(&ast);

    if checker.has_errors() {
        for err in checker.errors() {
            eprintln!("Type error: {}", err);
        }
        return Err(1);
    }

    // Generate C code - pass source for macro string extraction
    let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source_file.source()));
    let output = codegen.generate(&ast).map_err(|e| {
        eprintln!("Code generation error: {}", e);
        1
    })?;

    // Write to temp file
    let temp_dir = run_temp_build_dir(file);
    std::fs::create_dir_all(&temp_dir).map_err(|e| {
        eprintln!("Failed to create temp directory: {}", e);
        1
    })?;

    let c_file = temp_dir.join("main.c");
    let exe_file = if cfg!(windows) {
        temp_dir.join("main.exe")
    } else {
        temp_dir.join("main")
    };

    std::fs::write(&c_file, &output.data).map_err(|e| {
        eprintln!("Failed to write temp file: {}", e);
        1
    })?;

    // Find and invoke C compiler
    let compiler = find_c_compiler().ok_or_else(|| {
        eprintln!("Error: No C compiler found on the system.");
        eprintln!("QuantaLang needs a C compiler to compile and run programs.");
        eprintln!();
        if cfg!(windows) {
            eprintln!("Install one of: cl.exe (MSVC), gcc (MinGW), or clang");
        } else {
            eprintln!("Install one of: cc, gcc, or clang");
        }
        1
    })?;

    invoke_c_compiler(&compiler, &c_file, &exe_file, false)?;

    // Verify the executable was created
    if !exe_file.exists() {
        eprintln!(
            "Error: C compilation reported success but executable not found at '{}'",
            exe_file.display()
        );
        // Check if MSVC put it somewhere else (current directory)
        let alt_name = temp_dir.join("temp.exe");
        if alt_name.exists() {
            eprintln!("Found executable in current directory instead - moving it");
            let _ = std::fs::rename(alt_name, &exe_file);
        } else {
            return Err(1);
        }
    }

    // Run the compiled program directly (Win32 WriteFile in the runtime
    // ensures output works even under MinTTY/git-bash).
    let status = {
        let mut run_cmd = std::process::Command::new(&exe_file);
        run_cmd.args(args);
        run_cmd.status().map_err(|e| {
            eprintln!("Failed to run program: {}", e);
            1i32
        })?
    };

    // Clean up temp files
    let _ = std::fs::remove_dir_all(&temp_dir);

    if status.success() {
        Ok(())
    } else {
        Err(status.code().unwrap_or(1))
    }
}

fn cmd_test(
    directory: &PathBuf,
    filter: Option<&str>,
    verbose: bool,
    no_fail_fast: bool,
) -> Result<(), i32> {
    // Discover .quanta test files
    let entries: Vec<_> = match std::fs::read_dir(directory) {
        Ok(dir) => dir
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "quanta")
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            eprintln!(
                "Error reading test directory '{}': {}",
                directory.display(),
                e
            );
            return Err(1);
        }
    };

    let mut tests: Vec<PathBuf> = entries.iter().map(|e| e.path()).collect();
    tests.sort();

    // Apply filter
    if let Some(pattern) = filter {
        tests.retain(|t| {
            t.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.contains(pattern))
                .unwrap_or(false)
        });
    }

    // Only include tests that have .expected files
    let test_pairs: Vec<(PathBuf, PathBuf)> = tests
        .iter()
        .filter_map(|quanta_file| {
            let expected = quanta_file.with_extension("expected");
            if expected.exists() {
                Some((quanta_file.clone(), expected))
            } else {
                None
            }
        })
        .collect();

    let total = test_pairs.len();
    let skipped = tests.len() - total;
    if total == 0 {
        println!(
            "No tests found with .expected files in '{}'",
            directory.display()
        );
        return Ok(());
    }

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut errors = 0usize;
    let mut failures: Vec<String> = Vec::new();

    println!("running {} tests\n", total);

    for (quanta_file, expected_file) in &test_pairs {
        let name = quanta_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("???");

        // --- Compile and capture output ---
        let result = (|| -> Result<String, String> {
            let source =
                std::fs::read_to_string(quanta_file).map_err(|e| format!("read: {}", e))?;
            let source = resolve_imports(&source, quanta_file).map_err(|_| "import".to_string())?;
            let run_base = quanta_file.parent().unwrap_or(Path::new("."));
            let source =
                preprocess_includes(&source, run_base).map_err(|_| "include".to_string())?;

            let source_file = SourceFile::new(quanta_file.to_string_lossy(), source);
            let mut lexer = Lexer::new(&source_file);
            let tokens = lexer.tokenize().map_err(|e| format!("lex: {}", e))?;
            let mut parser = Parser::new(&source_file, tokens);
            let mut ast = parser.parse().map_err(|e| format!("parse: {}", e))?;

            let source_dir = quanta_file.parent().unwrap_or(Path::new("."));
            let _ = resolve_modules(&mut ast, source_dir);

            let mut ctx = TypeContext::new();
            let mut checker = TypeChecker::new(&mut ctx);
            checker.set_source_dir(source_dir.to_path_buf());
            checker.check_module(&ast);
            if checker.has_errors() {
                let errs: Vec<_> = checker.errors().iter().map(|e| e.to_string()).collect();
                return Err(format!("type: {}", errs.join("; ")));
            }

            let mut codegen =
                CodeGenerator::with_source(&ctx, Target::C, Arc::from(source_file.source()));
            let output = codegen
                .generate(&ast)
                .map_err(|e| format!("codegen: {}", e))?;

            // Use a unique temp directory per test to avoid MSVC bat conflicts
            let test_dir = std::env::temp_dir().join(format!("quantatest_{}", name));
            let _ = std::fs::create_dir_all(&test_dir);
            let c_file = test_dir.join("main.c");
            let exe_file = test_dir.join(if cfg!(windows) { "main.exe" } else { "main" });

            std::fs::write(&c_file, &output.data).map_err(|e| format!("write: {}", e))?;

            let compiler = find_c_compiler().ok_or_else(|| "no C compiler".to_string())?;
            invoke_c_compiler(&compiler, &c_file, &exe_file, false)
                .map_err(|_| "cc".to_string())?;

            // MSVC bat outputs temp.exe in the c_file directory
            if !exe_file.exists() {
                let alt = test_dir.join("temp.exe");
                if alt.exists() {
                    let _ = std::fs::rename(&alt, &exe_file);
                }
            }
            if !exe_file.exists() {
                return Err("exe not created (link failed)".to_string());
            }

            let run_output = std::process::Command::new(&exe_file)
                .output()
                .map_err(|e| format!("run: {}", e))?;

            let _ = std::fs::remove_dir_all(&test_dir);

            let stdout = String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n");
            Ok(stdout)
        })();

        match result {
            Ok(actual) => {
                let expected = std::fs::read_to_string(expected_file)
                    .unwrap_or_default()
                    .replace("\r\n", "\n");

                if actual.trim_end() == expected.trim_end() {
                    passed += 1;
                    println!("test {} ... \x1b[32mok\x1b[0m", name);
                    if verbose {
                        for line in actual.lines() {
                            println!("  {}", line);
                        }
                    }
                } else {
                    failed += 1;
                    println!("test {} ... \x1b[31mFAILED\x1b[0m", name);
                    failures.push(format!(
                        "---- {} ----\nexpected:\n{}\nactual:\n{}\n",
                        name,
                        expected.trim_end(),
                        actual.trim_end()
                    ));
                    if !no_fail_fast {
                        break;
                    }
                }
            }
            Err(stage) => {
                errors += 1;
                println!("test {} ... \x1b[33mERROR\x1b[0m ({})", name, stage);
                if !no_fail_fast {
                    break;
                }
            }
        }
    }

    // Summary
    println!();
    if !failures.is_empty() {
        println!("failures:\n");
        for f in &failures {
            println!("{}", f);
        }
    }

    let status = if failed == 0 && errors == 0 {
        "\x1b[32mok\x1b[0m"
    } else {
        "\x1b[31mFAILED\x1b[0m"
    };
    println!(
        "test result: {}. {} passed; {} failed; {} errors; {} skipped\n",
        status, passed, failed, errors, skipped
    );

    if failed > 0 || errors > 0 {
        Err(1)
    } else {
        Ok(())
    }
}

fn cmd_lint(file: &PathBuf) -> Result<(), i32> {
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading file '{}': {}", file.display(), e);
        1
    })?;

    let source = resolve_imports(&source, file)?;
    let base = file.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, base)?;

    let source_file = SourceFile::new(file.to_string_lossy(), source.clone());

    // Lex
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        1
    })?;

    resolve_modules(&mut ast, base)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_dir(base.to_path_buf());
    checker.check_module(&ast);

    let mut warnings = 0u32;
    let mut errors = 0u32;

    // Report type errors
    for err in checker.errors() {
        let span = err.span;
        let pos = source_file.lookup_position(span.start);
        eprintln!(
            "\x1b[31merror\x1b[0m: {} ({}:{}:{})",
            err,
            file.display(),
            pos.line,
            pos.column
        );
        errors += 1;
    }

    // Report parse errors
    for err in parser.errors() {
        eprintln!("\x1b[31merror\x1b[0m: {} ({})", err, file.display());
        errors += 1;
    }

    // Lint checks: style warnings
    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = line_num + 1;

        // Trailing whitespace
        if line.len() > trimmed.len() + (line.len() - line.trim_end().len())
            && line.trim_end().len() < line.len()
        {
            eprintln!(
                "\x1b[33mwarning\x1b[0m: trailing whitespace ({}:{})",
                file.display(),
                line_num
            );
            warnings += 1;
        }

        // TODO/FIXME markers
        if trimmed.contains("TODO") || trimmed.contains("FIXME") || trimmed.contains("HACK") {
            eprintln!(
                "\x1b[33mwarning\x1b[0m: {} ({}:{})",
                if trimmed.contains("TODO") {
                    "TODO marker"
                } else if trimmed.contains("FIXME") {
                    "FIXME marker"
                } else {
                    "HACK marker"
                },
                file.display(),
                line_num
            );
            warnings += 1;
        }

        // Lines > 120 chars
        if line.len() > 120 {
            eprintln!(
                "\x1b[33mwarning\x1b[0m: line exceeds 120 characters ({} chars) ({}:{})",
                line.len(),
                file.display(),
                line_num
            );
            warnings += 1;
        }
    }

    // Summary
    if errors == 0 && warnings == 0 {
        println!("No issues found in '{}'", file.display());
    } else {
        println!(
            "{} error(s), {} warning(s) in '{}'",
            errors,
            warnings,
            file.display()
        );
    }

    if errors > 0 {
        Err(1)
    } else {
        Ok(())
    }
}

fn cmd_repl() -> Result<(), i32> {
    println!("QuantaLang REPL v{}", quantalang::VERSION);
    println!("Type :help for help, :quit to exit");
    println!();

    let mut ctx = TypeContext::new();
    let mut history: Vec<String> = Vec::new();

    loop {
        use std::io::{self, Write};

        print!(">>> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        history.push(input.to_string());

        if input.starts_with(':') {
            match input {
                ":quit" | ":q" | ":exit" => break,
                ":help" | ":h" => {
                    println!("Commands:");
                    println!("  :quit, :q      - Exit the REPL");
                    println!("  :help, :h      - Show this help");
                    println!("  :tokens <expr> - Show tokens for expression");
                    println!("  :ast <expr>    - Show AST for expression");
                    println!("  :type <expr>   - Show type of expression");
                    println!("  :history       - Show command history");
                    println!("  :clear         - Clear the screen");
                    println!();
                    println!("Or enter QuantaLang code to parse and analyze.");
                }
                ":history" => {
                    for (i, cmd) in history.iter().enumerate() {
                        println!("{:4}: {}", i + 1, cmd);
                    }
                }
                ":clear" => {
                    print!("\x1B[2J\x1B[1;1H");
                    io::stdout().flush().unwrap();
                }
                cmd if cmd.starts_with(":tokens ") => {
                    let expr = &cmd[8..];
                    let file = SourceFile::anonymous(expr);
                    let mut lexer = Lexer::new(&file);
                    match lexer.tokenize() {
                        Ok(tokens) => {
                            for token in tokens {
                                if !token.is_eof() {
                                    println!("  {:?}", token);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    }
                }
                cmd if cmd.starts_with(":ast ") => {
                    let expr = &cmd[5..];
                    // Wrap in a function to make it parseable
                    let wrapped = format!("fn __repl__() {{ {} }}", expr);
                    let file = SourceFile::anonymous(wrapped.clone());
                    let mut lexer = Lexer::new(&file);
                    match lexer.tokenize() {
                        Ok(tokens) => {
                            let mut parser = Parser::new(&file, tokens);
                            match parser.parse() {
                                Ok(ast) => {
                                    println!("AST:");
                                    for item in &ast.items {
                                        println!("  {:?}", item);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Parse error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Lexer error: {}", e);
                        }
                    }
                }
                cmd if cmd.starts_with(":type ") => {
                    let expr = &cmd[6..];
                    let wrapped = format!("fn __repl__() {{ {} }}", expr);
                    let file = SourceFile::anonymous(wrapped.clone());
                    let mut lexer = Lexer::new(&file);
                    match lexer.tokenize() {
                        Ok(tokens) => {
                            let mut parser = Parser::new(&file, tokens);
                            match parser.parse() {
                                Ok(ast) => {
                                    let mut checker = TypeChecker::new(&mut ctx);
                                    checker.check_module(&ast);
                                    if checker.has_errors() {
                                        for err in checker.errors() {
                                            eprintln!("Type error: {}", err);
                                        }
                                    } else {
                                        println!("Type check passed!");
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Parse error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Lexer error: {}", e);
                        }
                    }
                }
                _ => {
                    eprintln!("Unknown command: {}", input);
                    eprintln!("Type :help for available commands");
                }
            }
            continue;
        }

        // Parse as a module item or expression
        let file = SourceFile::anonymous(input);
        let mut lexer = Lexer::new(&file);

        match lexer.tokenize() {
            Ok(tokens) => {
                println!("Tokens: {}", tokens.len());

                // Try to parse
                let mut parser = Parser::new(&file, tokens.clone());
                match parser.parse() {
                    Ok(ast) => {
                        println!("Parsed {} item(s)", ast.items.len());
                        for item in &ast.items {
                            println!("  - {}", item_kind_name(&item.kind));
                        }

                        // Type check
                        let mut checker = TypeChecker::new(&mut ctx);
                        checker.check_module(&ast);
                        if checker.has_errors() {
                            println!("Type errors:");
                            for err in checker.errors() {
                                println!("  {}", err);
                            }
                        } else {
                            println!("Type check: OK");
                        }
                    }
                    Err(e) => {
                        // Show tokens on parse failure
                        println!("Tokens:");
                        for token in &tokens {
                            if !token.is_eof() {
                                print!("{} ", token.kind);
                            }
                        }
                        println!();
                        eprintln!("Parse error: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Lexer error: {}", e);
            }
        }
    }

    println!("\nGoodbye!");
    Ok(())
}

// =============================================================================
// LSP COMMAND
// =============================================================================

fn cmd_lsp() -> Result<(), i32> {
    eprintln!(
        "QuantaLang LSP server v{} starting on stdio...",
        quantalang::VERSION
    );

    match quantalang::lsp::run_server() {
        Ok(()) => {
            eprintln!("LSP server shut down cleanly.");
            Ok(())
        }
        Err(e) => {
            eprintln!("LSP server error: {}", e);
            Err(1)
        }
    }
}

fn cmd_fmt(file: &PathBuf, check: bool, write: bool) -> Result<(), i32> {
    let source = std::fs::read_to_string(file).map_err(|e| {
        eprintln!("Error reading '{}': {}", file.display(), e);
        1
    })?;

    let formatter = quantalang::fmt::Formatter::default_formatter();
    let formatted = formatter.format_str(&source).map_err(|e| {
        eprintln!("Format error: {}", e);
        1
    })?;

    if check {
        if source != formatted {
            eprintln!("{} would be reformatted", file.display());
            return Err(1);
        }
        println!("{}: OK", file.display());
        return Ok(());
    }

    if write {
        std::fs::write(file, &formatted).map_err(|e| {
            eprintln!("Error writing '{}': {}", file.display(), e);
            1
        })?;
        println!("Formatted {}", file.display());
    } else {
        print!("{}", formatted);
    }
    Ok(())
}

// =============================================================================
// LOCAL PACKAGE REGISTRY
// =============================================================================

/// An entry in the local registry index (registry/index.json).
#[derive(Debug, serde::Deserialize)]
struct LocalRegistryEntry {
    version: String,
    description: String,
    #[allow(dead_code)]
    author: String,
    #[allow(dead_code)]
    checksum: String,
    #[allow(dead_code)]
    path: String,
}

/// Top-level shape of registry/index.json.
#[derive(Debug, serde::Deserialize)]
struct LocalRegistryIndex {
    packages: HashMap<String, LocalRegistryEntry>,
}

/// Load the local file-based package registry.
///
/// Searches for `registry/index.json` relative to the compiler executable, then
/// falls back to the compile-time `CARGO_MANIFEST_DIR` path (good for `cargo run`).
fn load_local_registry_index() -> HashMap<String, LocalRegistryEntry> {
    // Try relative to the running executable first
    let candidates: Vec<std::path::PathBuf> = vec![
        // Works when invoked via `cargo run` from compiler/
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("registry")
            .join("index.json"),
        // Works for an installed binary next to a registry/ sibling
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("../registry/index.json")))
            .unwrap_or_default(),
    ];

    for path in &candidates {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(index) = serde_json::from_str::<LocalRegistryIndex>(&data) {
                return index.packages;
            }
        }
    }
    HashMap::new()
}

fn cmd_pkg(cmd: PkgCommands) -> Result<(), i32> {
    match cmd {
        PkgCommands::Init { path } => {
            let manifest_path = path.join("Quanta.toml");
            if manifest_path.exists() {
                eprintln!("Quanta.toml already exists in {}", path.display());
                return Err(1);
            }
            let dir_name = path
                .canonicalize()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "my-project".to_string());
            let manifest = format!(
                "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\n",
                dir_name
            );
            std::fs::write(&manifest_path, &manifest).map_err(|e| {
                eprintln!("Error creating Quanta.toml: {}", e);
                1
            })?;
            println!("Created {}", manifest_path.display());
            Ok(())
        }
        PkgCommands::Add { name, version } => {
            let manifest_path = Path::new("Quanta.toml");
            if !manifest_path.exists() {
                eprintln!("No Quanta.toml found. Run `quantac pkg init` first.");
                return Err(1);
            }
            let mut content = std::fs::read_to_string(manifest_path).map_err(|e| {
                eprintln!("Error reading Quanta.toml: {}", e);
                1
            })?;
            let ver = version.unwrap_or_else(|| "*".to_string());
            content.push_str(&format!("{} = \"{}\"\n", name, ver));
            std::fs::write(manifest_path, &content).map_err(|e| {
                eprintln!("Error writing Quanta.toml: {}", e);
                1
            })?;
            println!("Added {} = \"{}\"", name, ver);
            Ok(())
        }
        PkgCommands::Resolve { path } => {
            let manifest_path = path.join("Quanta.toml");
            if !manifest_path.exists() {
                eprintln!("No Quanta.toml found in {}", path.display());
                return Err(1);
            }
            println!("Resolving dependencies from {}...", manifest_path.display());
            let content = std::fs::read_to_string(&manifest_path).map_err(|e| {
                eprintln!("Error reading manifest: {}", e);
                1
            })?;
            println!("Manifest loaded ({} bytes)", content.len());

            // Check dependencies against the local registry
            let index = load_local_registry_index();
            // Parse [dependencies] lines from the manifest
            let mut in_deps = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed == "[dependencies]" {
                    in_deps = true;
                    continue;
                }
                if trimmed.starts_with('[') {
                    in_deps = false;
                    continue;
                }
                if in_deps {
                    if let Some((name, _ver)) = trimmed.split_once('=') {
                        let dep_name = name.trim();
                        if dep_name.is_empty() {
                            continue;
                        }
                        if let Some(entry) = index.get(dep_name) {
                            println!(
                                "  {} = {} ... found ({})",
                                dep_name, entry.version, entry.description
                            );
                        } else {
                            println!("  {} ... NOT FOUND in local registry", dep_name);
                        }
                    }
                }
            }
            println!("Resolution complete.");
            Ok(())
        }
        PkgCommands::Search { query } => {
            let index = load_local_registry_index();
            let query_lower = query.to_lowercase();
            let mut found = 0u32;

            println!("Searching local registry for '{}'...", query);
            for (name, entry) in &index {
                if name.to_lowercase().contains(&query_lower)
                    || entry.description.to_lowercase().contains(&query_lower)
                {
                    println!("  {} v{} - {}", name, entry.version, entry.description);
                    found += 1;
                }
            }

            if found == 0 {
                println!("No packages found matching '{}'.", query);
            } else {
                println!("{} package(s) found.", found);
            }
            Ok(())
        }
    }
}

// =============================================================================
// MODULE RESOLUTION
// =============================================================================

/// Resolve `mod foo;` declarations by loading and parsing external module files.
///
/// For each `mod foo;` (a mod declaration with no body), this function:
/// 1. Looks for `foo.quanta` in the same directory, or `foo/mod.quanta`
/// 2. Parses that file
/// 3. Recursively resolves sub-module declarations
/// 4. Collects all item names defined in the module
/// 5. Prefixes each definition with `foo_` (functions, structs, enums)
/// 6. Renames intra-module references in function bodies
/// 7. Appends the prefixed items into the main AST
///
/// Multi-segment paths like `foo::bar::baz()` resolve to `foo_bar_baz`
/// during lowering since lower_path joins segments with `_`.
/// Find the stdlib directory. Searches:
/// 1. `stdlib/` relative to the compiler executable
/// 2. `../stdlib/` relative to the compiler executable (for dev builds)
/// 3. `QUANTALANG_STDLIB` environment variable
fn find_stdlib_path() -> Option<PathBuf> {
    // Check env var first
    if let Ok(path) = std::env::var("QUANTALANG_STDLIB") {
        let p = PathBuf::from(path);
        if p.is_dir() {
            return Some(p);
        }
    }
    // Relative to the compiler executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // stdlib/ next to the executable
            let candidate = exe_dir.join("stdlib");
            if candidate.is_dir() {
                return Some(candidate);
            }
            // ../stdlib/ (dev layout: compiler/target/release/quantac → ../../stdlib)
            for ancestor in exe_dir.ancestors().skip(1).take(4) {
                let candidate = ancestor.join("stdlib");
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn resolve_modules(ast: &mut Module, source_dir: &Path) -> Result<(), i32> {
    let mut ledger = None;
    resolve_modules_with_prefix(ast, source_dir, "", &mut ledger)
}

fn resolve_modules_recording_inputs(
    ast: &mut Module,
    source_dir: &Path,
    ledger: &mut InputDigestLedger,
) -> Result<(), i32> {
    let mut ledger = Some(ledger);
    resolve_modules_with_prefix(ast, source_dir, "", &mut ledger)
}

/// Resolve modules with a prefix for nested module support.
/// The prefix is prepended to all mangled names (e.g., "utils_" for sub-modules of utils).
fn resolve_modules_with_prefix(
    ast: &mut Module,
    source_dir: &Path,
    prefix: &str,
    ledger: &mut Option<&mut InputDigestLedger>,
) -> Result<(), i32> {
    // Collect module names from `mod foo;` declarations (content == None).
    let mod_names: Vec<String> = ast
        .items
        .iter()
        .filter_map(|item| {
            if let ItemKind::Mod(ref m) = item.kind {
                if m.content.is_none() {
                    return Some(m.name.name.to_string());
                }
            }
            None
        })
        .collect();

    if mod_names.is_empty() {
        return Ok(());
    }

    let mut new_items: Vec<ast::Item> = Vec::new();

    for mod_name in &mod_names {
        // Look for foo.quanta or foo/mod.quanta
        let mod_file = source_dir.join(format!("{}.quanta", mod_name));
        let mod_dir_file = source_dir.join(mod_name).join("mod.quanta");

        // Search order: source directory → stdlib directory → skip
        let stdlib_file = find_stdlib_path().map(|p| p.join(format!("{}.quanta", mod_name)));

        let (actual_file, sub_source_dir) = if mod_file.exists() {
            (mod_file, source_dir.to_path_buf())
        } else if mod_dir_file.exists() {
            (mod_dir_file, source_dir.join(mod_name))
        } else if let Some(ref sf) = stdlib_file {
            if sf.exists() {
                (
                    sf.clone(),
                    sf.parent().unwrap_or(Path::new(".")).to_path_buf(),
                )
            } else {
                continue;
            }
        } else {
            continue;
        };

        // Read and parse the module file
        let mod_bytes = std::fs::read(&actual_file).map_err(|e| {
            eprintln!(
                "Error reading module file '{}': {}",
                actual_file.display(),
                e
            );
            1
        })?;
        if let Some(ledger) = ledger.as_deref_mut() {
            ledger.record("module", &actual_file, &mod_bytes);
        }
        let mod_source = String::from_utf8(mod_bytes).map_err(|e| {
            eprintln!(
                "Error reading module file '{}': {}",
                actual_file.display(),
                e
            );
            1
        })?;

        let mod_source_file = SourceFile::new(actual_file.to_string_lossy(), mod_source);
        let mut mod_lexer = Lexer::new(&mod_source_file);
        let mod_tokens = mod_lexer.tokenize().map_err(|e| {
            eprintln!("Lexer error in module '{}': {}", mod_name, e);
            1
        })?;

        let mut mod_parser = Parser::new(&mod_source_file, mod_tokens);
        let mut mod_ast = mod_parser.parse().map_err(|e| {
            eprintln!("Parse error in module '{}': {}", mod_name, e);
            for err in mod_parser.errors() {
                eprintln!("  {}", err);
            }
            1
        })?;

        // The full prefix for this module's items
        let full_prefix = if prefix.is_empty() {
            mod_name.clone()
        } else {
            format!("{}_{}", prefix, mod_name)
        };

        // Recursively resolve sub-modules within this module
        resolve_modules_with_prefix(&mut mod_ast, &sub_source_dir, &full_prefix, ledger)?;

        // Collect names defined in this module (for intra-module rewriting)
        let mod_defined: std::collections::HashSet<String> = mod_ast
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Function(f) => Some(f.name.name.to_string()),
                _ => None,
            })
            .collect();

        // Merge module items with name prefixing.
        // Functions are prefixed: `add` → `math_helpers_add`
        // This matches how lower_path joins path segments with `_`:
        // `math_helpers::add(...)` emits a call to `math_helpers_add`.
        for item in mod_ast.items {
            match item.kind {
                ItemKind::Function(f) => {
                    let mut prefixed_fn = *f;
                    let original_name = prefixed_fn.name.name.to_string();
                    prefixed_fn.name = ast::Ident {
                        name: Arc::from(format!("{}_{}", full_prefix, original_name)),
                        span: prefixed_fn.name.span,
                    };
                    // Rewrite intra-module calls in the function body:
                    // if this function calls `helper()` and `helper` is defined
                    // in the same module, rewrite to `math_helpers_helper()`.
                    if let Some(ref mut body) = prefixed_fn.body {
                        rewrite_intra_module_calls(body, &mod_defined, &full_prefix);
                    }
                    new_items.push(ast::Item::new(
                        ItemKind::Function(Box::new(prefixed_fn)),
                        Visibility::default(),
                        Vec::new(),
                        Span::dummy(),
                    ));
                }
                ItemKind::Struct(_) | ItemKind::Enum(_) | ItemKind::Impl(_) => {
                    new_items.push(item);
                }
                _ => {
                    new_items.push(item);
                }
            }
        }
    }

    // Build a map of all imported function names: bare_name → prefixed_name
    let mut imported_fns: HashMap<String, String> = HashMap::new();
    for item in &new_items {
        if let ItemKind::Function(f) = &item.kind {
            let prefixed = f.name.name.to_string();
            // Extract the bare name by stripping the module prefix
            // e.g., "core_i32_min" → "i32_min", "math_lerp_f64" → "lerp_f64"
            for mod_name in &mod_names {
                let module_prefix = if prefix.is_empty() {
                    mod_name.clone()
                } else {
                    format!("{}_{}", prefix, mod_name)
                };
                let prefix_with_sep = format!("{}_", module_prefix);
                if let Some(bare) = prefixed.strip_prefix(&prefix_with_sep) {
                    imported_fns.insert(bare.to_string(), prefixed.clone());
                }
            }
        }
    }

    // Append module items to the main AST
    ast.items.extend(new_items);

    // Rewrite calls in the main program's existing functions to use prefixed names
    if !imported_fns.is_empty() {
        for item in &mut ast.items {
            if let ItemKind::Function(f) = &mut item.kind {
                if let Some(ref mut body) = f.body {
                    rewrite_imported_calls(body, &imported_fns);
                }
            }
        }
    }

    Ok(())
}

/// Rewrite calls to module-local functions within a function body.
fn rewrite_intra_module_calls(body: &mut ast::Block, mod_defined: &HashSet<String>, prefix: &str) {
    for stmt in &mut body.stmts {
        match &mut stmt.kind {
            ast::StmtKind::Expr(expr) | ast::StmtKind::Semi(expr) => {
                rewrite_expr_node(expr, mod_defined, prefix);
            }
            ast::StmtKind::Local(local) => {
                if let Some(ref mut init) = local.init {
                    rewrite_expr_node(&mut init.expr, mod_defined, prefix);
                }
            }
            _ => {}
        }
    }
}

fn rewrite_expr_node(expr: &mut ast::Expr, mod_defined: &HashSet<String>, prefix: &str) {
    match &mut expr.kind {
        ast::ExprKind::Call { func, args } => {
            if let ast::ExprKind::Ident(ref mut ident) = func.kind {
                if mod_defined.contains(ident.name.as_ref()) {
                    ident.name = Arc::from(format!("{}_{}", prefix, ident.name));
                }
            }
            rewrite_expr_node(func, mod_defined, prefix);
            for arg in args {
                rewrite_expr_node(arg, mod_defined, prefix);
            }
        }
        ast::ExprKind::Binary { left, right, .. } => {
            rewrite_expr_node(left, mod_defined, prefix);
            rewrite_expr_node(right, mod_defined, prefix);
        }
        ast::ExprKind::Unary { expr: inner, .. } => {
            rewrite_expr_node(inner, mod_defined, prefix);
        }
        ast::ExprKind::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            rewrite_expr_node(condition, mod_defined, prefix);
            rewrite_intra_module_calls(then_branch, mod_defined, prefix);
            if let Some(ref mut eb) = else_branch {
                rewrite_expr_node(eb, mod_defined, prefix);
            }
        }
        ast::ExprKind::Block(block) => {
            rewrite_intra_module_calls(block, mod_defined, prefix);
        }
        ast::ExprKind::Return(Some(ref mut inner)) => {
            rewrite_expr_node(inner, mod_defined, prefix);
        }
        _ => {}
    }
}

/// Rewrite bare function calls in the main program to use module-prefixed names.
/// E.g., `i32_min(a, b)` → `core_i32_min(a, b)` when `core.quanta` defines `i32_min`.
fn rewrite_imported_calls(body: &mut ast::Block, imported: &HashMap<String, String>) {
    for stmt in &mut body.stmts {
        match &mut stmt.kind {
            ast::StmtKind::Expr(expr) | ast::StmtKind::Semi(expr) => {
                rewrite_imported_expr(expr, imported);
            }
            ast::StmtKind::Local(local) => {
                if let Some(ref mut init) = local.init {
                    rewrite_imported_expr(&mut init.expr, imported);
                }
            }
            _ => {}
        }
    }
}

fn rewrite_imported_expr(expr: &mut ast::Expr, imported: &HashMap<String, String>) {
    match &mut expr.kind {
        ast::ExprKind::Call { func, args } => {
            if let ast::ExprKind::Ident(ref mut ident) = func.kind {
                if let Some(prefixed) = imported.get(ident.name.as_ref()) {
                    ident.name = Arc::from(prefixed.as_str());
                }
            }
            rewrite_imported_expr(func, imported);
            for arg in args {
                rewrite_imported_expr(arg, imported);
            }
        }
        ast::ExprKind::Binary { left, right, .. } => {
            rewrite_imported_expr(left, imported);
            rewrite_imported_expr(right, imported);
        }
        ast::ExprKind::Unary { expr: inner, .. } => {
            rewrite_imported_expr(inner, imported);
        }
        ast::ExprKind::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            rewrite_imported_expr(condition, imported);
            rewrite_imported_calls(then_branch, imported);
            if let Some(ref mut eb) = else_branch {
                rewrite_imported_expr(eb, imported);
            }
        }
        ast::ExprKind::Block(block) => {
            rewrite_imported_calls(block, imported);
        }
        ast::ExprKind::Return(Some(ref mut inner)) => {
            rewrite_imported_expr(inner, imported);
        }
        ast::ExprKind::Assign { value, .. } => {
            rewrite_imported_expr(value, imported);
        }
        _ => {}
    }
}

fn cmd_compile(
    input: &PathBuf,
    output: Option<&std::path::Path>,
    opt_level: u8,
    debug: bool,
    target_override: Option<&str>,
) -> Result<(), i32> {
    // Read source file
    let source = std::fs::read_to_string(input).map_err(|e| {
        eprintln!("Error reading file '{}': {}", input.display(), e);
        1
    })?;

    // Resolve `// import <pkg>` and `use <pkg>;` directives
    let source = resolve_imports(&source, input)?;

    // Expand `include!("path")` directives
    let base_dir = input.parent().unwrap_or(Path::new("."));
    let source = preprocess_includes(&source, base_dir)?;

    let source_file = SourceFile::new(input.to_string_lossy(), source);

    // Tokenize
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().map_err(|e| {
        eprintln!("Lexer error: {}", e);
        1
    })?;

    // Parse
    let mut parser = Parser::new(&source_file, tokens);
    let mut ast = parser.parse().map_err(|e| {
        eprintln!("Parse error: {}", e);
        for err in parser.errors() {
            eprintln!("  {}", err);
        }
        1
    })?;

    // Resolve `mod foo;` declarations - load and merge external module files
    let source_dir = input.parent().unwrap_or(Path::new("."));
    resolve_modules(&mut ast, source_dir)?;

    // Type check
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_dir(source_dir.to_path_buf());
    checker.check_module(&ast);

    if checker.has_errors() {
        for err in checker.errors() {
            // Show error with source location: file:line:col
            let line = source_file.lookup_line(err.span.start);
            let line_start = source_file.line_start(line).unwrap_or(err.span.start);
            let col = err.span.start.0.saturating_sub(line_start.0) as usize;
            eprintln!(
                "error[{}:{}:{}]: {}",
                input.display(),
                line + 1,
                col + 1,
                err.error
            );

            // Show the source line with an underline
            if let Some(src_line) = source_file.source().lines().nth(line) {
                eprintln!("  {} | {}", line + 1, src_line);
                let padding = format!("{}", line + 1).len();
                let underline_pos = col;
                let underline_len =
                    (err.span.end.0.saturating_sub(err.span.start.0) as usize).max(1);
                eprintln!(
                    "  {} | {}{}",
                    " ".repeat(padding),
                    " ".repeat(underline_pos),
                    "^".repeat(underline_len.min(src_line.len().saturating_sub(underline_pos)))
                );
            }

            if let Some(help) = &err.help {
                eprintln!("  help: {}", help);
            }
            for note in &err.notes {
                eprintln!("  note: {}", note);
            }
        }
        return Err(1);
    }

    // Select target: explicit --target flag > output extension > default (C)
    let target = if let Some(t) = target_override {
        parse_codegen_target(t).map_err(|err| {
            eprintln!("{}", err);
            1
        })?
    } else if let Some(ext) = output.and_then(|p| p.extension()).and_then(|e| e.to_str()) {
        target_from_extension(ext).unwrap_or(Target::C)
    } else {
        Target::C
    };

    // Determine output path using target's default extension
    let output_path = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| input.with_extension(target.extension()));

    // Code generation (pass source for macro expansion)
    let mut codegen = CodeGenerator::with_source(&ctx, target, source_file.source().into());
    // Enable ReShade boilerplate for .fx output files
    if output_path.extension().and_then(|e| e.to_str()) == Some("fx") {
        codegen.reshade = true;
    }
    let generated = codegen.generate(&ast).map_err(|e| {
        eprintln!("Code generation error: {}", e);
        1
    })?;

    // Write output
    std::fs::write(&output_path, &generated.data).map_err(|e| {
        eprintln!("Failed to write output: {}", e);
        1
    })?;

    println!("Compiled {} -> {}", input.display(), output_path.display());

    if debug {
        println!("Debug info: enabled");
    }
    if opt_level > 0 {
        println!("Optimization level: O{}", opt_level);
    }

    // For LLVM target, try to compile the .ll file to a native executable
    if target == Target::LlvmIr {
        let exe_ext = if cfg!(windows) { "exe" } else { "" };
        let exe_path = if exe_ext.is_empty() {
            input.with_extension("")
        } else {
            input.with_extension(exe_ext)
        };

        // Try clang first
        let clang_ok = std::process::Command::new("clang")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if clang_ok {
            let mut cmd = std::process::Command::new("clang");
            cmd.arg(&output_path);
            cmd.arg("-o");
            cmd.arg(&exe_path);
            if opt_level > 0 {
                cmd.arg(format!("-O{}", opt_level));
            }
            if debug {
                cmd.arg("-g");
            }
            if !cfg!(windows) {
                cmd.arg("-lm");
            }

            match cmd.output() {
                Ok(result) if result.status.success() => {
                    println!("Linked {} -> {}", output_path.display(), exe_path.display());
                }
                Ok(result) => {
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    eprintln!("clang linking failed: {}", stderr.trim());
                    eprintln!(
                        "LLVM IR file is still available at: {}",
                        output_path.display()
                    );
                }
                Err(e) => {
                    eprintln!("Failed to invoke clang: {}", e);
                    eprintln!(
                        "LLVM IR file is still available at: {}",
                        output_path.display()
                    );
                }
            }
        } else {
            println!();
            println!("LLVM IR generated at {}", output_path.display());
            if cfg!(windows) {
                println!(
                    "To compile: clang {} -o {}",
                    output_path.display(),
                    exe_path.display()
                );
            } else {
                println!(
                    "To compile: clang {} -o {} -lm",
                    output_path.display(),
                    exe_path.display()
                );
            }
        }
    }

    // x86-64: try nasm → ld pipeline for native executable
    if target == Target::X86_64 {
        let obj_path = input.with_extension("o");
        let exe_path = input.with_extension(if cfg!(windows) { "exe" } else { "" });
        let nasm_ok = std::process::Command::new("nasm")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if nasm_ok {
            let fmt = if cfg!(windows) { "win64" } else { "elf64" };
            if let Ok(r) = std::process::Command::new("nasm")
                .args(["-f", fmt])
                .arg(&output_path)
                .arg("-o")
                .arg(&obj_path)
                .output()
            {
                if r.status.success() {
                    println!("Assembled -> {}", obj_path.display());
                    let lr = if cfg!(windows) {
                        std::process::Command::new("link.exe")
                            .args(["/entry:main", "/subsystem:console"])
                            .arg(&obj_path)
                            .arg(&format!("/out:{}", exe_path.display()))
                            .output()
                    } else {
                        std::process::Command::new("ld")
                            .arg(&obj_path)
                            .arg("-o")
                            .arg(&exe_path)
                            .arg("-lc")
                            .output()
                    };
                    if let Ok(r) = lr {
                        if r.status.success() {
                            println!("Linked -> {}", exe_path.display());
                        }
                    }
                }
            }
        } else {
            println!(
                "\nx86-64 assembly at {}. Install nasm to build native.",
                output_path.display()
            );
        }
    }

    // WASM: detect wasmtime/wasmer and show run instructions
    if target == Target::Wasm {
        let wt = std::process::Command::new("wasmtime")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if wt {
            println!("Run: wasmtime {}", output_path.display());
        } else {
            println!(
                "\nWASM at {}. Install wasmtime to run.",
                output_path.display()
            );
        }
    }

    Ok(())
}

/// Watch shader files for changes and recompile automatically.
///
/// Usage:
///   quantac watch shaders/ --target=spirv
///   quantac watch shader.quanta --target=spirv
fn cmd_watch(path: &PathBuf, target_str: &str) -> Result<(), i32> {
    use std::collections::HashMap;
    use std::time::{Duration, SystemTime};

    let target = parse_codegen_target(target_str).map_err(|err| {
        eprintln!("{}", err);
        1
    })?;
    let target_ext = match target {
        Target::SpirV => "spv",
        Target::C => "c",
        Target::LlvmIr => "ll",
        Target::Rust => "rs",
        _ => {
            eprintln!(
                "Watch target '{}' is not supported. Supported: spirv, c, llvm, rust",
                target_str
            );
            return Err(1);
        }
    };

    // Collect .quanta files to watch
    let files_to_watch: Vec<PathBuf> = if path.is_dir() {
        std::fs::read_dir(path)
            .map_err(|e| {
                eprintln!("Failed to read directory '{}': {}", path.display(), e);
                1
            })?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("quanta") {
                    Some(p)
                } else {
                    None
                }
            })
            .collect()
    } else if path.extension().and_then(|e| e.to_str()) == Some("quanta") {
        vec![path.clone()]
    } else {
        eprintln!("Expected a .quanta file or directory");
        return Err(1);
    };

    if files_to_watch.is_empty() {
        eprintln!("No .quanta files found in '{}'", path.display());
        return Err(1);
    }

    println!(
        "Watching {} file(s) for changes (target: {})...",
        files_to_watch.len(),
        target_str
    );
    for f in &files_to_watch {
        println!("  {}", f.display());
    }
    println!("Press Ctrl+C to stop.\n");

    // Track modification times
    let mut last_modified: HashMap<PathBuf, SystemTime> = HashMap::new();
    for f in &files_to_watch {
        if let Ok(meta) = std::fs::metadata(f) {
            if let Ok(modified) = meta.modified() {
                last_modified.insert(f.clone(), modified);
            }
        }
    }

    // Initial compilation
    for f in &files_to_watch {
        let output = f.with_extension(target_ext);
        match compile_single_file(f, &output) {
            Ok(()) => println!("[OK] {} -> {}", f.display(), output.display()),
            Err(msg) => eprintln!("[ERR] {}: {}", f.display(), msg),
        }
    }

    // Watch loop
    loop {
        std::thread::sleep(Duration::from_millis(500));

        for f in &files_to_watch {
            let modified = match std::fs::metadata(f) {
                Ok(meta) => meta.modified().ok(),
                Err(_) => continue,
            };

            if let Some(mod_time) = modified {
                let last = last_modified.get(f);
                if last.is_none() || last.unwrap() < &mod_time {
                    last_modified.insert(f.clone(), mod_time);

                    let output = f.with_extension(target_ext);
                    let start = std::time::Instant::now();
                    match compile_single_file(f, &output) {
                        Ok(()) => {
                            let elapsed = start.elapsed();
                            println!(
                                "[OK] {} -> {} ({:.1}ms)",
                                f.file_name().unwrap().to_string_lossy(),
                                output.file_name().unwrap().to_string_lossy(),
                                elapsed.as_secs_f64() * 1000.0
                            );

                            // Auto-validate SPIR-V if spirv-val is available
                            if target_ext == "spv" {
                                let spirv_val_paths =
                                    ["C:\\VulkanSDK\\1.4.341.1\\Bin\\spirv-val.exe", "spirv-val"];
                                for val_path in &spirv_val_paths {
                                    if let Ok(result) = std::process::Command::new(val_path)
                                        .arg("--target-env")
                                        .arg("vulkan1.0")
                                        .arg(&output)
                                        .output()
                                    {
                                        if result.status.success() {
                                            println!("     spirv-val: PASSED (Vulkan 1.0)");
                                        } else {
                                            let stderr = String::from_utf8_lossy(&result.stderr);
                                            eprintln!(
                                                "     spirv-val: FAILED\n     {}",
                                                stderr.trim()
                                            );
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                        Err(msg) => eprintln!(
                            "[ERR] {}: {}",
                            f.file_name().unwrap().to_string_lossy(),
                            msg
                        ),
                    }
                }
            }
        }
    }
}

/// Compile a single .quanta file to the given output path.
fn compile_single_file(input: &Path, output: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(input).map_err(|e| format!("read error: {}", e))?;

    // Resolve `// import <pkg>` and `use <pkg>;` directives
    let source = resolve_imports(&source, input)
        .map_err(|code| format!("import resolution failed (exit {})", code))?;

    let source_file = SourceFile::new(input.to_string_lossy(), source);

    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer
        .tokenize()
        .map_err(|e| format!("lexer error: {}", e))?;

    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse().map_err(|e| format!("parse error: {}", e))?;

    if !parser.errors().is_empty() {
        return Err(format!("parse errors: {}", parser.errors().len()));
    }

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.check_module(&ast);

    if checker.has_errors() {
        let errs: Vec<String> = checker.errors().iter().map(|e| format!("{}", e)).collect();
        return Err(format!("type errors:\n  {}", errs.join("\n  ")));
    }

    let target = output
        .extension()
        .and_then(|e| e.to_str())
        .and_then(target_from_extension)
        .unwrap_or(Target::C);

    let mut codegen = CodeGenerator::with_source(&ctx, target, source_file.source().into());
    let generated = codegen
        .generate(&ast)
        .map_err(|e| format!("codegen error: {}", e))?;

    std::fs::write(output, &generated.data).map_err(|e| format!("write error: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rust_codegen_target_aliases() {
        assert_eq!(parse_codegen_target("rust"), Ok(Target::Rust));
        assert_eq!(parse_codegen_target("rs"), Ok(Target::Rust));
    }

    #[test]
    fn infers_rust_target_from_rs_extension() {
        assert_eq!(target_from_extension("rs"), Some(Target::Rust));
    }

    #[test]
    fn c_link_libraries_cover_host_runtime_dependencies() {
        assert_eq!(c_link_libraries("windows", false), &["-lws2_32"]);
        assert_eq!(c_link_libraries("windows", true), &["ws2_32.lib"]);
        assert_eq!(c_link_libraries("linux", false), &["-lm"]);
        assert_eq!(c_link_libraries("macos", true), &[] as &[&str]);
    }

    #[test]
    fn source_digest_hex_returns_known_sha256() {
        assert_eq!(
            source_digest_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn language_version_string_matches_public_tuple() {
        assert_eq!(language_version_string(), "1.0.0");
    }

    #[test]
    fn check_policy_evaluation_sorts_and_deduplicates_violations() {
        let policy = LoadedCheckPolicy {
            source: "policy.json".to_string(),
            source_digest: CheckReceiptSourceDigest {
                algorithm: "sha256",
                hex: source_digest_hex(b"policy"),
            },
            builtin_profile: None,
            builtin_profile_digest: None,
            profile: CheckPolicyProfile {
                schema: "quantalang-check-policy/v1".to_string(),
                allowed_effects: vec!["Console".to_string()],
                denied_effects: vec!["Network".to_string()],
                direct_effect_allowlist: BTreeMap::new(),
                propagated_effect_allowlist: BTreeMap::new(),
                require_source_digest: true,
                require_input_graph_digest: false,
            },
        };
        let outcome = CheckOutcome {
            source: "source.quanta".to_string(),
            compiler_version: quantalang::VERSION,
            language_version: language_version_string(),
            source_digest: CheckReceiptSourceDigest {
                algorithm: "sha256",
                hex: source_digest_hex(b"source"),
            },
            input_graph_digest: input_graph_digest(&[]),
            input_digests: Vec::new(),
            items: 1,
            tokens: 1,
            parse_errors: Vec::new(),
            type_errors: Vec::new(),
            function_summaries: vec![
                FunctionEffectSummary {
                    function: "b".to_string(),
                    declared_effects: vec!["Network".to_string(), "Network".to_string()],
                    observed_capabilities: BTreeMap::new(),
                    propagated_effects: BTreeMap::new(),
                },
                FunctionEffectSummary {
                    function: "a".to_string(),
                    declared_effects: vec!["FileSystem".to_string()],
                    observed_capabilities: BTreeMap::new(),
                    propagated_effects: BTreeMap::new(),
                },
            ],
        };

        let decision = evaluate_check_policy(&policy, &outcome);
        assert_eq!(decision.schema, "quantalang-check-policy/v1");
        assert_eq!(decision.source, "policy.json");
        assert_eq!(decision.source_digest.algorithm, "sha256");
        assert_eq!(check_policy_status(&decision), "failed");
        let keys = decision
            .violations
            .iter()
            .map(|violation| {
                (
                    violation.function.as_str(),
                    violation.effect.as_str(),
                    violation.surface,
                    violation.kind,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec![
                ("a", "FileSystem", "declared_effects", "DisallowedEffect"),
                ("b", "Network", "declared_effects", "DeniedEffect"),
            ]
        );
    }

    #[test]
    fn check_policy_requires_valid_input_graph_digest() {
        let policy = LoadedCheckPolicy {
            source: "policy.json".to_string(),
            source_digest: CheckReceiptSourceDigest {
                algorithm: "sha256",
                hex: source_digest_hex(b"policy"),
            },
            builtin_profile: None,
            builtin_profile_digest: None,
            profile: CheckPolicyProfile {
                schema: "quantalang-check-policy/v1".to_string(),
                allowed_effects: Vec::new(),
                denied_effects: Vec::new(),
                direct_effect_allowlist: BTreeMap::new(),
                propagated_effect_allowlist: BTreeMap::new(),
                require_source_digest: false,
                require_input_graph_digest: true,
            },
        };
        let outcome = CheckOutcome {
            source: "source.quanta".to_string(),
            compiler_version: quantalang::VERSION,
            language_version: language_version_string(),
            source_digest: CheckReceiptSourceDigest {
                algorithm: "sha256",
                hex: source_digest_hex(b"source"),
            },
            input_graph_digest: CheckReceiptSourceDigest {
                algorithm: "sha1",
                hex: "abc".to_string(),
            },
            input_digests: Vec::new(),
            items: 1,
            tokens: 1,
            parse_errors: Vec::new(),
            type_errors: Vec::new(),
            function_summaries: Vec::new(),
        };

        let decision = evaluate_check_policy(&policy, &outcome);
        assert_eq!(check_policy_status(&decision), "failed");
        assert_eq!(decision.violations.len(), 1);
        assert_eq!(decision.violations[0].kind, "MissingInputGraphDigest");
        assert_eq!(decision.violations[0].surface, "input_graph_digest");
    }

    #[test]
    fn check_policy_loads_profile_and_digest() {
        let path = std::env::temp_dir().join(format!(
            "quantalang_check_policy_load_{}.json",
            std::process::id()
        ));
        std::fs::write(
            &path,
            r#"{
              "schema": "quantalang-check-policy/v1",
              "allowed_effects": ["Console"],
              "unknown_future_field": true
            }"#,
        )
        .expect("write policy load fixture");

        let loaded = load_check_policy(&path).expect("policy should load");
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.profile.schema, "quantalang-check-policy/v1");
        assert_eq!(loaded.profile.allowed_effects, vec!["Console"]);
        assert!(loaded.profile.direct_effect_allowlist.is_empty());
        assert!(loaded.profile.propagated_effect_allowlist.is_empty());
        assert!(!loaded.profile.require_input_graph_digest);
        assert_eq!(loaded.source_digest.algorithm, "sha256");
        assert_eq!(loaded.source_digest.hex.len(), 64);
    }

    #[test]
    fn run_temp_build_dirs_are_unique_for_same_source() {
        let source = PathBuf::from("semantic-corpus/programs/scalar_branch.quanta");

        let first = run_temp_build_dir(&source);
        let second = run_temp_build_dir(&source);

        assert_ne!(first, second);
        assert!(first
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .starts_with("quantalang_run_scalar_branch_"));
    }

    #[test]
    fn run_temp_build_dirs_sanitize_source_stems() {
        let source = PathBuf::from("semantic-corpus/programs/weird file!.quanta");
        let dir = run_temp_build_dir(&source);
        let name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        assert!(name.starts_with("quantalang_run_weird_file__"));
        assert!(!name.contains(' '));
        assert!(!name.contains('!'));
    }
}
