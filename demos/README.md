# QuantaLang Visual Demo

Current status (2026-06-15): historical/experimental local demo artifact. This
directory preserves a Vulkan/SPIR-V proof path from the graphics work, including
hardcoded shader-generation helpers and machine-specific host code. It is not
the current release gate; current public proof lives in the C backend,
HLSL/GLSL quickstart, semantic corpus receipts, and Cargo test baseline.

**Experimental goal:** render a colored triangle on the GPU using
QuantaLang-generated SPIR-V shader artifacts.

## What This Proves

1. The historical SPIR-V helpers can generate shader artifacts for inspection
2. The checked-in host code documents one local Vulkan execution path
3. The demo is useful for backend preservation and future validation work
4. It does not prove a portable, current, source-to-SPIR-V release guarantee

## Building

### Prerequisites
- Windows 10/11
- Vulkan SDK (tested with 1.4.341)
- MSVC (Visual Studio 2022/2025)
- Rust + Cargo (for the QuantaLang compiler)

### Steps

```batch
:: 1. Generate the triangle shaders from the QuantaLang compiler
cd quantalang
cargo run --manifest-path compiler/Cargo.toml --example gen_triangle

:: 2. Validate the SPIR-V output
%VULKAN_SDK%\Bin\spirv-val.exe demos/hardcoded_vert.spv
%VULKAN_SDK%\Bin\spirv-val.exe demos/hardcoded_frag.spv

:: 3. Build the Vulkan rendering host
cd demos
cl vulkan_render.c /I %VULKAN_SDK%/Include /link vulkan-1.lib user32.lib gdi32.lib

:: 4. Run (from the quantalang root directory)
cd ..
demos\quantalang_demo.exe
```

## Output

```
=== QuantaLang Visual Demo ===
The Graphics Programming Language

GPU: NVIDIA GeForce RTX 4090
Swapchain: 1280x720
Shaders loaded: vert=1076 bytes, frag=488 bytes
Graphics pipeline: CREATED

=== Rendering ===
Rendered 180 frames

=== QuantaLang Demo Complete ===
```

In the historical local run, a 1280x720 window opened displaying a colored
triangle. Re-run and validate the demo locally before treating it as current
GPU evidence.

## Architecture

```
QuantaLang Compiler (Rust)
    │
    ├── SPIR-V Backend (spirv.rs; see STATUS.md for current line count)
    │   ├── generate_triangle_vertex_shader() → hardcoded_vert.spv
    │   └── generate_triangle_fragment_shader() → hardcoded_frag.spv
    │
    └── C Backend (c.rs) → same math functions run on CPU
            │
            ▼
    Vulkan Rendering Host (vulkan_render.c)
    ├── Win32 window (1280×720)
    ├── VkInstance + VkSurfaceKHR
    ├── VkDevice (RTX 4090)
    ├── VkSwapchainKHR (B8G8R8A8_SRGB)
    ├── VkRenderPass + VkFramebuffer
    ├── VkPipeline (vertex + fragment stages)
    └── Render loop (180 frames, double-buffered)
```

## QuantaLang Features Demonstrated

- **Backend preservation**: SPIR-V shader-generation artifacts and Vulkan host code
- **C/shader contrast**: examples for comparing CPU-oriented and shader-oriented paths
- **SPIR-V validation target**: validate generated artifacts with `spirv-val` before use
- **Local GPU execution record**: not a portable release claim
