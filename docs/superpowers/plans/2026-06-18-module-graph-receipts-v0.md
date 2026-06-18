# Module Graph Receipts v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a checked module graph receipt that exposes the semantic corpus source input topology behind existing `input_graph_digest` evidence.

**Architecture:** Mirror the recent symbol graph receipt pattern with a focused `module_graph` module that builds and verifies `quantalang-module-graph-receipt/v0`. Reuse the existing check/lowering input ledger so receipt inputs and digests stay aligned with `quantalang-check-receipt/v1`, then aggregate the receipt through `quantac corpus verify` and the substrate receipt.

**Tech Stack:** Rust, `serde`, `sha2`, existing `quantac` CLI test harness, semantic corpus JSON receipts.

## Global Constraints

- The canonical semantic corpus currently has eight single-file programs; the receipt must honestly record entry-only graphs for those programs.
- v0 records entry/include/import/module inputs and program-to-input edges only.
- v0 must not claim full name resolution, package public API graph, LSP navigation readiness, call graph, or package registry dependency completion.
- All paths in semantic-corpus receipts must be corpus-relative and must reject absolute paths, drive-prefixed paths, rooted paths, `..`, and paths outside the corpus root.
- Manual edits use `apply_patch`.
- Write failing tests before production code.
- Run targeted regression slices, not the full suite by default.

---

## File Structure

- Create `compiler/src/module_graph.rs`: public receipt builder, path validation, digest helpers, verifier, summary helpers, and unit tests.
- Modify `compiler/src/main.rs`: import the module, read and verify the module receipt in `cmd_corpus_verify`, print `module graph receipt: ok`, extend substrate receipt structs and validation.
- Modify `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`: add `module_surface`.
- Create `semantic-corpus/receipts/module-graph-2026-06-18.json`: canonical checked receipt.
- Modify `compiler/tests/cli.rs`: add module graph corpus verify drift tests and substrate module receipt tests.

---

### Task 1: Receipt Contract Tests

**Files:**
- Modify: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: existing `temp_semantic_corpus`, `assert_corpus_verify_rejects`, `quantac`, `read_json`, and JSON mutation helpers in `compiler/tests/cli.rs`.
- Produces: failing tests that define expected CLI behavior before compiler implementation.

- [ ] **Step 1: Write failing corpus verify success test**

Add:

```rust
#[test]
fn corpus_verify_accepts_module_graph_receipt() {
    if !c_backend_available() {
        eprintln!("skipping module graph receipt verification because no C backend is available");
        return;
    }
    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run quantac corpus verify with module graph receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept module graph receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("module graph receipt: ok"),
        "corpus verify should report module graph receipt status:\n{}",
        stdout
    );
}
```

- [ ] **Step 2: Write failing receipt drift tests**

Add a helper near the existing receipt-copy helpers:

```rust
fn write_module_graph_receipt_copy<F>(corpus_root: &Path, mutate: F)
where
    F: FnOnce(serde_json::Value) -> serde_json::Value,
{
    let receipt_path = corpus_root
        .join("receipts")
        .join("module-graph-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read module graph receipt"))
            .expect("parse module graph receipt");
    let rendered = serde_json::to_string_pretty(&mutate(receipt))
        .expect("render modified module graph receipt");
    fs::write(&receipt_path, format!("{rendered}\n"))
        .expect("write modified module graph receipt");
}
```

Then add focused tampering tests:

```rust
#[test]
fn corpus_verify_rejects_module_graph_schema_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_schema");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("quantalang-module-graph-receipt/v9".into());
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "module graph receipt has unsupported schema");
}

#[test]
fn corpus_verify_rejects_module_graph_program_count_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_program_count");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph source_set.program_count mismatch",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_path_escape() {
    let corpus_root = temp_semantic_corpus("module_graph_path_escape");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] = serde_json::Value::String("../outside.quanta".into());
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program.path must stay within corpus root",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_source_digest_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_source_digest");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program scalar_branch source_digest mismatch",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_input_graph_digest_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_input_graph_digest");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["input_graph_digest"]["hex"] =
            serde_json::Value::String("1".repeat(64));
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program scalar_branch input_graph_digest mismatch",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_inputs_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_inputs");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["inputs"][0]["role"] =
            serde_json::Value::String("forged".into());
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program scalar_branch inputs drift",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_edges_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_edges");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["edges"][0]["kind"] =
            serde_json::Value::String("forged_edge".into());
        receipt
    });

    assert_corpus_verify_rejects(
        &corpus_root,
        "module graph program scalar_branch edges drift",
    );
}

#[test]
fn corpus_verify_rejects_module_graph_summary_drift() {
    let corpus_root = temp_semantic_corpus("module_graph_summary");
    write_module_graph_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["input_count"] = serde_json::Value::from(999);
        receipt
    });

    assert_corpus_verify_rejects(&corpus_root, "module graph summary drift");
}
```

