# Homebrew formula template for BuildLang (buildc, qdb, port).
# Before publishing to a tap (e.g. HarperZ9/homebrew-tap), replace each sha256
# placeholder with the exact digest from the matching GitHub Release SHA256SUMS.

class Buildlang < Formula
  desc "Deterministic systems and GPU compiler (buildc), BuildDB (qdb), and port tool"
  homepage "https://github.com/HarperZ9/buildlang"
  version "1.15.0"
  license "AGPL-3.0-only"

  on_macos do
    on_arm do
      url "https://github.com/HarperZ9/buildlang/releases/download/v#{version}/buildlang-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_MACOS_ARM64_SHA256_FROM_RELEASE_SHA256SUMS"
    end
    on_intel do
      url "https://github.com/HarperZ9/buildlang/releases/download/v#{version}/buildlang-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_MACOS_X64_SHA256_FROM_RELEASE_SHA256SUMS"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/HarperZ9/buildlang/releases/download/v#{version}/buildlang-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_LINUX_X64_SHA256_FROM_RELEASE_SHA256SUMS"
    end
  end

  def install
    bin.install "buildc", "qdb", "port"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/buildc --version")
    assert_match "qdb", shell_output("#{bin}/qdb --version")
    assert_predicate bin/"port", :executable?
  end
end
