# Check Policy Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add portable `quantalang-check-policy/v1` JSON profiles to `quantac check` so capability/effect receipts can be converted into deterministic policy pass/fail decisions.

**Architecture:** Keep the first implementation inside `compiler/src/main.rs`, next to the existing check receipt pipeline, because `main.rs` already owns CLI parsing, source/policy file reads, source digesting, receipt rendering, and exit codes. Add a small policy profile model, policy evidence/violation model, evaluator, optional receipt policy object, and human diagnostics; exercise behavior through binary CLI tests in `compiler/tests/cli.rs`.

**Tech Stack:** Rust 2021, Clap, Serde/serde_json, existing SHA-256 helper, existing `FunctionEffectSummary` check evidence, existing Cargo/pytest verification gates.

---

## File Structure

- Modify `compiler/tests/cli.rs`: add policy fixture helpers and red tests for allow, deny, invalid schema, and receipt policy data.
- Modify `compiler/src/main.rs`: add CLI `--policy`, policy profile loading, evaluator, policy receipt object, policy diagnostics, and exit-code integration.
- Modify `README.md`: mention `quantac check --policy` in the CLI/security posture.
- Modify `docs/EFFECTS_GUIDE.md`: document the policy profile schema and behavior.

## Task 1: Add Policy CLI Red Tests

**Files:**
- Modify: `compiler/tests/cli.rs`

- [ ] **Step 1: Add a policy fixture helper**

Add this helper near `receipt_from_stdout`:

```rust
fn write_temp_policy(label: &str, json: &str) -> PathBuf {
    let policy = std::env::temp_dir().join(format!(
        "quantalang_check_policy_{}_{}.json",
        label,
        std::process::id()
    ));
    fs::write(&policy, json).unwrap_or_else(|err| {
        panic!("write policy fixture {}: {}", policy.display(), err)
    });
    policy
}
```

- [ ] **Step 2: Add passing policy receipt test**

Add this test after the existing `check_receipt_source_digest_changes_when_source_changes` test:

```rust
#[test]
fn check_policy_allows_console_receipt() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_console_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "console_allow",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["Console"],
          "require_source_digest": true
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write policy console fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with passing policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "console policy check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "passed");
    assert_eq!(receipt["policy"]["schema"], "quantalang-check-policy/v1");
    assert_eq!(receipt["policy"]["status"], "passed");
    assert_eq!(receipt["policy"]["source_digest"]["algorithm"], "sha256");
    assert_eq!(
        receipt["policy"]["source_digest"]["hex"]
            .as_str()
            .expect("policy digest")
            .len(),
        64
    );
    assert!(receipt["policy"]["violations"].as_array().unwrap().is_empty());
}
```

- [ ] **Step 3: Add denied-effect policy test**

Add this test after the passing policy test:

```rust
#[test]
fn check_policy_denies_filesystem_even_when_typecheck_passes() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_deny_fs_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "deny_fs",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "denied_effects": ["FileSystem"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#)
        .expect("write denied filesystem fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with denied filesystem policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(!output.status.success(), "policy denial should fail check");
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DeniedEffect"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("policy denies effect `FileSystem`")
        }),
        "expected FileSystem denied violation in {violations:#?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Policy violation"),
        "stderr should include policy diagnostic:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 4: Add allow-list policy test**

Add this test after the denied-effect test:

```rust
#[test]
fn check_policy_allow_list_rejects_unlisted_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_allow_list_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "allow_console_only",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["Console"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#)
        .expect("write allow-list filesystem fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with allow-list policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(!output.status.success(), "unlisted effect should fail policy");
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DisallowedEffect"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
        }),
        "expected FileSystem disallowed violation in {violations:#?}"
    );
}
```

- [ ] **Step 5: Add invalid schema test**

Add this test after the allow-list test:

```rust
#[test]
fn check_policy_rejects_unsupported_schema() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_bad_schema_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "bad_schema",
        r#"{
          "schema": "quantalang-check-policy/v0",
          "allowed_effects": ["Console"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write bad schema fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .output()
        .expect("run quantac check with bad policy schema");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(!output.status.success(), "unsupported policy schema should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Unsupported check policy schema 'quantalang-check-policy/v0'"),
        "stderr should report unsupported schema:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 6: Run red CLI tests**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy -- --nocapture
```

Expected: FAIL because `quantac check` does not accept `--policy`.

- [ ] **Step 7: Commit red tests**

```powershell
git add compiler/tests/cli.rs
git commit -m "test: require check policy profiles"
```

## Task 2: Add Policy Model and Evaluator

**Files:**
- Modify: `compiler/src/main.rs`

- [ ] **Step 1: Add policy structs**

Add these structs after `CheckReceiptDiagnostic`:

```rust
#[derive(Clone, Debug, serde::Deserialize)]
struct CheckPolicyProfile {
    schema: String,
    #[serde(default)]
    allowed_effects: Vec<String>,
    #[serde(default)]
    denied_effects: Vec<String>,
    #[serde(default)]
    require_source_digest: bool,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug)]
