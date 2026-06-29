# Effect Provenance Contracts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Buildc/Buildlang distinguish direct capability use from propagated callee effects, surface both in machine receipts, and enforce typed provenance policy contracts in `buildc check`.

**Architecture:** Add a second provenance map to type inference and function summaries. Keep direct ambient helpers/macros in `observed_capabilities`; record effectful function calls in `propagated_effects`. Extend check receipts and policy evaluation so accountability artifacts can prove which boundary touched an effect and which callers inherited it.

**Tech Stack:** Rust compiler crate, `serde`/`serde_json` receipt serialization, Cargo unit and CLI integration tests, GitHub Actions CI.

---

## Current Context

- Repo: `C:\dev\public\pubscan\buildlang`
- Branch: `main`
- Latest baseline commit: `81edf4203271e2764d4bf0e088362c87ac8f0dc4`
- Design spec: `docs/superpowers/specs/2026-06-14-effect-provenance-contracts-design.md`
- Existing direct capability summary field:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionEffectSummary {
    pub function: String,
    pub declared_effects: Vec<String>,
    pub observed_capabilities: BTreeMap<String, BTreeSet<String>>,
}
```

- Existing inference records direct runtime helpers/macros with:

```rust
fn record_capability_source(&mut self, effect_name: &str, source_name: &str) {
    self.capability_sources
        .entry(effect_name.to_string())
        .or_default()
        .insert(source_name.to_string());
}
```

- Existing function-call propagation currently folds callee capability effects back into `capability_sources`. This is the behavior this plan changes.

## Desired Receipt Shape

For this source:

```build
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
```

`buildc check --receipt -` should emit this logical shape:

```json
{
  "declared_effects": {
    "load_config": ["FileSystem"],
    "main": ["FileSystem"]
  },
  "observed_capabilities": {
    "load_config": {
      "FileSystem": ["read_file"]
    },
    "main": {}
  },
  "propagated_effects": {
    "load_config": {},
    "main": {
      "FileSystem": ["load_config"]
    }
  }
}
```

Direct means the function itself touched an ambient capability helper or macro.
Propagated means the function called another effectful function and inherited its effect row.

## Task 1: Add Failing Type-Checker Provenance Tests

- [ ] Edit `compiler/src/types/check.rs`.
- [ ] Extend `FunctionEffectSummary` only after the failing test is in place.
- [ ] Add tests near the existing capability summary tests.

Add this test first:

```rust
#[test]
fn check_summary_separates_direct_and_propagated_capabilities() {
    let src = r#"
        fn load_config() ~ FileSystem {
            read_file("ops.txt");
        }

        fn main() ~ FileSystem {
            load_config();
        }
    "#;
    let ast = parse(src);
    let result = TypeChecker::new().check(&ast);
    assert!(
        result.errors.is_empty(),
        "expected clean type check, got {:?}",
        result.errors
    );

    let load_config = result
        .function_effect_summaries
        .iter()
        .find(|summary| summary.function == "load_config")
        .expect("load_config summary");
    assert_eq!(
        load_config
            .observed_capabilities
            .get("FileSystem")
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["read_file".to_string()]
    );
    assert!(
        load_config.propagated_effects.is_empty(),
        "direct boundary should not report propagated callees"
    );

    let main = result
        .function_effect_summaries
        .iter()
        .find(|summary| summary.function == "main")
        .expect("main summary");
    assert!(
        main.observed_capabilities.is_empty(),
        "caller should not report callee helper as direct IO"
    );
    assert_eq!(
        main.propagated_effects
            .get("FileSystem")
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["load_config".to_string()]
    );
}
```

Add this companion test:

```rust
#[test]
fn check_summary_records_console_macro_as_direct_not_propagated() {
    let src = r#"
        fn main() ~ Console {
            println!("ops");
        }
    "#;
    let ast = parse(src);
    let result = TypeChecker::new().check(&ast);
    assert!(
        result.errors.is_empty(),
        "expected clean type check, got {:?}",
        result.errors
    );

    let main = result
        .function_effect_summaries
        .iter()
        .find(|summary| summary.function == "main")
        .expect("main summary");
    assert_eq!(
        main.observed_capabilities
            .get("Console")
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["println".to_string()]
    );
    assert!(
        main.propagated_effects.is_empty(),
        "macro capability should remain direct provenance"
    );
}
```

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml check_summary_separates_direct_and_propagated_capabilities --quiet
```

