# Quantac Check Receipts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deterministic JSON accountability receipts to `quantac check`.

**Architecture:** Keep capability evidence owned by the type/effect checker. Add a small function summary model to `compiler/src/types/check.rs`, then have `compiler/src/main.rs` render a check outcome either as existing human output or as `quantalang-check-receipt/v1` JSON. CLI tests exercise the built `quantac` binary so stdout/stderr routing and exit codes are covered.

**Tech Stack:** Rust compiler CLI with `clap`, `serde`, and `serde_json`; existing type checker capability source tracking; Cargo integration tests in `compiler/tests/cli.rs`.

---

## File Structure

- Modify: `compiler/src/types/check.rs`
  - Add `FunctionEffectSummary`.
  - Store summaries in `TypeChecker`.
  - Expose `function_effect_summaries()`.
  - Add unit tests that summaries record capability evidence and reset between `check_module` calls.
- Modify: `compiler/src/main.rs`
  - Add `--receipt <PATH>` to `Commands::Check`.
  - Refactor the check pipeline into `run_check`.
  - Add serializable `CheckReceipt`, `CheckReceiptDiagnostic`, and rendering helpers.
  - Route human progress to stderr when receipt target is `-`.
- Modify: `compiler/tests/cli.rs`
  - Add receipt CLI tests for a passing console program and a failing file capability program.
- Modify: `README.md` and `docs/EFFECTS_GUIDE.md`
  - Document the new `quantac check --receipt` surface.

## Task 1: Type Checker Function Effect Summaries

- [ ] **Step 1: Write failing type checker tests**

Add these tests to the `#[cfg(test)]` module in `compiler/src/types/check.rs`:

```rust
    #[test]
    fn check_summary_records_declared_effects_and_capability_sources() {
        let source = r#"fn main() ~ Console { println!("ops"); }"#;
        let source_file = crate::lexer::SourceFile::new("summary_test.quanta", source);
        let mut lexer = crate::lexer::Lexer::new(&source_file);
        let tokens = lexer.tokenize().expect("tokenize summary fixture");
        let mut parser = crate::parser::Parser::new(&source_file, tokens);
        let module = parser.parse().expect("parse summary fixture");

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&module);

        let summaries = checker.function_effect_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].function, "main");
        assert_eq!(summaries[0].declared_effects, vec!["Console"]);
        assert_eq!(
            summaries[0]
                .observed_capabilities
                .get("Console")
                .expect("Console capability should be observed")
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["println!"]
        );
    }

    #[test]
    fn check_summaries_reset_between_modules() {
        let first = r#"fn main() ~ Console { println!("ops"); }"#;
        let second = r#"fn helper() {}"#;

        let parse_module = |name: &str, source: &str| {
            let source_file = crate::lexer::SourceFile::new(name, source);
            let mut lexer = crate::lexer::Lexer::new(&source_file);
            let tokens = lexer.tokenize().expect("tokenize summary fixture");
            let mut parser = crate::parser::Parser::new(&source_file, tokens);
            parser.parse().expect("parse summary fixture")
        };

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&parse_module("first.quanta", first));
        assert_eq!(checker.function_effect_summaries().len(), 1);

        checker.check_module(&parse_module("second.quanta", second));
        let summaries = checker.function_effect_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].function, "helper");
        assert!(summaries[0].declared_effects.is_empty());
        assert!(summaries[0].observed_capabilities.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml check_summary --quiet
```

Expected: FAIL because `function_effect_summaries` and summary fields do not exist.

- [ ] **Step 3: Add summary model and storage**

In `compiler/src/types/check.rs`, change the imports:

```rust
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
```

Add this public struct before `TypeChecker`:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionEffectSummary {
    pub function: String,
    pub declared_effects: Vec<String>,
    pub observed_capabilities: BTreeMap<String, BTreeSet<String>>,
}
```

Add this field to `TypeChecker`:

```rust
    function_effect_summaries: Vec<FunctionEffectSummary>,
```

Initialize it in `TypeChecker::new`:

```rust
            function_effect_summaries: Vec::new(),
```

Add this getter near `errors()`:

```rust
    pub fn function_effect_summaries(&self) -> &[FunctionEffectSummary] {
        &self.function_effect_summaries
    }
