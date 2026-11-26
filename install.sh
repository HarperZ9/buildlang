#!/usr/bin/env sh
# SPDX-License-Identifier: AGPL-3.0-only
# Zero-friction Linux/macOS installer for BuildLang (buildc, qdb, port).
# Verifies SHA256SUMS fail-closed before extracting any archive.
#
# Quick install:
#   curl -fsSL https://raw.githubusercontent.com/HarperZ9/buildlang/main/install.sh | sh
#
# Local verification (zero network):
#   sh install.sh --self-test
#   sh install.sh --from-dir ./dist --install-dir ./out --no-path

set -eu

REPO="${BUILDLANG_REPO:-HarperZ9/buildlang}"
VERSION="${BUILDLANG_VERSION:-}"
INSTALL_DIR="${BUILDLANG_INSTALL_DIR:-$HOME/.buildlang/bin}"
FROM_DIR=""
NO_PATH=0
SELF_TEST=0

usage() {
  cat <<'EOF'
Usage: install.sh [--version v1.15.0] [--install-dir <dir>] [--from-dir <dir>] [--no-path] [--self-test]
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      shift
      [ "$#" -gt 0 ] || { echo "Missing value for --version" >&2; exit 1; }
      VERSION="$1"
      ;;
    --install-dir)
      shift
      [ "$#" -gt 0 ] || { echo "Missing value for --install-dir" >&2; exit 1; }
      INSTALL_DIR="$1"
      ;;
    --from-dir)
      shift
      [ "$#" -gt 0 ] || { echo "Missing value for --from-dir" >&2; exit 1; }
      FROM_DIR="$1"
      ;;
    --no-path)
      NO_PATH=1
      ;;
    --self-test)
      SELF_TEST=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
  shift
done

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux)
      case "$arch" in
        x86_64|amd64) echo "x86_64-unknown-linux-gnu" ;;
        *) echo "Unsupported Linux architecture: $arch" >&2; return 1 ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        arm64|aarch64) echo "aarch64-apple-darwin" ;;
        x86_64|amd64) echo "x86_64-apple-darwin" ;;
        *) echo "Unsupported macOS architecture: $arch" >&2; return 1 ;;
      esac
      ;;
    *)
      echo "Unsupported OS for install.sh: $os (use install.ps1 on Windows)" >&2
      return 1
      ;;
  esac
}

sha256_file() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print tolower($1)}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print tolower($1)}'
  else
    echo "Fail-closed: neither sha256sum nor shasum found" >&2
    return 1
  fi
}

resolve_version() {
  if [ -n "$VERSION" ]; then
    case "$VERSION" in
      v*) echo "$VERSION" ;;
      *) echo "v$VERSION" ;;
    esac
    return 0
  fi
  if [ -n "$FROM_DIR" ]; then
    echo "v1.15.0"
    return 0
  fi
  effective="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest")"
  tag="${effective##*/}"
  if [ -z "$tag" ] || [ "$tag" = "latest" ]; then
    echo "Could not resolve latest release tag from https://github.com/${REPO}/releases/latest" >&2
    return 1
  fi
  echo "$tag"
}

verify_sha256() {
  archive="$1"
  sums="$2"
  asset="$3"
  [ -f "$sums" ] || { echo "Checksum file not found: $sums" >&2; return 1; }
  expected="$(awk -v target="$asset" '
    /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
    {
      h = tolower($1)
      f = $2
      sub(/^\*/, "", f)
      if (f == target && h ~ /^[0-9a-f]{64}$/) { print h; exit }
    }
  ' "$sums")"
  if [ -z "$expected" ]; then
    echo "Fail-closed: $asset not listed in $sums" >&2
    return 1
  fi
  actual="$(sha256_file "$archive")"
  if [ "$actual" != "$expected" ]; then
    echo "Fail-closed SHA-256 mismatch for $asset: expected $expected, got $actual" >&2
    return 1
  fi
  echo "SHA-256 verified: $asset ($actual)"
}

append_path_idempotent() {
  dir="$1"
  line="export PATH=\"$dir:\$PATH\""
  for rc in "$HOME/.profile" "$HOME/.bashrc" "$HOME/.zshrc"; do
    if [ -f "$rc" ]; then
      if ! grep -F "$dir" "$rc" >/dev/null 2>&1; then
        printf '\n# BuildLang\n%s\n' "$line" >> "$rc"
      fi
    fi
  done
}

