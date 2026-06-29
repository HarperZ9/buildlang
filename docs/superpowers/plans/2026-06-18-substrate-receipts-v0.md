# Substrate Receipts v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a first `buildlang-substrate-receipt/v0` sample receipt and verify it through `buildc corpus verify` so CPU, Rust subset, memory, semantic, representation, and evidence claims are checked through one native substrate contract.

**Architecture:** Keep the first slice inside the existing semantic corpus verification path. Add a checked-in substrate receipt under `semantic-corpus/receipts/`, deserialize it in `compiler/src/main.rs`, validate it structurally against the semantic corpus manifest and referenced execution receipts, then report `substrate receipt: ok` from `buildc corpus verify`.

**Tech Stack:** Rust 2021, Clap-powered `buildc` CLI, Serde/serde_json, existing semantic corpus receipt model, Cargo CLI integration tests.

## Global Constraints

- The schema string is exactly `buildlang-substrate-receipt/v0`.
- The first receipt is an evidence aggregation layer, not a backend promotion claim.
- C may be marked `production-anchor` only when backed by an execution receipt with stdout validation evidence.
- Rust may be marked `experimental-subset` only when backed by explicit subset execution or metadata evidence.
- Experimental or unverified backends must keep explicit maturity labels and unsupported behavior posture.
- `memory_surface.known_gaps` and `representation_surface.fallback_policy` are required.
- No SPIR-V, LLVM, WASM, x86-64, ARM64, or self-hosted compiler production claim is added in this slice.

---

## File Structure

- Create `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`: canonical v0 substrate receipt for the current 8-program semantic corpus.
- Modify `compiler/tests/cli.rs`: add CLI tests for valid substrate receipt verification and invalid copied-corpus fixtures.
- Modify `compiler/src/main.rs`: add substrate receipt DTOs, validation helpers, and hook into `cmd_corpus_verify`.
- Modify `README.md`: document Substrate Receipts near the backend/semantic corpus receipt section.
- Modify `STATUS.md`: add a concise status note that substrate receipts aggregate evidence and do not promote experimental backends.

---

### Task 1: Add Substrate Receipt Fixture And Red CLI Tests

**Files:**
- Create: `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`
- Modify: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: existing `temp_semantic_corpus(label: &str) -> PathBuf`, `buildc() -> Command`, `repo_root() -> PathBuf`, and `c_backend_ready() -> bool` helpers in `compiler/tests/cli.rs`.
- Produces: failing CLI tests that define the exact verifier behavior required by later tasks.

- [ ] **Step 1: Create the substrate receipt fixture**

Create `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json` with this exact JSON:

```json
{
  "schema": "buildlang-substrate-receipt/v0",
  "receipt_id": "substrate-semantic-corpus-2026-06-18",
  "created_at": "2026-06-18",
  "compiler": "buildc",
  "language": "buildlang",
  "source_set": {
    "kind": "semantic-corpus",
    "manifest": "manifest.json",
    "program_count": 8
  },
  "semantic_surface": {
    "check_receipt_schema": "buildlang-check-receipt/v1",
    "requires_source_digest": true,
    "requires_input_graph_digest": true,
    "effect_surfaces": [
      "declared_effects",
      "observed_capabilities",
      "propagated_effects"
    ]
  },
  "execution_surface": {
    "c": {
      "target": "c",
      "maturity": "production-anchor",
      "evidence_class": "generated-artifact-execution",
      "receipt": "receipts/c-execution-2026-06-13.json"
    },
    "rust": {
      "target": "rust",
      "maturity": "experimental-subset",
      "evidence_class": "generated-artifact-execution",
      "receipt": "receipts/rust-execution-2026-06-13.json",
      "unsupported_mir_policy": "unsupported MIR returns a codegen error rather than silent fallback"
    },
    "spirv": {
      "target": "spirv",
      "maturity": "experimental-unverified",
      "evidence_class": "not-yet-runtime-verified",
      "status": "unverified",
      "unsupported_mir_policy": "GPU validation requires a future spirv-val or Vulkan-host receipt"
    }
  },
  "memory_surface": {
    "ownership_model": "rust-inspired",
    "verified_surfaces": [
      "references_mutation",
      "tuple_ownership_reuse",
      "struct_aggregate_reuse",
      "field_assignment_reuse",
      "nested_field_reuse",
      "deref_reuse"
    ],
    "known_gaps": [
      "full interprocedural borrow proof",
      "self-hosted stdlib execution",
      "runtime-linked async execution"
    ]
  },
  "representation_surface": {
    "ir": "MIR",
    "fallback_policy": "unsupported or partial targets must not claim production maturity",
    "backend_maturity_descriptor": "compiler/src/codegen/backend/STATUS.md"
  },
  "evidence_surface": {
    "commands": [
      "cargo test --manifest-path compiler/Cargo.toml semantic_corpus_manifest --quiet",
      "cargo test --manifest-path compiler/Cargo.toml semantic_corpus_receipt --quiet",
      "cargo test --manifest-path compiler/Cargo.toml generated_rust_runs --quiet",
      "cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture"
    ]
  }
}
```

