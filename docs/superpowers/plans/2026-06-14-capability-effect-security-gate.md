# Capability Effect Security Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make ambient file, network, process, environment, clock, console, GPU, and foreign-function access visible as typed QuantaLang effects enforced by `quantac`.

**Architecture:** Add a compiler-owned capability registry in the type layer, register capability effects as built-ins, and feed required effects into existing type inference. Reuse the current function checker’s undeclared-effect diagnostics, adding capability source notes so users see which ambient call triggered the requirement.

**Tech Stack:** Rust compiler code in `compiler/src/types`, existing QuantaLang parser/typechecker/codegen tests, Cargo integration tests for `quantac check`.

---

## File Structure

- Create: `compiler/src/types/capabilities.rs`
  - Owns the public capability vocabulary and maps ambient callable names to effect names.
- Modify: `compiler/src/types/mod.rs`
  - Exports the capability registry.
- Modify: `compiler/src/types/effects.rs`
  - Registers capability effects as built-in effects.
- Modify: `compiler/src/types/ty.rs`
  - Adds a function-type constructor that preserves both effects and lifetime parameters.
- Modify: `compiler/src/types/infer.rs`
  - Adds capability effect accumulation and source tracking for ambient calls and effectful callees.
- Modify: `compiler/src/types/check.rs`
  - Preserves declared effects in function bindings, assigns `Foreign` to extern function bindings, and attaches source notes to capability diagnostics.
- Modify: `compiler/tests/cli.rs`
  - Adds a `quantac check` smoke test for a capability diagnostic.
- No changes in this first implementation to codegen output, runtime C helpers, or semantic-corpus receipt JSON.

## Task 1: Capability Registry

**Files:**
- Create: `compiler/src/types/capabilities.rs`
- Modify: `compiler/src/types/mod.rs`
- Modify: `compiler/src/types/effects.rs`

- [ ] **Step 1: Write the failing registry tests**

Add this test module in the new file `compiler/src/types/capabilities.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ambient_runtime_calls_to_capability_effects() {
        assert_eq!(capability_effect_for_call("read_file"), Some("FileSystem"));
        assert_eq!(capability_effect_for_call("write_file"), Some("FileSystem"));
        assert_eq!(capability_effect_for_call("list_dir"), Some("FileSystem"));
        assert_eq!(capability_effect_for_call("tcp_connect"), Some("Network"));
        assert_eq!(capability_effect_for_call("process_exit"), Some("Process"));
        assert_eq!(capability_effect_for_call("getenv"), Some("Environment"));
        assert_eq!(capability_effect_for_call("clock_ms"), Some("Clock"));
        assert_eq!(capability_effect_for_call("quanta_vk_init"), Some("Gpu"));
        assert_eq!(capability_effect_for_call("sqrt"), None);
    }

    #[test]
    fn lists_stable_capability_effect_names() {
        assert!(capability_effect_names().contains(&"Console"));
        assert!(capability_effect_names().contains(&"FileSystem"));
        assert!(capability_effect_names().contains(&"Network"));
        assert!(capability_effect_names().contains(&"Process"));
        assert!(capability_effect_names().contains(&"Environment"));
        assert!(capability_effect_names().contains(&"Clock"));
        assert!(capability_effect_names().contains(&"Foreign"));
        assert!(capability_effect_names().contains(&"Gpu"));
    }
}
```

- [ ] **Step 2: Run the registry tests to verify they fail**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

Expected: compilation fails because `compiler/src/types/capabilities.rs`, `capability_effect_for_call`, and `capability_effect_names` do not exist yet.

- [ ] **Step 3: Add the minimal registry implementation**

Create `compiler/src/types/capabilities.rs` with:

```rust
//! Capability-effect registry for ambient runtime surfaces.

pub const CONSOLE: &str = "Console";
pub const FILE_SYSTEM: &str = "FileSystem";
pub const NETWORK: &str = "Network";
pub const PROCESS: &str = "Process";
pub const ENVIRONMENT: &str = "Environment";
pub const CLOCK: &str = "Clock";
pub const FOREIGN: &str = "Foreign";
pub const GPU: &str = "Gpu";

const CAPABILITY_EFFECTS: &[&str] = &[
    CONSOLE,
    FILE_SYSTEM,
    NETWORK,
    PROCESS,
    ENVIRONMENT,
    CLOCK,
    FOREIGN,
    GPU,
];

pub fn capability_effect_names() -> &'static [&'static str] {
    CAPABILITY_EFFECTS
}

pub fn is_capability_effect(name: &str) -> bool {
    CAPABILITY_EFFECTS.contains(&name)
}

pub fn capability_effect_for_call(name: &str) -> Option<&'static str> {
    match name {
        "println" | "print" => Some(CONSOLE),
        "read_file" | "write_file" | "file_exists" | "read_bytes" | "write_bytes"
        | "append_file" | "list_dir" | "is_dir" | "file_size" => Some(FILE_SYSTEM),
        "tcp_connect" | "tcp_send" | "tcp_recv" | "tcp_close" => Some(NETWORK),
        "exit" | "process_exit" => Some(PROCESS),
        "getenv" | "args_count" | "args_get" => Some(ENVIRONMENT),
        "read_line" | "read_all" | "stdin_is_pipe" => Some(CONSOLE),
        "clock_ms" | "time_unix" => Some(CLOCK),
        "quanta_vk_init" | "quanta_vk_load_shader_file" | "quanta_vk_run_compute"
        | "quanta_vk_shutdown" | "quanta_vk_create_graphics_pipeline"
        | "quanta_vk_set_push_constant_f32" | "quanta_vk_draw_frame"
        | "quanta_vk_should_close" | "quanta_vk_request_close"
        | "quanta_vk_device_name" => Some(GPU),
        _ => None,
    }
}
```

Add this export to `compiler/src/types/mod.rs`:

```rust
pub mod capabilities;
pub use capabilities::*;
```

Extend `EffectContext::register_builtin_effects` in `compiler/src/types/effects.rs` after the existing built-in effects:

```rust
        for (idx, name) in super::capabilities::capability_effect_names()
            .iter()
            .enumerate()
        {
            self.register_effect(EffectDef::new(DefId::new(0, 100 + idx as u32), *name));
        }
```

- [ ] **Step 4: Run the registry tests to verify they pass**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

Expected: the two capability registry tests pass.

- [ ] **Step 5: Commit the registry**

```powershell
git add compiler/src/types/capabilities.rs compiler/src/types/mod.rs compiler/src/types/effects.rs
git commit -m "feat: add capability effect registry"
```

## Task 2: Typechecker Enforcement

**Files:**
- Modify: `compiler/src/types/ty.rs`
- Modify: `compiler/src/types/infer.rs`
- Modify: `compiler/src/types/check.rs`

- [ ] **Step 1: Write failing typechecker tests**

Add helper tests to the `#[cfg(test)]` module in `compiler/src/types/check.rs`:

```rust
    fn check_source(source: &str) -> Vec<TypeErrorWithSpan> {
        let source_file = crate::lexer::SourceFile::new("capability_test.quanta", source);
        let mut lexer = crate::lexer::Lexer::new(&source_file);
        let tokens = lexer.tokenize().expect("tokenize capability fixture");
        let mut parser = crate::parser::Parser::new(&source_file, tokens);
        let module = parser.parse().expect("parse capability fixture");

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&module);
        checker.take_errors()
    }

    #[test]
    fn ambient_file_call_requires_filesystem_effect() {
        let errors = check_source(r#"fn main() { read_file("ops.txt"); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "FileSystem"
            )),
            "expected FileSystem effect error, got {errors:#?}"
        );
        assert!(
            errors.iter().any(|err| err.notes.iter().any(|note| note.contains("read_file"))),
            "expected diagnostic note naming read_file, got {errors:#?}"
        );
    }

    #[test]
    fn declared_filesystem_effect_allows_file_call() {
        let errors = check_source(r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#);

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    #[test]
    fn wrong_declared_effect_does_not_allow_file_call() {
        let errors = check_source(r#"fn main() ~ Network { read_file("ops.txt"); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndeclaredEffect { effect_name, .. } if effect_name == "FileSystem"
            )),
            "expected undeclared FileSystem error, got {errors:#?}"
        );
    }

    #[test]
    fn foreign_call_requires_foreign_effect() {
        let errors = check_source(
            r#"
            extern "C" { fn touch(); }
            fn main() { touch(); }
            "#,
        );

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "Foreign"
            )),
            "expected Foreign effect error, got {errors:#?}"
        );
        assert!(
            errors.iter().any(|err| err.notes.iter().any(|note| note.contains("touch"))),
            "expected diagnostic note naming touch, got {errors:#?}"
        );
    }
```

- [ ] **Step 2: Run the typechecker tests to verify they fail**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