Expected red result:

```text
error[E0609]: no field `propagated_effects` on type `&FunctionEffectSummary`
```

Commit after the failing tests are captured:

```powershell
git add compiler/src/types/check.rs
git commit -m "test: require effect provenance summaries"
```

## Task 2: Implement Type Inference Provenance Split

- [ ] Edit `compiler/src/types/infer.rs`.
- [ ] Add a second map to `TypeInfer`:

```rust
/// Capability effects mapped to effectful callees that propagated them.
propagated_effect_sources: BTreeMap<String, BTreeSet<String>>,
```

- [ ] Initialize it in `TypeInfer::new`:

```rust
propagated_effect_sources: BTreeMap::new(),
```

- [ ] Add a getter:

```rust
pub fn propagated_effect_sources(&self) -> &BTreeMap<String, BTreeSet<String>> {
    &self.propagated_effect_sources
}
```

- [ ] Add a recorder:

```rust
fn record_propagated_effect_source(&mut self, effect_name: &str, callee_name: &str) {
    self.propagated_effect_sources
        .entry(effect_name.to_string())
        .or_default()
        .insert(callee_name.to_string());
}
```

- [ ] In the `TyKind::Fn(fn_ty)` call branch, keep the merge into `current_effects` and change the provenance recorder from direct capability source to propagated effect source.

Replace the current block:

```rust
if !fn_ty.effects.is_empty() {
    self.current_effects = self.current_effects.merge(&fn_ty.effects);
    if let Some(name) = call_name.as_deref() {
        for effect in &fn_ty.effects.effects {
            if super::capabilities::is_capability_effect(effect.name.as_ref()) {
                self.record_capability_source(effect.name.as_ref(), name);
            }
        }
    }
}
```

With:

```rust
if !fn_ty.effects.is_empty() {
    self.current_effects = self.current_effects.merge(&fn_ty.effects);
    if let Some(name) = call_name.as_deref() {
        for effect in &fn_ty.effects.effects {
            if super::capabilities::is_capability_effect(effect.name.as_ref()) {
                self.record_propagated_effect_source(effect.name.as_ref(), name);
            }
        }
    }
}
```