- [ ] **Step 2: Add a helper for writing modified substrate receipts in copied corpus tests**

In `compiler/tests/cli.rs`, add this helper after `temp_semantic_corpus`:

```rust
fn write_substrate_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("substrate-semantic-corpus-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read substrate receipt"))
            .expect("parse substrate receipt");
    let receipt = transform(receipt);
    let rendered =
        serde_json::to_string_pretty(&receipt).expect("render modified substrate receipt");
    fs::write(&receipt_path, format!("{rendered}\n")).expect("write modified substrate receipt");
}
```

- [ ] **Step 3: Add the valid receipt CLI test**

In `compiler/tests/cli.rs`, add this test after `corpus_verify_checks_manifest_receipts_and_c_execution`:

```rust
#[test]
fn corpus_verify_checks_substrate_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping substrate receipt verification because no C backend is available");
        return;
    }

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run buildc corpus verify with substrate receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept substrate receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("substrate receipt: ok"),
        "corpus verify should report substrate receipt status:\n{}",
        stdout
    );
}
```

- [ ] **Step 4: Add invalid schema test**

In `compiler/tests/cli.rs`, add this test after the valid receipt test:

```rust
#[test]
fn corpus_verify_rejects_substrate_receipt_schema_drift() {
    if !c_backend_ready() {
        eprintln!("skipping substrate schema drift verification because no C backend is available");
        return;
    }

    let corpus_root = temp_semantic_corpus("substrate_schema");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("buildlang-substrate-receipt/v9".into());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against bad substrate schema");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject bad substrate schema"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate receipt has unsupported schema"),
        "stderr should name schema drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 5: Add program count mismatch test**

In `compiler/tests/cli.rs`, add:

```rust
#[test]
fn corpus_verify_rejects_substrate_program_count_drift() {
    if !c_backend_ready() {
        eprintln!("skipping substrate program-count verification because no C backend is available");
        return;
    }

    let corpus_root = temp_semantic_corpus("substrate_program_count");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against bad substrate program count");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate program count drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate source_set.program_count mismatch"),
        "stderr should name program-count drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 6: Add production backend missing receipt test**

In `compiler/tests/cli.rs`, add:

```rust
#[test]
fn corpus_verify_rejects_production_substrate_backend_without_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping substrate production receipt verification because no C backend is available");
        return;
    }

    let corpus_root = temp_semantic_corpus("substrate_missing_receipt");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        if let Some(c) = receipt["execution_surface"]["c"].as_object_mut() {
            c.remove("receipt");
        }
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against missing production receipt");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject production backend without receipt"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate execution_surface.c is production-anchor but receipt is missing"),
        "stderr should name missing production receipt:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 7: Add empty evidence commands test**

In `compiler/tests/cli.rs`, add:

```rust
#[test]
fn corpus_verify_rejects_empty_substrate_evidence_commands() {
    if !c_backend_ready() {
        eprintln!("skipping substrate evidence command verification because no C backend is available");
        return;
    }

    let corpus_root = temp_semantic_corpus("substrate_empty_commands");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["evidence_surface"]["commands"] = serde_json::Value::Array(Vec::new());
        receipt
    });

    let output = buildc()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run buildc corpus verify against empty substrate commands");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject empty evidence commands"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate evidence_surface.commands must not be empty"),
        "stderr should name empty evidence commands:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 8: Run red test slice**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