Expected: at least the file-call and foreign-call tests fail because ambient calls and extern functions do not yet add capability effects.

- [ ] **Step 3: Preserve function effects and lifetimes together**

Add this constructor to `impl Ty` in `compiler/src/types/ty.rs` below `function_with_lifetimes`:

```rust
    pub fn function_with_effects_and_lifetimes(
        params: Vec<Ty>,
        ret: Ty,
        effects: super::effects::EffectRow,
        lifetime_params: Vec<Arc<str>>,
    ) -> Self {
        Self::new(TyKind::Fn(FnTy {
            params,
            ret: Box::new(ret),
            is_unsafe: false,
            abi: None,
            effects,
            lifetime_params,
        }))
    }
```

- [ ] **Step 4: Track capability sources in inference**

In `compiler/src/types/infer.rs`, import `BTreeMap` and `BTreeSet`:

```rust
use std::collections::{BTreeMap, BTreeSet};
```

Add this field to `TypeInfer`:

```rust
    capability_sources: BTreeMap<String, BTreeSet<String>>,
```

Initialize it in both constructors:

```rust
            capability_sources: BTreeMap::new(),
```

Add these methods to `impl TypeInfer`:

```rust
    pub fn capability_sources(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.capability_sources
    }

    fn record_capability_source(&mut self, effect_name: &str, source_name: &str) {
        self.capability_sources
            .entry(effect_name.to_string())
            .or_default()
            .insert(source_name.to_string());
    }

    fn call_name(func: &ast::Expr) -> Option<&str> {
        match &func.kind {
            ExprKind::Ident(ident) => Some(ident.name.as_ref()),
            ExprKind::Path(path) if path.is_simple() => path.last_ident().map(|i| i.name.as_ref()),
            _ => None,
        }
    }
```

In `infer_call`, capture `let call_name = Self::call_name(func).map(str::to_string);` before inferring `func_ty`.

When propagating `fn_ty.effects`, record capability sources:

```rust
                if let Some(name) = call_name.as_deref() {
                    for effect in &fn_ty.effects.effects {
                        if super::capabilities::is_capability_effect(effect.name.as_ref()) {
                            self.record_capability_source(effect.name.as_ref(), name);
                        }
                    }
                }
```

In the `TyKind::Error` branch of `infer_call`, add ambient built-in capability effects:

```rust
            TyKind::Error => {
                if let Some(name) = call_name.as_deref() {
                    if let Some(effect_name) = super::capabilities::capability_effect_for_call(name) {
                        self.current_effects.add(super::effects::Effect::new(effect_name));
                        self.record_capability_source(effect_name, name);
                    }
                }
                for arg in args {
                    let _ = self.infer_expr(arg);
                }
                Ty::fresh_var()
            }
```

- [ ] **Step 5: Preserve declared effects in function bindings**

In `compiler/src/types/check.rs`, update `collect_function` so function variables carry declared effects:

```rust
        let effects = self.lower_effect_annotations(&f.sig.effects);
        let fn_ty = Ty::function_with_effects_and_lifetimes(
            param_tys,
            sig.ret,
            effects,
            sig.lifetime_params.clone(),
        );
```

Update `collect_extern_block` to register foreign functions with `Foreign`:

```rust
    fn collect_extern_block(&mut self, eb: &ast::ExternBlockDef, _span: Span) {
        for foreign_item in &eb.items {
            if let ast::ForeignItemKind::Fn(f) = &foreign_item.kind {
                let def_id = self.ctx.fresh_def_id();
                let sig = self.lower_fn_sig(&f.generics, &f.sig);
                self.ctx.register_function(def_id, sig.clone());
                let param_tys: Vec<_> = sig.params.iter().map(|(_, ty)| ty.clone()).collect();
                let effects = super::effects::EffectRow::closed([super::effects::Effect::new(
                    super::capabilities::FOREIGN,
                )]);
                let fn_ty = Ty::function_with_effects_and_lifetimes(
                    param_tys,
                    sig.ret,
                    effects,
                    sig.lifetime_params.clone(),
                );
                self.ctx.define_var(f.name.name.clone(), fn_ty);
            }
        }
    }
```

When collecting results from `TypeInfer` in `check_function`, capture capability sources:

```rust
            let (body_ty, body_effects, capability_sources, infer_errors, has_return) = {
                ...
                let capability_sources = infer.capability_sources().clone();
                (body_ty, body_effects, capability_sources, infer.take_errors(), has_return)
            };
```

