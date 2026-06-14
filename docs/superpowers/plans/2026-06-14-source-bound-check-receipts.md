# Source-Bound Check Receipts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bind every `quantac check --receipt` artifact to the exact checked source bytes with compiler version, language version, and SHA-256 digest metadata.

**Architecture:** Keep receipt rendering in `compiler/src/main.rs`, where the existing check pipeline already reads source files and builds `CheckOutcome`. Add a small source digest type and helper, compute digest from raw file bytes before import/include preprocessing, carry the metadata through `CheckOutcome`, and serialize it as additive `quantalang-check-receipt/v1` fields.

**Tech Stack:** Rust 2021, `sha2` for SHA-256, `serde`/`serde_json`, Clap CLI tests in `compiler/tests/cli.rs`, existing Cargo test gates.

---

## File Structure

- Modify `compiler/Cargo.toml`: add the RustCrypto `sha2` crate under normal dependencies because the binary needs it at runtime.
- Modify `compiler/src/main.rs`: add `CheckReceiptSourceDigest`, add source metadata to `CheckReceipt` and `CheckOutcome`, add `language_version_string`, `source_digest_hex`, and `source_digest` tests, then compute the digest from source bytes.
- Modify `compiler/tests/cli.rs`: extend existing receipt tests and add path/content determinism tests through the built `quantac` binary.
- Modify `docs/EFFECTS_GUIDE.md`: document that check receipts are source-bound.
- Modify `README.md`: mention source digest metadata in the capability-effects receipt description if wording still reads too weak.

## Task 1: Add Source Digest Red Tests

**Files:**
- Modify: `compiler/tests/cli.rs`

- [ ] **Step 1: Extend the passing receipt test with required metadata assertions**

In `compiler/tests/cli.rs`, inside `check_receipt_stdout_records_passing_capabilities`, after:

```rust
assert_eq!(receipt["schema"], "quantalang-check-receipt/v1");
assert_eq!(receipt["status"], "passed");
```

insert:

```rust
assert_eq!(receipt["compiler"], "quantac");
assert_eq!(receipt["compiler_version"], env!("CARGO_PKG_VERSION"));
assert_eq!(receipt["language_version"], "1.0.0");
assert_eq!(receipt["source_digest"]["algorithm"], "sha256");
let digest = receipt["source_digest"]["hex"]
    .as_str()
    .expect("source digest hex string");
assert_eq!(digest.len(), 64);
assert!(
    digest.chars().all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
    "digest should be lowercase hex: {digest}"
);
```

- [ ] **Step 2: Extend the failing receipt test with required metadata assertions**

In `compiler/tests/cli.rs`, inside `check_receipt_file_records_failing_capability_diagnostic`, after:

```rust
assert_eq!(receipt["schema"], "quantalang-check-receipt/v1");
assert_eq!(receipt["status"], "failed");
```

insert:

```rust
assert_eq!(receipt["compiler_version"], env!("CARGO_PKG_VERSION"));
assert_eq!(receipt["language_version"], "1.0.0");
assert_eq!(receipt["source_digest"]["algorithm"], "sha256");
assert_eq!(
    receipt["source_digest"]["hex"]
        .as_str()
        .expect("failing receipt digest")
        .len(),
    64
);
```

- [ ] **Step 3: Add a helper for reading receipt JSON from stdout**

Near the top-level CLI test helpers in `compiler/tests/cli.rs`, add:

```rust
fn receipt_from_stdout(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout should be JSON receipt: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
```

If an existing local helper already parses receipt stdout after Task 1 edits,
reuse that helper instead of adding a duplicate.

- [ ] **Step 4: Add identical-content digest test**

Add this test after `check_receipt_file_records_failing_capability_diagnostic`:

```rust
#[test]
fn check_receipt_source_digest_ignores_path_for_identical_content() {
    let id = std::process::id();
    let left = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_digest_left_{id}.quanta"
    ));
    let right = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_digest_right_{id}.quanta"
    ));
    let source = r#"fn main() ~ Console { println!("same"); }"#;
    fs::write(&left, source).expect("write left digest fixture");
    fs::write(&right, source).expect("write right digest fixture");

    let left_output = quantac()
        .arg("check")
        .arg(&left)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run left digest receipt");
    let right_output = quantac()
        .arg("check")
        .arg(&right)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run right digest receipt");

    let _ = fs::remove_file(&left);
    let _ = fs::remove_file(&right);

    assert!(left_output.status.success(), "left check should pass");
    assert!(right_output.status.success(), "right check should pass");
    let left_receipt = receipt_from_stdout(&left_output);
    let right_receipt = receipt_from_stdout(&right_output);
    assert_ne!(left_receipt["source"], right_receipt["source"]);
    assert_eq!(
        left_receipt["source_digest"]["hex"],
        right_receipt["source_digest"]["hex"]
    );
}
```

- [ ] **Step 5: Add changed-content digest test**

Add this test after the identical-content test:

```rust
#[test]
fn check_receipt_source_digest_changes_when_source_changes() {
    let id = std::process::id();
    let first = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_digest_first_{id}.quanta"
    ));
    let second = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_digest_second_{id}.quanta"
    ));
    fs::write(&first, r#"fn main() ~ Console { println!("first"); }"#)
        .expect("write first digest fixture");
    fs::write(&second, r#"fn main() ~ Console { println!("second"); }"#)
        .expect("write second digest fixture");

    let first_output = quantac()
        .arg("check")
        .arg(&first)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run first digest receipt");
    let second_output = quantac()
        .arg("check")
        .arg(&second)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run second digest receipt");

    let _ = fs::remove_file(&first);
    let _ = fs::remove_file(&second);

    assert!(first_output.status.success(), "first check should pass");
    assert!(second_output.status.success(), "second check should pass");
    let first_receipt = receipt_from_stdout(&first_output);
    let second_receipt = receipt_from_stdout(&second_output);
    assert_ne!(
        first_receipt["source_digest"]["hex"],
        second_receipt["source_digest"]["hex"]
    );
}
```

- [ ] **Step 6: Run red CLI tests**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
```

Expected: FAIL because `compiler_version`, `language_version`, and `source_digest` are not emitted yet.

- [ ] **Step 7: Commit red tests**

```powershell
git add compiler/tests/cli.rs
git commit -m "test: require source-bound check receipts"
```

## Task 2: Add SHA-256 Digest Helper

**Files:**
- Modify: `compiler/Cargo.toml`
- Modify: `compiler/src/main.rs`

- [ ] **Step 1: Add `sha2` dependency**

In `compiler/Cargo.toml`, under `serde_json = "1.0"`, add:

```toml
sha2 = "0.10"
```

- [ ] **Step 2: Import the hasher**

At the top of `compiler/src/main.rs`, after the Clap import, add:

```rust
use sha2::{Digest, Sha256};
```

- [ ] **Step 3: Add source digest helpers**

In `compiler/src/main.rs`, after `type_error_kind`, add:

```rust
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
```

- [ ] **Step 4: Add unit tests for helpers**

At the end of `compiler/src/main.rs`, add:

```rust
#[cfg(test)]
mod check_receipt_tests {
    use super::*;

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
}
```

- [ ] **Step 5: Run red/green helper test**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml source_digest --quiet
```

Expected: PASS after implementation.

- [ ] **Step 6: Commit digest helper**

```powershell
git add compiler/Cargo.toml compiler/Cargo.lock compiler/src/main.rs
git commit -m "feat: add check receipt source digest helper"
```

## Task 3: Wire Source Metadata Into Receipts

**Files:**
- Modify: `compiler/src/main.rs`

- [ ] **Step 1: Add serializable source digest type**

In `compiler/src/main.rs`, before `struct CheckReceipt`, add:

```rust
#[derive(Clone, serde::Serialize)]
struct CheckReceiptSourceDigest {
    algorithm: &'static str,
    hex: String,
}
```

- [ ] **Step 2: Extend `CheckReceipt`**

Change `CheckReceipt` to include:

```rust
compiler_version: &'static str,
language_version: String,
source_digest: CheckReceiptSourceDigest,
```

The beginning of the struct should become:

```rust
#[derive(serde::Serialize)]
struct CheckReceipt {
    schema: &'static str,
    compiler: &'static str,
    compiler_version: &'static str,
    language_version: String,
    source: String,
    source_digest: CheckReceiptSourceDigest,
    status: &'static str,
    items: usize,
    tokens: usize,
    declared_effects: BTreeMap<String, Vec<String>>,
    observed_capabilities: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    diagnostics: Vec<CheckReceiptDiagnostic>,
}
```

- [ ] **Step 3: Extend `CheckOutcome`**

Change `CheckOutcome` to include:

```rust
compiler_version: &'static str,
language_version: String,
source_digest: CheckReceiptSourceDigest,
```

The beginning of the struct should become:

```rust
struct CheckOutcome {
    source: String,
    compiler_version: &'static str,
    language_version: String,
    source_digest: CheckReceiptSourceDigest,
    items: usize,
    tokens: usize,
    parse_errors: Vec<String>,
    type_errors: Vec<TypeErrorWithSpan>,
    function_summaries: Vec<FunctionEffectSummary>,
}
```

- [ ] **Step 4: Read source bytes before UTF-8 conversion**

In `run_check`, replace:

```rust
let source = std::fs::read_to_string(file).map_err(|e| {
    eprintln!("Error reading file '{}': {}", file.display(), e);
    1
})?;
```

with:

```rust
let source_bytes = std::fs::read(file).map_err(|e| {
    eprintln!("Error reading file '{}': {}", file.display(), e);
    1
})?;
let source_digest = CheckReceiptSourceDigest {
    algorithm: "sha256",
    hex: source_digest_hex(&source_bytes),
};
let source = String::from_utf8(source_bytes).map_err(|e| {
    eprintln!("Error reading file '{}': {}", file.display(), e);
    1
})?;
```