```

Expected: FAIL. The valid test fails because `buildc corpus verify` does not yet report `substrate receipt: ok`; invalid tests fail because no substrate verifier exists.

- [ ] **Step 9: Commit red tests and fixture**

```powershell
git add semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json compiler/tests/cli.rs
git commit -m "test: require substrate receipt verification"
```

---

### Task 2: Implement Substrate Receipt Validation

**Files:**
- Modify: `compiler/src/main.rs`

**Interfaces:**
- Consumes: `SemanticCorpusManifest`, `CorpusExecutionReceipt`, `read_json<T>()`, and `verify_receipt(...)`.
- Produces: `verify_substrate_receipt(...) -> Result<(), i32>`, used by `cmd_corpus_verify`.

- [ ] **Step 1: Add substrate receipt DTOs**

In `compiler/src/main.rs`, after `CorpusExecutionProgram`, add:

```rust
#[derive(serde::Deserialize)]
struct SubstrateReceipt {
    schema: String,
    receipt_id: String,
    created_at: String,
    compiler: String,
    language: String,
    source_set: SubstrateSourceSet,
    semantic_surface: SubstrateSemanticSurface,
    execution_surface: BTreeMap<String, SubstrateExecutionTarget>,
    memory_surface: SubstrateMemorySurface,
    representation_surface: SubstrateRepresentationSurface,
    evidence_surface: SubstrateEvidenceSurface,
}

#[derive(serde::Deserialize)]
struct SubstrateSourceSet {
    kind: String,
    manifest: String,
    program_count: usize,
}

#[derive(serde::Deserialize)]
struct SubstrateSemanticSurface {
    check_receipt_schema: String,
    requires_source_digest: bool,
    requires_input_graph_digest: bool,
    #[serde(default)]
    effect_surfaces: Vec<String>,
}

#[derive(serde::Deserialize)]
struct SubstrateExecutionTarget {
    target: String,
    maturity: String,
    evidence_class: String,
    #[serde(default)]
    receipt: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    unsupported_mir_policy: Option<String>,
}

#[derive(serde::Deserialize)]
struct SubstrateMemorySurface {
    ownership_model: String,
    #[serde(default)]
    verified_surfaces: Vec<String>,
    #[serde(default)]
    known_gaps: Vec<String>,
}

#[derive(serde::Deserialize)]
struct SubstrateRepresentationSurface {
    ir: String,
    fallback_policy: String,
    backend_maturity_descriptor: String,
}

#[derive(serde::Deserialize)]
struct SubstrateEvidenceSurface {
    #[serde(default)]
    commands: Vec<String>,
}
```

- [ ] **Step 2: Add small validation helpers**

After `write_json`, add:

```rust
fn require_non_empty(value: &str, field: &str) -> Result<(), i32> {
    if value.trim().is_empty() {
        eprintln!("substrate {field} must not be empty");
        return Err(1);
    }
    Ok(())
}

fn require_substrate_path(root: &Path, relative: &str, field: &str) -> Result<PathBuf, i32> {
    require_non_empty(relative, field)?;
    let path = root.join(relative);
    if !path.is_file() {
        eprintln!(
            "substrate {field} path not found: {}",
            path.display()
        );
        return Err(1);
    }
    Ok(path)
}