- [ ] Edit `compiler/src/types/check.rs`.
- [ ] Extend the summary type:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionEffectSummary {
    pub function: String,
    pub declared_effects: Vec<String>,
    pub observed_capabilities: BTreeMap<String, BTreeSet<String>>,
    pub propagated_effects: BTreeMap<String, BTreeSet<String>>,
}
```

- [ ] When checking a function body, capture the new inference map with the existing body tuple:

```rust
let propagated_effect_sources = infer.propagated_effect_sources().clone();
```

- [ ] Include `propagated_effect_sources` in the tuple destructuring and summary construction:

```rust
self.function_effect_summaries.push(FunctionEffectSummary {
    function: func_name.clone(),
    declared_effects,
    observed_capabilities: capability_sources.clone(),
    propagated_effects: propagated_effect_sources.clone(),
});
```

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml check_summary_separates_direct_and_propagated_capabilities --quiet
cargo test --manifest-path compiler/Cargo.toml check_summary_records_console_macro_as_direct_not_propagated --quiet
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

Expected green result:

```text
test result: ok
```

Commit:

```powershell
git add compiler/src/types/infer.rs compiler/src/types/check.rs
git commit -m "feat: track propagated effect provenance"
```

## Task 3: Add Failing CLI Receipt Tests

- [ ] Edit `compiler/tests/cli.rs`.
- [ ] Add a receipt test near the existing `check_receipt_*` tests.

Add:

```rust
#[test]
fn check_receipt_records_propagated_effects_separately() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("ops.qnt");
    fs::write(
        &input,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write input");

    let output = Command::new(buildc())
        .args(["check", "--receipt", "-"])
        .arg(&input)
        .output()
        .expect("run buildc check");

    assert!(
        output.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["load_config"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"].as_object().unwrap().len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["load_config"]
            .as_object()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
}
```

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt_records_propagated_effects_separately -- --nocapture
```

Expected red result:

```text
assertion failed
```

The failing assertion should be against the missing `propagated_effects` receipt field.

Commit:

```powershell
git add compiler/tests/cli.rs
git commit -m "test: require propagated effect receipts"
```

## Task 4: Implement Propagated Effect Receipts

- [ ] Edit `compiler/src/main.rs`.
- [ ] Extend `CheckReceipt`:

```rust
propagated_effects: BTreeMap<String, BTreeMap<String, Vec<String>>>,
```

- [ ] Update `build_check_receipt`.

Add a map next to `observed_capabilities`:

```rust
let mut propagated_effects = BTreeMap::new();
```

Populate it from every function summary:

```rust
let propagated_for_function = summary
    .propagated_effects
    .iter()
    .map(|(effect, sources)| {
        (
            effect.clone(),
            sources.iter().cloned().collect::<Vec<_>>(),
        )
    })
    .collect::<BTreeMap<_, _>>();
propagated_effects.insert(summary.function.clone(), propagated_for_function);
```

Return it in the receipt:

```rust
CheckReceipt {
    schema: "buildlang-check-receipt/v1",
    input,
    input_graph_digest: outcome.input_graph_digest.clone(),
    declared_effects,
    observed_capabilities,
    propagated_effects,
    diagnostics,
    policy,
}
```

- [ ] Update any unit test fixtures that construct `FunctionEffectSummary`:

```rust
propagated_effects: BTreeMap::new(),
```

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt_records_propagated_effects_separately -- --nocapture
cargo test --manifest-path compiler/Cargo.toml check_policy_evaluation_sorts_and_deduplicates_violations --quiet
```

Expected green result:

```text
test result: ok
```

Commit:

```powershell
git add compiler/src/main.rs compiler/tests/cli.rs
git commit -m "feat: add propagated effect receipts"
```

## Task 5: Add Failing Policy Contract Tests

- [ ] Edit `compiler/tests/cli.rs`.
- [ ] Add tests near the existing check policy tests.

Add this direct allowlist rejection test:

```rust
#[test]
fn check_policy_direct_allowlist_rejects_unapproved_direct_effect() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("ops.qnt");
    fs::write(
        &input,
        r#"
fn main() ~ FileSystem {
    read_file("ops.txt");
}
"#,
    )
    .expect("write input");
    let policy = write_temp_policy(
        &dir,
        serde_json::json!({
            "schema": "buildlang-check-policy/v1",
            "allowed_effects": ["FileSystem"],
            "direct_effect_allowlist": {
                "FileSystem": ["load_config"]
            }
        }),
    );

    let output = Command::new(buildc())
        .args(["check", "--receipt", "-", "--policy"])
        .arg(&policy)
        .arg(&input)
        .output()
        .expect("run buildc check");

    assert!(!output.status.success(), "policy should reject direct helper");
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["allowed"], false);
    assert_eq!(
        receipt["policy"]["violations"][0]["kind"],
        "DirectEffectNotAllowed"
    );
    assert_eq!(
        receipt["policy"]["violations"][0]["surface"],
        "observed_capabilities"
    );
    assert_eq!(receipt["policy"]["violations"][0]["function"], "main");
    assert_eq!(receipt["policy"]["violations"][0]["effect"], "FileSystem");
    assert_eq!(receipt["policy"]["violations"][0]["source"], "read_file");
}
```

Add this allowlisted boundary pass test:

```rust
#[test]
fn check_policy_provenance_allowlists_accept_boundary_and_caller() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("ops.qnt");
    fs::write(
        &input,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write input");
    let policy = write_temp_policy(
        &dir,
        serde_json::json!({
            "schema": "buildlang-check-policy/v1",
            "allowed_effects": ["FileSystem"],
            "direct_effect_allowlist": {
                "FileSystem": ["load_config"]
            },
            "propagated_effect_allowlist": {
                "FileSystem": ["main"]
            }
        }),
    );

    let output = Command::new(buildc())
        .args(["check", "--receipt", "-", "--policy"])
        .arg(&policy)
        .arg(&input)
        .output()
        .expect("run buildc check");

    assert!(
        output.status.success(),
        "policy should accept allowlisted provenance: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["allowed"], true);
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
}
```

Add this propagated allowlist rejection test:

```rust
#[test]
fn check_policy_propagated_allowlist_rejects_unlisted_caller() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("ops.qnt");
    fs::write(
        &input,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write input");
    let policy = write_temp_policy(
        &dir,
        serde_json::json!({
            "schema": "buildlang-check-policy/v1",
            "allowed_effects": ["FileSystem"],
            "direct_effect_allowlist": {
                "FileSystem": ["load_config"]
            },
            "propagated_effect_allowlist": {
                "FileSystem": []
            }
        }),
    );

    let output = Command::new(buildc())
        .args(["check", "--receipt", "-", "--policy"])
        .arg(&policy)
        .arg(&input)
        .output()
        .expect("run buildc check");

    assert!(!output.status.success(), "policy should reject propagated caller");
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["allowed"], false);
    assert_eq!(
        receipt["policy"]["violations"][0]["kind"],
        "PropagatedEffectNotAllowed"
    );
    assert_eq!(
        receipt["policy"]["violations"][0]["surface"],
        "propagated_effects"
    );
    assert_eq!(receipt["policy"]["violations"][0]["function"], "main");
    assert_eq!(receipt["policy"]["violations"][0]["effect"], "FileSystem");
    assert_eq!(receipt["policy"]["violations"][0]["source"], "load_config");
}
```

- [ ] Edit `compiler/src/main.rs`.
- [ ] Add a unit test beside `check_policy_evaluation_sorts_and_deduplicates_violations`.

Add:

```rust
#[test]
fn check_policy_requires_valid_input_graph_digest() {
    let outcome = CheckOutcome {
        input: "ops.qnt".to_string(),
        input_graph_digest: DigestReceipt {
            algorithm: "sha1".to_string(),
            hex: "abc".to_string(),
            inputs: vec!["ops.qnt".to_string()],
        },
        type_errors: Vec::new(),
        function_summaries: Vec::new(),
    };
    let profile = LoadedCheckPolicy {
        path: PathBuf::from("policy.json"),
        profile: CheckPolicyProfile {
            schema: "buildlang-check-policy/v1".to_string(),
            allowed_effects: BTreeSet::new(),
            denied_effects: BTreeSet::new(),
            direct_effect_allowlist: BTreeMap::new(),
            propagated_effect_allowlist: BTreeMap::new(),
            require_source_digest: false,
            require_input_graph_digest: true,
        },
    };

    let policy = evaluate_check_policy(&outcome, Some(&profile));
    assert_eq!(policy.allowed, false);
    assert_eq!(policy.violations.len(), 1);
    assert_eq!(policy.violations[0].kind, "MissingInputGraphDigest");
    assert_eq!(policy.violations[0].surface, "input_graph_digest");
}
```

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy_direct_allowlist_rejects_unapproved_direct_effect -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy_provenance_allowlists_accept_boundary_and_caller -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy_propagated_allowlist_rejects_unlisted_caller -- --nocapture
cargo test --manifest-path compiler/Cargo.toml check_policy_requires_valid_input_graph_digest --quiet
```

