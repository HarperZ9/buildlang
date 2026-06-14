# Input-Bound Check Receipts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record every source file that feeds `quantac check` in check receipts, not only the entry file.

**Architecture:** Add a small input digest ledger in `compiler/src/main.rs`, pass it through entry reading, import resolution, include preprocessing, and module resolution, then serialize sorted records in `CheckReceipt`.

**Tech Stack:** Rust 2021, existing `sha2` helper, serde JSON receipts, CLI tests in `compiler/tests/cli.rs`.

---

### Task 1: Red Test For Included Inputs

**Files:**
- Modify: `compiler/tests/cli.rs`

- [ ] **Step 1: Add CLI regression**

Add a test that creates an entry file with `include!("shared.quanta");`, checks
it with `--receipt -`, and asserts the receipt contains two input records:
`entry` and `include`.

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt_input_digests_include_transitive_sources -- --nocapture
```

Expected: FAIL because `input_digests` is not serialized yet.

### Task 2: Implement Input Digest Ledger

**Files:**
- Modify: `compiler/src/main.rs`

- [ ] **Step 1: Add serializable record type**

Add `CheckReceiptInputDigest { role, source, digest }` and a non-serializable
`InputDigestLedger` that records exact bytes with `source_digest_hex`.

- [ ] **Step 2: Pass ledger through check input readers**

Change `run_check`, `resolve_imports`, `preprocess_includes`, and
`resolve_modules` so every file read for the check records into the ledger.

- [ ] **Step 3: Serialize records**

Add `input_digests` to `CheckOutcome` and `CheckReceipt`, sorted
deterministically before receipt rendering.

- [ ] **Step 4: Verify GREEN**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt_input_digests_include_transitive_sources -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
```

Expected: both pass.

### Task 3: Document Receipt Semantics

**Files:**
- Modify: `README.md`
- Modify: `docs/EFFECTS_GUIDE.md`

- [ ] **Step 1: Update public wording**

Clarify that `source_digest` binds the entry source while `input_digests`
binds all entry/import/include/module inputs.

- [ ] **Step 2: Verify docs**

Run:

```powershell
python -m pytest -q tests/test_docs_landing_page.py
git diff --check
```

Expected: both pass.

### Task 4: Final Verification And Push

- [ ] **Step 1: Focused tests**

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy -- --nocapture
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

- [ ] **Step 2: Full gates**

```powershell
cargo test --manifest-path compiler/Cargo.toml --quiet
$env:RUSTFLAGS='-Dwarnings'; cargo test --manifest-path compiler/Cargo.toml --quiet
cargo fmt --manifest-path compiler/Cargo.toml -- --check
```

- [ ] **Step 3: Hygiene, commit, push, CI watch**

```powershell
git diff --check
git check-ignore -q .env
powershell -NoProfile -ExecutionPolicy Bypass -File C:/dev/scratch/portfolio-stabilization-2026-06-13/scan-diff-secrets.ps1 -Repo C:/dev/public/pubscan/quantalang
git push origin main
gh run list -R HarperZ9/quantalang --branch main --limit 8 --json databaseId,workflowName,status,conclusion,headSha,displayTitle,createdAt,url
```
