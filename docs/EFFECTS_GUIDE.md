# QuantaLang Algebraic Effects Guide

Algebraic effects are QuantaLang's signature feature. Think of them as checked exceptions crossed with dependency injection: a function declares what side effects it performs, and the caller decides how to handle them. This gives you compile-time control over I/O, rendering, logging, and anything else that touches the outside world.

---

## Why Effects Matter for Graphics

In a game engine, your rendering code calls into Vulkan, DirectX, or OpenGL. With effects, you write the rendering logic once and swap the backend at the call site:

- Production: Vulkan handler
- Testing: mock handler that logs draw calls
- Profiling: handler that records timing
- Replay: handler that plays back recorded frames

No interfaces, no virtual dispatch, no runtime overhead. The effect handler is resolved at compile time.

---

## Defining an Effect

An effect declares a set of operations. It does not implement them -- that is the handler's job.

```quanta
effect Greeting {
    fn greet(name: str) -> (),
}
```

This says: "There exists a side effect called `Greeting` with one operation `greet` that takes a string and returns nothing." It is a contract, like a trait but for side effects.

An effect can have multiple operations:

```quanta
effect Render {
    fn draw(description: str) -> (),
    fn clear(r: f64, g: f64, b: f64) -> (),
    fn swap_buffers() -> (),
}
```

---

## Performing an Effect

A function that uses an effect must declare it in its signature with `~`:

```quanta
fn welcome() ~ Greeting {
    perform Greeting.greet("Alice");
}
```

The `~ Greeting` annotation means: "this function performs the Greeting effect." The compiler tracks this -- if you forget the annotation, you get a compile error. If you call a function that performs effects, your function must either handle them or propagate them in its own signature.

```quanta
fn welcome_everyone() ~ Greeting {
    perform Greeting.greet("Alice");
    perform Greeting.greet("Bob");
    perform Greeting.greet("Charlie");
}
```

---

## Capability Effects

Some effects are built into the compiler because they describe ambient runtime
capabilities rather than user-defined operations. `quantac check` surfaces these
as ordinary effect requirements, so operational access has to appear in the
function type instead of hiding behind a runtime helper.

| Capability | Direct ambient surfaces |
|------------|-------------------------|
| `FileSystem` | `read_file`, `write_file`, `file_exists`, `read_bytes`, `write_bytes`, `append_file`, `list_dir`, `is_dir`, `file_size` |
| `Network` | `tcp_connect`, `tcp_send`, `tcp_recv`, `tcp_close` |
| `Process` | `exit`, `process_exit` |
| `Environment` | `getenv`, `args_count`, `args_get` |
| `Clock` | `clock_ms`, `time_unix` |
| `Console` | `read_line`, `read_all`, `stdin_is_pipe`, direct print helpers, console macros such as `println!`, `print!`, `eprintln!`, `eprint!`, and diagnostic logging macros |
| `Foreign` | calls to functions declared in `extern` blocks |
| `Gpu` | direct `quanta_vk_*` runtime helpers |

```quanta
fn load_config() {
    read_file("ops.toml");
}
```

`quantac check` rejects that function because it performs `FileSystem` without
declaring it. The fixed version makes the capability part of the signature:

```quanta
fn load_config() ~ FileSystem {
    read_file("ops.toml");
}
```

The same rule applies to FFI:

```quanta
extern "C" { fn touch(); }

fn call_foreign() ~ Foreign {
    touch();
}
```

Diagnostics include a note naming the ambient call or macro, for example
`read_file`, `touch`, or `println`, so receipts and review tooling can point to
the exact capability source.

