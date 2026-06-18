# Doctor Substrate Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `quantac doctor` print a diagnostic `Substrate evidence:` section derived from the verified substrate receipt without changing doctor exit semantics.

**Architecture:** Keep the substrate receipt as the source of truth. Split substrate verification into a pure validation helper plus the existing hard-gate printing wrapper, then reuse the pure helper to render doctor-only evidence rows on stdout. `quantac corpus verify` remains the command that exits nonzero and prints field-level drift diagnostics.

**Tech Stack:** Rust 2021, Clap-powered `quantac` CLI, Serde/serde_json, existing semantic corpus receipt model, Cargo CLI integration tests.

## Global Constraints

- `quantac doctor` remains diagnostic-only and exits successfully for missing or invalid substrate evidence.
- `quantac corpus verify` remains the hard failing gate for substrate receipt drift.
- No new substrate schema version is introduced.
- No new backend production claim is added.
- No SPIR-V, LLVM, WASM, x86-64, or ARM64 execution proof is added.
- No generated substrate receipt writer is added.
- No new `quantac substrate` command is added.
- Doctor output must be derived from the substrate receipt content, not README or STATUS prose.
- Missing substrate evidence is reported on stdout as `receipt   missing`.
- Invalid substrate evidence is reported on stdout as `receipt   invalid`.
- Valid substrate evidence is reported on stdout as `receipt   ok`.

---

## File Structure

- Modify `compiler/tests/cli.rs`: add red CLI expectations for the new doctor substrate output.
- Modify `compiler/src/main.rs`: add quiet JSON loading, pure substrate validation, doctor evidence row formatting, unit coverage for missing/invalid evidence, and doctor output wiring.
- Modify `README.md`: mention that `doctor` reports substrate evidence posture.

---

### Task 1: Add Red Doctor Substrate Tests

**Files:**
- Modify: `compiler/tests/cli.rs`
- Modify: `compiler/src/main.rs`

**Interfaces:**
- Consumes: existing `quantac() -> Command`, `repo_root() -> PathBuf`, and `doctor_reports_adoption_readiness_summary`.
- Produces: failing tests that define `quantac doctor` substrate output and helper behavior.

- [ ] **Step 1: Extend the doctor CLI smoke test**

In `compiler/tests/cli.rs`, inside `doctor_reports_adoption_readiness_summary`, extend the `expected` array to include these strings after `"Backend maturity:"`:

```rust
        "Substrate evidence:",
        "receipt   ok",
        "quantalang-substrate-receipt/v0",
        "corpus    ok",
        "8 semantic program(s)",
        "c         anchor",
        "rust      subset",
        "spirv     unverified",
        "memory    partial",
        "6 verified surface(s), 3 known gap(s)",
        "repr      MIR",
```

The full expected array should become:

```rust
    for expected in [
        "QuantaLang Doctor",
        "quantac:",
        "C backend:",
        "stdlib:",
        "registry:",
        "Backend maturity:",
        "Substrate evidence:",
        "receipt   ok",
        "quantalang-substrate-receipt/v0",
        "corpus    ok",
        "8 semantic program(s)",
        "c         anchor",
        "rust      subset",
        "spirv     unverified",
        "memory    partial",
        "6 verified surface(s), 3 known gap(s)",
        "repr      MIR",
        "c        primary",
        "rust     experimental",
    ] {
        assert!(
            stdout.contains(expected),
            "doctor output should contain {expected:?}:\n{}",
            stdout
        );
    }
```

- [ ] **Step 2: Add unit tests for missing and invalid substrate rows**

In `compiler/src/main.rs`, inside the existing `#[cfg(test)] mod tests`, add these tests after `language_version_string_matches_public_tuple`:

```rust
    #[test]
    fn doctor_substrate_rows_report_missing_when_root_is_absent() {
        assert_eq!(
            substrate_evidence_rows(None),
            vec![
                "  receipt   missing  run quantac corpus verify from a repository checkout"
                    .to_string()
            ]
        );
    }

    #[test]
    fn doctor_substrate_rows_report_invalid_when_receipt_is_malformed() {
        let root = std::env::temp_dir().join(format!(
            "quantalang_doctor_substrate_invalid_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("receipts")).expect("create substrate fixture");
        std::fs::write(
            root.join("manifest.json"),
            r#"{
  "schema": "quantalang-semantic-corpus/v1",
  "programs": []
}
"#,
        )
        .expect("write malformed-doctor manifest");
        std::fs::write(
            root.join("receipts")
                .join("substrate-semantic-corpus-2026-06-18.json"),
            r#"{
  "schema": "quantalang-substrate-receipt/v9"
}
"#,
        )
        .expect("write malformed-doctor substrate receipt");

        assert_eq!(
            substrate_evidence_rows(Some(&root)),
            vec!["  receipt   invalid  run quantac corpus verify for details".to_string()]
        );

        let _ = std::fs::remove_dir_all(&root);
    }
```

- [ ] **Step 3: Run the unit red test**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml doctor_substrate --quiet
```

Expected: FAIL to compile because `substrate_evidence_rows` does not exist yet.

- [ ] **Step 4: Run the CLI red test**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli doctor -- --nocapture
```

Expected: FAIL because `quantac doctor` does not yet print `Substrate evidence:`.

- [ ] **Step 5: Commit red tests**

Run:

```powershell
git add compiler/tests/cli.rs compiler/src/main.rs
git commit -m "test: require doctor substrate evidence"
```

---

### Task 2: Add Quiet Substrate Validation And Doctor Rows

**Files:**
- Modify: `compiler/src/main.rs`

**Interfaces:**
- Consumes: `SemanticCorpusManifest`, `SubstrateReceipt`, `SubstrateExecutionTarget`, `CorpusExecutionReceipt`, `find_semantic_corpus_root()`, and the existing substrate receipt fixture path.
- Produces:
  - `read_json_quiet<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String>`
  - `validate_substrate_receipt(corpus_root: &Path, receipt: &SubstrateReceipt, manifest: &SemanticCorpusManifest) -> Result<(), String>`
  - `substrate_evidence_rows(corpus_root: Option<&Path>) -> Vec<String>`

- [ ] **Step 1: Replace `read_json` with a quiet base helper**

In `compiler/src/main.rs`, replace the existing `read_json` function with:

```rust
fn read_json_quiet<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    serde_json::from_str(&content)
        .map_err(|err| format!("failed to parse {}: {}", path.display(), err))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, i32> {
    read_json_quiet(path).map_err(|message| {
        eprintln!("{message}");
        1
    })
}
```

- [ ] **Step 2: Add pure substrate validation helpers**

Replace `require_non_empty` and `require_substrate_path` with pure validation helpers:

```rust
fn validate_non_empty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("substrate {field} must not be empty"));
    }
    Ok(())
}

fn validate_substrate_path(root: &Path, relative: &str, field: &str) -> Result<PathBuf, String> {
    validate_non_empty(relative, field)?;
    let path = root.join(relative);
    if !path.is_file() {
        return Err(format!(
            "substrate {field} path not found: {}",
            path.display()
        ));
    }
    Ok(path)
}
```

Keep `receipt_has_stdout_validator` unchanged.

- [ ] **Step 3: Convert `verify_substrate_receipt` into a printing wrapper**

Rename the current validation body to `validate_substrate_receipt` and update each validation failure from:

```rust
eprintln!("message");
return Err(1);
```

to:

```rust
return Err("message".to_string());
```

For formatted messages, use `format!`:

```rust
return Err(format!(
    "substrate receipt has unsupported schema '{}'",
    receipt.schema
));
```

For referenced execution receipts, use the quiet JSON loader:

```rust
let execution_receipt: CorpusExecutionReceipt = read_json_quiet(&execution_receipt_path)?;
```

After the pure function, re-add the public hard-gate wrapper with the same signature currently used by `cmd_corpus_verify`:

```rust
fn verify_substrate_receipt(
    corpus_root: &Path,
    receipt: &SubstrateReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), i32> {
    validate_substrate_receipt(corpus_root, receipt, manifest).map_err(|message| {
        eprintln!("{message}");
        1
    })
}
```

