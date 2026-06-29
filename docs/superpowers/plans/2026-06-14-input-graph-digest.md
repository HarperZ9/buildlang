# Input Graph Digest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a portable graph-level digest to `buildc check --receipt` so the full checked source input set can be compared with one SHA-256 value.

**Architecture:** Keep the receipt schema additive. Reuse `CheckReceiptSourceDigest` for `input_graph_digest`, compute it from sorted `input_digests` records in `compiler/src/main.rs`, and prove behavior through CLI receipt tests.

**Tech Stack:** Rust 2021, existing `sha2` helper, serde JSON receipts, Cargo CLI tests.

---

### Task 1: Add Red Tests

**Files:**
- Modify: `compiler/tests/cli.rs`

- [ ] **Step 1:** Add assertions that `check_receipt_input_digests_track_included_source_changes` sees `input_graph_digest` change when only the included file changes.
- [ ] **Step 2:** Add a second CLI test that builds the same entry/include source graph in two different temp directories and asserts `input_graph_digest` is identical while detailed input source paths differ.
- [ ] **Step 3:** Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli input_graph_digest -- --nocapture
```

Expected: FAIL because receipts do not yet emit `input_graph_digest`.

### Task 2: Implement Digest

**Files:**
- Modify: `compiler/src/main.rs`

- [ ] **Step 1:** Add `input_graph_digest: CheckReceiptSourceDigest` to `CheckOutcome` and `CheckReceipt`.
- [ ] **Step 2:** Add `input_graph_digest(records: &[CheckReceiptInputDigest])`.
- [ ] **Step 3:** Compute the digest after `InputDigestLedger::into_sorted_records()` in `run_check`.
- [ ] **Step 4:** Re-run the focused CLI tests until green.

### Task 3: Docs And Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/EFFECTS_GUIDE.md`

- [ ] **Step 1:** Document `input_graph_digest` beside `input_digests`.
- [ ] **Step 2:** Run focused tests:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy -- --nocapture
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

- [ ] **Step 3:** Run full gates, hygiene, commit, push, and watch CI/Pages.
