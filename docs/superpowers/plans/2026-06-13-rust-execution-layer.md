# Rust Execution Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add executable smoke validation for the QuantaC Rust backend so selected generated Rust programs are compiled, run, and checked for deterministic stdout.

**Architecture:** Extend the existing Rust backend test harness with a runtime verifier beside `assert_rustc_metadata_ok`. Keep metadata tests as broad compile coverage and add a small execution subset for scalar branching, references, structs/arrays, and tuple ownership reuse.

**Tech Stack:** Rust compiler crate, QuantaLang parser/type checker/MIR lowerer, `rustc`, Cargo unit tests, repository status docs.

**Implementation result (2026-06-13):** Completed using checked-in semantic
corpus programs instead of duplicating inline source in every runtime smoke
test. The planned `project-docs/...` status paths do not exist in this repo;
the implemented status updates are in `README.md`, `STATUS.md`,
`compiler/src/codegen/backend/STATUS.md`, and this plan/design pair.

---

### Task 1: Add Executable Rust Test Harness

**Files:**
- Modify: `compiler/src/codegen/backend/rust.rs`

- [ ] **Step 1: Write the failing executable smoke test**

Add this test near the existing `generated_rust_compiles_for_scalar_branch_subset` test:

```rust
#[test]
fn generated_rust_runs_for_scalar_branch_subset() {
    let source = r#"
fn choose(x: i32) -> i32 {
    if x > 0 { x } else { 0 }
}

fn main() {
    let v: i32 = choose(4);
    println("{}", v);
}
"#;
    let rust = compile_quanta_to_rust(source);
    assert_rustc_run_stdout("run_scalar_branch", &rust, "4\n");
}
```

- [ ] **Step 2: Run the single test and verify red**

Run:

```powershell
python C:\Users\Zain\AGENTS\warden_shell\tools\safe_exec.py --timeout 60 -- cargo test --manifest-path public/pubscan/quantalang/compiler/Cargo.toml generated_rust_runs_for_scalar_branch_subset --quiet
```

Expected: compile failure because `assert_rustc_run_stdout` is not defined.

- [ ] **Step 3: Add the minimal runtime verifier helper**

Add this helper after `assert_rustc_metadata_ok`:

```rust
fn assert_rustc_run_stdout(name: &str, rust_source: &str, expected_stdout: &str) {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let dir = std::env::temp_dir().join(format!(
        "quantalang_rust_backend_run_{}_{}",
        name,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let source_path = dir.join("generated.rs");
    let exe_path = dir.join(format!(
        "generated{}",
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::write(&source_path, rust_source).expect("write generated Rust");

    let compile = std::process::Command::new(&rustc)
        .arg(&source_path)
        .arg("-o")
        .arg(&exe_path)
        .output()
        .expect("invoke rustc");
    assert!(
        compile.status.success(),
        "rustc failed for {name}\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
        rust_source
    );

    let run = std::process::Command::new(&exe_path)
        .output()
        .expect("run generated Rust executable");
    assert!(
        run.status.success(),
        "generated executable failed for {name}\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
        rust_source
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected_stdout);
}
```

- [ ] **Step 4: Run the single test and verify green**

Run the same command from Step 2.

Expected: 1 test passed.

### Task 2: Add Runtime Smoke Coverage

**Files:**
- Modify: `compiler/src/codegen/backend/rust.rs`

- [ ] **Step 1: Add reference/mutation execution test**

```rust
#[test]
fn generated_rust_runs_for_reference_subset() {
    let source = r#"
fn add_to(x: &mut i32, amount: i32) {
    *x = *x + amount;
}

fn read_value(x: &i32) -> i32 {
    *x
}

fn main() {
    let mut n: i32 = 10;
    add_to(&mut n, 5);
    let val: i32 = read_value(&n);
    println("{}", val);
}
"#;
    let rust = compile_quanta_to_rust(source);
    assert_rustc_run_stdout("run_references", &rust, "15\n");
}
```

- [ ] **Step 2: Add structs/arrays execution test**