- [ ] **Step 3: Run tests and verify RED**

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --test cli module_graph -- --nocapture
```

Expected: tests fail because `module-graph-2026-06-18.json` is missing and
`quantac corpus verify` does not print `module graph receipt: ok`.

- [ ] **Step 4: Commit the red tests after implementation turns them green**

Commit after Task 4 passes:

```text
git add compiler/tests/cli.rs
git commit -m "test: require module graph receipts"
```

---

### Task 2: Module Graph Builder

**Files:**
- Create: `compiler/src/module_graph.rs`
- Modify: `compiler/src/main.rs`

**Interfaces:**
- Consumes: `SemanticCorpusManifest`, `SemanticCorpusProgram`, `CheckReceiptSourceDigest`, `CheckReceiptInputDigest`, `InputDigestLedger`, `input_graph_digest`, `source_text_digest_hex`, `read_json`.
- Produces:
  - `pub(crate) const MODULE_GRAPH_RECEIPT: &str = "module-graph-2026-06-18.json";`
  - `pub(crate) struct ModuleGraphReceipt`
  - `pub(crate) fn build_module_graph_receipt(root: &Path, manifest: &SemanticCorpusManifest) -> Result<ModuleGraphReceipt, String>`
  - `pub(crate) fn verify_module_graph_receipt(root: &Path, receipt: &ModuleGraphReceipt, manifest: &SemanticCorpusManifest) -> Result<(), String>`

- [ ] **Step 1: Add module declaration and import seam**

In `compiler/src/main.rs`, add:

```rust
mod module_graph;
use module_graph::{verify_module_graph_receipt, ModuleGraphReceipt, MODULE_GRAPH_RECEIPT};
```

- [ ] **Step 2: Implement receipt model**

In `compiler/src/module_graph.rs`, define serializable/deserializable structs:

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct ModuleGraphReceipt {
    pub(crate) schema: String,
    pub(crate) receipt_id: String,
    pub(crate) created_at: String,
    pub(crate) compiler: String,
    pub(crate) language: String,
    pub(crate) source_set: ModuleGraphSourceSet,
    pub(crate) module_model: ModuleGraphModel,
    pub(crate) programs: Vec<ModuleGraphProgram>,
    pub(crate) summary: ModuleGraphSummary,
}
```

Add companion structs for `ModuleGraphSourceSet`, `ModuleGraphModel`,
`ModuleGraphProgram`, `ModuleGraphInput`, `ModuleGraphEdge`,
`ModuleGraphSummary`, and an internal `ProgramDigestProjection<'a>`.

- [ ] **Step 3: Implement path and digest helpers**

Add helpers:

```rust
fn validate_corpus_relative_path(root: &Path, relative: &str, field: &str) -> Result<PathBuf, String>
fn corpus_relative_path(root: &Path, path: &Path, field: &str) -> Result<String, String>
fn digest_hex(bytes: &[u8]) -> String
fn module_digest(bytes: &[u8]) -> CheckReceiptSourceDigest
fn module_model() -> ModuleGraphModel
```

Use the same lexical path rejection rules as `symbol_graph::validate_corpus_relative_path`.

- [ ] **Step 4: Build per-program graph from input digests**

For each manifest program:

1. Validate the entry path.
2. Read entry bytes.
3. Re-run the same source pipeline as `check_file` through import resolution,
   include preprocessing, parsing, and module resolution with an
   `InputDigestLedger::text_normalized()`.
