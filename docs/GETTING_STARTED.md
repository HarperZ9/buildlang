# Getting Started with BuildLang

Current status (2026-06-15): the verified adoption path is source build,
`buildc run` through the C backend, HLSL/GLSL shader-source output, and the
semantic corpus receipt checks. SPIR-V, LLVM, WASM, Rust, x86-64, and ARM64 are
experimental research targets; examples below label those paths accordingly.

BuildLang is "The Effects Language" -- a systems programming language with
algebraic effects, designed for systems experiments and shader-oriented code
generation. Today, C is the verified native execution path and HLSL/GLSL are the
practical shader-output path.

---

## Prerequisites

- **Rust toolchain** (1.75+): [rustup.rs](https://rustup.rs)
- **C compiler** (one of): gcc, clang, or MSVC (`cl.exe` on Windows)
- **Vulkan SDK** (optional): for `spirv-val` shader validation -- [vulkan.lunarg.com](https://vulkan.lunarg.com/sdk/home)

---

## Installing the Compiler

The quickest path installs the `buildc` binary from crates.io:

```bash
cargo install buildlang
```

(Formerly published as `quantalang`, now deprecated.) Or build from source:

```bash
cd compiler
cargo build --release
```

The binary is at `compiler/target/release/buildc` (or `buildc.exe` on Windows). Add it to your PATH.

Check the local compiler and backend readiness:

```bash
buildc doctor
```

---

## Your First Program

Create `hello.bld`:

```build
fn main() {
    println!("Hello from BuildLang!");
}
```

Compile and run:

```bash
buildc run hello.bld
```

The repository includes the same flow as tested examples:

```bash
buildc run examples/quickstart/hello.bld
buildc run examples/quickstart/ledger.bld
buildc run examples/quickstart/effects_greeting.bld
```

---

## Your First Shader

Create `shader.bld`:

```build
fn aces_tonemap(x: f64) -> f64 {
    let num = x * (2.51 * x + 0.03);
    let den = x * (2.43 * x + 0.59) + 0.14;
    clamp(num / den, 0.0, 1.0)
}

#[fragment]
fn main(color: vec3) -> vec4 {
    let r = aces_tonemap(color.x);
    let g = aces_tonemap(color.y);
    let b = aces_tonemap(color.z);
    vec4(r, g, b, 1.0)
}
```

Compile to HLSL shader source:

```bash
buildc shader.bld --target hlsl -o shader.hlsl
```

Compile to CPU (C source for testing):

```bash
buildc shader.bld --target c -o shader.c
```

The SPIR-V backend is still an experimental research target. Use it for backend
experiments, not as the current release promise:

```bash
buildc shader.bld --target spirv -o shader.spv
```

For a tested HLSL shader quickstart:

```bash
buildc examples/quickstart/vignette_shader.bld --target hlsl -o vignette_shader.hlsl
```

---

## Multi-Target Compilation

BuildLang exposes several code generation backends with different maturity:

```bash
buildc file.bld --target=c        # C99 source, verified adoption path
buildc file.bld --target=hlsl     # HLSL shader source
buildc file.bld --target=glsl     # GLSL shader source
buildc file.bld --target=llvm     # Experimental LLVM IR
buildc file.bld --target=wasm     # Experimental WebAssembly/WAT path
buildc file.bld --target=spirv    # Experimental Vulkan SPIR-V
buildc file.bld --target=rust     # Experimental Rust source subset
buildc file.bld --target=x86-64   # Experimental x86-64 assembly
buildc file.bld --target=arm64    # Experimental ARM64 assembly
```

The output format is also inferred from the `-o` extension:

```bash
buildc shader.bld -o shader.spv   # infers --target=spirv, experimental
buildc shader.bld -o shader.c     # infers --target=c
buildc shader.bld -o shader.ll    # infers --target=llvm
```

---

## Shader Hot Reload

Watch a directory and recompile shader source on every save:

```bash
buildc watch shaders/ --target=hlsl
```

For SPIR-V experiments, use `--target=spirv` and validate the output with the
Vulkan SDK before loading it into a renderer.

---

## CLI Commands

```
buildc lex <file>           Tokenize and print tokens
buildc parse <file>         Parse and print AST
buildc check <file>         Type-check without compiling
buildc build [path]         Compile to C -> invoke C compiler -> native executable
buildc run <file>           Compile and run immediately
buildc fmt <file>           Format source code
buildc pkg <subcommand>     Package manager
buildc watch <path>         Watch and recompile on change
buildc lsp                  Start Language Server Protocol server
buildc repl                 Interactive REPL
buildc doctor               Diagnose compiler/toolchain/backend readiness
buildc corpus verify        Verify semantic corpus receipts and C stdout
buildc corpus verify --root <dir> --write
                             Verify a corpus copy and refresh its C receipt
buildc version              Print version
```

---

## Key Language Features

### Algebraic Effects

Effects are BuildLang's signature feature -- like checked exceptions crossed with dependency injection. You declare what side effects a function performs, and the caller decides how to handle them.

```build
effect Render {
    fn draw(description: str) -> (),
}

fn render_scene() ~ Render {
    perform Render.draw("player at (5, 1, 3)")
}

fn main() {
    // Application-specific rendering handler
    handle {
        render_scene()
    } with {
        Render.draw(desc) => {
            println!("RENDER: {}", desc)
        },
    }
}
```

See [EFFECTS_GUIDE.md](EFFECTS_GUIDE.md) for the full effects tutorial.

### Structs and Enums

```build
struct Point {
    x: i32,
    y: i32,
}

fn add_points(a: Point, b: Point) -> Point {
    Point { x: a.x + b.x, y: a.y + b.y }
}

enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
}
```

### Pattern Matching

```build
fn area(s: Shape) -> f64 {
    match s {
        Shape::Circle(r) => 3.14159 * r * r,
        Shape::Rectangle(w, h) => w * h,
    }
}
```

### Closures with Captures

```build
let offset: i32 = 10;
let add_offset = |x: i32| -> i32 { x + offset };
println!("{}", add_offset(5));   // 15
```

### Traits (Static Dispatch)

```build
trait Shape {
    fn area(self) -> f64;
    fn name(self) -> str;
}

struct Circle {
    radius: f64,
}

impl Shape for Circle {
    fn area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
    fn name(self) -> str {
        "Circle"
    }
}
```

### Vector and Matrix Math

Built-in `vec2`, `vec3`, `vec4`, and `mat4` types with operator overloading:

```build
let pos = vec3(1.0, 2.0, 3.0);
let dir = normalize(pos);
let d = dot(dir, vec3(0.0, 1.0, 0.0));

let model = mat4_translate(vec3(5.0, 0.0, 3.0));
let world_pos = model * vec4(0.0, 0.0, 0.0, 1.0);

// Swizzling
let xy = pos.xy;       // vec2
let rgb = pos.xyz;     // vec3
let bgr = pos.zyx;     // vec3
```

### GLSL Built-in Functions

All GLSL.std.450 builtins are available in normal code:

```
sin  cos  tan  asin  acos  atan
pow  exp  log  sqrt  inversesqrt
abs  floor  ceil  fract  round
clamp  mix  smoothstep  step
length  distance  dot  cross  normalize  reflect
min  max
```

These functions are available to the compiler's shader-oriented paths. On the
verified C route they lower to C math helpers; SPIR-V lowering is still treated
as experimental backend work.

---

## VS Code Extension

BuildLang ships with a VS Code extension providing syntax highlighting:

```bash
cd editors/vscode
npm install
npm run compile
```

Then open VS Code, go to Extensions > Install from VSIX, or use the debug launch configuration to test.

---

## Next Steps

- [SHADER_GUIDE.md](SHADER_GUIDE.md) -- Write vertex, fragment, and compute shaders
- [EFFECTS_GUIDE.md](EFFECTS_GUIDE.md) -- Master algebraic effects for rendering pipelines
- `examples/quickstart/` -- tested source examples for CPU execution and HLSL output
- `semantic-corpus/` -- 8-program C/Rust receipt corpus
- `tests/programs/` -- mixed legacy/current fixture corpus, not the release gate
- `examples/graphics/` and `demos/` -- historical/experimental Vulkan and SPIR-V artifacts