install_buildlang() {
  tag="$1"
  target="$2"
  dest="$3"
  local_dir="$4"
  skip_path="$5"
  asset="buildlang-${tag}-${target}.tar.gz"
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  archive="$work/$asset"
  sums="$work/SHA256SUMS"

  if [ -n "$local_dir" ]; then
    [ -f "$local_dir/$asset" ] || { echo "Archive not found: $local_dir/$asset" >&2; return 1; }
    cp "$local_dir/$asset" "$archive" || return 1
    cp "$local_dir/SHA256SUMS" "$sums" || return 1
  else
    base_url="https://github.com/${REPO}/releases/download/${tag}"
    echo "Downloading $asset and SHA256SUMS from $REPO ($tag)..."
    curl -fsSL "$base_url/SHA256SUMS" -o "$sums" || return 1
    curl -fsSL "$base_url/$asset" -o "$archive" || return 1
  fi

  verify_sha256 "$archive" "$sums" "$asset" || return 1

  extract="$work/extract"
  mkdir -p "$extract" "$dest" || return 1
  tar -xzf "$archive" -C "$extract" || return 1

  for bin in buildc qdb port; do
    found="$(find "$extract" -type f -name "$bin" | head -n 1)"
    if [ -z "$found" ]; then
      echo "Fail-closed: expected binary $bin missing from $asset" >&2
      return 1
    fi
    cp "$found" "$dest/$bin" || return 1
    chmod 755 "$dest/$bin" || return 1
  done

  if [ "$skip_path" -eq 0 ]; then
    append_path_idempotent "$dest"
  fi

  for bin in buildc qdb; do
    ver="$("$dest/$bin" --version)" || return 1
    echo "Verified $bin: $ver"
  done
  [ -x "$dest/port" ] || { echo "Verification failed: port not executable in $dest" >&2; return 1; }
  echo "Verified port: installed at $dest/port"

  echo "Installed BuildLang $tag (buildc, qdb, port) into $dest"
  rm -rf "$work"
  trap - EXIT
}

run_self_test() {
  target="$(detect_target)"
  root="$(mktemp -d)"
  stage="$root/stage"
  dist="$root/dist"
  out_ok="$root/install-ok"
  out_bad="$root/install-bad"
  mkdir -p "$stage" "$dist"
  tag="v1.15.0"
  asset="buildlang-${tag}-${target}.tar.gz"

  for bin in buildc qdb port; do
    cat > "$stage/$bin" <<EOF
#!/usr/bin/env sh
echo "$bin 1.15.0 (self-test)"
EOF
    chmod 755 "$stage/$bin"
  done
  tar -czf "$dist/$asset" -C "$stage" buildc qdb port
  digest="$(sha256_file "$dist/$asset")"
  printf '%s  %s\n' "$digest" "$asset" > "$dist/SHA256SUMS"

  # Control 1: valid archive + matching checksum installs and verifies all 3 tools.
  install_buildlang "$tag" "$target" "$out_ok" "$dist" 1
  for bin in buildc qdb port; do
    [ -x "$out_ok/$bin" ] || { echo "Self-test failed: $bin missing" >&2; rm -rf "$root"; exit 1; }
  done

  # Control 2: planted 1-byte archive corruption must fail closed before extraction.
  printf '\377' >> "$dist/$asset"
  if install_buildlang "$tag" "$target" "$out_bad" "$dist" 1 >/dev/null 2>&1; then
    echo "Self-test failed: tampered archive was NOT refused" >&2
    rm -rf "$root"
    exit 1
  fi
  if [ -e "$out_bad/buildc" ]; then
    echo "Self-test failed: tampered archive extracted binaries" >&2
    rm -rf "$root"
    exit 1
  fi

  rm -rf "$root"
  echo "BUILDLANG INSTALL.SH SELF_TEST PASS valid_install=1 planted_tamper_refused=1"
}

if [ "$SELF_TEST" -eq 1 ]; then
  run_self_test
  exit 0
fi

TARGET="$(detect_target)"
TAG="$(resolve_version)"
install_buildlang "$TAG" "$TARGET" "$INSTALL_DIR" "$FROM_DIR" "$NO_PATH"