Expected red results:

```text
policy should reject direct helper
policy should reject propagated caller
error[E0560]: struct `CheckPolicyProfile` has no field named `direct_effect_allowlist`
```

Commit:

```powershell
git add compiler/src/main.rs compiler/tests/cli.rs
git commit -m "test: require provenance policy contracts"
```

## Task 6: Implement Provenance Policy Contracts

- [ ] Edit `compiler/src/main.rs`.
- [ ] Extend `CheckPolicyProfile`:

```rust
#[serde(default)]
direct_effect_allowlist: BTreeMap<String, Vec<String>>,
#[serde(default)]
propagated_effect_allowlist: BTreeMap<String, Vec<String>>,
#[serde(default)]
require_input_graph_digest: bool,
```

- [ ] Extend `CheckPolicyEvidence`:

```rust
struct CheckPolicyEvidence {
    function: String,
    effect: String,
    surface: &'static str,
    source: String,
}
```

- [ ] Extend `CheckPolicyViolation`:

```rust
#[serde(default, skip_serializing_if = "String::is_empty")]
source: String,
```

- [ ] Update its ordering tuple:

```rust
(
    &self.function,
    &self.effect,
    self.surface,
    &self.source,
    self.kind,
    &self.message,
)
```

- [ ] Add helper functions:

```rust
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

fn digest_is_sha256_hex(digest: &DigestReceipt) -> bool {
    digest.algorithm == "sha256"
        && digest.hex.len() == 64
        && digest.hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}
```

- [ ] Update `collect_check_policy_evidence`.

Declared effect evidence:

```rust
evidence.push(CheckPolicyEvidence {
    function: summary.function.clone(),
    effect: effect.clone(),
    surface: "declared_effects",
    source: String::new(),
});
```

Observed direct capability evidence:

```rust
for (effect, sources) in &summary.observed_capabilities {
    for source in sources {
        evidence.push(CheckPolicyEvidence {
            function: summary.function.clone(),
            effect: effect.clone(),
            surface: "observed_capabilities",
            source: source.clone(),
        });
    }
}
```

Propagated effect evidence:

```rust
for (effect, sources) in &summary.propagated_effects {
    for source in sources {
        evidence.push(CheckPolicyEvidence {
            function: summary.function.clone(),
            effect: effect.clone(),
            surface: "propagated_effects",
            source: source.clone(),
        });
    }
}
```

- [ ] Update `evaluate_check_policy`.

After source digest enforcement:

```rust
if profile.profile.require_input_graph_digest && !digest_is_sha256_hex(&outcome.input_graph_digest) {
    violations.push(CheckPolicyViolation {
        kind: "MissingInputGraphDigest",
        effect: String::new(),
        function: outcome.input.clone(),
        surface: "input_graph_digest",
        source: String::new(),
        message: "policy requires a valid sha256 input graph digest".to_string(),
    });
}
```

Inside the evidence loop, add direct and propagated checks:

```rust
if item.surface == "observed_capabilities"
    && !allowlist_allows(
        &profile.profile.direct_effect_allowlist,
        &item.effect,
        &item.function,
    )
{
    violations.push(CheckPolicyViolation {
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
}

if item.surface == "propagated_effects"
    && !allowlist_allows(
        &profile.profile.propagated_effect_allowlist,
        &item.effect,
        &item.function,
    )
{
    violations.push(CheckPolicyViolation {
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
```

- [ ] Update all in-file struct literals for `CheckPolicyViolation`, `CheckPolicyEvidence`, `CheckPolicyProfile`, and `FunctionEffectSummary` to include the new fields.

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy_direct_allowlist_rejects_unapproved_direct_effect -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy_provenance_allowlists_accept_boundary_and_caller -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy_propagated_allowlist_rejects_unlisted_caller -- --nocapture
cargo test --manifest-path compiler/Cargo.toml check_policy_requires_valid_input_graph_digest --quiet
cargo test --manifest-path compiler/Cargo.toml check_policy_evaluation_sorts_and_deduplicates_violations --quiet
```

Expected green result:

```text
test result: ok
```

Commit:

```powershell
git add compiler/src/main.rs compiler/tests/cli.rs
git commit -m "feat: enforce effect provenance contracts"
```

## Task 7: Document the Operator-Facing Contract

- [ ] Edit `README.md`.
- [ ] Add a concise policy example under the existing check receipt or policy section:

```json
{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["FileSystem", "Network"],
  "direct_effect_allowlist": {
    "FileSystem": ["load_config"]
  },
  "propagated_effect_allowlist": {
    "FileSystem": ["main"]
  },
  "require_source_digest": true,
  "require_input_graph_digest": true
}
```

- [ ] Explain the distinction in two paragraphs:

```markdown
`observed_capabilities` records direct ambient capability use inside a function,
such as `read_file`, `http_get`, `println`, process helpers, or FFI helpers.
These entries are the accountability boundary for code that actually touches the
outside world.

