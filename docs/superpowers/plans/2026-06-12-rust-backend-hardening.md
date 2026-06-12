# Rust Backend Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden the QuantaC Rust backend so generated Rust is continuously checked by `rustc` for the ownership, borrow, lifetime, struct, array, and branch subset that the backend claims to support.

**Architecture:** Add backend-local regression tests that lower real `.quanta` programs to Rust source and run `rustc --emit=metadata` on the generated source. Use those tests to expose backend gaps, then keep implementation changes scoped to `compiler/src/codegen/backend/rust.rs`.

**Tech Stack:** Rust compiler crate, QuantaLang parser/type checker/MIR lowerer, `rustc --emit=metadata`, Cargo unit tests.

---

### Task 1: Add Rust Backend Compilation Harness

**Files:**
- Modify: `compiler/src/codegen/backend/rust.rs`

- [ ] **Step 1: Write the failing test helpers**

Add a `tests`-module helper that parses source, type-checks it, lowers to Rust, writes generated Rust to a temp file under `std::env::temp_dir()`, and invokes `rustc --emit=metadata`.

```rust
fn compile_quanta_to_rust(source: &str) -> String {
    let source_file = SourceFile::new("rust_backend_test.quanta", source);
    let mut lexer = Lexer::new(&source_file);
    let tokens = lexer.tokenize().expect("lexing should succeed");
    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser.parse().expect("parsing should succeed");
    assert!(parser.errors().is_empty(), "unexpected parser errors: {:?}", parser.errors());

    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.check_module(&ast);
    assert!(!checker.has_errors(), "unexpected type errors: {:?}", checker.errors());

    let mut codegen = CodeGenerator::with_source(&ctx, Target::Rust, source_file.source().into());
    codegen
        .generate(&ast)
        .expect("rust codegen should succeed")
        .as_string()
        .expect("generated Rust should be UTF-8")
}
```

- [ ] **Step 2: Add `rustc` verifier helper**

```rust
fn assert_rustc_metadata_ok(name: &str, rust_source: &str) {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let dir = std::env::temp_dir().join(format!(
        "quantalang_rust_backend_{}_{}",
        name,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let source_path = dir.join("generated.rs");
    let metadata_path = dir.join("generated.rmeta");
    std::fs::write(&source_path, rust_source).expect("write generated Rust");
    let output = std::process::Command::new(rustc)
        .arg("--emit=metadata")
        .arg("-o")
        .arg(&metadata_path)
        .arg(&source_path)
        .output()
        .expect("invoke rustc");
    assert!(
        output.status.success(),
        "rustc failed for {name}\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        rust_source
    );
}
```

- [ ] **Step 3: Run the focused test target**

Run: `C:\Users\Zain\.cargo\bin\cargo.exe test --manifest-path compiler\Cargo.toml codegen::backend::rust -- --nocapture`

Expected: existing Rust backend test passes before new behavior tests are added.

### Task 2: Add Subset Behavior Tests

**Files:**
- Modify: `compiler/src/codegen/backend/rust.rs`

- [ ] **Step 1: Add scalar and branch compile tests**

```rust
#[test]
fn generated_rust_compiles_for_scalar_branch_subset() {
    let source = r#"
fn choose(x: i32) -> i32 {
    if x > 0 { x } else { 0 }
}

fn main() {
    let v: i32 = choose(4);
    println!("{}", v);
}
"#;
    let rust = compile_quanta_to_rust(source);
    assert_rustc_metadata_ok("scalar_branch", &rust);
}
```

- [ ] **Step 2: Add references and mutable update compile test**

```rust
#[test]
fn generated_rust_compiles_for_reference_subset() {
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
    println!("{}", val);
}
"#;
    let rust = compile_quanta_to_rust(source);
    assert_rustc_metadata_ok("references", &rust);
}
```

- [ ] **Step 3: Add structs and arrays compile test**

```rust
#[test]
fn generated_rust_compiles_for_structs_and_arrays() {
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
    println!("{}", total);
}
"#;
    let rust = compile_quanta_to_rust(source);
    assert_rustc_metadata_ok("structs_arrays", &rust);
}
```

- [ ] **Step 4: Add lifetime smoke compile test**

```rust
#[test]
fn generated_rust_compiles_for_lifetime_smoke_program() {
    let source = r#"
fn identity(x: &i32) -> &i32 {
    x
}

fn main() {
    let a: i32 = 42;
    let r: &i32 = identity(&a);
    println!("{}", *r);
}
"#;
    let rust = compile_quanta_to_rust(source);
    assert_rustc_metadata_ok("lifetime_smoke", &rust);
}
```

- [ ] **Step 5: Run tests and verify red**

Run: `C:\Users\Zain\.cargo\bin\cargo.exe test --manifest-path compiler\Cargo.toml codegen::backend::rust -- --nocapture`

Expected: at least one new behavior test fails if generated Rust is not compile-clean for the claimed subset.

### Task 3: Harden Rust Backend Output

**Files:**
- Modify: `compiler/src/codegen/backend/rust.rs`

- [ ] **Step 1: Fix only the failures exposed by Task 2**

Use the `rustc` stderr printed by the failing tests. Keep fixes scoped to backend lowering: type mapping, value lowering, place lowering, function signatures, or emitted runtime helpers.

- [ ] **Step 2: Re-run the focused tests**

Run: `C:\Users\Zain\.cargo\bin\cargo.exe test --manifest-path compiler\Cargo.toml codegen::backend::rust -- --nocapture`

Expected: all Rust backend behavior tests pass.

### Task 4: Verify Compiler Slice

**Files:**
- Modify: `compiler/src/codegen/backend/rust.rs`

- [ ] **Step 1: Format**

Run: `C:\Users\Zain\.cargo\bin\cargo.exe fmt --manifest-path compiler\Cargo.toml`

Expected: exits 0.

- [ ] **Step 2: Run targeted codegen tests**

Run: `C:\Users\Zain\.cargo\bin\cargo.exe test --manifest-path compiler\Cargo.toml codegen --quiet`

Expected: codegen tests pass.

- [ ] **Step 3: Run borrow/lifetime type tests**

Run: `C:\Users\Zain\.cargo\bin\cargo.exe test --manifest-path compiler\Cargo.toml lifetime --quiet`

Expected: lifetime tests pass.

- [ ] **Step 4: Run full compiler tests if the targeted slice is clean**

Run: `C:\Users\Zain\.cargo\bin\cargo.exe test --manifest-path compiler\Cargo.toml --quiet`

Expected: full compiler test suite passes or any unrelated pre-existing failures are reported with file/test names.