```

At the start of `check_module`, before the item loop, add:

```rust
        self.function_effect_summaries.clear();
```

- [ ] **Step 4: Populate summaries after body inference**

In `check_function`, after `let func_name = f.name.name.to_string();`, push:

```rust
            let mut declared_effects: Vec<String> = expected_effects
                .effects
                .iter()
                .map(|effect| effect.name.to_string())
                .collect();
            declared_effects.sort();
            self.function_effect_summaries.push(FunctionEffectSummary {
                function: func_name.clone(),
                declared_effects,
                observed_capabilities: capability_sources.clone(),
            });
```

- [ ] **Step 5: Run tests to verify GREEN**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml check_summary --quiet
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

Expected: both commands pass.

- [ ] **Step 6: Commit type checker summaries**

Run:

```powershell
git add compiler/src/types/check.rs
git commit -m "feat: expose check effect summaries"
```

## Task 2: CLI Receipt Emission

- [ ] **Step 1: Write failing CLI tests**

Add these tests to `compiler/tests/cli.rs` after `check_reports_capability_effect_for_ambient_file_call`:

```rust
#[test]
fn check_receipt_stdout_records_passing_capabilities() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_pass_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write passing receipt fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "passing receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON receipt");
    assert_eq!(receipt["schema"], "quantalang-check-receipt/v1");
    assert_eq!(receipt["status"], "passed");
    assert_eq!(receipt["declared_effects"]["main"], serde_json::json!(["Console"]));
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Console"],
        serde_json::json!(["println!"])
    );
    assert!(receipt["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn check_receipt_file_records_failing_capability_diagnostic() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_fail_{}.quanta",
        std::process::id()
    ));
    let receipt_path = fixture.with_extension("receipt.json");
    fs::write(&fixture, r#"fn main() { read_file("ops.txt"); }"#)
        .expect("write failing receipt fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg(&receipt_path)
        .output()
        .expect("run quantac check --receipt file");

    let receipt_text = fs::read_to_string(&receipt_path).expect("read receipt file");
    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&receipt_path);

    assert!(
        !output.status.success(),
        "failing capability check should return nonzero"
    );
    let receipt: serde_json::Value =
        serde_json::from_str(&receipt_text).expect("receipt file should be JSON");
    assert_eq!(receipt["schema"], "quantalang-check-receipt/v1");
    assert_eq!(receipt["status"], "failed");
    assert_eq!(
        receipt["observed_capabilities"]["main"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    let diagnostics = receipt["diagnostics"].as_array().expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            diag["stage"] == "type"
                && diag["kind"] == "UnhandledEffect"
                && diag["message"].as_str().unwrap_or("").contains("FileSystem")
        }),
        "expected FileSystem UnhandledEffect diagnostic in {diagnostics:#?}"
    );
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
```

Expected: FAIL because `check` does not accept `--receipt`.

- [ ] **Step 3: Extend the check command arguments**

In `compiler/src/main.rs`, change the check command to:

```rust
    Check {
        /// Input file
        file: PathBuf,

        /// Write a machine-readable check receipt to a path, or '-' for stdout
        #[arg(long, value_name = "PATH")]
        receipt: Option<PathBuf>,
    },
```

Change the command match arm to:

```rust
        Some(Commands::Check { file, receipt }) => cmd_check(&file, receipt.as_deref()),