`quantac check <file> --receipt <path>` writes a deterministic
`quantalang-check-receipt/v1` JSON artifact with compiler/language version
metadata, a SHA-256 digest of the entry source bytes, an `input_digests` ledger
for every entry, import, include, and module file read by the check pipeline, an
`input_graph_digest` fingerprint for the whole checked source graph, declared
effects, observed capability sources, propagated effect callees, pass/fail
status, and compact diagnostics. Use `--receipt -` when a CI step or wrapper
wants the receipt on stdout.
Use `quantac receipt verify receipt.json` to re-check a saved receipt against
the current source graph. Add `--source path/to/app.quanta` when the source has
moved and the receipt's embedded source path should be overridden. Verification
checks the receipt schema, compiler/language identity, entry source digest,
input graph digest, file-backed policy digest, and any recorded built-in profile
digest. It also replays the compiler check and compares the saved
`declared_effects`, `observed_capabilities`, `propagated_effects`, diagnostics,
and policy violations against the current compiler result. Add `--json` to emit
a `quantalang-receipt-verification/v1` report with one pass/fail record per
verification check.
Add `--expect-profile ci-review` when a verification job must prove the receipt
was accepted under a specific built-in profile, not merely under whatever policy
metadata the receipt currently contains.
Add `--expect-policy-digest sha256:<hex>` when a verification job must prove the
receipt was accepted under an exact file-backed or built-in policy digest.

`observed_capabilities` records direct ambient capability use inside a function,
such as `read_file`, `tcp_connect`, `println!`, process helpers, or FFI helpers.
These entries are the accountability boundary for code that actually touches the
outside world.

`propagated_effects` records effectful callees that make a caller inherit a
typed effect. This lets policy allow a small number of audited boundary
functions while still proving which higher-level workflows depend on them.

Policy profiles turn receipt evidence into an enforceable CI gate:

```json
{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["FileSystem", "Console"],
  "direct_effect_allowlist": {
    "FileSystem": ["load_config"]
  },
  "direct_capability_source_allowlist": {
    "FileSystem": {
      "load_config": ["read_file"]
    }
  },
  "propagated_effect_allowlist": {
    "FileSystem": ["main"]
  },
  "propagated_effect_source_allowlist": {
    "FileSystem": {
      "main": ["load_config"]
    }
  },
  "require_source_digest": true,
  "require_input_graph_digest": true,
  "require_effect_allowlist": true,
  "require_provenance_allowlists": true,
  "require_source_allowlists": true,
  "require_allowlist_coverage": true
}
```

Run it with:

```bash
quantac check app.quanta --policy console-only.json --receipt receipt.json
```

Denied effects always fail. If `allowed_effects` is non-empty, any declared
effect, observed capability, or propagated effect outside the allow-list also
fails. Set `require_effect_allowlist` to make `allowed_effects` authoritative
even when the list is empty; this is how a pure receipt can remain pure in CI
instead of silently accepting later effect drift. `direct_effect_allowlist`
applies only to `observed_capabilities`;
`direct_capability_source_allowlist` narrows approved direct boundaries to
specific ambient helper, macro, or FFI sources; `propagated_effect_allowlist`
applies only to `propagated_effects`; `propagated_effect_source_allowlist`
narrows approved propagated callers to specific effectful callees.
Effect names in those policy fields must resolve to either a built-in
capability effect or an effect present in the checked source graph. Unknown
names are reported as `UnknownPolicyEffect` violations, which catches policy
typos such as `Netwrok` before they can weaken a CI gate.
Set `require_provenance_allowlists` to require every direct capability boundary
and propagated capability caller to be explicitly named.
Set `require_source_allowlists` to require every approved direct capability
boundary and propagated caller to also name its exact source entries.
Set `require_allowlist_coverage` to reject stale direct or propagated allowlist
entries, including source-level direct capability and propagated-effect
entries, that are not matched by the current receipt evidence.

For adoption, start from observed evidence instead of hand-copying receipt
fields:

```bash
quantac check app.quanta --receipt receipt.json
quantac policy scaffold receipt.json --output policy.json
```

The scaffolded policy keeps digest, effect-inventory, provenance, source, and
coverage requirements enabled, fills exact direct and propagated allowlists
from the receipt, and should be reviewed before it becomes a CI gate.

Use `quantac policy list` to see built-in starting profiles, or
`quantac policy list --json` to emit a machine-readable
`quantalang-policy-catalog/v1` catalog with profile names, summaries, policy
schemas, and SHA-256 digests. Then emit one profile with:

```bash
quantac policy print pure --output policy.json
```

Or run a built-in profile directly during a check:

```bash
quantac check app.quanta --profile ci-review --receipt -
```

For stored receipts, pin verification to that same built-in profile:

```bash
quantac receipt verify receipt.json --expect-profile ci-review --json
```

For file-backed policies, pin the policy document digest instead:

```bash
quantac receipt verify receipt.json --expect-policy-digest sha256:<hex> --json
```

The built-in profiles are valid `quantalang-check-policy/v1` JSON and are meant
for CI bootstrapping: `pure` denies every built-in ambient capability,
`console-only` permits only console access, `offline` permits local file,
environment, clock, and console work while denying network/process/FFI/GPU, and
`ci-review` requires source/input graph digests while denying the highest-risk
capabilities. `strict-accountability` also requires source/input graph digests,
an authoritative effect allow-list, direct and propagated provenance allowlists,
exact source allowlists, and coverage for every allowlist entry; run it
directly to reject ambient capability use by default, or print it and fill
project-specific allowlists. Receipts from
direct profile checks record `policy.source` as `builtin:<name>`,
`policy.profile` as the profile name, and
`policy.profile_digest` as the SHA-256 digest of the emitted built-in policy
JSON.
For locked CI gates, add `--expect-profile-digest <hex>` alongside `--profile`
using the digest from `quantac policy list --json` or a prior trusted receipt;
the check fails before source analysis if the selected built-in profile has
changed.

---

## Handling an Effect

The caller wraps the effectful code in a `handle/with` block and provides implementations for each operation:

```quanta
fn main() {
    handle {
        welcome()
    } with {
        Greeting.greet(name) => {
            println!("Hello, {}!", name)
        },
    }
}
```

When `welcome()` executes `perform Greeting.greet("Alice")`, control transfers to the handler. The handler runs `println!("Hello, Alice!")`, then control returns to the point after the `perform`.

---

## Full Example: Render Effect

Here is the pattern that makes effects powerful for game engines:

```quanta
effect Render {
    fn draw(description: str) -> (),
}

// Pure math -- no effects, no side effects
fn phong_lighting(normal: vec3, light_dir: vec3) -> vec3 {
    let ambient = vec3(0.1, 0.1, 0.1);
    let n = normalize(normal);
    let l = normalize(light_dir);
    let diff = dot(n, l);
    let diffuse = if diff > 0.0 {
        vec3(diff, diff, diff)
    } else {
        vec3(0.0, 0.0, 0.0)
    };
    ambient + diffuse
}

// Scene logic -- performs the Render effect but does not know HOW rendering works
fn render_scene() ~ Render {
    let normal = vec3(0.0, 1.0, 0.0);
    let light_dir = normalize(vec3(1.0, 1.0, 0.5));
    let color = phong_lighting(normal, light_dir);
    println!("Lighting: ({}, {}, {})", color.x, color.y, color.z);

    let model = mat4_translate(vec3(5.0, 1.0, 3.0));
    let world_pos = model * vec4(0.0, 0.0, 0.0, 1.0);

    perform Render.draw("player at (5, 1, 3)")
}
```

### Production Handler (Vulkan)

```quanta
fn main() {
    handle {
        render_scene()
    } with {
        Render.draw(desc) => {
            // In production: submit Vulkan draw commands
            println!("VULKAN DRAW: {}", desc)
        },
    }
}
```

### Test Handler (Mock)

```quanta
fn test_render() {
    handle {
        render_scene()
    } with {
        Render.draw(desc) => {
            // In tests: just log what would be drawn
            println!("MOCK DRAW: {}", desc)
        },
    }
}
```

### Profiling Handler

```quanta
fn profile_render() {
    handle {
        render_scene()
    } with {
        Render.draw(desc) => {
            println!("PROFILE: draw call recorded: {}", desc)
        },
    }
}
```

The `render_scene` function is identical in all three cases. Only the handler changes. This is the power of algebraic effects: the function that performs work does not decide how side effects are executed.

---

## Effect Propagation