struct LoadedCheckPolicy {
    source: String,
    source_digest: CheckReceiptSourceDigest,
    profile: CheckPolicyProfile,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct CheckPolicyEvidence {
    function: String,
    effect: String,
    surface: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
struct CheckPolicyViolation {
    kind: &'static str,
    effect: String,
    function: String,
    surface: &'static str,
    message: String,
}

#[derive(Clone, Debug)]
struct CheckPolicyDecision {
    schema: String,
    source: String,
    source_digest: CheckReceiptSourceDigest,
    violations: Vec<CheckPolicyViolation>,
}
```

- [ ] **Step 2: Add policy helper functions**

Add these helpers after `source_digest_hex`:

```rust
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

fn collect_check_policy_evidence(outcome: &CheckOutcome) -> BTreeSet<CheckPolicyEvidence> {
    let mut evidence = BTreeSet::new();
    for summary in &outcome.function_summaries {
        for effect in &summary.declared_effects {
            evidence.insert(CheckPolicyEvidence {
                function: summary.function.clone(),
                effect: effect.clone(),
                surface: "declared_effects",
            });
        }
        for effect in summary.observed_capabilities.keys() {
            evidence.insert(CheckPolicyEvidence {
                function: summary.function.clone(),
                effect: effect.clone(),
                surface: "observed_capabilities",
            });
        }
    }
    evidence
}

fn evaluate_check_policy(policy: &LoadedCheckPolicy, outcome: &CheckOutcome) -> CheckPolicyDecision {
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
            message: "policy requires sha256 source digest".to_string(),
        });
    }

    for item in collect_check_policy_evidence(outcome) {
        if denied.contains(item.effect.as_str()) {
            violations.insert(CheckPolicyViolation {
                kind: "DeniedEffect",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                message: format!("policy denies effect `{}`", item.effect),
            });
        } else if !allowed.is_empty() && !allowed.contains(item.effect.as_str()) {
            violations.insert(CheckPolicyViolation {
                kind: "DisallowedEffect",
                effect: item.effect.clone(),
                function: item.function.clone(),
                surface: item.surface,
                message: format!("policy does not allow effect `{}`", item.effect),
            });
        }
    }

    CheckPolicyDecision {
        schema: policy.profile.schema.clone(),
        source: policy.source.clone(),
        source_digest: policy.source_digest.clone(),
        violations: violations.into_iter().collect(),
    }
}
```

- [ ] **Step 3: Add evaluator unit test**

Add this test inside the existing bottom `#[cfg(test)] mod tests`:

```rust
#[test]
fn check_policy_evaluation_sorts_and_deduplicates_violations() {
    let policy = LoadedCheckPolicy {
        source: "policy.json".to_string(),
        source_digest: CheckReceiptSourceDigest {
            algorithm: "sha256",
            hex: source_digest_hex(b"policy"),
        },
        profile: CheckPolicyProfile {
            schema: "quantalang-check-policy/v1".to_string(),
            allowed_effects: vec!["Console".to_string()],
            denied_effects: vec!["Network".to_string()],
            require_source_digest: true,
            extra: BTreeMap::new(),
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
        items: 1,
        tokens: 1,
        parse_errors: Vec::new(),
        type_errors: Vec::new(),
        function_summaries: vec![
            FunctionEffectSummary {
                function: "b".to_string(),
                declared_effects: vec!["Network".to_string(), "Network".to_string()],
                observed_capabilities: BTreeMap::new(),
            },
            FunctionEffectSummary {
                function: "a".to_string(),
                declared_effects: vec!["FileSystem".to_string()],
                observed_capabilities: BTreeMap::new(),
            },
        ],
    };

    let decision = evaluate_check_policy(&policy, &outcome);
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
```

