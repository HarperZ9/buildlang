#!/usr/bin/env bash
# Install the shader tools the compiler tests call, on an Ubuntu runner:
#   spirv-tools          spirv-val, spirv-as, spirv-dis
#   glslang-tools        glslangValidator
#   mesa-vulkan-drivers  lavapipe, a CPU Vulkan device (with the libvulkan1 loader)
#   DXC                  the release pinned in the workflow by DXC_TAG,
#                        DXC_ASSET and DXC_SHA256, checked against that SHA-256
#
# The tests run with BUILDLANG_SHADER_TOOLS_REQUIRE=1, so a tool this script
# fails to provide fails the test that needs it instead of skipping it.
set -euo pipefail

: "${DXC_TAG:?the workflow pins DXC_TAG}"
: "${DXC_ASSET:?the workflow pins DXC_ASSET}"
: "${DXC_SHA256:?the workflow pins DXC_SHA256}"

sudo apt-get update -q
sudo apt-get install -y -q --no-install-recommends \
  spirv-tools glslang-tools mesa-vulkan-drivers libvulkan1 vulkan-tools

dest="${RUNNER_TEMP:-/tmp}/dxc"
mkdir -p "$dest"
curl -fsSL --retry 3 -o "$dest/$DXC_ASSET" \
  "https://github.com/microsoft/DirectXShaderCompiler/releases/download/$DXC_TAG/$DXC_ASSET"
echo "$DXC_SHA256  $dest/$DXC_ASSET" | sha256sum -c -
tar -xzf "$dest/$DXC_ASSET" -C "$dest"
chmod +x "$dest/bin/dxc"*

if [ -n "${GITHUB_PATH:-}" ]; then
  echo "$dest/bin" >> "$GITHUB_PATH"
  echo "LD_LIBRARY_PATH=$dest/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" >> "$GITHUB_ENV"
fi
export PATH="$dest/bin:$PATH"
export LD_LIBRARY_PATH="$dest/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

spirv-val --version
glslangValidator --version | head -n 1
dxc --version
vulkaninfo --summary 2>/dev/null | grep -E 'deviceName|deviceType' || echo "vulkaninfo found no device"