- [ ] **Step 4: Add substrate doctor row helpers**

After `verify_substrate_receipt`, add:

```rust
fn substrate_invalid_rows() -> Vec<String> {
    vec!["  receipt   invalid  run quantac corpus verify for details".to_string()]
}

fn substrate_missing_rows() -> Vec<String> {
    vec![
        "  receipt   missing  run quantac corpus verify from a repository checkout".to_string(),
    ]
}

fn substrate_target<'a>(
    receipt: &'a SubstrateReceipt,
    target: &str,
) -> Result<&'a SubstrateExecutionTarget, ()> {
    receipt.execution_surface.get(target).ok_or(())
}

fn substrate_evidence_rows(corpus_root: Option<&Path>) -> Vec<String> {
    let Some(corpus_root) = corpus_root else {
        return substrate_missing_rows();
    };
    let manifest_path = corpus_root.join("manifest.json");
    let substrate_receipt_path = corpus_root
        .join("receipts")
        .join("substrate-semantic-corpus-2026-06-18.json");

    if !manifest_path.is_file() || !substrate_receipt_path.is_file() {
        return substrate_missing_rows();
    }

    let manifest: SemanticCorpusManifest = match read_json_quiet(&manifest_path) {
        Ok(manifest) => manifest,
        Err(_) => return substrate_invalid_rows(),
    };
    let receipt: SubstrateReceipt = match read_json_quiet(&substrate_receipt_path) {
        Ok(receipt) => receipt,
        Err(_) => return substrate_invalid_rows(),
    };

    if validate_substrate_receipt(corpus_root, &receipt, &manifest).is_err() {
        return substrate_invalid_rows();
    }

    let Ok(c_target) = substrate_target(&receipt, "c") else {
        return substrate_invalid_rows();
    };
    let Ok(rust_target) = substrate_target(&receipt, "rust") else {
        return substrate_invalid_rows();
    };
    let Ok(spirv_target) = substrate_target(&receipt, "spirv") else {
        return substrate_invalid_rows();
    };

    let c_status = match c_target.maturity.as_str() {
        "production-anchor" => "anchor",
        _ => return substrate_invalid_rows(),
    };
    let rust_status = match rust_target.maturity.as_str() {
        "experimental-subset" => "subset",
        _ => return substrate_invalid_rows(),
    };
    let spirv_status = if spirv_target.status.as_deref() == Some("unverified")
        || !spirv_target
            .unsupported_mir_policy
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        "unverified"
    } else {
        return substrate_invalid_rows();
    };

    vec![
        format!("  receipt   ok       {}", receipt.schema),
        format!(
            "  corpus    ok       {} semantic program(s)",
            manifest.programs.len()
        ),
        format!("  c         {c_status}   production execution evidence"),
        format!("  rust      {rust_status}   experimental executable subset"),
        format!("  spirv     {spirv_status} explicit unsupported-MIR posture"),
        format!(
            "  memory    partial  {} verified surface(s), {} known gap(s)",
            receipt.memory_surface.verified_surfaces.len(),
            receipt.memory_surface.known_gaps.len()
        ),
        format!(
            "  repr      {}      fallback policy recorded",
            receipt.representation_surface.ir
        ),
    ]
}
```

- [ ] **Step 5: Run focused unit tests**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml doctor_substrate --quiet
```

Expected: PASS. The missing and invalid helper tests pass without printing substrate validation errors to stderr.

- [ ] **Step 6: Verify hard corpus errors are preserved**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
```

Expected: PASS. Existing invalid substrate receipt CLI tests still see the same stderr substrings from `quantac corpus verify`.