```

- [ ] **Step 4: Add receipt data structures**

In `compiler/src/main.rs`, add these structs near the corpus receipt structs:

```rust
#[derive(serde::Serialize)]
struct CheckReceipt {
    schema: &'static str,
    compiler: &'static str,
    source: String,
    status: &'static str,
    items: usize,
    tokens: usize,
    declared_effects: BTreeMap<String, Vec<String>>,
    observed_capabilities: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    diagnostics: Vec<CheckReceiptDiagnostic>,
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

struct CheckOutcome {
    source: String,
    items: usize,
    tokens: usize,
    parse_errors: Vec<String>,
    type_errors: Vec<quantalang::types::TypeErrorWithSpan>,
    function_summaries: Vec<quantalang::types::FunctionEffectSummary>,
}
```

- [ ] **Step 5: Add diagnostic kind and receipt rendering helpers**

In `compiler/src/main.rs`, add:

```rust
fn type_error_kind(error: &quantalang::types::TypeError) -> &'static str {
    match error {
        quantalang::types::TypeError::TypeMismatch { .. } => "TypeMismatch",
        quantalang::types::TypeError::InfiniteType { .. } => "InfiniteType",
        quantalang::types::TypeError::MutabilityMismatch { .. } => "MutabilityMismatch",
        quantalang::types::TypeError::UnknownEffect { .. } => "UnknownEffect",
        quantalang::types::TypeError::UnhandledEffect { .. } => "UnhandledEffect",
        quantalang::types::TypeError::UndeclaredEffect { .. } => "UndeclaredEffect",
        quantalang::types::TypeError::UnknownEffectOperation { .. } => "UnknownEffectOperation",
        quantalang::types::TypeError::MissingHandlerClause { .. } => "MissingHandlerClause",
        _ => "TypeError",
    }
}

fn build_check_receipt(outcome: &CheckOutcome) -> CheckReceipt {
    let mut declared_effects = BTreeMap::new();
    let mut observed_capabilities = BTreeMap::new();
    for summary in &outcome.function_summaries {
        declared_effects.insert(summary.function.clone(), summary.declared_effects.clone());
        let mut caps = BTreeMap::new();
        for (effect, sources) in &summary.observed_capabilities {
            caps.insert(effect.clone(), sources.iter().cloned().collect::<Vec<_>>());
        }
        observed_capabilities.insert(summary.function.clone(), caps);
    }

    let mut diagnostics = Vec::new();
    diagnostics.extend(outcome.parse_errors.iter().map(|message| CheckReceiptDiagnostic {
        stage: "parse",
        kind: "ParseError".to_string(),
        message: message.clone(),
        help: None,
        notes: Vec::new(),
    }));
    diagnostics.extend(outcome.type_errors.iter().map(|err| CheckReceiptDiagnostic {
        stage: "type",
        kind: type_error_kind(&err.error).to_string(),
        message: err.error.to_string(),
        help: err.help.clone(),
        notes: err.notes.clone(),
    }));

    CheckReceipt {
        schema: "quantalang-check-receipt/v1",
        compiler: "quantac",
        source: outcome.source.clone(),
        status: if diagnostics.is_empty() { "passed" } else { "failed" },
        items: outcome.items,
        tokens: outcome.tokens,
        declared_effects,
        observed_capabilities,
        diagnostics,
    }
}
```

If the `other` binding creates a warning, replace the fallback arm with `_ => "TypeError"`.

- [ ] **Step 6: Refactor check execution into `run_check`**

Replace the body of `cmd_check` with a small orchestrator:

```rust
fn cmd_check(file: &Path, receipt: Option<&Path>) -> Result<(), i32> {
    let receipt_to_stdout = receipt == Some(Path::new("-"));
    let outcome = run_check(file, receipt_to_stdout)?;
    let receipt_value = receipt.map(|_| build_check_receipt(&outcome));

    render_check_human_output(&outcome, receipt_to_stdout);
    if let Some(receipt_value) = receipt_value {
        write_check_receipt(receipt.expect("receipt path is present"), &receipt_value)?;
    }

    if outcome.parse_errors.is_empty() && outcome.type_errors.is_empty() {
        Ok(())
    } else {
        Err(1)
    }
}
```

Create `run_check`, `render_check_human_output`, and `write_check_receipt` from the existing `cmd_check` code. Preserve these existing operations in order:

```rust
let source = std::fs::read_to_string(file)...
let source = resolve_imports(&source, file)?;
let chk_base = file.parent().unwrap_or(Path::new("."));
let source = preprocess_includes(&source, chk_base)?;
let source_file = SourceFile::new(file.to_string_lossy(), source);
let mut lexer = Lexer::new(&source_file);
let tokens = lexer.tokenize()?;
let token_count = tokens.len();
let mut parser = Parser::new(&source_file, tokens);
let mut ast = parser.parse().unwrap();
let parse_errors = parser.errors().to_vec();
resolve_modules(&mut ast, chk_base)?;
let mut ctx = TypeContext::new();
let mut checker = TypeChecker::new(&mut ctx);
checker.set_source_dir(chk_base.to_path_buf());
checker.check_module(&ast);
let type_errors = checker.errors().to_vec();
let function_summaries = checker.function_effect_summaries().to_vec();
```

`render_check_human_output` must write to stderr when `receipt_to_stdout` is true and otherwise preserve current stdout/stderr routing.

`write_check_receipt` must write pretty JSON:

```rust
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
```

- [ ] **Step 7: Run tests to verify GREEN**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_reports_capability_effect_for_ambient_file_call -- --nocapture
```