- [ ] **Step 5: Populate `CheckOutcome`**

In the `Ok(CheckOutcome {` block near the end of `run_check`, add:

```rust
compiler_version: quantalang::VERSION,
language_version: language_version_string(),
source_digest,
```

The beginning of the block should become:

```rust
Ok(CheckOutcome {
    source: file.to_string_lossy().to_string(),
    compiler_version: quantalang::VERSION,
    language_version: language_version_string(),
    source_digest,
    items: item_count,
    tokens: token_count,
    parse_errors,
    type_errors: checker.errors().to_vec(),
    function_summaries: checker.function_effect_summaries().to_vec(),
})
```

- [ ] **Step 6: Populate `CheckReceipt`**

In `build_check_receipt`, add these fields after `compiler: "quantac",`:

```rust
compiler_version: outcome.compiler_version,
language_version: outcome.language_version.clone(),
```

Add this field after `source: outcome.source.clone(),`:

```rust
source_digest: outcome.source_digest.clone(),
```

- [ ] **Step 7: Run receipt tests**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
cargo test --manifest-path compiler/Cargo.toml source_digest --quiet
```

Expected: both PASS.

- [ ] **Step 8: Commit receipt wiring**

```powershell
git add compiler/src/main.rs compiler/tests/cli.rs compiler/Cargo.toml compiler/Cargo.lock
git commit -m "feat: bind check receipts to source digest"
```

## Task 4: Document Source-Bound Receipts

**Files:**
- Modify: `README.md`
- Modify: `docs/EFFECTS_GUIDE.md`

- [ ] **Step 1: Update README capability receipt wording**

In `README.md`, in the "Capability Effects" section after the paragraph ending
with "instead of remaining invisible compiler side channels.", add:

```markdown
`quantac check --receipt` also binds each receipt to the checked source bytes
with a SHA-256 digest plus compiler and language version metadata, giving CI and
review tooling a stable evidence record for the exact source that passed or
failed the capability gate.
```

- [ ] **Step 2: Update effects guide receipt wording**

In `docs/EFFECTS_GUIDE.md`, replace:

```markdown
`quantac check <file> --receipt <path>` writes a deterministic
`quantalang-check-receipt/v1` JSON artifact with declared effects, observed
capability sources, pass/fail status, and compact diagnostics. Use `--receipt -`
when a CI step or wrapper wants the receipt on stdout.
```

with:

```markdown
`quantac check <file> --receipt <path>` writes a deterministic
`quantalang-check-receipt/v1` JSON artifact with compiler/language version
metadata, a SHA-256 digest of the checked source bytes, declared effects,
observed capability sources, pass/fail status, and compact diagnostics. Use
`--receipt -` when a CI step or wrapper wants the receipt on stdout.
```

- [ ] **Step 3: Run docs and format checks**

Run:

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
python -m pytest -q tests/test_docs_landing_page.py
git diff --check
```

Expected: all PASS.

- [ ] **Step 4: Commit docs**

```powershell
git add README.md docs/EFFECTS_GUIDE.md
git commit -m "docs: describe source-bound check receipts"
```

## Task 5: Final Verification, Push, and Remote Checks

**Files:**
- Verify full repository state; no source edits required unless a gate fails.

- [ ] **Step 1: Run focused verification**

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
cargo test --manifest-path compiler/Cargo.toml source_digest --quiet
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

Expected: all PASS.

- [ ] **Step 2: Run full compiler verification**

```powershell
cargo test --manifest-path compiler/Cargo.toml --quiet
$env:RUSTFLAGS='-Dwarnings'
cargo test --manifest-path compiler/Cargo.toml --quiet
$code=$LASTEXITCODE
Remove-Item Env:\RUSTFLAGS -ErrorAction SilentlyContinue
exit $code
```

Expected: both compiler test runs PASS.

- [ ] **Step 3: Run hygiene gates**

```powershell
git diff --check
git diff origin/main..HEAD --check
git check-ignore -q .env; if ($LASTEXITCODE -eq 0) { 'env-ignored' } else { 'env-not-ignored' }
powershell -NoProfile -ExecutionPolicy Bypass -File C:\dev\scratch\portfolio-stabilization-2026-06-13\scan-diff-secrets.ps1 -Repo C:\dev\public\pubscan\quantalang
```

Expected: no whitespace errors, `.env` ignored, and secret scan prints `no-matches`.

- [ ] **Step 4: Push**

```powershell
git push origin main
```

Expected: push succeeds.

- [ ] **Step 5: Watch GitHub workflows**

```powershell
gh run list -R HarperZ9/quantalang --branch main --limit 8 --json databaseId,workflowName,status,conclusion,headSha,displayTitle,createdAt
```

Poll until the pushed head has completed `CI` and `pages-build-deployment`.
Expected: both conclude `success`.
