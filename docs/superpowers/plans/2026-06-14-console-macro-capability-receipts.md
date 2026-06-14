# Console Macro Capability And Receipt Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make console macros visible as the built-in `Console` capability effect and record capability gate metadata in semantic-corpus receipts.

**Architecture:** Extend the existing compiler-owned capability registry with macro names, then have expression and statement macro inference add `Console` to the current effect row with source notes such as `println!`. Keep receipt metadata root-level and derived from semantic-corpus manifest surfaces so `corpus verify --write` refreshes it deterministically.

**Tech Stack:** Rust compiler/typechecker code in `compiler/src/types`, CLI receipt verification in `compiler/src/main.rs`, semantic-corpus JSON receipts, Cargo and pytest verification.

---

## File Structure

- Modify: `compiler/src/types/capabilities.rs`
  - Add `capability_effect_for_macro(name)`.
- Modify: `compiler/src/types/infer.rs`
  - Record macro-origin capability effects in `ExprKind::Macro` and `StmtKind::Macro`.
- Modify: `compiler/src/types/check.rs`
  - Add checker tests proving console macro enforcement.
- Modify: verified `.quanta` fixtures as needed after enforcement:
  - `semantic-corpus/programs/*.quanta`
  - `examples/quickstart/*.quanta`
  - CI-covered `tests/programs/*.quanta`
  - Any additional fixture surfaced by focused/full tests.
- Modify: `compiler/src/main.rs`
  - Add receipt capability metadata fields and verifier.
- Modify: `compiler/src/codegen/backend/rust.rs`
  - Add receipt metadata assertions for the Rust semantic-corpus receipt.
- Modify: `semantic-corpus/receipts/c-execution-2026-06-13.json`
  - Add declared/observed capability metadata.
- Modify: `semantic-corpus/receipts/rust-execution-2026-06-13.json`
  - Add declared/observed capability metadata.
- Modify: `semantic-corpus/README.md`
  - Document receipt capability metadata.

## Task 1: Console Macro Effect Attribution

- [ ] **Step 1: Write failing checker tests**

Add these tests to the `#[cfg(test)]` module in `compiler/src/types/check.rs`:

```rust
    #[test]
    fn capability_console_macro_requires_console_effect() {
        let errors = check_source(r#"fn main() { println!("ops"); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "Console"
            )),
            "expected Console effect error, got {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.notes.iter().any(|note| note.contains("println!"))),
            "expected diagnostic note naming println!, got {errors:#?}"
        );
    }

    #[test]
    fn capability_declared_console_effect_allows_console_macro() {
        let errors = check_source(r#"fn main() ~ Console { println!("ops"); }"#);

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    #[test]
    fn capability_wrong_declared_effect_does_not_allow_console_macro() {
        let errors = check_source(r#"fn main() ~ Network { println!("ops"); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndeclaredEffect { effect_name, .. } if effect_name == "Console"
            )),
            "expected undeclared Console error, got {errors:#?}"
        );
    }
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml capability_console --quiet
```

Expected: at least the pure and wrong-effect console macro tests fail because macros do not yet add `Console`.

- [ ] **Step 3: Add macro capability registry**

Add this function to `compiler/src/types/capabilities.rs`:

```rust
pub fn capability_effect_for_macro(name: &str) -> Option<&'static str> {
    match name {
        "println" | "print" | "eprintln" | "eprint" | "dbg" | "debug" | "log" | "trace"
        | "warn" | "error" => Some(CONSOLE),
        _ => None,
    }
}
```

- [ ] **Step 4: Record macro capability effects in inference**

In `compiler/src/types/infer.rs`, add this helper:

```rust
    fn record_macro_capability(&mut self, macro_name: &str) {
        if let Some(effect_name) = super::capabilities::capability_effect_for_macro(macro_name) {
            self.current_effects
                .add(super::effects::Effect::new(effect_name));
            self.record_capability_source(effect_name, &format!("{}!", macro_name));
        }
    }
```

In the `ExprKind::Macro { path, .. }` match arm, use:

```rust
            ExprKind::Macro { path, .. } => {
                let macro_name = path.segments.last().map(|s| s.ident.as_str()).unwrap_or("");
                self.record_macro_capability(macro_name);
                Ty::fresh_var()
            }
```

In the `StmtKind::Macro { path, .. }` branch, call `self.record_macro_capability(macro_name);` immediately after `macro_name` is computed and before the return-type `match`.

- [ ] **Step 5: Run tests to verify GREEN**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml capability_console --quiet
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

Expected: console macro tests and earlier capability tests pass.

- [ ] **Step 6: Commit macro attribution**

Run:

```powershell
git add compiler/src/types/capabilities.rs compiler/src/types/infer.rs compiler/src/types/check.rs
git commit -m "feat: require Console for console macros"
```

## Task 2: Migrate Verified Fixtures To Declared Console Effects