fn receipt_has_stdout_validator(receipt: &CorpusExecutionReceipt) -> bool {
    receipt
        .validator_chain
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case("stdout assertion"))
}
```

- [ ] **Step 3: Add the substrate verifier**

After `verify_receipt`, add:

```rust
fn verify_substrate_receipt(
    corpus_root: &Path,
    receipt: &SubstrateReceipt,
    manifest: &SemanticCorpusManifest,
) -> Result<(), i32> {
    if receipt.schema != "buildlang-substrate-receipt/v0" {
        eprintln!(
            "substrate receipt has unsupported schema '{}'",
            receipt.schema
        );
        return Err(1);
    }
    if receipt.compiler != "buildc" {
        eprintln!(
            "substrate compiler mismatch: expected 'buildc', found '{}'",
            receipt.compiler
        );
        return Err(1);
    }
    if receipt.language != "buildlang" {
        eprintln!(
            "substrate language mismatch: expected 'buildlang', found '{}'",
            receipt.language
        );
        return Err(1);
    }
    require_non_empty(&receipt.receipt_id, "receipt_id")?;
    require_non_empty(&receipt.created_at, "created_at")?;

    if receipt.source_set.kind != "semantic-corpus" {
        eprintln!(
            "substrate source_set.kind mismatch: expected 'semantic-corpus', found '{}'",
            receipt.source_set.kind
        );
        return Err(1);
    }
    let manifest_path =
        require_substrate_path(corpus_root, &receipt.source_set.manifest, "source_set.manifest")?;
    if manifest_path != corpus_root.join("manifest.json") {
        eprintln!(
            "substrate source_set.manifest must point at manifest.json, found {}",
            receipt.source_set.manifest
        );
        return Err(1);
    }
    if receipt.source_set.program_count != manifest.programs.len() {
        eprintln!(
            "substrate source_set.program_count mismatch: expected {}, found {}",
            manifest.programs.len(),
            receipt.source_set.program_count
        );
        return Err(1);
    }

    if receipt.semantic_surface.check_receipt_schema != "buildlang-check-receipt/v1" {
        eprintln!(
            "substrate semantic_surface.check_receipt_schema mismatch: found '{}'",
            receipt.semantic_surface.check_receipt_schema
        );
        return Err(1);
    }
    if !receipt.semantic_surface.requires_source_digest {
        eprintln!("substrate semantic_surface.requires_source_digest must be true");
        return Err(1);
    }
    if !receipt.semantic_surface.requires_input_graph_digest {
        eprintln!("substrate semantic_surface.requires_input_graph_digest must be true");
        return Err(1);
    }
    for required in [
        "declared_effects",
        "observed_capabilities",
        "propagated_effects",
    ] {
        if !receipt
            .semantic_surface
            .effect_surfaces
            .iter()
            .any(|surface| surface == required)
        {
            eprintln!("substrate semantic_surface.effect_surfaces missing {required}");
            return Err(1);
        }
    }

    if receipt.execution_surface.is_empty() {
        eprintln!("substrate execution_surface must not be empty");
        return Err(1);
    }
    for (label, target) in &receipt.execution_surface {
        require_non_empty(&target.target, &format!("execution_surface.{label}.target"))?;
        require_non_empty(
            &target.maturity,
            &format!("execution_surface.{label}.maturity"),
        )?;
        require_non_empty(
            &target.evidence_class,
            &format!("execution_surface.{label}.evidence_class"),
        )?;

        match target.maturity.as_str() {
            "production-anchor" => {
                let Some(relative_receipt) = target.receipt.as_deref() else {
                    eprintln!(
                        "substrate execution_surface.{label} is production-anchor but receipt is missing"
                    );
                    return Err(1);
                };
                let execution_receipt_path = require_substrate_path(
                    corpus_root,
                    relative_receipt,
                    &format!("execution_surface.{label}.receipt"),
                )?;
                let execution_receipt: CorpusExecutionReceipt = read_json(&execution_receipt_path)?;
                if execution_receipt.backend != target.target {
                    eprintln!(
                        "substrate execution_surface.{label}.receipt backend mismatch: expected '{}', found '{}'",
                        target.target, execution_receipt.backend
                    );
                    return Err(1);
                }
                if !receipt_has_stdout_validator(&execution_receipt) {
                    eprintln!(
                        "substrate execution_surface.{label} production-anchor requires stdout assertion evidence"
                    );
                    return Err(1);
                }
            }
            "experimental-subset" => {
                if target.receipt.is_none()
                    && target
                        .unsupported_mir_policy
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .is_empty()
                {
                    eprintln!(
                        "substrate execution_surface.{label} experimental-subset requires receipt or unsupported_mir_policy"
                    );
                    return Err(1);
                }
                if let Some(relative_receipt) = target.receipt.as_deref() {
                    let execution_receipt_path = require_substrate_path(
                        corpus_root,
                        relative_receipt,
                        &format!("execution_surface.{label}.receipt"),
                    )?;
                    let execution_receipt: CorpusExecutionReceipt = read_json(&execution_receipt_path)?;
                    if execution_receipt.backend != target.target {
                        eprintln!(
                            "substrate execution_surface.{label}.receipt backend mismatch: expected '{}', found '{}'",
                            target.target, execution_receipt.backend
                        );
                        return Err(1);
                    }
                }
            }
            maturity if maturity.starts_with("experimental") => {
                if target.status.as_deref() != Some("unverified")
                    && target
                        .unsupported_mir_policy
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .is_empty()
                {
                    eprintln!(
                        "substrate execution_surface.{label} experimental target requires status=unverified or unsupported_mir_policy"
                    );
                    return Err(1);
                }
            }
            other => {
                eprintln!("substrate execution_surface.{label} has unknown maturity '{other}'");
                return Err(1);
            }
        }
    }

    require_non_empty(&receipt.memory_surface.ownership_model, "memory_surface.ownership_model")?;
    if receipt.memory_surface.known_gaps.is_empty() {
        eprintln!("substrate memory_surface.known_gaps must not be empty");
        return Err(1);
    }
    if receipt.memory_surface.verified_surfaces.is_empty() {
        eprintln!("substrate memory_surface.verified_surfaces must not be empty");
        return Err(1);
    }

    if receipt.representation_surface.ir != "MIR" {
        eprintln!(
            "substrate representation_surface.ir mismatch: expected 'MIR', found '{}'",
            receipt.representation_surface.ir
        );
        return Err(1);
    }
    require_non_empty(
        &receipt.representation_surface.fallback_policy,
        "representation_surface.fallback_policy",
    )?;
    require_non_empty(
        &receipt.representation_surface.backend_maturity_descriptor,
        "representation_surface.backend_maturity_descriptor",
    )?;

    if receipt.evidence_surface.commands.is_empty() {
        eprintln!("substrate evidence_surface.commands must not be empty");
        return Err(1);
    }
    if !receipt
        .evidence_surface
        .commands
        .iter()
        .all(|command| !command.trim().is_empty())
    {
        eprintln!("substrate evidence_surface.commands must contain only non-empty commands");
        return Err(1);
    }

    Ok(())
}
```

- [ ] **Step 4: Run focused tests**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
```