Effects propagate through the call stack. If function A calls function B which performs an effect, function A must either handle it or declare it:

```quanta
effect Logger {
    fn log(message: str) -> (),
}

effect Render {
    fn draw(description: str) -> (),
}

// This function performs Logger
fn compute_something() ~ Logger {
    perform Logger.log("Computing...");
}

// This function performs both Logger and Render
fn render_with_logging() ~ Logger, Render {
    perform Logger.log("Starting render");
    perform Render.draw("scene");
    perform Logger.log("Render complete");
}

fn main() {
    handle {
        handle {
            render_with_logging()
        } with {
            Render.draw(desc) => {
                println!("DRAW: {}", desc)
            },
        }
    } with {
        Logger.log(msg) => {
            println!("[LOG] {}", msg)
        },
    }
}
```

Handlers can be nested. The inner handler resolves `Render`, the outer handler resolves `Logger`.

---

## Effects vs. Alternatives

| Approach               | Problem                                      |
|------------------------|----------------------------------------------|
| Global state           | Untraceable, untestable                      |
| Dependency injection   | Boilerplate, runtime overhead                |
| Virtual dispatch       | Vtable indirection, allocation               |
| Monads (Haskell)       | Complex types, hard to compose               |
| **Algebraic effects**  | Declared in signature, handled at call site, zero-cost |

Effects give you:
- **Compile-time tracking:** The type checker knows which effects a function performs.
- **Caller control:** The handler is at the call site, not baked into the callee.
- **Composability:** Multiple effects compose naturally -- just list them with commas.
- **Testability:** Swap a Vulkan handler for a mock handler in one line.

---

## Implementation Details

Under the hood, QuantaLang compiles effects to `setjmp`/`longjmp` on the C backend. When you `perform` an effect:

1. The runtime saves the current continuation (registers + stack pointer) with `setjmp`
2. Control jumps to the nearest matching handler via `longjmp`
3. The handler executes, then resumes the continuation

This is efficient -- no heap allocation, no garbage collection, no virtual dispatch. The overhead is one `setjmp` per `perform`, which is comparable to a function call on modern hardware.

---

## Quick Reference

```quanta
// Define an effect
effect EffectName {
    fn operation(param: Type) -> ReturnType,
}

// Declare that a function performs an effect
fn my_function() ~ EffectName {
    perform EffectName.operation(value);
}

// Multiple effects
fn my_function() ~ Effect1, Effect2 {
    perform Effect1.op1();
    perform Effect2.op2();
}

// Handle an effect
handle {
    my_function()
} with {
    EffectName.operation(param) => {
        // handler body
    },
}
```

---

## Patterns for Game Engines

### Swap rendering backend

```quanta
effect GPU {
    fn submit_draw_call(mesh: str, shader: str) -> (),
}

fn render_frame() ~ GPU {
    perform GPU.submit_draw_call("player_mesh", "pbr_shader");
    perform GPU.submit_draw_call("terrain_mesh", "terrain_shader");
}

// Vulkan backend
handle { render_frame() } with {
    GPU.submit_draw_call(mesh, shader) => {
        println!("vkCmdDraw: {} with {}", mesh, shader)
    },
}
```

### Record and replay

```quanta
effect Input {
    fn get_key(key: str) -> bool,
}

fn game_tick() ~ Input {
    let fire = perform Input.get_key("space");
}

// Live input
handle { game_tick() } with {
    Input.get_key(key) => {
        // poll real keyboard
        println!("Polling key: {}", key)
    },
}

// Replay from recording
handle { game_tick() } with {
    Input.get_key(key) => {
        // return recorded value
        println!("Replaying key: {}", key)
    },
}
```

---

## Next Steps

- `tests/programs/27_effects_showcase.quanta` -- minimal working effect example
- `tests/programs/38_graphics_demo.quanta` -- effects + vector math + rendering
- [SHADER_GUIDE.md](SHADER_GUIDE.md) -- write shaders that compile to CPU and GPU
- [GETTING_STARTED.md](GETTING_STARTED.md) -- full language overview