Expected: both pass.

- [ ] **Step 8: Commit CLI receipt emission**

Run:

```powershell
git add compiler/src/main.rs compiler/tests/cli.rs
git commit -m "feat: emit quantac check receipts"
```

## Task 3: Public Documentation

- [ ] **Step 1: Update README command table**

In `README.md`, update the `check` row to:

```markdown
| `quantac check <file> [--receipt PATH|-]` | Type-check and optionally emit a JSON accountability receipt |
```

- [ ] **Step 2: Add effects-guide receipt section**

In `docs/EFFECTS_GUIDE.md`, add this paragraph after the capability diagnostics paragraph:

```markdown
`quantac check <file> --receipt <path>` writes a deterministic
`quantalang-check-receipt/v1` JSON artifact with declared effects, observed
capability sources, pass/fail status, and compact diagnostics. Use
`--receipt -` when a CI step or wrapper wants the receipt on stdout.
```

- [ ] **Step 3: Run docs test**

Run:

```powershell
python -m pytest -q tests/test_docs_landing_page.py
```

Expected: `4 passed`.

- [ ] **Step 4: Commit docs**

Run:

```powershell
git add README.md docs/EFFECTS_GUIDE.md
git commit -m "docs: describe check receipts"
```

## Task 4: Final Verification And Push

- [ ] **Step 1: Format**

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
```

- [ ] **Step 2: Focused tests**

```powershell
cargo test --manifest-path compiler/Cargo.toml check_summary --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
cargo test --manifest-path compiler/Cargo.toml capability --quiet
python -m pytest -q tests/test_docs_landing_page.py
```

- [ ] **Step 3: Full compiler suite**

```powershell
cargo test --manifest-path compiler/Cargo.toml --quiet
```

- [ ] **Step 4: Warning-clean suite**

```powershell
$env:RUSTFLAGS='-Dwarnings'; cargo test --manifest-path compiler/Cargo.toml --quiet
```

- [ ] **Step 5: Hygiene**

```powershell
git diff --check
git diff origin/main..HEAD --check
git check-ignore -q .env
powershell -NoProfile -ExecutionPolicy Bypass -File C:/dev/scratch/portfolio-stabilization-2026-06-13/scan-diff-secrets.ps1 -Repo C:/dev/public/pubscan/quantalang
```

- [ ] **Step 6: Push and verify GitHub runs**

```powershell
git push origin main
gh run list -R HarperZ9/quantalang --branch main --limit 8 --json databaseId,workflowName,status,conclusion,headSha,displayTitle,createdAt
$head = git rev-parse HEAD
$runs = gh run list -R HarperZ9/quantalang --branch main --limit 8 --json databaseId,workflowName,headSha | ConvertFrom-Json
$ci = $runs | Where-Object { $_.headSha -eq $head -and $_.workflowName -eq "CI" } | Select-Object -First 1
$pages = $runs | Where-Object { $_.headSha -eq $head -and $_.workflowName -eq "pages-build-deployment" } | Select-Object -First 1
gh run watch $ci.databaseId -R HarperZ9/quantalang --exit-status
gh run watch $pages.databaseId -R HarperZ9/quantalang --exit-status
```

Expected: CI and Pages complete successfully for the pushed head.

## Self-Review Notes

- Every behavior change has a red test before implementation.
- The plan does not add syntax, signing, policy profiles, or backend receipt changes.
- Receipt evidence comes from `TypeChecker`, not CLI string scanning.
- The plan keeps existing check output behavior unless `--receipt -` needs stdout reserved for JSON.