Expected: still FAIL until `cmd_corpus_verify` calls the verifier.

- [ ] **Step 5: Commit verifier helpers**

```powershell
git add compiler/src/main.rs
git commit -m "feat: add substrate receipt verifier"
```

---

### Task 3: Wire Substrate Verification Into `buildc corpus verify`

**Files:**
- Modify: `compiler/src/main.rs`

**Interfaces:**
- Consumes: `verify_substrate_receipt(...) -> Result<(), i32>` from Task 2.
- Produces: `buildc corpus verify` validates `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json` and prints `substrate receipt: ok`.

- [ ] **Step 1: Read and validate substrate receipt in `cmd_corpus_verify`**

Inside `cmd_corpus_verify`, after:

```rust
let c_receipt_path = receipts_dir.join("c-execution-2026-06-13.json");
let rust_receipt_path = receipts_dir.join("rust-execution-2026-06-13.json");
```

add:

```rust
let substrate_receipt_path =
    receipts_dir.join("substrate-semantic-corpus-2026-06-18.json");
let substrate_receipt: SubstrateReceipt = read_json(&substrate_receipt_path)?;
verify_substrate_receipt(
    &corpus_root,
    &substrate_receipt,
    &manifest,
)?;
```

- [ ] **Step 2: Print substrate status**