4. Convert sorted ledger records into `ModuleGraphInput` values.
5. Create one edge per input role:
   - `entry` -> `program_entry`
   - `include` -> `program_include`
   - `import` -> `program_import`
   - `module` -> `program_module`
6. Compute `module_graph_digest` from id, path, source digest,
   input graph digest, inputs, edges, and known gaps.

- [ ] **Step 5: Add summary unit test and verify RED/GREEN**

Add a unit test in `module_graph.rs`:

```rust
#[test]
fn summary_sorts_and_deduplicates_roles_and_edges() {
    let digest = CheckReceiptSourceDigest {
        algorithm: "sha256",
        hex: "0".repeat(64),
    };
    let programs = vec![
        ModuleGraphProgram {
            id: "left".to_string(),
            path: "programs/left.quanta".to_string(),
            source_digest: digest.clone(),
            input_graph_digest: digest.clone(),
            module_graph_digest: digest.clone(),
            inputs: vec![
                ModuleGraphInput {
                    id: "input:module:programs/shared.quanta".to_string(),
                    role: "module".to_string(),
                    path: "programs/shared.quanta".to_string(),
                    source_digest: digest.clone(),
                },
                ModuleGraphInput {
                    id: "input:entry:programs/left.quanta".to_string(),
                    role: "entry".to_string(),
                    path: "programs/left.quanta".to_string(),
                    source_digest: digest.clone(),
                },
            ],
            edges: vec![
                ModuleGraphEdge {
                    kind: "program_module".to_string(),
                    from: "program:left".to_string(),
                    to: "input:module:programs/shared.quanta".to_string(),
                },
                ModuleGraphEdge {
                    kind: "program_entry".to_string(),
                    from: "program:left".to_string(),
                    to: "input:entry:programs/left.quanta".to_string(),
                },
            ],
            known_gaps: program_known_gaps(),
        },
        ModuleGraphProgram {
            id: "right".to_string(),
            path: "programs/right.quanta".to_string(),
            source_digest: digest.clone(),
            input_graph_digest: digest.clone(),
            module_graph_digest: digest.clone(),
            inputs: vec![ModuleGraphInput {
                id: "input:entry:programs/right.quanta".to_string(),
                role: "entry".to_string(),
                path: "programs/right.quanta".to_string(),
                source_digest: digest,
            }],
            edges: vec![ModuleGraphEdge {
                kind: "program_entry".to_string(),
                from: "program:right".to_string(),
                to: "input:entry:programs/right.quanta".to_string(),
            }],
            known_gaps: program_known_gaps(),
        },
    ];
    let summary = summarize_programs(&programs);
    assert_eq!(summary.input_roles, vec!["entry", "module"]);
    assert_eq!(summary.edge_kinds, vec!["program_entry", "program_module"]);
    assert_eq!(summary.input_count, 3);
}
```

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --bin quantac module_graph --quiet
```

Expected after implementation: PASS.

---

### Task 3: Module Graph Verifier and Corpus Integration

**Files:**
- Modify: `compiler/src/module_graph.rs`
- Modify: `compiler/src/main.rs`

**Interfaces:**
- Consumes: `build_module_graph_receipt`.
- Produces: corpus verification that reads the module graph receipt, compares it structurally, and prints `module graph receipt: ok`.

- [ ] **Step 1: Implement verifier comparison**

`verify_module_graph_receipt` should:

1. Validate schema/compiler/language/source_set.
2. Validate `source_set.manifest` points to `manifest.json`.
3. Rebuild the expected receipt.
4. Compare receipt id, created_at, module model, programs, per-program
   digests, inputs, edges, known gaps, and summary.
5. Return diagnostics using the design wording.

- [ ] **Step 2: Integrate in `cmd_corpus_verify`**

Add:

```rust
let module_receipt_path = receipts_dir.join(MODULE_GRAPH_RECEIPT);
let module_receipt: ModuleGraphReceipt = read_json(&module_receipt_path)?;
verify_module_graph_receipt(&corpus_root, &module_receipt, &manifest)?;
```

Print after memory layout and before symbol graph:

```rust
println!("module graph receipt: ok");
```

- [ ] **Step 3: Run module graph tests**

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --test cli module_graph -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --bin quantac module_graph --quiet
```