- [ ] **Step 4: Run evaluator unit test**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --bin quantac check_policy --quiet
```

Expected: PASS after the helper implementation is present.

- [ ] **Step 5: Commit policy model**

```powershell
git add compiler/src/main.rs
git commit -m "feat: add check policy evaluator"
```

## Task 3: Wire Policy Into CLI, Receipts, and Exit Codes

**Files:**
- Modify: `compiler/src/main.rs`

- [ ] **Step 1: Add CLI policy option**

In `Commands::Check`, add:

```rust
/// Evaluate a machine-readable check policy profile
#[arg(long, value_name = "PATH")]
policy: Option<PathBuf>,
```

Change the command match arm to:

```rust
Some(Commands::Check {
    file,
    receipt,
    policy,
}) => cmd_check(&file, receipt.as_deref(), policy.as_deref()),
```

- [ ] **Step 2: Add receipt policy type**

Add this struct after `CheckPolicyDecision`:

```rust
#[derive(serde::Serialize)]
struct CheckReceiptPolicy {
    schema: String,
    source: String,
    source_digest: CheckReceiptSourceDigest,
    status: &'static str,
    violations: Vec<CheckPolicyViolation>,
}
```

- [ ] **Step 3: Add optional policy field to receipt**

Add this field to `CheckReceipt`:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
policy: Option<CheckReceiptPolicy>,
```

- [ ] **Step 4: Change `build_check_receipt` signature and status**

Change:

```rust
fn build_check_receipt(outcome: &CheckOutcome) -> CheckReceipt {
```

to:

```rust
fn build_check_receipt(
    outcome: &CheckOutcome,
    policy: Option<&CheckPolicyDecision>,
) -> CheckReceipt {
```

Before constructing `CheckReceipt`, add:

```rust
let policy_failed = policy
    .map(|decision| !decision.violations.is_empty())
    .unwrap_or(false);
let receipt_policy = policy.map(|decision| CheckReceiptPolicy {
    schema: decision.schema.clone(),
    source: decision.source.clone(),
    source_digest: decision.source_digest.clone(),
    status: check_policy_status(decision),
    violations: decision.violations.clone(),
});
```

Change status selection to:

```rust
status: if diagnostics.is_empty() && !policy_failed {
    "passed"
} else {
    "failed"
},
```

Set:

```rust
policy: receipt_policy,
```

- [ ] **Step 5: Add policy human diagnostics**

Add this helper before `cmd_check`:

```rust
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
```

- [ ] **Step 6: Wire policy in `cmd_check`**

Replace `cmd_check` with:

```rust
fn cmd_check(file: &Path, receipt: Option<&Path>, policy: Option<&Path>) -> Result<(), i32> {
    let receipt_to_stdout = receipt == Some(Path::new("-"));
    let loaded_policy = policy.map(load_check_policy).transpose()?;
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
```

- [ ] **Step 7: Run policy and receipt tests**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --bin quantac check_policy --quiet
```

Expected: all PASS.

- [ ] **Step 8: Commit CLI wiring**

```powershell
git add compiler/src/main.rs compiler/tests/cli.rs
git commit -m "feat: enforce check policy profiles"
```

## Task 4: Document Policy Profiles

**Files:**
- Modify: `README.md`
- Modify: `docs/EFFECTS_GUIDE.md`

- [ ] **Step 1: Update README command table**

Change the check row in `README.md` to:

```markdown
| `quantac check <file> [--receipt PATH|-] [--policy policy.json]` | Type-check, optionally evaluate policy, and optionally emit a JSON accountability receipt |
```

- [ ] **Step 2: Add README policy paragraph**

After the existing source-bound receipt paragraph in the Capability Effects section, add:

```markdown
`quantac check --policy <policy.json>` evaluates a portable
`quantalang-check-policy/v1` profile against declared effects and observed
capabilities. Policy failures make the check fail even when type checking
passes, and receipts record the policy path, policy digest, status, and
structured violations.
```

- [ ] **Step 3: Add effects-guide policy example**

After the receipt paragraph in `docs/EFFECTS_GUIDE.md`, add:

```markdown
Policy profiles turn receipt evidence into an enforceable CI gate:

```json
{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["Console"],
  "denied_effects": ["FileSystem", "Network", "Process", "Foreign"],
  "require_source_digest": true
}
```

Run it with:

```bash
quantac check app.quanta --policy console-only.json --receipt receipt.json
```

Denied effects always fail. If `allowed_effects` is non-empty, any declared
effect or observed capability outside the allow-list also fails.
```

- [ ] **Step 4: Run docs gates**

Run:

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
python -m pytest -q tests/test_docs_landing_page.py
git diff --check
```

Expected: all PASS.

- [ ] **Step 5: Commit docs**

```powershell
git add README.md docs/EFFECTS_GUIDE.md
git commit -m "docs: describe check policy profiles"
```

## Task 5: Final Verification, Push, and Remote Checks

**Files:**
- Verify repository state; no source edits unless a gate fails.

- [ ] **Step 1: Run focused verification**

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy -- --nocapture
cargo test --manifest-path compiler/Cargo.toml check_policy --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
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