- [ ] **Step 7: Confirm doctor CLI is still red**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli doctor -- --nocapture
```

Expected: FAIL because the helper exists but `cmd_doctor` is not wired to print it yet.

- [ ] **Step 8: Commit quiet validation and rows**

Run:

```powershell
git add compiler/src/main.rs
git commit -m "feat: prepare substrate evidence rows for doctor"
```

---

### Task 3: Wire Substrate Evidence Into `quantac doctor`

**Files:**
- Modify: `compiler/src/main.rs`

**Interfaces:**
- Consumes: `substrate_evidence_rows(corpus_root: Option<&Path>) -> Vec<String>` from Task 2.
- Produces: `cmd_doctor() -> Result<(), i32>` output containing `Substrate evidence:`.

- [ ] **Step 1: Add the printing helper**

In `compiler/src/main.rs`, after `print_tool_probe`, add:

```rust
fn print_substrate_evidence(corpus_root: Option<&Path>) {
    println!();
    println!("Substrate evidence:");
    for row in substrate_evidence_rows(corpus_root) {
        println!("{row}");
    }
}
```

- [ ] **Step 2: Call the helper from `cmd_doctor`**

In `cmd_doctor`, after the backend maturity rows and before the final practical readiness section, replace:

```rust
    println!();
    if c_compiler.is_some() {
```

with:

```rust
    let corpus_root = find_semantic_corpus_root();
    print_substrate_evidence(corpus_root.as_deref());

    println!();
    if c_compiler.is_some() {
```

- [ ] **Step 3: Run the doctor CLI test**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli doctor -- --nocapture
```

Expected: PASS. The output includes `Substrate evidence:` and the valid receipt rows.

- [ ] **Step 4: Run substrate and corpus verification slices**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
```

Expected: PASS. `quantac corpus verify` remains the hard failing gate for substrate drift.

- [ ] **Step 5: Commit doctor wiring**

Run:

```powershell
git add compiler/src/main.rs compiler/tests/cli.rs
git commit -m "feat: report substrate evidence in doctor"
```

---

### Task 4: Document Doctor Substrate Evidence And Run Gates

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: `quantac doctor` output from Task 3.
- Produces: concise public docs that name doctor substrate visibility without promoting any backend.

- [ ] **Step 1: Update README doctor description**

In `README.md`, in the Install section paragraph immediately after `quantac doctor`, replace:

```markdown
`doctor` reports the installed compiler version, C-backend readiness, stdlib and
local registry discovery, optional backend tools, and the current backend
maturity table.
```

with:

```markdown
`doctor` reports the installed compiler version, C-backend readiness, stdlib and
local registry discovery, optional backend tools, the current backend maturity
table, and the substrate receipt evidence posture for the semantic corpus.
```

- [ ] **Step 2: Run formatting and focused tests**

Run:

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
cargo test --manifest-path compiler/Cargo.toml doctor_substrate --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli doctor -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
```

Expected: all commands exit 0.

- [ ] **Step 3: Run diff and secret hygiene**

Run:

```powershell
git diff --check
git diff --unified=0 -- README.md compiler/src/main.rs compiler/tests/cli.rs | rg --ignore-case "^\+[^+].*(api[_-]?key\s*[:=]|secret\s*[:=]|token\s*[:=]|password\s*[:=]|credential\s*[:=]|BEGIN (RSA|OPENSSH|PRIVATE)|AKIA|sk-[A-Za-z0-9])"
git check-ignore -v .env .env.local
```

Expected:

- `git diff --check` exits 0.
- The added-line credential-shaped scan exits 1 with no matches.
- `.env` and `.env.local` are ignored by `.gitignore`.

- [ ] **Step 4: Commit docs**

Run:

```powershell
git add README.md
git commit -m "docs: mention doctor substrate evidence"
```

---

## Plan Self-Review

- Spec coverage: Task 1 defines doctor output and missing/invalid helper behavior; Task 2 adds quiet validation and row derivation from receipt fields; Task 3 wires doctor output while preserving `corpus verify`; Task 4 documents and gates the change.
- Red-flag scan: The plan contains no unfinished implementation steps; each code-changing step includes concrete code or exact replacement text.
- Type consistency: `read_json_quiet`, `validate_substrate_receipt`, `substrate_evidence_rows`, and `print_substrate_evidence` are defined before later tasks consume them.
- Scope check: The plan does not add a new command, schema version, generated receipt writer, backend proof, or doctor failure mode.