```rust
#[test]
fn generated_rust_runs_for_structs_and_arrays() {
    let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn sum_array(arr: [i32; 3]) -> i32 {
    arr[0] + arr[1] + arr[2]
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let values = [p.x, p.y, 5];
    let total = sum_array(values);
    println("{}", total);
}
"#;
    let rust = compile_quanta_to_rust(source);
    assert_rustc_run_stdout("run_structs_arrays", &rust, "12\n");
}
```

- [ ] **Step 3: Add tuple ownership execution test**

```rust
#[test]
fn generated_rust_runs_for_tuple_after_by_value_call() {
    let source = r#"
fn sum(pair: (i32, i32)) -> i32 {
    pair.0 + pair.1
}

fn main() {
    let pair = (3, 4);
    let first = sum(pair);
    let second = sum(pair);
    println("{}", first + second);
}
"#;
    let rust = compile_quanta_to_rust(source);
    assert_rustc_run_stdout("run_tuple_after_by_value_call", &rust, "14\n");
}
```

- [ ] **Step 4: Run the executable smoke slice**

Run:

```powershell
python C:\Users\Zain\AGENTS\warden_shell\tools\safe_exec.py --timeout 90 -- cargo test --manifest-path public/pubscan/quantalang/compiler/Cargo.toml generated_rust_runs --quiet
```

Expected: 4 tests passed.

### Task 3: Update Status Artifacts

**Files:**
- Modify: `README.md`
- Modify: `STATUS.md`
- Modify: `project-docs/records/QUANTALANG-QUANTAC-STACK-ASSESSMENT-2026-06-13.md`
- Modify: `project-docs/roadmaps/contracts/backend-capability-descriptor-quantalang-2026-06-12.json`

- [ ] **Step 1: Update test counts**

After the full test run, update the documented compiler test count from 631 to the verified new count.

- [ ] **Step 2: Update Rust backend status**

State that the Rust backend has broad metadata validation plus a narrower executable stdout smoke slice.

- [ ] **Step 3: Update backend descriptor evidence**

Update the Rust backend notes to include the generated Rust executable smoke tests and the verified count.

### Task 4: Verify

**Files:**
- No new files beyond this plan.

- [ ] **Step 1: Format check**

Run:

```powershell
python C:\Users\Zain\AGENTS\warden_shell\tools\safe_exec.py --timeout 60 -- cargo fmt --manifest-path public/pubscan/quantalang/compiler/Cargo.toml -- --check
```

Expected: exit 0.

- [ ] **Step 2: Rust generated metadata slice**

Run:

```powershell
python C:\Users\Zain\AGENTS\warden_shell\tools\safe_exec.py --timeout 90 -- cargo test --manifest-path public/pubscan/quantalang/compiler/Cargo.toml generated_rust_compiles --quiet
```

Expected: all generated Rust metadata tests pass.

- [ ] **Step 3: Rust execution slice**

Run:

```powershell
python C:\Users\Zain\AGENTS\warden_shell\tools\safe_exec.py --timeout 90 -- cargo test --manifest-path public/pubscan/quantalang/compiler/Cargo.toml generated_rust_runs --quiet
```

Expected: all generated Rust execution tests pass.

- [ ] **Step 4: CLI Rust target slice**

Run:

```powershell
python C:\Users\Zain\AGENTS\warden_shell\tools\safe_exec.py --timeout 90 -- cargo test --manifest-path public/pubscan/quantalang/compiler/Cargo.toml rust_target --quiet
```

Expected: CLI Rust target/alias test passes.

- [ ] **Step 5: Full compiler suite**

Run:

```powershell
python C:\Users\Zain\AGENTS\warden_shell\tools\safe_exec.py --timeout 180 -- cargo test --manifest-path public/pubscan/quantalang/compiler/Cargo.toml --quiet
```

Expected: full suite passes with the verified count and 11 ignored tests.

- [ ] **Step 6: Documentation gates**

Run:

```powershell
python C:\Users\Zain\AGENTS\warden_shell\tools\safe_exec.py -- python -m json.tool project-docs/roadmaps/contracts/backend-capability-descriptor-quantalang-2026-06-12.json
python C:\Users\Zain\AGENTS\warden_shell\tools\safe_exec.py -- git -C public/pubscan/quantalang diff --check -- README.md STATUS.md compiler/src/codegen/backend/rust.rs docs/superpowers/plans/2026-06-13-rust-execution-layer.md
```

Expected: both commands exit 0.