Expected after receipt artifact exists: PASS.

---

### Task 4: Substrate Integration

**Files:**
- Modify: `compiler/src/main.rs`
- Modify: `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`
- Modify: `compiler/tests/cli.rs`

**Interfaces:**
- Consumes: `MODULE_GRAPH_RECEIPT`.
- Produces: substrate validation for `module_surface.module_receipt`.

- [ ] **Step 1: Add substrate struct**

In `main.rs`, add `module_surface: SubstrateModuleSurface` to
`SubstrateReceipt` and define:

```rust
#[derive(serde::Deserialize)]
struct SubstrateModuleSurface {
    resolver: String,
    digest_anchor: String,
    module_receipt: String,
    #[serde(default)]
    known_gaps: Vec<String>,
}
```

- [ ] **Step 2: Validate substrate module surface**

In `validate_substrate_receipt`, require:

```rust
receipt.module_surface.resolver == "quantac source input resolver"
receipt.module_surface.digest_anchor == "quantalang-check-receipt/v1 input_graph_digest"
receipt.module_surface.module_receipt == format!("receipts/{MODULE_GRAPH_RECEIPT}")
```

Then validate the path exists under the corpus root with `validate_substrate_path`.

- [ ] **Step 3: Add substrate drift tests**

Add tests that mutate `module_surface.module_receipt` to a missing path and an
escaping path, then assert `quantac corpus verify` fails with a substrate
module surface diagnostic.

- [ ] **Step 4: Run substrate slice**

Run:

```text
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
```

Expected: PASS.

---

### Task 5: Canonical Receipt Artifact

**Files:**
- Create: `semantic-corpus/receipts/module-graph-2026-06-18.json`
- Modify: `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json`

**Interfaces:**
- Consumes: `build_module_graph_receipt`.
- Produces: checked-in canonical receipt and substrate reference.

- [ ] **Step 1: Generate canonical receipt**

Use a temporary helper or test-only debug print if needed, then remove it. The
checked-in JSON must have:

```json
"schema": "quantalang-module-graph-receipt/v0"
```

and a program count of `8`.

- [ ] **Step 2: Update substrate JSON**

Insert `module_surface` after `representation_surface` and before
`symbol_surface`, matching the design.

- [ ] **Step 3: Run corpus verifier**

Run:

```text
cargo run --manifest-path compiler/Cargo.toml -- corpus verify --root semantic-corpus
```

Expected stdout includes:

```text
module graph receipt: ok
symbol graph receipt: ok
c execution: 8 passed
```

---

### Task 6: Final Verification and Commits

**Files:**
- All changed files.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: verified commits.

- [ ] **Step 1: Run targeted slices**

Run:

```text
cargo fmt --manifest-path compiler/Cargo.toml -- --check
cargo test --manifest-path compiler/Cargo.toml --bin quantac module_graph --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli module_graph -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
cargo run --manifest-path compiler/Cargo.toml -- corpus verify --root semantic-corpus
git diff --check
```

- [ ] **Step 2: Run credential and `.env` commit gate**

Run:

```text
git check-ignore .env .env.local .env.production
git diff --cached | rg -n "(?i)(api[_-]?key|token=|password=|BEGIN (RSA|OPENSSH|PRIVATE)|AKIA|xox[baprs]-|sk-[A-Za-z0-9])"
```

Expected: `.env` paths are ignored; credential scan returns no matches.

- [ ] **Step 3: Commit implementation**

Use small commits if practical:

```text
git add compiler/tests/cli.rs
git commit -m "test: require module graph receipts"
git add compiler/src/main.rs compiler/src/module_graph.rs
git commit -m "feat: verify module graph receipts"
git add semantic-corpus/receipts/module-graph-2026-06-18.json semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json
git commit -m "docs: record module graph receipt"
```

## Self-Review

- Spec coverage: builder, verifier, substrate integration, canonical receipt,
  drift tests, and targeted verification are covered.
- Placeholder scan: no `TBD`, `TODO`, or `implement later` placeholders are
  present.
- Type consistency: exported names match the planned `main.rs` imports and the
  receipt filename matches the design.