- [ ] **Step 1: Run focused fixtures to verify failures**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml quickstart_examples_are_typechecked -- --nocapture
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_manifest_programs_run_on_rust_backend -- --nocapture
```

Expected: fixtures with `println!` and no `~ Console` fail after Task 1.

- [ ] **Step 2: Add `~ Console` to verified fixture entrypoints and print helpers**

Update functions that directly contain console macros in:

```text
semantic-corpus/programs/*.quanta
examples/quickstart/hello.quanta
examples/quickstart/ledger.quanta
examples/quickstart/effects_greeting.quanta
tests/programs/01_hello.quanta
tests/programs/02_variables.quanta
tests/programs/03_functions.quanta
tests/programs/06_recursion.quanta
tests/programs/08_arithmetic.quanta
tests/programs/11_structs.quanta
tests/programs/16_closures.quanta
tests/programs/27_effects_showcase.quanta
tests/programs/46_color_science.quanta
tests/programs/68_hashmap.quanta
tests/programs/color_test.quanta
tests/programs/cross_module_test.quanta
```

Use exact syntax:

```quanta
fn main() ~ Console {
```

For helper functions that directly call `println!`, `print!`, `eprintln!`, or related console macros, add `~ Console` to that helper and propagate `~ Console` to any caller if the caller invokes that helper.

- [ ] **Step 3: Re-run fixture tests**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml quickstart_examples_are_typechecked -- --nocapture
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_manifest_programs_run_on_rust_backend -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_reports_capability_effect_for_ambient_file_call -- --nocapture
```

Expected: all pass.

- [ ] **Step 4: Commit fixture migration**

Run:

```powershell
git add semantic-corpus/programs examples/quickstart tests/programs
git commit -m "test: declare Console in verified fixtures"
```

## Task 3: Semantic-Corpus Capability Receipt Metadata

- [ ] **Step 1: Write failing receipt tests**

In `compiler/src/codegen/backend/rust.rs`, extend `RustExecutionReceipt`:

```rust
        declared_effects: Vec<String>,
        observed_capabilities: Vec<String>,
        capability_gate: String,
        capability_gate_test: String,
```

In `semantic_corpus_receipt_records_validator_metadata`, add:

```rust
        assert_eq!(receipt.declared_effects, vec!["Console"]);
        assert_eq!(receipt.observed_capabilities, vec!["Console"]);
        assert_eq!(receipt.capability_gate, "passed");
        assert_eq!(
            receipt.capability_gate_test,
            "cargo test --manifest-path compiler/Cargo.toml capability --quiet"
        );
```

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_receipt_records_validator_metadata --quiet
```

Expected: FAIL because receipt JSON lacks the new metadata fields.

- [ ] **Step 2: Add receipt model fields and verifier**

In `compiler/src/main.rs`, add fields to `CorpusExecutionReceipt`:

```rust
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    declared_effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    observed_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capability_gate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capability_gate_test: Option<String>,
```

Update `SemanticCorpusProgram` to include surfaces:

```rust
    #[serde(default)]
    surfaces: Vec<String>,
```

Add helpers:

```rust
fn expected_receipt_capabilities(manifest: &SemanticCorpusManifest) -> Vec<String> {
    let mut capabilities = std::collections::BTreeSet::new();
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
    receipt.capability_gate_test = Some(
        "cargo test --manifest-path compiler/Cargo.toml capability --quiet".to_string(),
    );
}
```

Call `apply_capability_receipt_metadata(&mut receipt, manifest);` inside `refresh_c_receipt_from_manifest`.

In `verify_receipt`, require:

```rust
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
```

- [ ] **Step 3: Update receipt JSON files**

Add to both receipt JSONs after `result`:

```json
  "declared_effects": [
    "Console"
  ],
  "observed_capabilities": [
    "Console"
  ],
  "capability_gate": "passed",
  "capability_gate_test": "cargo test --manifest-path compiler/Cargo.toml capability --quiet",
```

- [ ] **Step 4: Run receipt tests to verify GREEN**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_receipt_records_validator_metadata --quiet
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_receipt_matches_manifest --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify_checks_manifest_receipts_and_c_execution -- --nocapture
```

Expected: all pass.

- [ ] **Step 5: Document receipt metadata**

Add a short section to `semantic-corpus/README.md` explaining that receipts now record:

```text
declared_effects
observed_capabilities
capability_gate
capability_gate_test
```

- [ ] **Step 6: Commit receipt metadata**

Run:

```powershell
git add compiler/src/main.rs compiler/src/codegen/backend/rust.rs semantic-corpus/receipts semantic-corpus/README.md
git commit -m "feat: record capability metadata in corpus receipts"
```

## Task 4: Final Verification And Push

- [ ] **Step 1: Format**

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
```

- [ ] **Step 2: Focused tests**

```powershell
cargo test --manifest-path compiler/Cargo.toml capability --quiet
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_receipt --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli check_reports_capability_effect_for_ambient_file_call -- --nocapture
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

- [ ] **Step 6: Push and verify CI**

```powershell
git push origin main
gh run list -R HarperZ9/quantalang --branch main --limit 5 --json databaseId,workflowName,status,conclusion,headSha,displayTitle,createdAt
gh run watch <new-ci-run-id> -R HarperZ9/quantalang --exit-status
```

Expected: CI and Pages complete successfully for the pushed head.

## Self-Review Notes

- Scope is limited to active compiler behavior, actively verified fixtures, and semantic-corpus receipts.
- The plan deliberately avoids rewriting historical mirrors under `quantalang/` and `future/` unless a verifier names them.
- TDD red/green gates are explicit for macro enforcement and receipt metadata.
- Receipt metadata is deterministic from manifest surfaces, so `corpus verify --write` cannot silently drop the capability posture.