Inside `cmd_corpus_verify`, after:

```rust
println!("rust receipt: ok");
```

add:

```rust
println!("substrate receipt: ok");
```

- [ ] **Step 3: Run the substrate CLI test slice**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
```

Expected: PASS. If C backend is unavailable on the machine, these tests print skip messages and pass without assertions.

- [ ] **Step 4: Run the existing corpus verification tests**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
```

Expected: PASS. Existing tests should now also tolerate the extra `substrate receipt: ok` output.

- [ ] **Step 5: Run semantic corpus receipt guards**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_manifest --quiet
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_receipt --quiet
```

Expected: PASS.

- [ ] **Step 6: Commit corpus verify integration**

```powershell
git add compiler/src/main.rs compiler/tests/cli.rs
git commit -m "feat: verify substrate receipt in corpus checks"
```

---

### Task 4: Document Substrate Receipts And Run Gates

**Files:**
- Modify: `README.md`
- Modify: `STATUS.md`

**Interfaces:**
- Consumes: implemented `buildc corpus verify` output from Task 3.
- Produces: public docs that describe substrate receipts as evidence aggregation, not backend promotion.

- [ ] **Step 1: Update README backend receipt section**

In `README.md`, in the backend section immediately after the paragraph beginning ``The Rust target emits source for a subset of MIR``, add:

```markdown
`buildc corpus verify` also validates a Substrate Receipt
(`buildlang-substrate-receipt/v0`) for the same semantic corpus. This receipt
aggregates existing evidence across semantic, execution, memory, representation,
and command surfaces: C remains the production execution anchor, Rust remains an
experimental subset lane, and unverified GPU/native lanes must keep explicit
maturity and unsupported-behavior labels. The receipt is an evidence contract,
not a backend promotion claim.
```

- [ ] **Step 2: Update STATUS summary**

In `STATUS.md`, in the long summary paragraph under `## Summary`, add this sentence after the sentence that mentions `buildc corpus verify`:

```markdown
The same corpus path now carries a `buildlang-substrate-receipt/v0` aggregation receipt that checks source-set size, backend maturity, memory gaps, representation fallback policy, and evidence commands without promoting experimental backends.
```

- [ ] **Step 3: Run formatting and focused verification**

Run:

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_manifest --quiet
cargo test --manifest-path compiler/Cargo.toml semantic_corpus_receipt --quiet
```

Expected: all commands exit 0, except C-backend-dependent CLI tests may self-skip when no C compiler is available.

- [ ] **Step 4: Run diff and secret hygiene**

Run:

```powershell
git diff --check
git diff -- README.md STATUS.md compiler/src/main.rs compiler/tests/cli.rs semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json | rg --ignore-case "api[_-]?key|secret|token|password|credential|BEGIN (RSA|OPENSSH|PRIVATE)|AKIA|sk-[A-Za-z0-9]"
git check-ignore -v .env .env.local
```

Expected: `git diff --check` exits 0; the credential-shaped diff scan exits 1 with no matches; `.env` and `.env.local` are ignored by `.gitignore`.

- [ ] **Step 5: Commit docs and final verified state**

```powershell
git add README.md STATUS.md
git commit -m "docs: describe substrate receipt evidence"
```

If README/STATUS edits are committed together with Task 3 in the actual execution branch, do not create an empty commit. Instead run:

```powershell
git status --short
```

Expected: no unstaged documentation changes remain.

---

## Plan Self-Review

- Spec coverage: Tasks cover the v0 sample receipt, verifier, schema/source-set/program-count/backend-maturity/memory-gap/representation/evidence validation, docs updates, and existing semantic corpus gates.
- Intentional scope limit: The plan does not add a generated receipt writer or new backend proof. That matches the design's first-slice constraint.
- Type consistency: Later tasks consume `SubstrateReceipt`, `verify_substrate_receipt`, and `write_substrate_receipt_copy` exactly as defined earlier.
- Open-marker scan: The plan contains only concrete tasks with fixed implementation choices.
