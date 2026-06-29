# Getting Started with BuildLang

Write shader-oriented BuildLang examples, emit HLSL/GLSL shader source, and
run CPU examples through the verified C backend. SPIR-V remains experimental;
inspect generated `.fx` files before trying them in ReShade.

## Install

```bash
# Build from source (requires Rust toolchain)
cd buildlang/compiler
cargo build --release

# Add to PATH
export PATH="$PWD/target/release:$PATH"

# Verify
buildc doctor
```

## Hello Shader (5 minutes)

Create `hello.bld`:

```build
#[uniform]
const brightness: f64 = 1.0;

fn adjust(c: f64) -> f64 {
    c * brightness
}

#[fragment]
fn PS_Hello(uv: vec2) -> vec4 {
    let color = tex2d(uv);
    vec4(adjust(color.x), adjust(color.y), adjust(color.z), 1.0)
}
```

Compile to ReShade:

```bash
buildc hello.bld --target hlsl -o hello.fx
```

Inspect `hello.fx` before use. If it matches the ReShade conventions you need,
copy it into `reshade-shaders/Shaders/` and test it in a local ReShade runtime.

The repository also includes a tested shader quickstart:

```bash
buildc examples/quickstart/vignette_shader.bld --target hlsl -o vignette_shader.hlsl
```

## What Just Happened

| BuildLang | Generated HLSL |
|------------|---------------|
| `#[uniform] const brightness: f64 = 1.0;` | `uniform float brightness < ui_type = "slider"; ... > = 1.0;` |
| `#[fragment] fn PS_Hello(uv: vec2) -> vec4` | `float4 PS_Hello(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target0` |
| `tex2d(uv)` | `tex2D(ReShade::BackBuffer, uv)` |
| `vec4(r, g, b, 1.0)` | `float4(r, g, b, 1.0)` |
| (auto-generated) | `technique Build_PS_Hello { pass { ... } }` |

## Language Basics

### Types

```build
let x: i32 = 42;        // 32-bit integer
let y: f64 = 3.14;      // 64-bit float (maps to float in HLSL)
let v: vec4 = vec4(1.0, 0.0, 0.0, 1.0);  // RGBA color
let b: bool = true;
```

### Functions

```build
fn add(a: f64, b: f64) -> f64 {
    a + b    // last expression is the return value
}
```

### Control Flow

```build
if x > 0.5 {
    1.0
} else {
    0.0
}

while i < 16.0 {
    // loop body
    i = i + 1.0;
}

for j in 0..10 {
    // counted loop
}
```

### Structs

```build
struct Color {
    r: f64,
    g: f64,
    b: f64,
}

impl Color {
    fn luminance(self) -> f64 {
        self.r * 0.2126 + self.g * 0.7152 + self.b * 0.0722
    }
}
```

## Shader Features

### Uniforms (ReShade Sliders)

```build
#[uniform]
const exposure: f64 = 0.0;

#[uniform]
const saturation: f64 = 1.0;
```

These become adjustable sliders in ReShade's UI.

### Texture Sampling

```build
let color = tex2d(uv);              // Sample backbuffer
let depth = tex2d_depth(uv);        // Sample depth buffer
```

### Fragment Shaders

```build
#[fragment]
fn PS_MyEffect(uv: vec2) -> vec4 {
    // uv.x, uv.y = screen coordinates (0..1)
    // Return: output color as vec4
    let color = tex2d(uv);
    vec4(color.x, color.y, color.z, 1.0)
}
```

The compiler auto-generates:
- `SV_Position` parameter
- `TEXCOORD` semantic on `uv`
- `SV_Target0` return semantic
- ReShade `technique` + `pass` block

### Shader Math Intrinsics

All standard shader math functions are available:

```build
sin(x)  cos(x)  tan(x)  sqrt(x)  pow(x, y)  abs(x)  exp(x)
floor(x)  ceil(x)  round(x)  fract(x)  min(a, b)  max(a, b)
clamp(x, lo, hi)  smoothstep(edge0, edge1, x)  mix(a, b, t)
dot(a, b)  cross(a, b)  normalize(v)  length(v)  reflect(i, n)
```

### Color Space Safety

```build
fn tonemap(c: vec3 with ColorSpace<Linear>) -> vec3 with ColorSpace<sRGB> {
    // The compiler enforces: input must be Linear, output is sRGB.
    // Passing sRGB to a function expecting Linear = compile error.
}
```

## Cross-Target Compilation

Same source, every target:

```bash
buildc shader.bld --target hlsl -o shader.fx      # ReShade / DirectX
buildc shader.bld --target glsl -o shader.glsl     # OpenGL / Vulkan
buildc shader.bld --target spirv -o shader.spv      # Vulkan binary
buildc shader.bld --target c -o shader.c            # CPU validation
```

## VS Code Extension

Install the BuildLang extension for:
- Syntax highlighting (keywords, types, shader intrinsics)
- Code snippets (`fn`, `fragment`, `uniform`, `vignette`, `hash`)
- LSP diagnostics (when `buildc` is in PATH)

## Example: SSAO Shader Fixture

See `demos/ssao.bld` for a BuildLang SSAO source fixture with depth
sampling, random kernel loop, occlusion computation, and adjustable uniforms.
It is useful for HLSL output inspection, but current release verification does
not claim ReShade runtime validation for this demo.

```bash
buildc demos/ssao.bld --target hlsl -o ssao.fx
```