`propagated_effects` records effectful callees that make a caller inherit a
typed effect. This lets policy allow a small number of audited boundary
functions while still proving which higher-level workflows depend on them.
```

- [ ] If `docs/` contains an effects guide, add the same contract there with the same terminology. Use `rg -n "observed_capabilities|check policy|policy"` to find the right location.

Run:

```powershell
rg -n "observed_capabilities|propagated_effects|direct_effect_allowlist|propagated_effect_allowlist" README.md docs
```

Expected output includes the README and any docs page updated in this task.

Commit:

```powershell
git add README.md docs
git commit -m "docs: describe effect provenance policy contracts"
```

## Task 8: Verification and Publish

- [ ] Run focused unit tests:

```powershell
cargo test --manifest-path compiler/Cargo.toml check_summary_separates_direct_and_propagated_capabilities --quiet
cargo test --manifest-path compiler/Cargo.toml check_summary_records_console_macro_as_direct_not_propagated --quiet
cargo test --manifest-path compiler/Cargo.toml check_policy_requires_valid_input_graph_digest --quiet
cargo test --manifest-path compiler/Cargo.toml check_policy_evaluation_sorts_and_deduplicates_violations --quiet
```

- [ ] Run focused CLI tests:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt_records_propagated_effects_separately -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy_direct_allowlist_rejects_unapproved_direct_effect -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy_provenance_allowlists_accept_boundary_and_caller -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy_propagated_allowlist_rejects_unlisted_caller -- --nocapture
```

- [ ] Run the existing adjacent regression slices:

```powershell
cargo test --manifest-path compiler/Cargo.toml capability --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli check_policy -- --nocapture
```

- [ ] Run formatting and warnings gates:

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
cargo clippy --manifest-path compiler/Cargo.toml --all-targets -- -D warnings
```

- [ ] Run root documentation/hygiene gates if present:

```powershell
if (Test-Path package.json) { npm test }
if (Test-Path scripts/ci/verify-docs.ps1) { powershell -ExecutionPolicy Bypass -File scripts/ci/verify-docs.ps1 }
```

- [ ] Run the required secret scan before committing or pushing any final changes:

```powershell
C:\dev\scratch\portfolio-stabilization-2026-06-13\scan-diff-secrets.ps1 -Repo C:\dev\public\pubscan\buildlang
```

Expected output:

```text
No staged or unstaged secret-like additions detected.
```

- [ ] Confirm worktree state:

```powershell
git status -sb
```

Expected output after all commits: the first line shows `main` tracking
`origin/main` with an ahead count, and no changed-file lines appear beneath it.

- [ ] Push:

```powershell
git push origin main
```

- [ ] Watch GitHub checks:

```powershell
gh run list --branch main --limit 5
gh run watch <run-id> --exit-status
```

Expected final state:

```text
CI success
pages-build-deployment success
```

## Implementation Order

1. Type-checker summary red tests.
2. Type inference provenance split.
3. CLI receipt red test.
4. Receipt serialization.
5. Policy contract red tests.
6. Policy contract enforcement.
7. Docs.
8. Full focused verification, secret scan, commit, push, and GitHub check watch.

## Acceptance Criteria

- `FunctionEffectSummary` exposes `observed_capabilities` and `propagated_effects` separately.
- Direct helper and macro calls appear only under `observed_capabilities`.
- Calls to effectful functions appear only under `propagated_effects`.
- `buildc check --receipt -` serializes `propagated_effects`.
- Policy profiles can restrict direct effect boundaries with `direct_effect_allowlist`.
- Policy profiles can restrict propagated effect callers with `propagated_effect_allowlist`.
- Policy profiles can require a valid SHA-256 input graph digest with `require_input_graph_digest`.
- Existing `allowed_effects`, `denied_effects`, and `require_source_digest` behavior remains intact.
- Focused tests, formatting, clippy, secret scan, push, and GitHub checks complete successfully.