When building `UnhandledEffect` or `UndeclaredEffect` diagnostics, add a note if `capability_sources` has entries:

```rust
                    if let Some(sources) = capability_sources.get(body_eff.name.as_ref()) {
                        err_with_span.notes.push(format!(
                            "capability `{}` was triggered by ambient call(s): {}",
                            body_eff.name,
                            sources.iter().cloned().collect::<Vec<_>>().join(", ")
                        ));
                    }
```

- [ ] **Step 6: Run the typechecker tests to verify they pass**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml capability --quiet
```

Expected: registry and typechecker capability tests pass.

- [ ] **Step 7: Commit typechecker enforcement**

```powershell
git add compiler/src/types/ty.rs compiler/src/types/infer.rs compiler/src/types/check.rs
git commit -m "feat: enforce ambient capability effects"
```

## Task 3: CLI Diagnostic Smoke

**Files:**
- Modify: `compiler/tests/cli.rs`

- [ ] **Step 1: Write the failing CLI test**

Add this test to `compiler/tests/cli.rs`:

```rust
#[test]
fn check_reports_capability_effect_for_ambient_file_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_capability_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() { read_file("ops.txt"); }"#)
        .expect("write capability fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "ambient file call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name triggering ambient call:\n{}",
        stderr
    );
}
```

- [ ] **Step 2: Run the CLI test to verify it passes after Task 2**

Run:

```powershell
cargo test --manifest-path compiler/Cargo.toml --test cli check_reports_capability_effect_for_ambient_file_call -- --nocapture
```

Expected: PASS. If it fails because `cmd_check` prints errors without notes, update the CLI error printer to include `err.help` and `err.notes`, matching `cmd_compile`.

- [ ] **Step 3: Commit CLI coverage**

```powershell
git add compiler/tests/cli.rs compiler/src/main.rs
git commit -m "test: cover capability diagnostics in quantac check"
```

## Task 4: Verification And Public Posture

**Files:**
- Modify only if implementation behavior changes public claims: `README.md`, `docs/EFFECTS_GUIDE.md`, `semantic-corpus/README.md`

- [ ] **Step 1: Run focused tests**

```powershell
cargo test --manifest-path compiler/Cargo.toml capability --quiet
cargo test --manifest-path compiler/Cargo.toml --test cli check_reports_capability_effect_for_ambient_file_call -- --nocapture
```

Expected: all pass.

- [ ] **Step 2: Run compiler suite**

```powershell
cargo test --manifest-path compiler/Cargo.toml --quiet
```

Expected: all non-ignored tests pass.

- [ ] **Step 3: Run warning-clean suite**

```powershell
$env:RUSTFLAGS='-Dwarnings'; cargo test --manifest-path compiler/Cargo.toml --quiet
```

Expected: all non-ignored tests pass with warnings denied.

- [ ] **Step 4: Run formatting and hygiene checks**

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
git diff --check
git check-ignore -q .env
powershell -NoProfile -ExecutionPolicy Bypass -File C:/dev/scratch/portfolio-stabilization-2026-06-13/scan-diff-secrets.ps1 -Repo C:/dev/public/pubscan/quantalang
```

Expected: all commands exit successfully; secret scan prints `no-matches`.

- [ ] **Step 5: Commit any docs/posture changes**

If docs changed:

```powershell
git add README.md docs/EFFECTS_GUIDE.md semantic-corpus/README.md
git commit -m "docs: describe capability effect gate"
```

If no docs changed, skip this commit and record that public claim updates are deferred until the gate is stable across examples.

- [ ] **Step 6: Push and verify CI**

```powershell
git push origin main
gh run list -R HarperZ9/quantalang --branch main --limit 3 --json databaseId,workflowName,status,conclusion,headSha,displayTitle,createdAt
```

Expected: latest CI runs on the pushed head. Watch the CI run to completion and fix any failures before calling the feature complete.

## Self-Review Notes

- Spec coverage: registry, built-in effect registration, runtime call enforcement, `Foreign` enforcement, capability diagnostics, and CLI smoke coverage are all mapped to tasks.
- Deferred by design: receipt schema changes, `println!` macro enforcement, and GPU-specific examples are listed in the approved design as later phases and are not required for the first working gate.
- No placeholders: all planned code steps include concrete code or concrete commands.
- Type consistency: effect names use the approved public names `Console`, `FileSystem`, `Network`, `Process`, `Environment`, `Clock`, `Foreign`, and `Gpu`.
