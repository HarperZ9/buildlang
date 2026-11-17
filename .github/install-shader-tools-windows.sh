#!/usr/bin/env bash
# Install the shader tools the compiler tests call, on a Windows runner
# (Git Bash). Every download is pinned in the workflow by URL and SHA-256 and
# refused when the digest differs:
#   SPIRV-Tools   spirv-val, spirv-as, spirv-dis (conda-forge win-64 build)
#   glslang       glslang.exe, also copied to glslangValidator.exe, the name
#                 the tests call (Khronos release zip)
#   DXC           dxc.exe with dxcompiler.dll and dxil.dll (Microsoft release zip)
#
# No Vulkan device is installed: the device tests run on Linux in the
# gpu-device job. The test job runs with BUILDLANG_SHADER_TOOLS_REQUIRE=1, so
# a tool this script fails to provide fails the test that needs it.
set -euo pipefail

: "${SPIRV_TOOLS_WIN_URL:?the workflow pins SPIRV_TOOLS_WIN_URL}"
: "${SPIRV_TOOLS_WIN_SHA256:?the workflow pins SPIRV_TOOLS_WIN_SHA256}"
: "${GLSLANG_WIN_URL:?the workflow pins GLSLANG_WIN_URL}"
: "${GLSLANG_WIN_SHA256:?the workflow pins GLSLANG_WIN_SHA256}"
: "${DXC_WIN_URL:?the workflow pins DXC_WIN_URL}"
: "${DXC_WIN_SHA256:?the workflow pins DXC_WIN_SHA256}"

# RUNNER_TEMP is a Windows path; its backslashes would act as escapes in the
# globs below, so convert it to a POSIX path first.
root="$(cygpath -u "${RUNNER_TEMP:-/tmp}")/shader-tools"
bin="$root/bin"
mkdir -p "$root" "$bin"

fetch() { # url sha256 file
  curl -fsSL --retry 3 -o "$3" "$1"
  echo "$2  $3" | sha256sum -c -
}

# Extraction uses the Windows bsdtar, called by full path: it reads zip and
# zstd, and the DXC zip stores its paths with backslashes, which Git Bash
# unzip extracts with a warning and exit status 1.
extract() { # archive dir
  mkdir -p "$2"
  /c/Windows/System32/tar.exe -xf "$(cygpath -w "$1")" -C "$(cygpath -w "$2")"
}

# SPIRV-Tools: a .conda file is a zip holding pkg-*.tar.zst.
fetch "$SPIRV_TOOLS_WIN_URL" "$SPIRV_TOOLS_WIN_SHA256" "$root/spirv-tools.conda"
extract "$root/spirv-tools.conda" "$root/spirv-tools"
extract "$(ls "$root"/spirv-tools/pkg-*.tar.zst)" "$root/spirv-tools/pkg"
cp "$root"/spirv-tools/pkg/Library/bin/*.exe "$root"/spirv-tools/pkg/Library/bin/*.dll "$bin/"

fetch "$GLSLANG_WIN_URL" "$GLSLANG_WIN_SHA256" "$root/glslang.zip"
extract "$root/glslang.zip" "$root/glslang"
cp "$root/glslang/bin/glslang.exe" "$bin/glslang.exe"
cp "$root/glslang/bin/glslang.exe" "$bin/glslangValidator.exe"

fetch "$DXC_WIN_URL" "$DXC_WIN_SHA256" "$root/dxc.zip"
extract "$root/dxc.zip" "$root/dxc"
cp "$root"/dxc/bin/x64/*.exe "$root"/dxc/bin/x64/*.dll "$bin/"

if [ -n "${GITHUB_PATH:-}" ]; then
  cygpath -w "$bin" >> "$GITHUB_PATH"
fi
export PATH="$bin:$PATH"

spirv-val --version
spirv-dis --version
glslangValidator --version | head -n 1
dxc --version
